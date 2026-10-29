//! Realtime fan-out and gateway authorization (server-only).
//!
//! A process-wide [`Hub`] tracks every open gateway socket per user. Anything
//! that produces an event (a channel message, a DM, a presence change) works
//! out WHO should hear it from the database at delivery time -- the members of
//! the server, the participants of the DM -- and hands the list to the hub.
//! Because the audience is recomputed per event there is no per-socket
//! subscription state to keep in sync: join a server and you start hearing it,
//! leave and you stop.
//!
//! Authorization for the gateway also lives here: a socket may only read or
//! post in a text channel of a server the (token-authenticated) user belongs
//! to; posts are size-limited and rate-limited per user.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use tokio::sync::mpsc;

use crate::db::{Db, Message};
use crate::permissions::{self, Permissions};
use crate::rate_limit::RateLimiter;
use crate::server_settings::WsServerMsg;

/// Identifies one open socket of a user.
pub type ConnId = u64;

/// Per-socket outbound queue depth. A client that falls this far behind drops
/// events (it can re-sync with `History`) rather than growing server memory.
const QUEUE_DEPTH: usize = 512;

/// Longest message body accepted, in characters.
pub const MAX_MESSAGE_CHARS: usize = 4000;

/// Image attachment limits, enforced at the post boundary (see
/// `0015_message_attachments.sql`): only these mimes, at most this many
/// decoded bytes per image, at most this many images per message.
const ATTACHMENT_MIMES: &[&str] = &["image/png", "image/jpeg", "image/webp", "image/gif"];
const MAX_ATTACHMENT_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_ATTACHMENTS_PER_MESSAGE: usize = 4;

/// Posts allowed per user per window.
const SEND_LIMIT: usize = 20;
const SEND_WINDOW: Duration = Duration::from_secs(10);

/// The set of open gateway sockets, keyed by user id.
#[derive(Default)]
pub struct Hub {
    conns: Mutex<HashMap<i64, Vec<(ConnId, mpsc::Sender<WsServerMsg>)>>>,
    next: AtomicU64,
}

static HUB: OnceLock<Hub> = OnceLock::new();

/// The process-wide hub.
pub fn hub() -> &'static Hub {
    HUB.get_or_init(Hub::default)
}

impl Hub {
    /// Register a new socket for `user_id`; returns its id and the receiver of
    /// the events routed to it.
    pub fn register(&self, user_id: i64) -> (ConnId, mpsc::Receiver<WsServerMsg>) {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel(QUEUE_DEPTH);
        self.conns
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .entry(user_id)
            .or_default()
            .push((id, tx));
        (id, rx)
    }

    /// Remove a socket. Returns `true` when that was the user's LAST open
    /// socket (the caller then marks them offline).
    pub fn unregister(&self, user_id: i64, id: ConnId) -> bool {
        let mut map = self.conns.lock().unwrap_or_else(|p| p.into_inner());
        let Some(list) = map.get_mut(&user_id) else {
            return true;
        };
        list.retain(|(cid, _)| *cid != id);
        if list.is_empty() {
            map.remove(&user_id);
            true
        } else {
            false
        }
    }

    /// Deliver `msg` to every open socket of every user in `users` (each user
    /// once even if listed twice). Never blocks: a socket whose queue is full
    /// misses the event. Returns how many sockets it was queued on.
    pub fn publish(&self, users: &[i64], msg: &WsServerMsg) -> usize {
        let map = self.conns.lock().unwrap_or_else(|p| p.into_inner());
        let mut seen: Vec<i64> = Vec::with_capacity(users.len());
        let mut queued = 0;
        for &u in users {
            if seen.contains(&u) {
                continue;
            }
            seen.push(u);
            if let Some(list) = map.get(&u) {
                for (_, tx) in list {
                    if tx.try_send(msg.clone()).is_ok() {
                        queued += 1;
                    }
                }
            }
        }
        queued
    }

    /// How many sockets `user_id` currently has open.
    pub fn connection_count(&self, user_id: i64) -> usize {
        self.conns
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&user_id)
            .map_or(0, Vec::len)
    }

    /// Test hook: drop every open socket. The process-wide hub is shared by
    /// all e2e tests; a previous test's router tasks keep running after its
    /// `e2e_lock` guard is released and may still be holding sockets, which
    /// would make [`Hub::unregister`] stop reporting "last socket". Call this
    /// at the start of an e2e test (while holding the lock) for a clean slate.
    #[cfg(test)]
    pub fn reset(&self) {
        self.conns.lock().unwrap_or_else(|p| p.into_inner()).clear();
    }
}

/// A reaction is one emoji (at most 16 chars), or a custom emoji's
/// `<:name:id>` token.
fn reaction_ok(emoji: &str) -> bool {
    let custom = emoji.starts_with("<:") && emoji.ends_with('>') && emoji.len() <= 64;
    !emoji.trim().is_empty() && (emoji.chars().count() <= 16 || custom)
}

pub(crate) fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn send_limiter() -> &'static RateLimiter {
    static LIMITER: OnceLock<RateLimiter> = OnceLock::new();
    LIMITER.get_or_init(|| RateLimiter::new(SEND_LIMIT, SEND_WINDOW))
}

/// The user ids that belong to `server_id`.
pub fn server_audience(db: &Db, server_id: i64) -> Vec<i64> {
    db.server_members(server_id)
        .into_iter()
        .map(|(id, _, _)| id)
        .collect()
}

/// Everyone who can see channel `channel_id`. Empty when the channel doesn't
/// exist.
pub fn channel_audience(db: &Db, channel_id: i64) -> Vec<i64> {
    let Some((_id, server_id, _name, _kind, _pos, _parent)) = db.channel_full(channel_id) else {
        return Vec::new();
    };
    permissions::ServerContext::load(db, server_id)
        .map(|ctx| ctx.members_with(channel_id, Permissions::VIEW_CHANNEL))
        .unwrap_or_default()
}

