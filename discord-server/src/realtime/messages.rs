//! Channel message posting, reading, editing and deletion: authorization,
//! mention resolution, attachment validation and media storage.

use std::sync::OnceLock;
use std::time::Duration;

use crate::db::{Db, Message};
use crate::permissions::{self, Permissions};
use crate::rate_limit::RateLimiter;
use crate::server_settings::{WsServerMsg, next_frame_seq};
use crate::realtime::{hub, now_millis, channel_audience};

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

fn send_limiter() -> &'static RateLimiter {
    static LIMITER: OnceLock<RateLimiter> = OnceLock::new();
    LIMITER.get_or_init(|| RateLimiter::new(SEND_LIMIT, SEND_WINDOW))
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
            author_decoration: db.user_avatar_decoration(user_id),
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
            seq: next_frame_seq(),
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
            seq: next_frame_seq(),
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
        &WsServerMsg::MessageDeleted { message_id, channel_id, seq: next_frame_seq() },
    );
    Ok(())
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
mod tests {
    use super::*;
    use crate::realtime::test_fixtures::{setup, with_private_channel};
    use crate::db::mention_tokens;
    use crate::server_settings::{WsServerMsg, AttachmentInput};
    use crate::realtime::channel_audience;

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
                seq: _,
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
                seq: _,
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
}