/// Everyone who should hear about `user_id`'s presence: the user themself and
/// every member of every server they belong to.
pub fn presence_audience(db: &Db, user_id: i64) -> Vec<i64> {
    let mut out = vec![user_id];
    for (server_id, _, _) in db.servers_for_user(user_id) {
        out.extend(server_audience(db, server_id));
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Tell the people who care that `user_id`'s presence became `status`.
pub fn announce_presence(db: &Db, user_id: i64, status: &str) {
    hub().publish(
        &presence_audience(db, user_id),
        &WsServerMsg::Presence {
            user_id,
            status: status.to_string(),
        },
    );
}

/// What others see of a user: their picked status (`status_choice`) and
/// whether they are active. Invisible reads as offline; a picked Idle or Do
/// Not Disturb holds whatever the activity.
pub fn shown_presence(choice: &str, active: bool) -> &'static str {
    match choice {
        "invisible" => "offline",
        "dnd" => "dnd",
        "idle" => "away",
        _ if active => "online",
        _ => "away",
    }
}

/// Store `user_id`'s shown presence and tell their audience when it changed.
pub fn refresh_presence(db: &Db, user_id: i64, active: bool) {
    let shown = shown_presence(&db.status_choice(user_id), active);
    if db.set_presence(user_id, shown).is_ok_and(|changed| changed) {
        announce_presence(db, user_id, shown);
    }
}

/// Tell the other members of `server_id` that `user_id` joined (`joined:
/// true`) or left (`joined: false`). Called by the join (invite redemption)
/// and leave (kick/ban) paths so the roster — and the `MEMBERS — N` count —
/// updates live on the members' open clients. The audience is recomputed from
/// the (already-persisted) membership after the change, so a join reaches the
/// existing members and a leave reaches the remaining ones. No roster data
/// rides on the frame; the client re-fetches `list_members`.
pub fn announce_membership(db: &Db, server_id: i64, user_id: i64, joined: bool) {
    hub().publish(
        &server_audience(db, server_id),
        &WsServerMsg::MemberChanged {
            server_id,
            user_id,
            joined,
        },
    );
}

/// Check that `user_id` holds `need` in text channel `channel_id`: the
/// channel must exist, be a text channel the user can see, in a server they
/// are a member of. Returns the channel's server id and the user's
/// permissions in it.
pub fn authorize_text_channel(
    db: &Db,
    user_id: i64,
    channel_id: i64,
    need: Permissions,
) -> Result<(i64, Permissions), String> {
    let (_id, server_id, _name, kind, _pos, _parent) =
        db.channel_full(channel_id).ok_or("channel not found")?;
    let ctx = permissions::ServerContext::load(db, server_id).ok_or("channel not found")?;
    let member = ctx.member(user_id).ok_or("not a member of this server")?;
    let perms = ctx.in_channel(&member, channel_id);
    if !perms.contains(Permissions::VIEW_CHANNEL) {
        return Err("channel not found".into());
    }
    if kind != "text" {
        return Err("not a text channel".into());
    }
    if !perms.contains(need) {
        return Err(permissions::missing(need));
    }
    Ok((server_id, perms))
}

/// The user ids a message from `author_id` in `channel_id` notifies: named
/// members plus the `@everyone` / `@here` / role expansions (see
/// [`crate::server_roles::resolve_mentions`]).
pub fn resolve_mentions(db: &Db, channel_id: i64, author_id: i64, content: &str) -> Vec<i64> {
    crate::server_roles::resolve_mentions(db, channel_id, author_id, content)
}

/// Validate one image attachment: allowed mime on BOTH the declared field and
/// the `data:` URL prefix (the URL prefix is what the browser actually renders,
/// so the two must agree), non-empty base64 body, and at most
/// [`MAX_ATTACHMENT_BYTES`] decoded. Decoded size is bounded arithmetically —
/// 4 base64 chars carry at most 3 bytes — so we never need a decoder here.
fn validate_attachment(filename: &str, mime: &str, data: &str) -> Result<(), String> {
    if filename.trim().is_empty() {
        return Err("attachment filename is empty".into());
    }
    if !ATTACHMENT_MIMES.iter().any(|m| *m == mime) {
        return Err(format!("attachment mime not allowed: {mime}"));
    }
    const SCHEMES: &[&str] = &[
        "data:image/png;base64,",
        "data:image/jpeg;base64,",
        "data:image/webp;base64,",
        "data:image/gif;base64,",
    ];
    let want = format!("data:{mime};base64,");
    if !SCHEMES.iter().any(|s| data.starts_with(s)) || !data.starts_with(&want) {
        return Err(format!("attachment data URL does not match its mime: {mime}"));
    }
    let body = data.len() - want.len();
    // 4 base64 chars decode to at most 3 bytes; +3 per partial group is the
    // upper bound, so reject on the bound without decoding.
    if body == 0 || (body + 3) / 4 * 3 > MAX_ATTACHMENT_BYTES {
        return Err(format!("attachment too large (max {MAX_ATTACHMENT_BYTES} bytes)"));
    }
    Ok(())
}

/// Validate, decode and store one attachment's bytes in the media store,
/// returning the `/media/<sha>.<ext>` path the DB + frame carry. `validate_attachment`
/// already checked the mime/prefix/size bound; here the base64 must actually
/// decode, the bytes must pass the media layer's magic-byte + decode-under-
/// limits validation, and the file is written content-addressed.
fn media_store_attachment(data: &str, mime: &str) -> Result<String, String> {
    let (_, bytes) = match crate::media::decode_data_image_url(data) {
        Some(p) => p,
        None => return Err("attachment base64 body does not decode".into()),
    };
    crate::media::store(bytes.as_slice(), mime, MAX_ATTACHMENT_BYTES)
}

/// Validate, persist and fan out one channel message. The audience is everyone
/// who can see the channel (including the sender's own sockets, which is how
/// the sender receives its echo). Needs Send Messages, plus Attach Files when
/// there are attachments.
///
/// A message must carry a body OR at least one image attachment (not neither);
/// `attachments` are validated by [`validate_attachment`] (mime allowlist,
/// size cap, count cap) and persisted in the same transaction as the message
/// row (`db::insert_message_with_attachments`). The persisted attachments
/// (id, filename, mime, data URL) ride the broadcast frame as
/// `attachments`, so clients render without a second lookup.
///
/// Before persisting, the message text is scanned for `@username` tokens and
/// each is resolved against the server's members (see [`resolve_mentions`]).
/// The resolved user ids are stored in `message_mentions` and carried on the
/// broadcast frame as `mentioned_user_ids`, so every client can tell whether
/// it was mentioned.
pub fn post_channel_message(
    db: &Db,
    user_id: i64,
    channel_id: i64,
    content: &str,
    reply_to_message_id: Option<i64>,
    attachments: Vec<crate::server_settings::AttachmentInput>,
) -> Result<(), String> {
    if attachments.len() > MAX_ATTACHMENTS_PER_MESSAGE {
        return Err(format!("too many attachments (max {MAX_ATTACHMENTS_PER_MESSAGE})"));
    }
    if content.trim().is_empty() && attachments.is_empty() {
        return Err("empty message".into());
    }
    if content.chars().count() > MAX_MESSAGE_CHARS {
        return Err(format!("message too long (max {MAX_MESSAGE_CHARS} characters)"));
    }
    for att in &attachments {
        validate_attachment(&att.filename, &att.mime, &att.data)?;
    }
    if let Err(retry) = send_limiter().check(&user_id.to_string()) {
        return Err(format!("rate limited: retry in {}s", retry.as_secs() + 1));
    }
    let (server_id, perms) = authorize_text_channel(db, user_id, channel_id, Permissions::SEND_MESSAGES)?;
    if !attachments.is_empty() && !perms.contains(Permissions::ATTACH_FILES) {
        return Err(permissions::missing(Permissions::ATTACH_FILES));
    }
    // A timed-out member is blocked from posting (their membership still
    // exists — the timeout is a deadline, not a removal). `authorize_text_channel`
    // already confirmed membership; here we check the `timeout_until` deadline
    // against the current epoch. `now_millis()/1000` keeps this on the same UTC
    // epoch-seconds convention as `timeout_until`.
    if db.is_timed_out(server_id, user_id, now_millis() / 1000) {
        return Err("you are timed out and cannot post messages".into());
    }
    let created_at = now_millis();
    // Move every attachment's bytes into the media store only now, after the
    // sender is authorized, and before touching the DB: the rows (and the
    // broadcast frame) carry the `/media/…` path, never image bytes.
    let media_rows = attachments
        .into_iter()
        .map(|a| media_store_attachment(&a.data, &a.mime).map(|url| (a.filename, a.mime, url)))
        .collect::<Result<Vec<(String, String, String)>, String>>()?;
    let (message_id, attachment_ids) = db
        .insert_message_with_attachments(
            channel_id,
            user_id,
            content,
            created_at,
            reply_to_message_id,
            &media_rows,
        )
        .map_err(|e| e.to_string())?;
    let mentioned_user_ids = resolve_mentions(db, channel_id, user_id, content);
    if let Err(e) = db.insert_message_mentions(message_id, &mentioned_user_ids) {
        return Err(format!("failed to store mentions: {e}"));
    }
    // Resolve the author's username for the wire frame so the live client can
    // label the message with a real name, not an id (mirrors DmMessage's
    // `author_username`).
    let author_username = db
        .user_by_id(user_id)
        .map(|(_, n)| n)
        .unwrap_or_default();
    // Pair the persisted attachment ids (from the same transaction) with the
    // sent rows so the frame carries the full persisted attachment record.
    let wire_attachments = attachment_ids
        .into_iter()
        .zip(media_rows)
        .map(|(id, (filename, mime, url))| {
            crate::server_settings::MessageAttachment::stored(id, filename, mime, url)
        })
        .collect();
    hub().publish(
        &channel_audience(db, channel_id),
        &WsServerMsg::Message {
            message_id,
            channel_id,
            server_id,
            author_id: user_id,
            author_username,
            author_avatar: db.user_avatar(user_id),
            content: content.to_string(),
            created_at,
            edited_at: None,
            new: true,
            mentioned_user_ids,
            reply_to_message_id,
            // A fresh post has no reactions yet; later adds ride the
            // `ReactionAdd` / `ReactionRemove` broadcast frames.
            reactions: Vec::new(),
            attachments: wire_attachments,
            // Cards follow in a `MessageEmbeds` frame once fetched.
            embeds: Vec::new(),
        },
    );
    crate::link_previews::schedule(db, channel_id, message_id, content, perms.contains(Permissions::EMBED_LINKS));
    Ok(())
}

/// Validate, persist and fan out an edit of one of the caller's own messages.
///
/// Authorization: the message must exist AND its `author_id` must be `user_id`
/// — anyone else (including another member of the same server, or an admin) is
/// rejected with "you can only edit your own messages". The new content is
/// size- and emptiness-checked like a post, then the row is updated in place
/// (content + a fresh `edited_at`) and the `@mention` rows are recomputed so
/// the stored mention list matches the new text.
///
/// The audience is everyone who can see the channel (including the editor's
/// own echo), the same as a post.
pub fn edit_channel_message(
    db: &Db,
    user_id: i64,
    message_id: i64,
    content: &str,
) -> Result<(), String> {
    if content.trim().is_empty() {
        return Err("empty message".into());
    }
    if content.chars().count() > MAX_MESSAGE_CHARS {
        return Err(format!("message too long (max {MAX_MESSAGE_CHARS} characters)"));
    }
    // The message must exist and belong to the caller; `channel_id` is recovered
    // from the stored row for the broadcast audience.
    let Some((author, channel_id)) = db.message_row(message_id) else {
        return Err("message not found".into());
    };
    if author != user_id {
        return Err("you can only edit your own messages".into());
    }
    let (_, perms) = authorize_text_channel(db, user_id, channel_id, Permissions::VIEW_CHANNEL)?;
    let edited_at = now_millis();
    db.update_message(message_id, content, edited_at)
        .map_err(|e| format!("failed to update message: {e}"))?;
    // Recompute mentions against the NEW content and replace the stored rows so
    // the persisted mention list matches the text.
    let mentioned_user_ids = resolve_mentions(db, channel_id, user_id, content);
    if let Err(e) = db.replace_message_mentions(message_id, &mentioned_user_ids) {
        return Err(format!("failed to store mentions: {e}"));
    }
    hub().publish(
        &channel_audience(db, channel_id),
        &WsServerMsg::MessageEdited {
            message_id,
            channel_id,
            content: content.to_string(),
            edited_at,
            mentioned_user_ids,
        },
    );
    // Links added or removed change the cards.
    crate::link_previews::schedule(db, channel_id, message_id, content, perms.contains(Permissions::EMBED_LINKS));
    Ok(())
}

/// Delete one of the caller's own messages and fan out the removal.
///
/// Authorization mirrors [`edit_channel_message`]: the message must exist and
/// its `author_id` must be `user_id`. The row is removed (hard delete — the
/// schema has no `deleted_at` on messages, same as `delete_channel`), and
/// everyone who can see the channel is told to drop it.
pub fn delete_channel_message(db: &Db, user_id: i64, message_id: i64) -> Result<(), String> {
    let Some((author, channel_id)) = db.message_row(message_id) else {
        return Err("message not found".into());
    };
    if author != user_id {
        return Err("you can only delete your own messages".into());
    }
    authorize_text_channel(db, user_id, channel_id, Permissions::VIEW_CHANNEL)?;
    db.delete_message(message_id)
        .map_err(|e| format!("failed to delete message: {e}"))?;
    hub().publish(
        &channel_audience(db, channel_id),
        &WsServerMsg::MessageDeleted { message_id, channel_id },
    );
    Ok(())
}

/// Persist that `user_id` reacted with `emoji` to `message_id`, then fan out a
/// `ReactionAdd` frame carrying the new per-emoji count to everyone who can
/// see the channel (the reactor's own echo included). The count is read back after the
/// write, never incremented, so a duplicate add converges to the truth.
/// Returns the resolved `(channel_id, count)` for the endpoint's result.
///
/// Authorization: the message must exist in a text channel where the caller
/// holds Add Reactions.
pub fn add_channel_reaction(
    db: &Db,
    user_id: i64,
    message_id: i64,
    emoji: &str,
) -> Result<(i64, u32), String> {
    if !reaction_ok(emoji) {
        return Err("invalid reaction emoji".into());
    }
    let (_author_id, channel_id) = match db.message_row(message_id) {
        Some(pair) => pair,
        None => return Err("message not found".into()),
    };
    authorize_text_channel(db, user_id, channel_id, Permissions::ADD_REACTIONS)?;
    db.add_reaction(message_id, user_id, emoji, now_millis())
        .map_err(|e| format!("failed to store reaction: {e}"))?;
    let count = reaction_count_for(db, message_id, emoji);
    hub().publish(
        &channel_audience(db, channel_id),
        &WsServerMsg::ReactionAdd {
            message_id,
            channel_id,
            user_id,
            emoji: emoji.to_string(),
            count,
        },
    );
    Ok((channel_id, count))
}

/// Drop `user_id`'s `emoji` reaction from `message_id` and fan out a
/// `ReactionRemove` frame with the post-state count. Taking back one's own
/// reaction only needs to see the channel; idempotent (removing an absent reaction is a
/// no-op that still broadcasts the unchanged count, so clients converge).
pub fn remove_channel_reaction(
    db: &Db,
    user_id: i64,
    message_id: i64,
    emoji: &str,
) -> Result<(i64, u32), String> {
    if !reaction_ok(emoji) {
        return Err("invalid reaction emoji".into());
    }
    let (_author_id, channel_id) = match db.message_row(message_id) {
        Some(pair) => pair,
        None => return Err("message not found".into()),
    };
    authorize_text_channel(db, user_id, channel_id, Permissions::VIEW_CHANNEL)?;
    let removed = db
        .remove_reaction(message_id, user_id, emoji)
        .map_err(|e| format!("failed to remove reaction: {e}"))?;
    // Whether a row was actually dropped does not gate the broadcast: a no-op
    // removal still reports the post-state count so clients converge.
    let _ = removed;
    let count = reaction_count_for(db, message_id, emoji);
    hub().publish(
        &channel_audience(db, channel_id),
        &WsServerMsg::ReactionRemove {
            message_id,
            channel_id,
            user_id,
            emoji: emoji.to_string(),
            count,
        },
    );
    Ok((channel_id, count))
}

/// How many users reacted with `emoji` on `message_id` — the authoritative
/// post-operation total for one emoji (the grouped store in
/// [`crate::db::Db::reactions_for_message`] returns per-emoji counts).
fn reaction_count_for(db: &Db, message_id: i64, emoji: &str) -> u32 {
    db.reactions_for_message(message_id, 0)
        .iter()
        .find(|(e, _, _)| e == emoji)
        .map(|(_, c, _)| *c)
        .unwrap_or(0)
}

/// The recent history of a channel, if `user_id` may read it. With
/// `before_id = None` the latest `limit` messages are returned (initial load);
/// with `Some(id)` the next `limit` messages strictly OLDER than `id` (scrollback).
pub fn history_for(
    db: &Db,
    user_id: i64,
    channel_id: i64,
    before_id: Option<i64>,
    limit: i64,
) -> Result<Vec<Message>, String> {
    authorize_text_channel(db, user_id, channel_id, Permissions::VIEW_CHANNEL)?;
    Ok(db.older_messages(channel_id, before_id, limit))
}

#[cfg(test)]
mod presence_tests {
    use super::shown_presence;

    #[test]
    fn picked_status_overrides_activity() {
        assert_eq!(shown_presence("online", true), "online");
        assert_eq!(shown_presence("online", false), "away");
        assert_eq!(shown_presence("idle", true), "away");
        assert_eq!(shown_presence("dnd", false), "dnd");
        assert_eq!(shown_presence("invisible", true), "offline");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Users 1..=4. Server 1: owner 1 + member 2, text channel 1, voice
    /// channel 2. Server 2: owner 3, text channel 3. User 4 is in no server.
    fn setup() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let migrations = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let db = Db::migrate(&dir.path().join("t.db"), &migrations).unwrap();
        db.lock()
            .unwrap()
            .execute_batch(
                "INSERT INTO users (id, username, email, created_at) VALUES
                    (1,'alice','a@x',1),(2,'bob','b@x',1),(3,'carol','c@x',1),(4,'dave','d@x',1);
                 INSERT INTO servers (id, name, owner_id, icon_url, created_at) VALUES
                    (1,'S1',1,NULL,1),(2,'S2',3,NULL,1);
                 INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES
                    (1,1,'owner',1),(1,2,'member',1),(2,3,'owner',1);
                 INSERT INTO channels (id, server_id, name, kind, position, parent_channel_id, created_at) VALUES
                    (1,1,'general','text',0,NULL,1),
                    (2,1,'lounge','voice',1,NULL,1),
                    (3,2,'other','text',0,NULL,1);",
            )
            .unwrap();
        (dir, db)
    }

    // ── pure hub behaviour ────────────────────────────────────────────────

    #[tokio::test]
    async fn hub_delivers_to_listed_users_only_and_reports_last_close() {
        let h = Hub::default();
        let (a1, mut ra1) = h.register(1);
        let (_a2, mut ra2) = h.register(1);
        let (_b, mut rb) = h.register(2);
        let (_c, mut rc) = h.register(3);
        let msg = WsServerMsg::Error { reason: "x".into() };
        // Users 1 and 2 (1 listed twice): 2 sockets for user 1 + 1 for user 2.
        assert_eq!(h.publish(&[1, 2, 1], &msg), 3);
        assert!(ra1.try_recv().is_ok() && ra2.try_recv().is_ok() && rb.try_recv().is_ok());
        assert!(rc.try_recv().is_err(), "user 3 was not in the audience");
        assert_eq!(h.connection_count(1), 2);
        assert!(!h.unregister(1, a1), "user 1 still has a second socket");
        assert!(h.unregister(2, _b), "user 2's only socket closed");
        assert_eq!(h.publish(&[2], &msg), 0);
    }

    #[tokio::test]
    async fn hub_drops_events_for_a_full_queue_instead_of_blocking() {
        let h = Hub::default();
        let (_id, mut rx) = h.register(1);
        let msg = WsServerMsg::Error { reason: "x".into() };
        for _ in 0..QUEUE_DEPTH {
            assert_eq!(h.publish(&[1], &msg), 1);
        }
        assert_eq!(h.publish(&[1], &msg), 0, "queue full: event dropped, not blocked");
        assert!(rx.try_recv().is_ok());
    }

    // ── authorization helpers ─────────────────────────────────────────────

    #[test]
    fn channel_authorization_rules() {
        let (_d, db) = setup();
        let send = Permissions::SEND_MESSAGES;
        assert_eq!(authorize_text_channel(&db, 1, 1, send).map(|r| r.0), Ok(1));
        assert_eq!(authorize_text_channel(&db, 2, 1, send).map(|r| r.0), Ok(1));
        assert_eq!(authorize_text_channel(&db, 4, 1, send), Err("not a member of this server".into()));
        assert_eq!(authorize_text_channel(&db, 1, 3, send), Err("not a member of this server".into()));
        assert_eq!(authorize_text_channel(&db, 1, 2, send), Err("not a text channel".into()));
        assert_eq!(authorize_text_channel(&db, 1, 99, send), Err("channel not found".into()));
        assert!(history_for(&db, 4, 1, None, 50).is_err());
        assert!(history_for(&db, 1, 1, None, 50).is_ok());
    }

    /// Channel 4 in server 1 is private: @everyone can't see it, bob can't.
    fn with_private_channel(db: &Db) {
        db.lock()
            .unwrap()
            .execute_batch(
                "INSERT INTO channels (id, server_id, name, kind, position, created_at) VALUES (4,1,'secret','text',2,1);
                 INSERT INTO channel_overwrites (channel_id, target_kind, target_id, allow, deny)
                    VALUES (4,'everyone',0,0,1);",
            )
            .unwrap();
    }

    #[test]
    fn a_private_channel_is_invisible_to_members_without_access() {
        let (_d, db) = setup();
        with_private_channel(&db);
        assert_eq!(authorize_text_channel(&db, 2, 4, Permissions::VIEW_CHANNEL), Err("channel not found".into()));
        assert!(history_for(&db, 2, 4, None, 50).is_err());
        assert_eq!(post_channel_message(&db, 2, 4, "let me in", None, Vec::new()).unwrap_err(), "channel not found");
        assert!(post_channel_message(&db, 1, 4, "owner only", None, Vec::new()).is_ok());
        assert_eq!(channel_audience(&db, 4), vec![1]);
        assert_eq!(channel_audience(&db, 1), vec![1, 2]);
        // bob isn't notified by name or by @everyone in a channel he can't see.
        assert_eq!(resolve_mentions(&db, 4, 1, "@bob @everyone"), Vec::<i64>::new());
        assert_eq!(resolve_mentions(&db, 1, 1, "@bob"), vec![2]);
    }

    #[test]
    fn a_read_only_channel_refuses_posts_but_shows_history() {
        let (_d, db) = setup();
        db.lock()
            .unwrap()
            .execute_batch("INSERT INTO channel_overwrites (channel_id, target_kind, target_id, allow, deny) VALUES (1,'everyone',0,0,2);")
            .unwrap();
        assert!(history_for(&db, 2, 1, None, 50).is_ok());
        assert_eq!(
            post_channel_message(&db, 2, 1, "hi", None, Vec::new()).unwrap_err(),
            "you need the Send Messages permission to do that"
        );
        assert!(post_channel_message(&db, 1, 1, "owner can", None, Vec::new()).is_ok());
    }

    #[test]
    fn history_pagination_walks_older_pages_by_id_cursor() {
        let (_d, db) = setup();
        // Five messages in channel 1 (ids are monotonic AUTOINCREMENT).
        let ids: Vec<i64> = (0..5)
            .map(|i| db.insert_message(1, 1, &format!("m{i}"), 100 + i, None).unwrap())
            .collect();
        assert!(ids.windows(2).all(|w| w[0] < w[1]), "ids must be ascending");

        // Initial load (`before_id = None`): latest `limit`, oldest-first.
        let first = db.older_messages(1, None, 3);
        assert_eq!(first.iter().map(|m| m.id).collect::<Vec<_>>(), vec![ids[2], ids[3], ids[4]]);

        // Scrollback: strictly older than the oldest held id, still oldest-first.
        let second = db.older_messages(1, Some(ids[2]), 3);
        assert_eq!(second.iter().map(|m| m.id).collect::<Vec<_>>(), vec![ids[0], ids[1]]);

        // Exhausted: nothing older than the very first message.
        assert!(db.older_messages(1, Some(ids[0]), 3).is_empty());

        // The authorization guard rides along on the cursor path too.
        assert!(history_for(&db, 1, 1, Some(ids[4]), 3).is_ok());
        assert!(history_for(&db, 4, 1, Some(ids[4]), 3).is_err());
    }

    #[test]
    fn posting_validates_size_emptiness_and_membership() {
        let (_d, db) = setup();
        assert_eq!(post_channel_message(&db, 1, 1, "   ", None, Vec::new()).unwrap_err(), "empty message");
        let long = "x".repeat(MAX_MESSAGE_CHARS + 1);
        assert!(post_channel_message(&db, 1, 1, &long, None, Vec::new()).unwrap_err().contains("too long"));
        assert!(post_channel_message(&db, 4, 1, "hi", None, Vec::new()).unwrap_err().contains("not a member"));
        assert!(post_channel_message(&db, 1, 2, "hi", None, Vec::new()).unwrap_err().contains("not a text"));
        assert!(post_channel_message(&db, 1, 1, "ok", None, Vec::new()).is_ok());
        assert_eq!(db.recent_messages(1, 10).len(), 1, "only the valid post persisted");
    }

    // ── image attachments ─────────────────────────────────────────────────

    #[test]
    fn attachment_only_post_with_one_image_lands_in_history() {
        let (_d, db) = setup();
        let png = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAIAAABLbSncAAAACXBIWXMAAAABAAAAAQBPJcTWAAAAEUlEQVR4nGOQjVqOFTEMLQkAzd1HgU4BMj8AAAAASUVORK5CYII=";
        let att = crate::server_settings::AttachmentInput {
            filename: "dot.png".into(),
            mime: "image/png".into(),
            data: png.into(),
        };
        // An attachment-only message (empty body) is accepted — a message
        // carrying body OR attachments, not neither.
        assert!(post_channel_message(&db, 1, 1, "", None, vec![att]).is_ok());
        let msgs = db.recent_messages(1, 10);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].attachments.len(), 1, "attachment comes back in history");
        let a = &msgs[0].attachments[0];
        assert_eq!(a.filename, "dot.png");
        assert_eq!(a.mime, "image/png");
        assert!(a.url.starts_with("/media/"), "attachment comes back pathified");
        assert!(a.id > 0, "the persisted attachment id rides the history row");
    }

    #[test]
    fn attachment_count_size_and_mime_limits_are_enforced() {
        let (_d, db) = setup();
        let good = crate::server_settings::AttachmentInput {
            filename: "dot.png".into(),
            mime: "image/png".into(),
            data: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAIAAABLbSncAAAACXBIWXMAAAABAAAAAQBPJcTWAAAAEUlEQVR4nGOQjVqOFTEMLQkAzd1HgU4BMj8AAAAASUVORK5CYII=".into(),
        };
        // 5 attachments on one message: rejected (max 4), nothing persisted.
        let five = vec![good.clone(), good.clone(), good.clone(), good.clone(), good.clone()];
        assert!(
            post_channel_message(&db, 1, 1, "x", None, five).unwrap_err()
                .contains("too many attachments")
        );
        assert!(db.recent_messages(1, 10).is_empty());
        // Oversize: the base64 body decodes past the 2 MB cap (upper bound
        // `(chars+3)/4*3` crosses MAX_ATTACHMENT_BYTES).
        let huge_data =
            String::new()
            + "data:image/png;base64,"
            + &"A".repeat(MAX_ATTACHMENT_BYTES / 3 * 4 + 4);
        assert!(
            post_channel_message(
                &db, 1, 1, "x", None,
                vec![crate::server_settings::AttachmentInput {
                    filename: "big.png".into(),
                    mime: "image/png".into(),
                    data: huge_data,
                }],
            )
            .unwrap_err()
            .contains("too large")
        );
        // Non-image declared mime rejected.
        let pdf = crate::server_settings::AttachmentInput {
            filename: "doc.pdf".into(),
            mime: "application/pdf".into(),
            data: "data:application/pdf;base64,JU".into(),
        };
        assert!(
            post_channel_message(&db, 1, 1, "x", None, vec![pdf]).unwrap_err()
                .contains("mime not allowed")
        );
        // Declared mime agrees but the data URL prefix disagrees: rejected.
        let liar = crate::server_settings::AttachmentInput {
            filename: "x.png".into(),
            mime: "image/png".into(),
            data: "data:image/gif;base64,R0lG".into(),
        };
        assert!(
            post_channel_message(&db, 1, 1, "x", None, vec![liar]).unwrap_err()
                .contains("does not match")
        );
        assert!(db.recent_messages(1, 10).is_empty(), "no rejected post persisted");
    }

    // ── @mention tokenization + resolution ────────────────────────────────

    #[test]
    fn mention_tokens_grammar() {
        use crate::db::mention_tokens;
        assert_eq!(mention_tokens("no mentions here"), Vec::<String>::new());
        assert_eq!(mention_tokens("@alice"), vec!["alice"]);
        assert_eq!(mention_tokens("hi @alice and @bob_2"), vec!["alice", "bob_2"]);
        // `@` immediately after a token starts a new token.
        assert_eq!(mention_tokens("@a@b"), vec!["a", "b"]);
        // A single contiguous run: `@alice_bob` is ONE token, not two.
        assert_eq!(mention_tokens("@alice_bob"), vec!["alice_bob"]);
        // Punctuation / whitespace delimit tokens.
        assert_eq!(mention_tokens("@alice."), vec!["alice"]);
        assert_eq!(mention_tokens("(@alice)!"), vec!["alice"]);
        // A bare `@` with no following word char yields no token.
        assert_eq!(mention_tokens("email@ x"), Vec::<String>::new());
    }

    #[test]
    fn resolve_mentions_matches_members_case_insensitively() {
        let (_d, db) = setup();
        // Server 1 members: alice (1, owner), bob (2, member).
        assert_eq!(resolve_mentions(&db, 1, 3, "hey @alice"), vec![1]);
        assert_eq!(resolve_mentions(&db, 1, 3, "@BOB ping"), vec![2]);
        assert_eq!(resolve_mentions(&db, 1, 3, "@Alice @bob"), vec![1, 2]);
        // Dedup: the same member named twice is listed once.
        assert_eq!(resolve_mentions(&db, 1, 3, "@alice @ALICE"), vec![1]);
        // A token with no matching member is dropped (plain text).
        assert_eq!(resolve_mentions(&db, 1, 3, "email @gmail.com"), Vec::<i64>::new());
        // Members of other servers do not resolve.
        assert_eq!(resolve_mentions(&db, 1, 3, "@carol"), Vec::<i64>::new());
    }

    #[test]
    fn posting_stores_and_replays_resolved_mentions() {
        // The hub below is the process-wide one; serialize with the other
        // hub-using e2e tests so their `hub().reset()` can't wipe our socket
        // mid-test (the e2e_lock, not the test ordering, prevents that).
        let _guard = crate::e2e_lock::guard();
        let (_d, db) = setup();
        // Reset the shared hub so the publish below is observable in a clean state.
        let hub = crate::realtime::hub();
        hub.reset();
        let (_id, mut rx) = hub.register(1); // alice's (sender) socket

        assert!(post_channel_message(&db, 1, 1, "@bob @alice and @nobody", None, Vec::new()).is_ok());
        // The echo frame carries the resolved mentions (bob=2, alice=1; nobody
        // dropped). Tests elsewhere publish membership frames on the same hub
        // without the lock, so skip anything that isn't a message.
        let frame = std::iter::from_fn(|| rx.try_recv().ok())
            .find(|f| matches!(f, WsServerMsg::Message { .. }))
            .expect("sender receives the echo");
        match frame {
            WsServerMsg::Message { mentioned_user_ids, .. } => {
                assert_eq!(mentioned_user_ids, vec![1, 2]);
            }
            other => panic!("expected a Message frame, got {other:?}"),
        }
        // The same list is persisted and comes back in history.
        let msgs = db.recent_messages(1, 10);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].mentioned_user_ids, vec![1, 2]);
    }

    #[test]
    fn editing_own_message_updates_row_mentions_and_bounces() {
        let _guard = crate::e2e_lock::guard();
        let (_d, db) = setup();
        let hub = crate::realtime::hub();
        hub.reset();
        let (_id, mut rx) = hub.register(1); // alice — the author
        let (_id2, mut rx2) = hub.register(2); // bob — co-member, not the author

        // Alice posts a mention-free message to channel 1.
        let mid = db.insert_message(1, 1, "original", 100, None).unwrap();

        // A co-member who is not the author is refused; an unknown id is too;
        // empty and oversized edits are refused like a post.
        assert_eq!(
            edit_channel_message(&db, 2, mid, "sneaky").unwrap_err(),
            "you can only edit your own messages"
        );
        assert_eq!(edit_channel_message(&db, 1, 9999, "x").unwrap_err(), "message not found");
        assert_eq!(edit_channel_message(&db, 1, mid, "  ").unwrap_err(), "empty message");
        let long = "x".repeat(MAX_MESSAGE_CHARS + 1);
        assert!(edit_channel_message(&db, 1, mid, &long).unwrap_err().contains("too long"));
        // The refused edits left the row untouched.
        assert_eq!(db.recent_messages(1, 10).iter().find(|m| m.id == mid).unwrap().content, "original");

        // Alice edits to a mention: content + edited_at updated, mentions recomputed.
        assert!(edit_channel_message(&db, 1, mid, "edited @bob").is_ok());
        let m = db
            .recent_messages(1, 10)
            .into_iter()
            .find(|m| m.id == mid)
            .expect("edited row still present");
        assert_eq!(m.content, "edited @bob");
        assert!(m.edited_at.is_some(), "edited_at stamped on the first edit");
        assert_eq!(m.mentioned_user_ids, vec![2], "mentions recomputed against the new text");

        // The edit bounces to EVERY socket of the channel's server (alice+bob).
        match rx.try_recv().expect("author receives the edit echo") {
            WsServerMsg::MessageEdited {
                message_id,
                channel_id,
                content,
                edited_at,
                mentioned_user_ids,
            } => {
                assert_eq!(mentioned_user_ids, vec![2]);
                assert_eq!((message_id, channel_id), (mid, 1));
                assert_eq!(content, "edited @bob");
                assert_eq!(edited_at, m.edited_at.unwrap());
            }
            other => panic!("expected MessageEdited, got {other:?}"),
        }
        assert!(
            matches!(
                rx2.try_recv().expect("co-member receives the edit echo"),
                WsServerMsg::MessageEdited { message_id, .. } if message_id == mid
            ),
            "the edit fans out to the whole server, not just the sender"
        );
    }

    #[test]
    fn deleting_own_message_removes_row_and_bounces() {
        let _guard = crate::e2e_lock::guard();
        let (_d, db) = setup();
        let hub = crate::realtime::hub();
        hub.reset();
        let (_id, mut rx) = hub.register(1); // alice — the author
        let (_id2, mut rx2) = hub.register(2); // bob — co-member

        let mid = db.insert_message(1, 1, "doomed", 100, None).unwrap();
        db.insert_message(1, 1, "keeper", 101, None).unwrap(); // a sibling that survives

        // A co-member who is not the author is refused; an unknown id is too.
        assert_eq!(
            delete_channel_message(&db, 2, mid).unwrap_err(),
            "you can only delete your own messages"
        );
        assert_eq!(delete_channel_message(&db, 1, 9999).unwrap_err(), "message not found");
        // The refused deletes left the row intact.
        assert_eq!(db.recent_messages(1, 10).len(), 2);

        assert!(delete_channel_message(&db, 1, mid).is_ok());
        let msgs = db.recent_messages(1, 10);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "keeper", "only the non-deleted row remains");
        assert!(msgs.iter().all(|m| m.id != mid), "the deleted row is absent");

        // The removal bounces to every socket of the channel's server.
        match rx.try_recv().expect("author receives the delete echo") {
            WsServerMsg::MessageDeleted {
                message_id,
                channel_id,
            } => assert_eq!((message_id, channel_id), (mid, 1)),
            other => panic!("expected MessageDeleted, got {other:?}"),
        }
        assert!(
            matches!(
                rx2.try_recv().expect("co-member receives the delete echo"),
                WsServerMsg::MessageDeleted { message_id, .. } if message_id == mid
            ),
            "the deletion fans out to the whole server, not just the sender"
        );
    }

    #[test]
    fn reaction_persists_and_bounces_to_the_server_audience() {
        // Regression for the non-persistent reaction gap: a member's react must
        // write a `message_reactions` row AND broadcast the post-state count to
        // every socket of the channel's server (the reactor's echo included).
        let _guard = crate::e2e_lock::guard();
        let (_d, db) = setup();
        let hub = crate::realtime::hub();
        hub.reset();
        let (_id, mut rx) = hub.register(1); // alice — the reactor
        let (_id2, mut rx2) = hub.register(2); // bob — co-member (viewer)

        let mid = db.insert_message(1, 1, "reactable", 100, None).unwrap();

        // A non-member of the channel's server cannot react.
        assert_eq!(
            add_channel_reaction(&db, 4, mid, "🎉").unwrap_err(),
            "not a member of this server"
        );
        // No row was written for the refused attempt.
        assert!(db.reactions_for_message(mid, 1).is_empty(), "refused add wrote nothing");

        // Alice reacts with 👍: resolved channel + post-state count 1,
        // read back from the DB (never a per-client increment).
        let (channel_id, count) = add_channel_reaction(&db, 1, mid, "👍").unwrap();
        assert_eq!((channel_id, count), (1, 1u32), "resolved channel + post-state count");

        // PERSISTENCE: the grouped store now shows one 👍 reacted by alice (1).
        // This is the row a cold load reads back via `reactions_for_message`,
        // so a reload restores the pill without re-clicking.
        let pills = db.reactions_for_message(mid, 1);
        assert_eq!(pills, vec![("👍".to_string(), 1u32, true)], "the reaction row persisted");

        // Bob reacting with the SAME emoji bumps the authoritative total to 2.
        let (_cid, count2) = add_channel_reaction(&db, 2, mid, "👍").unwrap();
        assert_eq!(count2, 2u32, "the count is the DB total after the write");

        // DROPPING alice's 👍 leaves bob's, so the total returns to 1.
        let (_cid, count3) = remove_channel_reaction(&db, 1, mid, "👍").unwrap();
        assert_eq!(count3, 1u32, "post-state total after removing alice's reaction");
        // A second remove of alice's now-absent reaction is a no-op at count 1.
        let (_cid, count4) = remove_channel_reaction(&db, 1, mid, "👍").unwrap();
        assert_eq!(count4, 1u32, "idempotent: removing an absent reaction keeps 1");
        // PERSISTENCE: alice's 👍 row is gone, bob's remains.
        let pills2 = db.reactions_for_message(mid, 1);
        assert_eq!(pills2, vec![("👍".to_string(), 1u32, false)], "bob still reacts, alice does not");

        // The FIRST reaction (alice's 👍, count 1) bounced to BOTH sockets:
        // alice's own echo plus bob's copy, each with the post-state count 1.
        match rx.try_recv().expect("reactor echo") {
            WsServerMsg::ReactionAdd { message_id, channel_id, user_id, emoji, count, .. } => {
                assert_eq!((message_id, channel_id, user_id, count), (mid, 1, 1, 1u32));
                assert_eq!(emoji, "👍", "the echo names the emoji");
            }
            other => panic!("expected ReactionAdd, got {other:?}"),
        }
        assert!(
            matches!(
                rx2.try_recv().expect("co-member sees the reaction live"),
                WsServerMsg::ReactionAdd { message_id, emoji, count, .. }
                    if message_id == mid && emoji == "👍" && count == 1u32
            ),
            "the reaction fans out to the whole server, not just the reactor"
        );
    }

    use crate::server_settings::WsClientMsg;
    use futures_util::{SinkExt, StreamExt};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_tungstenite::tungstenite::Message as WsMsg;

    type Ws = tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >;

    async fn boot() -> std::net::SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, server::router()).await;
        });
        addr
    }

    fn hex(s: &str) -> String {
        s.bytes().map(|b| format!("{b:02x}")).collect()
    }

    /// Connect a gateway socket authenticated with `token` (the wire arg).
    async fn connect(addr: std::net::SocketAddr, token: &str) -> Result<Ws, String> {
        let args = hex(&serde_json::to_string(&[token]).unwrap());
        let url = format!("ws://{addr}/_srv/_ws/message_gateway?args={args}");
        tokio_tungstenite::connect_async(url)
            .await
            .map(|(ws, _)| ws)
            .map_err(|e| e.to_string())
    }

    async fn send(ws: &mut Ws, msg: &WsClientMsg) {
        ws.send(WsMsg::Text(serde_json::to_string(msg).unwrap()))
            .await
            .unwrap();
    }

    /// Read frames until `pred` matches one (or 3s pass) and return it.
    async fn next_matching(
        ws: &mut Ws,
        pred: impl Fn(&WsServerMsg) -> bool,
    ) -> Option<WsServerMsg> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            match tokio::time::timeout(left, ws.next()).await {
                Ok(Some(Ok(WsMsg::Text(t)))) => {
                    let m: WsServerMsg = serde_json::from_str(&t).unwrap();
                    if pred(&m) {
                        return Some(m);
                    }
                }
                Ok(Some(Ok(_))) => continue,
                _ => return None,
            }
        }
    }

    /// True if NO frame satisfying `pred` arrives within 400ms.
    async fn hears_nothing(ws: &mut Ws, pred: impl Fn(&WsServerMsg) -> bool) -> bool {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(400);
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            match tokio::time::timeout(left, ws.next()).await {
                Ok(Some(Ok(WsMsg::Text(t)))) => {
                    if pred(&serde_json::from_str(&t).unwrap()) {
                        return false;
                    }
                }
                Ok(Some(Ok(_))) => continue,
                _ => return true,
            }
        }
    }

    /// POST a JSON args-tuple to `/_srv/<path>`, optionally with an
    /// `Authorization: Bearer` token and/or a legacy `x-user-id`.
    async fn http(
        addr: std::net::SocketAddr,
        path: &str,
        bearer: Option<&str>,
        legacy: Option<&str>,
        body: &str,
    ) -> serde_json::Value {
        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut r, mut w) = stream.into_split();
        let mut hdrs = String::new();
        if let Some(t) = bearer {
            hdrs.push_str(&format!("authorization: Bearer {t}\r\n"));
        }
        if let Some(u) = legacy {
            hdrs.push_str(&format!("x-user-id: {u}\r\n"));
        }
        let req = format!(
            "POST /_srv/{path} HTTP/1.1\r\nHost: {addr}\r\n{hdrs}\
             content-type: application/json\r\ncontent-length: {}\r\n\
             connection: close\r\n\r\n{body}",
            body.len()
        );
        w.write_all(req.as_bytes()).await.unwrap();
        let mut buf = Vec::new();
        r.read_to_end(&mut buf).await.unwrap();
        let s = String::from_utf8(buf).unwrap();
        let (_, b) = s.split_once("\r\n\r\n").unwrap_or(("", ""));
        serde_json::from_str(b).unwrap_or(serde_json::Value::Null)
    }

    fn is_msg(content: &'static str) -> impl Fn(&WsServerMsg) -> bool {
        move |m| matches!(m, WsServerMsg::Message { content: c, new: true, .. } if c == content)
    }

    // ── end to end through the real router ───────────────────────────────

    #[tokio::test]
    async fn gateway_authenticates_authorizes_and_fans_out() {
        let (_d, db) = setup();
        let _guard = crate::e2e_lock::guard();
        crate::realtime::hub().reset();
        server::install_state(db.clone());
        crate::session::set_legacy_header_for_tests(Some(false));
        let addr = boot().await;

        let (t1, t2, t3, t4) = (
            crate::session::issue_token(1),
            crate::session::issue_token(2),
            crate::session::issue_token(3),
            crate::session::issue_token(4),
        );

        // 1. Bad / forged / missing tokens never get a working socket.
        for bad in ["", "garbage", "v1.1.9999999999.deadbeef"] {
            match connect(addr, bad).await {
                Err(_) => {}
                Ok(mut ws) => {
                    let r = tokio::time::timeout(Duration::from_secs(2), ws.next()).await;
                    assert!(
                        !matches!(r, Ok(Some(Ok(WsMsg::Text(_))))),
                        "token {bad:?} must not get frames"
                    );
                }
            }
        }
        assert_eq!(hub().connection_count(1), 0);

        // 2. Alice + Bob (server 1), Carol (server 2) and Dave (no server).
        let mut alice = connect(addr, &t1).await.expect("alice connects");
        let mut bob = connect(addr, &t2).await.expect("bob connects");
        let mut carol = connect(addr, &t3).await.expect("carol connects");
        let mut dave = connect(addr, &t4).await.expect("dave connects");

        // Bob hears Alice's presence... she was online before he connected, so
        // instead: Bob's own arrival is announced to Alice (a shared server).
        assert!(
            next_matching(&mut alice, |m| matches!(
                m, WsServerMsg::Presence { user_id: 2, status } if status == "online"
            ))
            .await
            .is_some(),
            "alice is told bob came online (shared server)"
        );

        // 3. Fan-out: Alice posts; Alice (echo) and Bob receive it.
        send(&mut alice, &WsClientMsg::Send { channel_id: 1, content: "hello bob".into(), reply_to_message_id: None, attachments: Vec::new() }).await;
        assert!(next_matching(&mut alice, is_msg("hello bob")).await.is_some(), "echo to sender");
        assert!(next_matching(&mut bob, is_msg("hello bob")).await.is_some(), "bob receives");
        // ...but users outside server 1 hear nothing.
        assert!(hears_nothing(&mut carol, is_msg("hello bob")).await, "carol is in another server");
        assert!(hears_nothing(&mut dave, is_msg("hello bob")).await, "dave is in no server");

        // 4. Authorization: a non-member cannot post or read; voice is not text.
        send(&mut dave, &WsClientMsg::Send { channel_id: 1, content: "sneaky".into(), reply_to_message_id: None, attachments: Vec::new() }).await;
        let err = next_matching(&mut dave, |m| matches!(m, WsServerMsg::Error { .. })).await;
        assert!(matches!(err, Some(WsServerMsg::Error { ref reason }) if reason.contains("not a member")));
        send(&mut dave, &WsClientMsg::History { channel_id: 1, before_id: None }).await;
        let err = next_matching(&mut dave, |m| matches!(m, WsServerMsg::Error { .. })).await;
        assert!(matches!(err, Some(WsServerMsg::Error { ref reason }) if reason.contains("not a member")));
        send(&mut alice, &WsClientMsg::Send { channel_id: 2, content: "into voice".into(), reply_to_message_id: None, attachments: Vec::new() }).await;
        let err = next_matching(&mut alice, |m| matches!(m, WsServerMsg::Error { .. })).await;
        assert!(matches!(err, Some(WsServerMsg::Error { ref reason }) if reason.contains("not a text")));
        assert!(hears_nothing(&mut bob, is_msg("sneaky")).await);
        assert_eq!(db.recent_messages(1, 10).len(), 1, "only alice's valid post persisted");

        // 5. Members can read history; it replays the persisted message.
        send(&mut bob, &WsClientMsg::History { channel_id: 1, before_id: None }).await;
        let h = next_matching(&mut bob, |m| matches!(m, WsServerMsg::Message { new: false, .. })).await;
        assert!(matches!(h, Some(WsServerMsg::Message { ref content, .. }) if content == "hello bob"));

        // 6. Presence: when Bob's only socket closes, Alice is told he is offline.
        drop(bob);
        assert!(
            next_matching(&mut alice, |m| matches!(
                m, WsServerMsg::Presence { user_id: 2, status } if status == "offline"
            ))
            .await
            .is_some(),
            "alice is told bob went offline"
        );
        assert!(hears_nothing(&mut carol, |m| matches!(m, WsServerMsg::Presence { user_id: 2, .. })).await);

        crate::session::set_legacy_header_for_tests(None);
    }

    #[tokio::test]
    async fn http_endpoints_require_a_valid_token_and_dms_are_pushed() {
        let (_d, db) = setup();
        let _guard = crate::e2e_lock::guard();
        crate::realtime::hub().reset();
        server::install_state(db.clone());
        crate::session::set_legacy_header_for_tests(Some(false));
        let addr = boot().await;
        let (t1, t2) = (crate::session::issue_token(1), crate::session::issue_token(2));

        // The old stand-in header is rejected; a forged token is rejected; a
        // real token works.
        let v = http(addr, "list_servers", None, Some("1"), "[null]").await;
        assert!(v.get("Err").is_some(), "legacy x-user-id must not authenticate: {v:?}");
        let v = http(addr, "list_servers", Some("v1.1.9999999999.deadbeef"), Some("1"), "[null]").await;
        assert!(v.get("Err").is_some(), "forged token must not authenticate: {v:?}");
        let v = http(addr, "list_servers", Some(&t1), None, "[null]").await;
        assert!(v.get("Ok").is_some(), "valid token authenticates: {v:?}");
        assert_eq!(v["Ok"].as_array().map(Vec::len), Some(1), "alice is in one server");
        // The token, not the body, decides who is acting: bob sees his own list.
        let v = http(addr, "list_servers", Some(&t2), None, "[null]").await;
        assert_eq!(v["Ok"].as_array().map(Vec::len), Some(1));

        // DM push: bob has a socket open; alice DMs him over plain HTTP.
        let mut bob = connect(addr, &t2).await.expect("bob connects");
        let dm = http(addr, "start_dm", Some(&t1), None, r#"[{"user_id":2}]"#).await;
        let dm_id = dm["Ok"]["channel_id"].as_i64().unwrap_or_else(|| panic!("start_dm failed: {dm:?}"));
        let body = format!(r#"[{{"channel_id":{dm_id},"content":"psst"}}]"#);
        let sent = http(addr, "send_dm_message", Some(&t1), None, &body).await;
        assert!(sent.get("Ok").is_some(), "send_dm_message: {sent:?}");
        let pushed = next_matching(&mut bob, |m| matches!(m, WsServerMsg::DmMessage { .. })).await;
        match pushed {
            Some(WsServerMsg::DmMessage { dm_id: d, author_id, content, .. }) => {
                assert_eq!((d, author_id, content.as_str()), (dm_id, 1, "psst"));
            }
            other => panic!("bob should be pushed the DM, got {other:?}"),
        }

        crate::session::set_legacy_header_for_tests(None);
    }
}
