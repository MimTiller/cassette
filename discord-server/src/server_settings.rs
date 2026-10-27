//! Server-settings endpoints: rename a server, set its icon, transfer
//! ownership.
//!
//! ## Design
//!
//! The owner-only authorization policy lives in a pure, DB-backed service
//! ([`ServerSettings`]) that takes the DB handle and the acting user id as
//! plain arguments. It returns a plain `Result<T, String>` so it is fully
//! unit-testable against a temp-file SQLite database without the axum/tokio
//! stack. The three `#[server]` fns below are thin adapters: they pull the
//! shared `Db` out of server state, resolve the acting user from the request
//! header, and map a service failure to `ServerError::failed`.
//!
//! There is no real auth layer yet (the `users` table carries no credentials —
//! "auth is a later layer"), so the acting user is read from the `x-user-id`
//! request header. That is a stand-in that the real identity layer will
//! replace; the owner-gating logic does not depend on how the caller is
//! resolved.
//!
//! **Icon representation:** the icon is stored as a **URL string** in
//! `servers.icon_url`. The `set_server_icon` endpoint accepts `None` (clear) or
//! a `String` URL. Keeping one representation keeps the wire format stable and
//! consistent with the schema column.
//!
//! ## Owner-only guarantee
//!
//! Every mutation first checks the *current* owner in the database:
//!   * rename / set icon  → the caller must be the row in
//!     `server_memberships` with `role = 'owner'` for that server.
//!   * transfer ownership → the caller must be the current owner, and the
//!     new owner must already be a member; the transfer is done in a single
//!     transaction guarded by `servers.owner_id = <caller>` so a concurrent
//!     transfer can't race.

use serde::{Deserialize, Serialize};
use server::{server, ServerError};

#[cfg(feature = "server")]
use crate::db::Db;
// roles is pure logic and client-visible, but only the server-gated code in
// this file uses its names (the `#[server]` client stubs do not reference the
// arg types), so the import is gated too.
#[cfg(feature = "server")]
use crate::roles::{Action, ChannelKind, Role};

/// The acting user for the current request, resolved from a verified session
/// token (see [`crate::session`]); `None` when unauthenticated.
#[cfg(feature = "server")]
use crate::session::acting_user;

// ===========================================================================
// Realtime gateway: session-token auth, realtime hub fan-out.
// ===========================================================================
//
// A full-duplex `#[server::channel]` endpoint. The client connects with the
// signed session token `login` returned (a wire arg in the connect URL, since
// a browser WebSocket cannot set custom upgrade headers). The token is
// verified before any frame is exchanged; the user id is taken from the
// verified token. All events (channel messages, DMs, presence) are fanned out
// through `crate::realtime` to every socket that should hear them.

/// An image attachment the client sends with a message: `filename`, a declared
/// `mime` (must be one of `image/png`, `image/jpeg`, `image/webp`, `image/gif`),
/// and `data` = a `data:image/…;base64,` URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentInput {
    pub filename: String,
    pub mime: String,
    pub data: String,
}

/// The persisted form of an image attachment (`id` = `message_attachments.id`),
/// riding on history/live `Message` frames. `url` is a `/media/<sha>.<ext>` path
/// (the bytes live on disk); a legacy inline `data:` URL until the one-time
/// `media::migrate_existing` pass converts it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageAttachment {
    pub id: i64,
    pub filename: String,
    pub mime: String,
    pub url: String,
    /// The stored image's pixel size, which the client sizes the attachment
    /// box from. `None` when unknown (a legacy inline value, an unreadable
    /// file, or a server from before this field).
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
}

#[cfg(feature = "server")]
impl MessageAttachment {
    /// The wire record for a stored attachment, with its pixel size read from
    /// the media file (`media::dimensions`).
    pub fn stored(id: i64, filename: String, mime: String, url: String) -> Self {
        let size = crate::media::dimensions(&url);
        MessageAttachment {
            id,
            filename,
            mime,
            url,
            width: size.map(|(w, _)| w),
            height: size.map(|(_, h)| h),
        }
    }
}

/// Inbound frames the client sends.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WsClientMsg {
    /// Send a message to the channel. `reply_to_message_id` is the id of the
    /// message being replied to (`None` for a normal post); the server records
    /// it on the new row and echoes the resolved reference back on the frame.
    Send {
        channel_id: i64,
        content: String,
        #[serde(default)]
        reply_to_message_id: Option<i64>,
        /// Image attachments sent with this message (empty = none).
        #[serde(default)]
        attachments: Vec<AttachmentInput>,
    },
    /// Request the recent message history for a channel. `before_id` is the
    /// scrollback cursor: `None` for the initial load (latest `limit`), or the
    /// oldest message id the client already holds to fetch the next page back.
    History { channel_id: i64, before_id: Option<i64> },
    /// The client is typing in the channel.
    Typing { channel_id: i64 },
    /// Edit the content of one of the client's own messages.
    EditMessage { message_id: i64, content: String },
    /// Delete one of the client's own messages.
    DeleteMessage { message_id: i64 },
}

/// Outbound frames the server sends.
///
/// `PartialEq` is required on the client side: the `#[server::channel]`
/// macro's generated stub calls `server::use_socket::<WsServerMsg, _>`, whose
/// where-clause demands `In: DeserializeOwned + Clone + PartialEq + 'static`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WsServerMsg {
    /// A message, either a freshly posted one (`new: true`) or history
    /// (`new: false`).
    ///
    /// `mentioned_user_ids` is the resolved `@mention` list: the user ids the
    /// message text mentions (one entry per mentioned member). A client
    /// decides "am I mentioned?" by checking `mentioned_user_ids.contains(&my_id)`.
    /// Empty for messages with no mentions, and always empty for DM history
    /// (DMs carry no `@mention` resolution).
    Message {
        message_id: i64,
        channel_id: i64,
        author_id: i64,
        author_username: String,
        /// The author's avatar (a `/media/…` path) at post time,
        /// `None` = default initial circle. Lets clients render the author's
        /// avatar without a per-message user lookup.
        #[serde(default)]
        author_avatar: Option<String>,
        content: String,
        created_at: i64,
        edited_at: Option<i64>,
        new: bool,
        #[serde(default)]
        mentioned_user_ids: Vec<i64>,
        /// The id of the message this one replies to (`None` for a normal
        /// post). Clients resolve the replied-to author + snippet from their
        /// loaded buffer to render the reply-reference strip.
        #[serde(default)]
        reply_to_message_id: Option<i64>,
        /// Emoji reactions on this message, grouped by emoji: one
        /// `(emoji, count, reacted_by_me)` tuple per distinct emoji, where
        /// `reacted_by_me` is relative to the socket's own user (the server
        /// computes it from the verified token). History frames (`new: false`)
        /// carry the persisted state from `message_reactions` so a cold load
        /// restores pills; a fresh-post echo is empty (later changes ride the
        /// `ReactionAdd` / `ReactionRemove` frames).
        #[serde(default)]
        reactions: Vec<(String, u32, bool)>,
        /// The image attachments persisted with this message (empty = none),
        /// as stored in `message_attachments`. Older clients ignore them via
        /// the serde default.
        #[serde(default)]
        attachments: Vec<MessageAttachment>,
    },
    /// The most recent `Message { new: false }` frames for `History`.
    ///
    /// `has_more` is the client's scrollback-exhausted signal: it is `true`
    /// when this page was a full `limit` (more older messages exist, so the
    /// "Load earlier messages" pill should stay available) and `false` when the
    /// page is shorter than `limit` (the reader reached the start of the
    /// conversation). It is the server's authoritative word on whether another
    /// `History { before_id }` page would return anything — the client compares
    /// the marker's `has_more` to decide whether to keep the pill mounted,
    /// rather than re-deriving it from the page's own length.
    History { channel_id: i64, has_more: bool },
    /// A direct message pushed to a participant of the DM (always `new`).
    DmMessage { message_id: i64, dm_id: i64, author_id: i64, author_username: String, #[serde(default)] author_avatar: Option<String>, content: String, created_at: i64 },
    /// A presence status change for a user: 'online' / 'away' / 'offline'.
    Presence { user_id: i64, status: String },
    /// A channel member started typing. The client shows this transiently
    /// (a few seconds) and does not need an explicit stop event.
    Typing { channel_id: i64, user_id: i64 },
    /// A channel message was edited. The client replaces the stored content
    /// (and its timestamp) for `message_id` with the new values.
    MessageEdited { message_id: i64, channel_id: i64, content: String, edited_at: i64 },
    /// A channel message was deleted. The client drops its stored entry for
    /// `message_id`.
    MessageDeleted { message_id: i64, channel_id: i64 },
    /// A member joined (`joined: true`) or left (`joined: false`) the server.
    /// Fired by the join (invite redemption) and leave (kick/ban) paths so the
    /// server's *other* members see the roster change live — the client
    /// re-fetches `list_members` for the active server on receipt (the same
    /// re-fetch the local kick path already triggers). No roster data rides on
    /// the frame; the re-fetch supplies it.
    MemberChanged { server_id: i64, user_id: i64, joined: bool },
    /// A member added (or removed) an emoji reaction on a channel message.
    /// `count` is the message's total for `emoji` AFTER the operation (0 when
    /// the last reaction was removed), so clients SET the pill's count rather
    /// than incrementing locally — duplicate frames and out-of-order delivery
    /// converge. `user_id` is who the frame is about: the client highlights
    /// the pill when it names itself.
    ReactionAdd {
        message_id: i64,
        channel_id: i64,
        user_id: i64,
        emoji: String,
        count: u32,
    },
    ReactionRemove {
        message_id: i64,
        channel_id: i64,
        user_id: i64,
        emoji: String,
        count: u32,
    },
    /// The friends list changed for the socket's owner: a friend request was
    /// sent / auto-accepted (`action` = 'requested' / 'accepted'), declined or
    /// cancelled, or a friend was removed ('removed'). `friend_id` is the other
    /// user in the pair. The client re-fetches `list_friends` on receipt (the
    /// same re-fetch pattern as `MemberChanged`) — no relationship data rides
    /// on the frame; the re-fetch supplies it.
    FriendChanged { friend_id: i64, action: String },
    /// A gateway error for a specific operation.
    Error { reason: String },
}

/// How long an idle (no inbound frame) socket may sit before its owner's
/// presence flips from 'online' to 'away'. Any inbound frame resets the
/// clock back to 'online'.
#[cfg(feature = "server")]
const PRESENCE_IDLE_SECS: u64 = 30;

/// How often an open gateway socket re-verifies its session token.
#[cfg(feature = "server")]
const SESSION_RECHECK_SECS: u64 = 30;

/// The realtime message gateway. Mounted at `GET /_srv/_ws/message_gateway`.
///
/// `token` is a wire arg (encoded into the connect URL's `?args=` query by the
/// client stub): the signed session token `login` returned. A browser
/// WebSocket cannot set custom upgrade headers, so the token rides the URL. It
/// is verified before any frame is exchanged: a missing, forged or expired
/// token gets no working socket, and the user id is taken from the verified
/// token, never from the client.
///
/// Delivery: every event goes through [`crate::realtime::hub`]. A message
/// posted in a channel reaches every open socket of every member of that
/// channel's server (including the sender's own, which is its echo); DMs are
/// pushed the same way (see `dms::send_dm_message`); presence changes reach the
/// user's fellow server members. Reading or posting requires membership of the
/// channel's server and a text channel, and posts are size- and rate-limited.
#[server::channel]
pub async fn message_gateway(
    mut ch: server::Socket<WsClientMsg, WsServerMsg>,
    token: String,
    cookies: server::Cookies,
) -> Result<(), ServerError> {
    use crate::realtime::{announce_presence, history_for, hub, post_channel_message, server_audience_for_channel};

    let db = db_from_state()?;
    // The credential: the connect-URL wire arg (native clients, whose bearer
    // comes from the platform keystore) OR the httpOnly `session` cookie the
    // browser replays on the upgrade handshake — the web path, where JS never
    // sees the token. Both are HMAC-verified; a client-supplied user id is
    // never trusted.
    let credential = [Some(token.clone()), cookies.get("session").map(|c| c.to_string())]
        .into_iter()
        .flatten()
        .find(|t| crate::session::verify_token(t).is_some())
        .ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let user_id = crate::session::verify_token(&credential)
        .ok_or_else(|| ServerError::failed("unauthenticated"))?;

    let hub = hub();
    let (conn_id, mut events) = hub.register(user_id);

    // Presence: 'online' the moment the socket is authenticated.
    // `db.set_presence` returns whether the value actually changed, so we only
    // announce a transition the audience doesn't already know about.
    let mut presence = "online".to_string();
    if db.set_presence(user_id, &presence).is_ok_and(|changed| changed) {
        announce_presence(&db, user_id, &presence);
    }

    // Only inbound client frames count as activity (events pushed to this
    // socket must not keep an idle user "online").
    let mut last_activity = tokio::time::Instant::now();
    // The credential is re-checked periodically: an expired or revoked
    // session (password changed, account deleted) closes the socket.
    let mut recheck = tokio::time::interval(std::time::Duration::from_secs(SESSION_RECHECK_SECS));
    recheck.tick().await;
    loop {
        let idle_at = last_activity + std::time::Duration::from_secs(PRESENCE_IDLE_SECS);
        let frame = tokio::select! {
            r = ch.recv() => r,
            ev = events.recv() => {
                match ev {
                    Some(m) => {
                        if ch.send(m).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
                continue;
            }
            _ = recheck.tick() => {
                if crate::session::verify_token(&credential) != Some(user_id) {
                    break;
                }
                continue;
            }
            _ = tokio::time::sleep_until(idle_at), if presence == "online" => {
                presence = "away".to_string();
                if db.set_presence(user_id, "away").is_ok_and(|changed| changed) {
                    announce_presence(&db, user_id, "away");
                }
                continue;
            }
        };
        let Some(Ok(msg)) = frame else {
            break;
        };
        last_activity = tokio::time::Instant::now();
        if presence != "online" {
            presence = "online".to_string();
            if db.set_presence(user_id, "online").is_ok_and(|changed| changed) {
                announce_presence(&db, user_id, "online");
            }
        }
        match msg {
            WsClientMsg::Send {
                channel_id,
                content,
                reply_to_message_id,
                attachments,
            } => {
                // Success fans out through the hub (the sender's own echo
                // included); only failures are answered on this socket.
                if let Err(reason) =
                    post_channel_message(
                        &db,
                        user_id,
                        channel_id,
                        &content,
                        reply_to_message_id,
                        attachments,
                    )
                {
                    let _ = ch.send(WsServerMsg::Error { reason }).await;
                }
            }
            WsClientMsg::History {
                channel_id,
                before_id,
            } => match history_for(&db, user_id, channel_id, before_id, 50) {
                Ok(messages) => {
                    // The marker's `has_more` is the client's scrollback-exhausted
                    // signal: a full page (50, the `limit` just passed) means more
                    // older messages exist; a short page means the reader hit the
                    // start. It is captured BEFORE the `for` loop consumes
                    // `messages` (the iterator moves the Vec by value), and it
                    // rides the marker frame that precedes the page so the client
                    // learns the answer before the `Message { new: false }` frames
                    // land and the list rebuilds.
                    let has_more = messages.len() == 50;
                    let _ = ch
                        .send(WsServerMsg::History {
                            channel_id,
                            has_more,
                        })
                        .await;
                    for m in messages {
                        let author_username = db
                            .user_by_id(m.author_id)
                            .map(|(_, n)| n)
                            .unwrap_or_default();
                        let _ = ch
                            .send(WsServerMsg::Message {
                                message_id: m.id,
                                channel_id: m.channel_id,
                                author_id: m.author_id,
                                author_username,
                                author_avatar: db.user_avatar(m.author_id),
                                content: m.content,
                                created_at: m.created_at,
                                edited_at: m.edited_at,
                                new: false,
                                mentioned_user_ids: m.mentioned_user_ids,
                                reply_to_message_id: m.reply_to_message_id,
                                // Cold-load persistence: history frames carry the
                                // message's persisted reactions, with
                                // `reacted_by_me` relative to the socket's own user.
                                reactions: db.reactions_for_message(m.id, user_id),
                                // Image attachments persisted with the message
                                // (empty = none), so a cold history load renders
                                // them without a second lookup. The db-layer
                                // `Attachment` and wire-layer `MessageAttachment`
                                // are the same record shape, re-typed.
                                attachments: m.attachments
                                    .into_iter()
                                    .map(|a| crate::server_settings::MessageAttachment::stored(
                                        a.id, a.filename, a.mime, a.url,
                                    ))
                                    .collect(),
                                })
                            .await;
                    }
                }
                Err(reason) => {
                    let _ = ch
                        .send(WsServerMsg::Error {
                            reason: reason.to_string(),
                        })
                        .await;
                }
            },
            WsClientMsg::Typing { channel_id } => {
                let audience = server_audience_for_channel(&db, channel_id, user_id);
                hub.publish(
                    &audience,
                    &WsServerMsg::Typing { channel_id, user_id },
                );
            }
            WsClientMsg::EditMessage {
                message_id,
                content,
            } => {
                // Success fans out through the hub (echo to the editor
                // included); only failures are answered on this socket.
                if let Err(reason) =
                    crate::realtime::edit_channel_message(&db, user_id, message_id, &content)
                {
                    let _ = ch.send(WsServerMsg::Error { reason }).await;
                }
            }
            WsClientMsg::DeleteMessage { message_id } => {
                if let Err(reason) =
                    crate::realtime::delete_channel_message(&db, user_id, message_id)
                {
                    let _ = ch.send(WsServerMsg::Error { reason }).await;
                }
            }
        }
    }
    // Disconnect: the socket has closed (client went away, transport died, or
    // the loop broke on an error). When it was the user's last socket, mark
    // them 'offline' and tell their fellow server members.
    if hub.unregister(user_id, conn_id)
        && db.set_presence(user_id, "offline").is_ok_and(|changed| changed)
    {
        announce_presence(&db, user_id, "offline");
    }
    Ok(())
}

/// The result of any server-settings operation. Carries the updated server
/// name + icon so the client can refresh its local model in one round trip.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ServerSettingsResult {
    pub server_id: i64,
    pub name: String,
    pub icon_url: Option<String>,
    /// The current owner after the operation (changes on transfer).
    pub owner_id: i64,
}

/// The server-settings service. Wraps a `Db` handle.
///
/// Every method takes the acting user id explicitly so the authorization is
/// testable without any request context.
#[cfg(feature = "server")]
pub struct ServerSettings {
    db: Db,
}

#[cfg(feature = "server")]
impl ServerSettings {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Rename the server with `server_id` to `new_name`.
    ///
    /// # Errors
    /// - `not found` — the server id doesn't exist.
    /// - `not the server owner` — the caller is not the current owner.
    pub fn rename(&self, caller_id: i64, server_id: i64, new_name: &str) -> Result<ServerSettingsResult, String> {
        if new_name.trim().is_empty() {
            return Err("server name must not be empty".into());
        }
        let (sid, _name, _owner_id, _icon_url) = self
            .db
            .server_by_id(server_id)
            .ok_or("server not found")?;
        // Owner-only: the caller's standing in this server.
        let role = self.caller_role(server_id, caller_id)?;
        if !role.can(Action::RenameServer) {
            return Err("permission denied: only the server owner can rename the server".into());
        }
        self.db.rename_server(sid, new_name).map_err(|e| e.to_string())?;
        // Re-read to return a consistent snapshot (the rename already updated
        // the name; re-reading keeps the result faithful to disk).
        let (_id, name, owner_id, icon_url) = self
            .db
            .server_by_id(server_id)
            .ok_or("server not found")?;
        Ok(ServerSettingsResult {
            server_id,
            name,
            icon_url,
            owner_id,
        })
    }

    /// Set (or clear) the server icon.
    ///
    /// `icon` is `None` to clear the icon, or a URL string to set it.
    ///
    /// # Errors
    /// - `not found` — the server id doesn't exist.
    /// - `not the server owner` — the caller is not the current owner.
    pub fn set_icon(&self, caller_id: i64, server_id: i64, icon: Option<&str>) -> Result<ServerSettingsResult, String> {
        let (sid, _name, _owner_id, _icon_url) = self
            .db
            .server_by_id(server_id)
            .ok_or("server not found")?;
        let role = self.caller_role(server_id, caller_id)?;
        if !role.can(Action::SetServerIcon) {
            return Err("permission denied: only the server owner or an admin can set the server icon".into());
        }
        // Reject an empty string URL — treat it as an error rather than
        // silently storing garbage.
        if let Some(url) = icon {
            if url.trim().is_empty() {
                return Err("icon url must not be empty (pass null to clear)".into());
            }
            // Same posture as avatars: the bytes move to the media store, the
            // DB keeps the `/media/…` path. Server icons use the AVATAR cap.
            let (mime, bytes) = match crate::media::decode_data_image_url(url) {
                Some(p) => p,
                None => return Err("icon must be a data:image/…;base64, URL".into()),
            };
            match crate::media::store(bytes.as_slice(), &mime, crate::auth::AVATAR_MAX_BYTES) {
                Err(e) => return Err(e),
                Ok(path) => {
                    self.db
                        .set_server_icon(sid, Some(&path))
                        .map_err(|e| e.to_string())?;
                }
            }
        } else {
            self.db.set_server_icon(sid, None).map_err(|e| e.to_string())?;
        }
        let (_id, name, owner_id, icon_url) = self
            .db
            .server_by_id(server_id)
            .ok_or("server not found")?;
        Ok(ServerSettingsResult {
            server_id,
            name,
            icon_url,
            owner_id,
        })
    }

    /// Transfer server ownership from the current owner (`caller_id`) to
    /// `new_owner_id`.
    ///
    /// # Errors
    /// - `not found` — the server id doesn't exist.
    /// - `permission denied` — the caller is not the current owner.
    /// - `new owner is not a member of this server` — the target user has no
    ///   membership row for this server.
    pub fn transfer_ownership(
        &self,
        caller_id: i64,
        server_id: i64,
        new_owner_id: i64,
    ) -> Result<ServerSettingsResult, String> {
        let (_sid, _name, owner_id, _icon_url) = self
            .db
            .server_by_id(server_id)
            .ok_or("server not found")?;
        // Owner-only: the caller must be the current owner.
        if owner_id != caller_id {
            return Err("permission denied: only the server owner can transfer ownership".into());
        }
        self.db
            .transfer_server_ownership(server_id, caller_id, new_owner_id)
            .map_err(|e| e.to_string())?;
        let (_id, name, owner_id, icon_url) = self
            .db
            .server_by_id(server_id)
            .ok_or("server not found")?;
        Ok(ServerSettingsResult {
            server_id,
            name,
            icon_url,
            owner_id,
        })
    }

    // ── shared authorization helpers ───────────────────────────────────────

    /// Resolve the caller's [`Role`] in `server_id`.
    ///
    /// Returns `permission denied: not a member of this server` when the
    /// caller has no membership row. An unparseable role string (schema
    /// drift) is treated as a *denied* standing rather than trusted, so a
    /// corrupt row can never grant power.
    fn caller_role(&self, server_id: i64, caller_id: i64) -> Result<Role, String> {
        let role_str = self
            .db
            .membership_role(server_id, caller_id)
            .ok_or_else(|| "permission denied: not a member of this server".to_string())?;
        Role::from_db_str(&role_str).ok_or_else(|| {
            format!("permission denied: unknown stored role {role_str:?}")
        })
    }

    /// Check that `caller_id` may perform `action` in `server_id`, and
    /// return the caller's role on success. Combines membership resolution
    /// with the [`Role::can`] policy in one call — the single gate every
    /// mutation below goes through.
    fn require(&self, caller_id: i64, server_id: i64, action: Action) -> Result<Role, String> {
        let role = self.caller_role(server_id, caller_id)?;
        if !role.can(action) {
            return Err(format!(
                "permission denied: requires role '{}' or above (you are '{}')",
                action.minimum_role().label(),
                role.label()
            ));
        }
        Ok(role)
    }

    /// The current wall-clock seconds since the Unix epoch, for the
    /// `joined_at` / `created_at` timestamps the DB layer stores as `i64`.
    fn now_secs() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(1_000)
    }

    // ── channel management ─────────────────────────────────────────────────

    /// Create a channel in `server_id`. Admin+ (owner or admin).
    ///
    /// # Errors
    /// - `not found` — the server id doesn't exist.
    /// - `permission denied` — the caller's role is below admin.
    /// - `channel name must not be empty` / `channel name is already taken`.
    pub fn create_channel(
        &self,
        caller_id: i64,
        server_id: i64,
        name: &str,
        kind: ChannelKind,
        parent_channel_id: Option<i64>,
    ) -> Result<i64, String> {
        if name.trim().is_empty() {
            return Err("channel name must not be empty".into());
        }
        self.db
            .server_by_id(server_id)
            .ok_or("server not found")?;
        self.require(caller_id, server_id, Action::CreateChannel)?;
        self.db
            .insert_channel(server_id, name.trim(), kind, parent_channel_id, Self::now_secs())
            .map_err(|e| format!("failed to create channel: {e}"))
    }

    /// Rename a channel. Moderator+ (owner, admin, or moderator).
    ///
    /// # Errors
    /// - `channel not found` — the id doesn't exist.
    /// - `permission denied` — the caller's role is below moderator, or the
    ///   channel lives in a different server than `server_id`.
    pub fn rename_channel(
        &self,
        caller_id: i64,
        server_id: i64,
        channel_id: i64,
        new_name: &str,
    ) -> Result<String, String> {
        if new_name.trim().is_empty() {
            return Err("channel name must not be empty".into());
        }
        // Resolve the channel first so the permission check runs against the
        // channel's *own* server (a caller can't rename a channel in a server
        // they don't hold the role in by passing a foreign server_id).
        let (_cid, chan_server, _name, _kind, _pos, _parent) = self
            .db
            .channel_full(channel_id)
            .ok_or("channel not found")?;
        if chan_server != server_id {
            return Err("channel not found".into());
        }
        self.require(caller_id, server_id, Action::RenameChannel)?;
        self.db
            .rename_channel(channel_id, new_name.trim())
            .map_err(|e| format!("failed to rename channel: {e}"))
    }

    /// Delete a channel. Admin+ (owner or admin).
    ///
    /// # Errors
    /// - `channel not found` — the id doesn't exist (or is in another server).
    /// - `permission denied` — the caller's role is below admin.
    pub fn delete_channel(&self, caller_id: i64, server_id: i64, channel_id: i64) -> Result<(), String> {
        let (_cid, chan_server, _name, _kind, _pos, _parent) = self
            .db
            .channel_full(channel_id)
            .ok_or("channel not found")?;
        if chan_server != server_id {
            return Err("channel not found".into());
        }
        self.require(caller_id, server_id, Action::DeleteChannel)?;
        let removed = self
            .db
            .delete_channel(channel_id)
            .map_err(|e| format!("failed to delete channel: {e}"))?;
        if removed == 0 {
            return Err("channel not found".into());
        }
        Ok(())
    }

    // ── member management ──────────────────────────────────────────────────

    /// Add (or re-role) a member in `server_id` with `role`. Moderator+ for
    /// `AddMember`; granting `admin`/`owner` additionally requires `admin+`
    /// (enforced via `SetMemberRole`) because a plain moderator must not be
    /// able to promote someone above themselves.
    ///
    /// The caller can never be demoted or removed from the server by this
    /// path (that is `remove_member`), and the owner role cannot be granted
    /// here — only `transfer_ownership` moves it.
    pub fn add_member(
        &self,
        caller_id: i64,
        server_id: i64,
        user_id: i64,
        role: Role,
    ) -> Result<String, String> {
        // The owner role is reserved for the creator; it moves only via
        // transfer_ownership.
        if role == Role::Owner {
            return Err("cannot assign the owner role (use transfer_ownership)".into());
        }
        self.db
            .server_by_id(server_id)
            .ok_or("server not found")?;
        // Adding someone (even as a plain member) is a moderator+ duty.
        self.require(caller_id, server_id, Action::AddMember)?;
        // Granting admin is an admin+ duty: a moderator may add members but
        // not promote them to admin.
        if role >= Role::Admin {
            self.require(caller_id, server_id, Action::SetMemberRole)?;
        }
        // A banned user can't be re-added even by an admin — the ban table is
        // authoritative over a direct add (an unban must come first).
        if self.db.is_banned(server_id, user_id) {
            return Err("member is banned and cannot be re-added (unban first)".into());
        }
        // The upsert can re-role an existing member; only announce a *join*
        // when this is a brand-new membership (a re-role is a count no-op).
        let was_member = self.db.membership_role(server_id, user_id).is_some();
        self.db
            .upsert_membership(server_id, user_id, role, Self::now_secs())
            .map_err(|e| format!("failed to add member: {e}"))?;
        if !was_member {
            crate::realtime::announce_membership(&self.db, server_id, user_id, true);
        }
        Ok(role.db_str().to_string())
    }

    /// Remove a member from `server_id`. Moderator+ (owner, admin, or
    /// moderator). The owner can never be removed through this path —
    /// ownership must be transferred first (the DB layer demotes, never
    /// deletes, the owner row, and this method refuses to target it).
    ///
    /// # Errors
    /// - `not found` — the server id doesn't exist.
    /// - `permission denied` — the caller's role is below moderator.
    /// - `cannot remove the server owner` — `user_id` is the owner.
    /// - `member not found` — `user_id` has no membership row.
    pub fn remove_member(&self, caller_id: i64, server_id: i64, user_id: i64) -> Result<(), String> {
        let (_sid, _name, owner_id, _icon_url) = self
            .db
            .server_by_id(server_id)
            .ok_or("server not found")?;
        self.require(caller_id, server_id, Action::RemoveMember)?;
        // The owner is protected: removing them would orphan the server.
        if user_id == owner_id {
            return Err("cannot remove the server owner (transfer ownership first)".into());
        }
        let removed = self
            .db
            .remove_membership(server_id, user_id)
            .map_err(|e| format!("failed to remove member: {e}"))?;
        if removed == 0 {
            return Err("member not found".into());
        }
        // Tell the server's remaining members a member just left so their
        // live roster (and `MEMBERS — N`) updates without a reload.
        crate::realtime::announce_membership(&self.db, server_id, user_id, false);
        Ok(())
    }

    /// `caller_id` leaves `server_id`. Any member may leave except the owner,
    /// who must transfer ownership first (the server would be orphaned).
    ///
    /// # Errors
    /// - `server not found`, `not a member of this server`
    /// - `the owner can't leave (transfer ownership first)`
    pub fn leave(&self, caller_id: i64, server_id: i64) -> Result<(), String> {
        let (_sid, _name, owner_id, _icon_url) = self
            .db
            .server_by_id(server_id)
            .ok_or("server not found")?;
        if caller_id == owner_id {
            return Err("the owner can't leave (transfer ownership first)".into());
        }
        let removed = self
            .db
            .remove_membership(server_id, caller_id)
            .map_err(|e| format!("failed to leave: {e}"))?;
        if removed == 0 {
            return Err("not a member of this server".into());
        }
        crate::realtime::announce_membership(&self.db, server_id, caller_id, false);
        Ok(())
    }

    /// Ban a member from `server_id`: remove them AND bar rejoining. Admin+
    /// (owner or admin) — a ban is more severe than a kick, so it clears a
    /// higher bar than `remove_member` (moderator+). The owner can never be
    /// banned through this path (ownership must be transferred first, and the
    /// owner row is protected exactly as in `remove_member`).
    ///
    /// The membership removal and the ban record happen atomically (a single
    /// DB transaction), so a failure can never leave the user kicked-but-not-
    /// banned. Rejoining is blocked in both join paths (`redeem_invite` and
    /// `add_member`) by consulting the ban table — see those functions.
    ///
    /// # Errors
    /// - `not found` — the server id doesn't exist.
    /// - `permission denied` — the caller's role is below admin.
    /// - `cannot ban the server owner` — `user_id` is the owner.
    /// - `member not found` — `user_id` has no membership row.
    pub fn ban_member(
        &self,
        caller_id: i64,
        server_id: i64,
        user_id: i64,
        reason: Option<&str>,
    ) -> Result<(), String> {
        let (_sid, _name, owner_id, _icon_url) = self
            .db
            .server_by_id(server_id)
            .ok_or("server not found")?;
        self.require(caller_id, server_id, Action::BanMember)?;
        // The owner is protected: banning them would orphan the server.
        if user_id == owner_id {
            return Err("cannot ban the server owner (transfer ownership first)".into());
        }
        let removed = self
            .db
            .ban_and_remove(server_id, user_id, caller_id, reason, Self::now_secs())
            .map_err(|e| format!("failed to ban member: {e}"))?;
        if removed == 0 {
            return Err("member not found".into());
        }
        // A ban is a leave too: tell the remaining members the roster shrank.
        crate::realtime::announce_membership(&self.db, server_id, user_id, false);
        Ok(())
    }

    /// Time out a member of `server_id`: silence them from posting messages
    /// for `duration_secs` without removing their membership (a kick/ban drop
    /// the row; a timeout only sets a deadline). Admin+ (owner or admin), the
    /// same bar as a ban — a timeout restrains a member more strongly than a
    /// kick. The owner is protected exactly as in `ban_member` (timing them
    /// out would orphan the server). Re-timing overwrites the existing
    /// deadline in place (`db.set_timeout`), so a member can be extended.
    ///
    /// Enforcement is at the post boundary: [`crate::realtime::post_channel_message`]
    /// consults `db.is_timed_out` before persisting, comparing `timeout_until`
    /// against the current epoch.
    ///
    /// # Errors
    /// - `not found` — the server id doesn't exist.
    /// - `permission denied` — the caller's role is below admin.
    /// - `cannot time out the server owner` — `user_id` is the owner.
    /// - `member not found` — `user_id` has no membership row.
    pub fn timeout_member(
        &self,
        caller_id: i64,
        server_id: i64,
        user_id: i64,
        duration_secs: i64,
    ) -> Result<(), String> {
        let (_sid, _name, owner_id, _icon_url) = self
            .db
            .server_by_id(server_id)
            .ok_or("server not found")?;
        self.require(caller_id, server_id, Action::TimeoutMember)?;
        // A timeout of zero or less would be instantly expired — reject it at
        // the boundary rather than silently no-op.
        if duration_secs <= 0 {
            return Err("timeout duration must be positive".into());
        }
        // The owner is protected: timing them out would orphan the server.
        if user_id == owner_id {
            return Err("cannot time out the server owner (transfer ownership first)".into());
        }
        let timeout_until = Self::now_secs() + duration_secs;
        let updated = self
            .db
            .set_timeout(server_id, user_id, timeout_until)
            .map_err(|e| format!("failed to time out member: {e}"))?;
        if updated == 0 {
            return Err("member not found".into());
        }
        Ok(())
    }

    /// Change an existing member's role in `server_id`. Admin+ (owner or
    /// admin). The owner's role can't be changed and the owner role can't be
    /// assigned — ownership moves only via `transfer_ownership`.
    ///
    /// # Errors
    /// - `not found` — the server id doesn't exist.
    /// - `permission denied` — the caller's role is below admin.
    /// - `cannot change the server owner's role` / `cannot assign the owner
    ///   role`.
    /// - `member not found` — `user_id` has no membership row.
    pub fn set_member_role(
        &self,
        caller_id: i64,
        server_id: i64,
        user_id: i64,
        new_role: Role,
    ) -> Result<String, String> {
        let (_sid, _name, owner_id, _icon_url) = self
            .db
            .server_by_id(server_id)
            .ok_or("server not found")?;
        self.require(caller_id, server_id, Action::SetMemberRole)?;
        if new_role == Role::Owner {
            return Err("cannot assign the owner role (use transfer_ownership)".into());
        }
        if user_id == owner_id {
            return Err("cannot change the server owner's role (transfer ownership first)".into());
        }
        // The target must already be a member (this is a *change*, not an
        // invite — invites go through add_member).
        if self.db.membership_role(server_id, user_id).is_none() {
            return Err("member not found".into());
        }
        self.db
            .upsert_membership(server_id, user_id, new_role, Self::now_secs())
            .map_err(|e| format!("failed to set member role: {e}"))?;
        Ok(new_role.db_str().to_string())
    }

    // ── channel reads / unread counts ──────────────────────────────────────
    //
    // Per-user read state, gated on server membership. These are the two
    // operations the client needs to draw the unread badge: mark a channel
    // read up to a message id, and fetch the unread count for every channel in
    // a server.

    /// Record that `caller_id` has read up to and including `message_id` in
    /// `channel_id`. A `message_id` of 0 or less means "everything posted so
    /// far" (the channel's newest message), which a client can send for a
    /// channel whose messages it has not loaded (Mark As Read in the channel
    /// menu).
    ///
    /// The caller must be a member of the server that owns the channel. The
    /// write is monotonic (the stored id only ever moves forward), so marking
    /// read with an older id is a harmless no-op rather than a regression.
    ///
    /// # Errors
    /// - `channel not found` — the id doesn't exist.
    /// - `permission denied: not a member of this server` — no membership.
    /// - `failed to mark channel read: …` — the write failed.
    pub fn mark_read(
        &self,
        caller_id: i64,
        channel_id: i64,
        message_id: i64,
    ) -> Result<(), String> {
        // Resolve the owning server from the channel (a channel id maps to
        // exactly one server); membership is then checked against that server.
        let (_, server_id, _, _, _, _) = self
            .db
            .channel_full(channel_id)
            .ok_or("channel not found")?;
        if self.db.membership_role(server_id, caller_id).is_none() {
            return Err("permission denied: not a member of this server".into());
        }
        let message_id = if message_id > 0 {
            message_id
        } else {
            match self.db.latest_message_id(channel_id) {
                Some(id) => id,
                None => return Ok(()),
            }
        };
        self.db
            .mark_channel_read(caller_id, channel_id, message_id, Self::now_millis())
            .map_err(|e| format!("failed to mark channel read: {e}"))
    }

    /// The unread message count for every channel in `server_id` that
    /// `caller_id` belongs to.
    ///
    /// # Errors
    /// - `server not found` — the id doesn't exist.
    /// - `permission denied: not a member of this server` — no membership.
    pub fn unread_counts(
        &self,
        caller_id: i64,
        server_id: i64,
    ) -> Result<Vec<UnreadCount>, String> {
        if self.db.server_by_id(server_id).is_none() {
            return Err("server not found".into());
        }
        if self.db.membership_role(server_id, caller_id).is_none() {
            return Err("permission denied: not a member of this server".into());
        }
        let counts = self.db.unread_counts_for_server(caller_id, server_id);
        Ok(counts
            .into_iter()
            .map(|(channel_id, count, unread_mentions)| UnreadCount {
                channel_id,
                count,
                unread_mentions,
            })
            .collect())
    }

    /// All members of `server_id` with their role, ordered by role (owner
    /// first) then user id. Any member of the server may read the list.
    ///
    /// # Errors
    /// - `server not found` — the id doesn't exist.
    /// - `permission denied: not a member of this server` — no membership.
    pub fn list_members(&self, caller_id: i64, server_id: i64) -> Result<Vec<MemberInfo>, String> {
        if self.db.server_by_id(server_id).is_none() {
            return Err("server not found".into());
        }
        if self.db.membership_role(server_id, caller_id).is_none() {
            return Err("permission denied: not a member of this server".into());
        }
        Ok(self
            .db
            .server_members(server_id)
            .into_iter()
            .map(|(user_id, username, role)| MemberInfo {
                user_id,
                username,
                role,
                avatar_url: self.db.user_avatar(user_id),
            })
            .collect())
    }

    // ── message pins ───────────────────────────────────────────────────────
    //
    // Pinning a message in a channel. Pinning/unpinning is a moderator+ duty
    // (`Action::PinMessage` — Discord "Manage Messages" parity); *reading* the
    // pinned view is any membership (any member can see what is pinned). The
    // channel is resolved from the message id, so a caller cannot pin a
    // message in a server they hold no role in by passing a foreign id.

    /// Pin `message_id` in its channel. Moderator+ (owner, admin, or
    /// moderator) in the server that owns the message's channel. Idempotent: a
    /// second pin is a no-op.
    ///
    /// # Errors
    /// - `message not found` — the id is not a stored guild message.
    /// - `channel not found` — the message's channel is gone.
    /// - `permission denied: …` — the caller's role is below moderator, or the
    ///   caller is not a member of the owning server.
    pub fn pin_message(&self, caller_id: i64, message_id: i64) -> Result<PinResult, String> {
        let (_author_id, channel_id) = self
            .db
            .message_row(message_id)
            .ok_or("message not found")?;
        let (_, server_id, _, _, _, _) = self
            .db
            .channel_full(channel_id)
            .ok_or("channel not found")?;
        self.require(caller_id, server_id, Action::PinMessage)?;
        self.db
            .pin_message(message_id, caller_id, Self::now_millis())
            .map_err(|e| format!("failed to pin message: {e}"))?;
        Ok(PinResult {
            message_id,
            channel_id,
            pinned: true,
            pinned_count: self.db.pinned_count(channel_id),
        })
    }

    /// Unpin `message_id` from its channel. Moderator+ (same bar as
    /// [`Self::pin_message`]). Idempotent: unpinning an unpinned message is a
    /// no-op (`pinned: false`) rather than an error.
    ///
    /// # Errors
    /// - `message not found` — the id is not a stored guild message.
    /// - `channel not found` — the message's channel is gone.
    /// - `permission denied: …` — the caller's role is below moderator, or the
    ///   caller is not a member of the owning server.
    pub fn unpin_message(&self, caller_id: i64, message_id: i64) -> Result<PinResult, String> {
        let (_author_id, channel_id) = self
            .db
            .message_row(message_id)
            .ok_or("message not found")?;
        let (_, server_id, _, _, _, _) = self
            .db
            .channel_full(channel_id)
            .ok_or("channel not found")?;
        self.require(caller_id, server_id, Action::PinMessage)?;
        let removed = self
            .db
            .unpin_message(message_id)
            .map_err(|e| format!("failed to unpin message: {e}"))?;
        // After an unpin the message is *not* pinned — whether a row was just
        // removed or it was already unpinned (a no-op), the post-state is the
        // same. `pinned: false` unconditionally; `removed` only distinguishes
        // "we did the work" from "already gone" but that is not surfaced.
        let _ = removed;
        Ok(PinResult {
            message_id,
            channel_id,
            pinned: false,
            pinned_count: self.db.pinned_count(channel_id),
        })
    }

    /// Every pinned message in `channel_id`, in the order they were pinned
    /// (oldest pin first). Any member of the channel's server may read the
    /// list — pinning is moderator-only, but reading is not.
    ///
    /// # Errors
    /// - `channel not found` — the id doesn't exist.
    /// - `permission denied: not a member of this server` — no membership.
    pub fn list_pinned(&self, caller_id: i64, channel_id: i64) -> Result<Vec<PinnedMessage>, String> {
        let (_, server_id, _, _, _, _) = self
            .db
            .channel_full(channel_id)
            .ok_or("channel not found")?;
        if self.db.membership_role(server_id, caller_id).is_none() {
            return Err("permission denied: not a member of this server".into());
        }
        Ok(self
            .db
            .pinned_messages(channel_id)
            .into_iter()
            .map(|(m, pinned_by)| PinnedMessage {
                message_id: m.id,
                channel_id: m.channel_id,
                author_id: m.author_id,
                author_username: self
                    .db
                    .user_by_id(m.author_id)
                    .map(|(_, n)| n)
                    .unwrap_or_default(),
                content: m.content,
                created_at: m.created_at,
                edited_at: m.edited_at,
                mentioned_user_ids: m.mentioned_user_ids,
                pinned_by,
            })
            .collect())
    }

    /// The number of pinned messages in `channel_id`. Any member of the
    /// channel's server may read it (this backs the header pin-count badge).
    ///
    /// # Errors
    /// - `channel not found` — the id doesn't exist.
    /// - `permission denied: not a member of this server` — no membership.
    pub fn pinned_count(&self, caller_id: i64, channel_id: i64) -> Result<i64, String> {
        let (_, server_id, _, _, _, _) = self
            .db
            .channel_full(channel_id)
            .ok_or("channel not found")?;
        if self.db.membership_role(server_id, caller_id).is_none() {
            return Err("permission denied: not a member of this server".into());
        }
        Ok(self.db.pinned_count(channel_id))
    }

    /// The current wall-clock milliseconds since the Unix epoch, for
    /// `read_at` timestamps (the `messages.created_at` convention).
    fn now_millis() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(1_000_000)
    }
}

// ── #[server] endpoints ─────────────────────────────────────────────────────

/// Rename a server. Only the current owner may call (owner-only by policy).
///
/// The acting user is read from the `x-user-id` request header.
#[server]
pub async fn rename_server(input: RenameServerInput) -> Result<ServerSettingsResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .rename(caller, input.server_id, &input.new_name)
        .map_err(ServerError::failed)
}

/// Set (or clear) a server's icon. Owner or admin may call.
///
/// `icon` is a URL string, or `null` to clear the icon.
#[server]
pub async fn set_server_icon(input: SetServerIconInput) -> Result<ServerSettingsResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .set_icon(caller, input.server_id, input.icon.as_deref())
        .map_err(ServerError::failed)
}

/// Transfer server ownership to another member. Only the current owner may call.
#[server]
pub async fn transfer_server_ownership(input: TransferOwnershipInput) -> Result<ServerSettingsResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .transfer_ownership(caller, input.server_id, input.new_owner_id)
        .map_err(ServerError::failed)
}

// ── channel management endpoints ────────────────────────────────────────────

/// Create a channel in a server. Owner or admin may call.
#[server]
pub async fn create_channel(input: CreateChannelInput) -> Result<ChannelResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let kind = ChannelKind::from_db_str(&input.kind)
        .ok_or_else(|| ServerError::failed(format!("unknown channel kind: {}", input.kind)))?;
    let id = ServerSettings::new(db)
        .create_channel(caller, input.server_id, &input.name, kind, input.parent_channel_id)
        .map_err(ServerError::failed)?;
    Ok(ChannelResult { channel_id: id, server_id: input.server_id, name: Some(input.name) })
}

/// Rename a channel. Owner, admin, or moderator may call.
#[server]
pub async fn rename_channel(input: RenameChannelInput) -> Result<ChannelResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let name = ServerSettings::new(db)
        .rename_channel(caller, input.server_id, input.channel_id, &input.new_name)
        .map_err(ServerError::failed)?;
    Ok(ChannelResult { channel_id: input.channel_id, server_id: input.server_id, name: Some(name) })
}

/// Delete a channel. Owner or admin may call.
#[server]
pub async fn delete_channel(input: DeleteChannelInput) -> Result<ChannelResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .delete_channel(caller, input.server_id, input.channel_id)
        .map_err(ServerError::failed)?;
    Ok(ChannelResult { channel_id: input.channel_id, server_id: input.server_id, name: None })
}

// ── member management endpoints ─────────────────────────────────────────────

/// Add (or re-role) a member in a server. Moderator+ to add; admin+ to grant
/// an admin role. The owner role is not assignable here.
#[server]
pub async fn add_member(input: AddMemberInput) -> Result<MemberResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let role = Role::from_db_str(&input.role)
        .ok_or_else(|| ServerError::failed(format!("unknown role: {}", input.role)))?;
    let role_str = ServerSettings::new(db)
        .add_member(caller, input.server_id, input.user_id, role)
        .map_err(ServerError::failed)?;
    Ok(MemberResult { server_id: input.server_id, user_id: input.user_id, role: role_str })
}

/// Remove a member from a server. Owner, admin, or moderator may call. The
/// server owner can't be removed.
#[server]
pub async fn remove_member(input: RemoveMemberInput) -> Result<MemberResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .remove_member(caller, input.server_id, input.user_id)
        .map_err(ServerError::failed)?;
    Ok(MemberResult { server_id: input.server_id, user_id: input.user_id, role: String::new() })
}

/// Leave a server. Any member except the owner may.
#[server]
pub async fn leave_server(input: LeaveServerInput) -> Result<(), ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .leave(caller, input.server_id)
        .map_err(ServerError::failed)
}

/// Ban a member from a server: remove them AND bar rejoining (invite
/// redemption or a direct add). Owner or admin may call (a ban clears a
/// higher bar than a kick). The server owner can't be banned. `reason` is an
/// optional free-text note attached to the ban record.
#[server]
pub async fn ban_member(input: BanMemberInput) -> Result<MemberResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .ban_member(caller, input.server_id, input.user_id, input.reason.as_deref())
        .map_err(ServerError::failed)?;
    Ok(MemberResult { server_id: input.server_id, user_id: input.user_id, role: String::new() })
}

/// Time out a member of a server: silence them from posting messages for
/// `duration_secs` without removing their membership (a kick/ban drop the row;
/// a timeout only sets a deadline they can post again after). Owner or admin
/// may call (the same admin+ bar as a ban). The server owner can't be timed
/// out. A re-timeout overwrites the existing deadline in place.
#[server]
pub async fn timeout_member(input: TimeoutMemberInput) -> Result<MemberResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .timeout_member(caller, input.server_id, input.user_id, input.duration_secs)
        .map_err(ServerError::failed)?;
    Ok(MemberResult { server_id: input.server_id, user_id: input.user_id, role: String::new() })
}

/// Change an existing member's role. Owner or admin may call. The owner's
/// role can't be changed and the owner role can't be assigned.
#[server]
pub async fn set_member_role(input: SetMemberRoleInput) -> Result<MemberResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let role = Role::from_db_str(&input.role)
        .ok_or_else(|| ServerError::failed(format!("unknown role: {}", input.role)))?;
    let role_str = ServerSettings::new(db)
        .set_member_role(caller, input.server_id, input.user_id, role)
        .map_err(ServerError::failed)?;
    Ok(MemberResult { server_id: input.server_id, user_id: input.user_id, role: role_str })
}

// ── channel reads / unread counts ───────────────────────────────────────────

/// Mark a channel read up to a message id for the caller. Any member of the
/// owning server may call; the write is monotonic so it never regresses.
#[server]
pub async fn mark_read(input: MarkReadInput) -> Result<MarkReadResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .mark_read(caller, input.channel_id, input.message_id)
        .map_err(ServerError::failed)?;
    Ok(MarkReadResult {
        channel_id: input.channel_id,
        message_id: input.message_id,
    })
}

/// Unread message counts, per channel, for the caller across a server. Every
/// channel the caller is a member of is included, even when its count is 0.
#[server]
pub async fn unread_counts(input: UnreadCountsInput) -> Result<Vec<UnreadCount>, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .unread_counts(caller, input.server_id)
        .map_err(ServerError::failed)
}

/// List the members of a server with their roles. Any member of the server
/// may call; the list is ordered by role (owner first), then user id.
#[server]
pub async fn list_members(input: ListMembersInput) -> Result<Vec<MemberInfo>, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .list_members(caller, input.server_id)
        .map_err(ServerError::failed)
}

// ── message pins ─────────────────────────────────────────────────────────────
//
// Pin/unpin a message in a channel (moderator+), and read a channel's pinned
// view + count (any member). The acting user comes from the verified session
// token; the channel is resolved from the message on the pin/unpin path.

/// Pin a message in its channel. Owner, admin, or moderator may call
/// (Discord "Manage Messages" parity). Idempotent.
#[server]
pub async fn pin_message(input: PinInput) -> Result<PinResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .pin_message(caller, input.message_id)
        .map_err(ServerError::failed)
}

/// Unpin a message from its channel. Same moderator+ bar as `pin_message`.
/// Idempotent — unpinning an unpinned message is a no-op.
#[server]
pub async fn unpin_message(input: PinInput) -> Result<PinResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .unpin_message(caller, input.message_id)
        .map_err(ServerError::failed)
}

// ── message reactions ─────────────────────────────────────────────────────
//
// Add/remove an emoji reaction on a channel message (any member — unlike the
// moderator+ pin bar, Discord lets every member react), persisting a
// `(message, user, emoji)` row in `message_reactions` and broadcasting the
// post-state count to the channel's server audience over the gateway
// (`WsServerMsg::ReactionAdd` / `ReactionRemove`). The acting user comes from
// the verified session token, never from the request.

/// React to a message with an emoji. Any member of the message's channel's
/// server may call (Discord parity: reacting needs no elevated role). Idempotent
/// for a repeat add; the result's `count` is the message's total for that emoji
/// after the operation.
#[server]
pub async fn add_reaction(input: ReactionInput) -> Result<ReactionResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let (channel_id, count) =
        crate::realtime::add_channel_reaction(&db, caller, input.message_id, &input.emoji)
            .map_err(ServerError::failed)?;
    Ok(
        ReactionResult {
            message_id: input.message_id,
            channel_id,
            emoji: input.emoji,
            count,
            added: true,
        }
    )
}

/// Drop the caller's own emoji reaction from a message. Same member bar as
/// `add_reaction`; idempotent (removing a reaction that is not there is a
/// no-op). `count` is the message's total for the emoji after the removal —
/// 0 means the last one was dropped.
#[server]
pub async fn remove_reaction(input: ReactionInput) -> Result<ReactionResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let (channel_id, count) =
        crate::realtime::remove_channel_reaction(&db, caller, input.message_id, &input.emoji)
            .map_err(ServerError::failed)?;
    Ok(
        ReactionResult {
            message_id: input.message_id,
            channel_id,
            emoji: input.emoji,
            count,
            added: false,
        }
    )
}

/// Every pinned message in a channel, in pin order (oldest first). Any member
/// of the owning server may read the view.
#[server]
pub async fn list_pinned(input: PinnedChannelInput) -> Result<Vec<PinnedMessage>, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .list_pinned(caller, input.channel_id)
        .map_err(ServerError::failed)
}

/// The number of pinned messages in a channel (backs the header pin badge).
/// Any member of the owning server may read it.
#[server]
pub async fn pinned_count(input: PinnedChannelInput) -> Result<i64, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .pinned_count(caller, input.channel_id)
        .map_err(ServerError::failed)
}

/// Pull the shared `Db` out of server state, installed at startup by the
/// server bin via `server::install_state(db)`. `Db` is cheap to clone (it's
/// just an `Arc<Mutex<Connection>>`), so each request gets its own handle.
#[cfg(feature = "server")]
fn db_from_state() -> Result<Db, ServerError> {
    server::use_state::<Db>()
        .ok_or_else(|| ServerError::failed("database not installed"))
}

// ── wire types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct RenameServerInput {
    pub server_id: i64,
    pub new_name: String,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct SetServerIconInput {
    pub server_id: i64,
    /// A URL string, or `None` to clear the icon.
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct TransferOwnershipInput {
    pub server_id: i64,
    pub new_owner_id: i64,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct CreateChannelInput {
    pub server_id: i64,
    pub name: String,
    /// `'text'` / `'voice'` / `'category'`.
    pub kind: String,
    /// Parent (category) channel id, or `None` for top-level.
    #[serde(default)]
    pub parent_channel_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct RenameChannelInput {
    pub server_id: i64,
    pub channel_id: i64,
    pub new_name: String,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct DeleteChannelInput {
    pub server_id: i64,
    pub channel_id: i64,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct AddMemberInput {
    pub server_id: i64,
    pub user_id: i64,
    /// `'owner'` / `'admin'` / `'moderator'` / `'member'` (owner rejected).
    pub role: String,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct LeaveServerInput {
    pub server_id: i64,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct RemoveMemberInput {
    pub server_id: i64,
    pub user_id: i64,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct BanMemberInput {
    pub server_id: i64,
    pub user_id: i64,
    /// Optional free-text note stored on the ban record.
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct TimeoutMemberInput {
    pub server_id: i64,
    pub user_id: i64,
    /// How long to silence the member for, in seconds from the moment the
    /// timeout is applied. Must be positive; the server rejects non-positive
    /// values at the service boundary.
    pub duration_secs: i64,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct SetMemberRoleInput {
    pub server_id: i64,
    pub user_id: i64,
    pub role: String,
}

/// The id of the newest message the caller has read in a channel. The caller's
/// read state is advanced to (at least) this message id.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct MarkReadInput {
    pub channel_id: i64,
    pub message_id: i64,
}

/// The server whose channels the caller wants unread counts for.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct UnreadCountsInput {
    pub server_id: i64,
}

/// The server whose member list the caller wants.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct ListMembersInput {
    pub server_id: i64,
}

/// One member of a server: their id, display name, stored role string
/// (`'owner'` / `'admin'` / `'moderator'` / `'member'`), and avatar URL
/// (a `data:image/…;base64,` string, `None` = default initial circle).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct MemberInfo {
    pub user_id: i64,
    pub username: String,
    pub role: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

/// Confirmation that a channel was marked read: the channel and the message id
/// the read state was advanced to.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct MarkReadResult {
    pub channel_id: i64,
    pub message_id: i64,
}

/// The result of a channel-management operation. `name` is the channel's name
/// after the op (absent when the channel was deleted, or on a create where the
/// client already knows the name).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ChannelResult {
    pub channel_id: i64,
    pub server_id: i64,
    pub name: Option<String>,
}

/// The result of a member-management operation. `role` is the member's role
/// after the op (empty string when the member was removed).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct MemberResult {
    pub server_id: i64,
    pub user_id: i64,
    pub role: String,
}

/// One entry in a per-channel unread list for the caller: how many messages
/// the caller has not yet read in a single channel (`count` — every message
/// whose id is greater than the caller's stored
/// `channel_reads.last_read_message_id`), and how many of those unread
/// messages mention the caller by name (`unread_mentions` — a resolved
/// `@mention` row in `message_mentions` on an unread message).
///
/// `unread_mentions` is always `<= count`. A channel is fully read when both
/// are 0; `count > 0 && unread_mentions == 0` means "new messages, none of
/// them mention me". Every channel the caller is a member of appears in the
/// list, even when both counts are 0 (so the client can distinguish "0
/// unread" from "no channel").
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct UnreadCount {
    pub channel_id: i64,
    pub count: i64,
    #[serde(default)]
    pub unread_mentions: i64,
}

// ── message pins ─────────────────────────────────────────────────────────────

/// A single message to pin or unpin. The channel is resolved from the message
/// (a message belongs to exactly one channel), so the caller only supplies the
/// message id — there is no separate `channel_id` to keep in sync.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct PinInput {
    pub message_id: i64,
}

/// The channel whose pinned messages (or pin count) the caller wants.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct PinnedChannelInput {
    pub channel_id: i64,
}

/// The outcome of a pin or unpin. `pinned` is `true` when the message is now
/// pinned (the pin succeeded, or it was already pinned) and `false` when it is
/// now *not* pinned (an unpin). `pinned_count` is the channel's total pinned
/// messages after the operation — the client updates its header badge from this
/// without a second round trip.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct PinResult {
    pub message_id: i64,
    pub channel_id: i64,
    pub pinned: bool,
    pub pinned_count: i64,
}

/// Which message the caller (the session-token user) is adding or dropping one
/// emoji reaction on. The message's channel is resolved server-side from the
/// stored row, like the pin endpoints.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct ReactionInput {
    pub message_id: i64,
    pub emoji: String,
}

/// The outcome of an add/remove reaction. `count` is the message's total for
/// `emoji` AFTER the operation; `added` reports which way the call went.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ReactionResult {
    pub message_id: i64,
    pub channel_id: i64,
    pub emoji: String,
    pub count: u32,
    pub added: bool,
}

/// One entry in a channel's pinned view: the full message plus who pinned it.
/// `mentioned_user_ids` is populated (resolved `@mention` ids), exactly as in
/// [`crate::realtime`]'s history path, so a pinned view can render mentions.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct PinnedMessage {
    pub message_id: i64,
    pub channel_id: i64,
    pub author_id: i64,
    pub author_username: String,
    pub content: String,
    pub created_at: i64,
    pub edited_at: Option<i64>,
    #[serde(default)]
    pub mentioned_user_ids: Vec<i64>,
    /// The user id of whoever pinned this message (not necessarily its author).
    pub pinned_by: i64,
}

// ── tests ────────────────────────────────────────────────────────────────────
//
// The tests exercise the owner-only policy directly against the `ServerSettings`
// service (no HTTP / no server state needed). A rejection case is included for
// each mutation.

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use crate::db::Db;
    use std::path::Path;
    // The raw-tokio-TCP e2e clients (the `#[server]` HTTP round-trip tests)
    // call `.write_all` / `.read_to_end` on the split socket halves; those come
    // from the async io extension traits (in scope here via tokio's feature
    // unification with tokio-tungstenite, but still needing the `use`).
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A boxed future — used by the raw-HTTP e2e client below so the closure
    /// can name its return type without a `-> impl Future` annotation.
    type BoxFuture<T> = std::pin::Pin<
        Box<dyn std::future::Future<Output = T> + Send>,
    >;

    /// Build a temp-file database, run migrations, seed one server with an
    /// owner (user 1) and two members (users 2 and 3), and return the `Db`.
    fn setup_db() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let db = Db::migrate(&db_path, &migrations_dir).unwrap();

        // Seed users + one server + memberships directly via raw SQL
        // (the db layer doesn't expose a "create user" API yet).
        let c = db.lock().unwrap();
        c.execute_batch("
            INSERT INTO users (id, username, email, created_at) VALUES
                (1, 'owner',  'owner@example.com',  1000),
                (2, 'alice',  'alice@example.com',  1000),
                (3, 'bob',    'bob@example.com',    1000);
            INSERT INTO servers (id, name, owner_id, icon_url, created_at)
                VALUES (1, 'My Server', 1, NULL, 1000);
            INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES
                (1, 1, 'owner',  1000),
                (1, 2, 'member', 1000),
                (1, 3, 'member', 1000);
        ").unwrap();
        drop(c);

        (dir, db)
    }

    #[test]
    fn a_member_can_leave_but_the_owner_cannot() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db.clone());
        svc.leave(2, 1).unwrap();
        assert_eq!(db.membership_role(1, 2), None);
        assert_eq!(svc.leave(2, 1).unwrap_err(), "not a member of this server");
        assert_eq!(svc.leave(1, 1).unwrap_err(), "the owner can't leave (transfer ownership first)");
        assert_eq!(db.membership_role(1, 1).as_deref(), Some("owner"));
        assert_eq!(svc.leave(3, 99).unwrap_err(), "server not found");
    }

    /// On top of `setup_db`, seed two text channels (ids 1 and 2) into server 1
    /// and return `(dir, db)`. Channel ids are chosen explicitly so the tests
    /// can address them by literal.
    fn setup_db_with_channels() -> (tempfile::TempDir, Db) {
        let (dir, db) = setup_db();
        let c = db.lock().unwrap();
        c.execute_batch(
            "INSERT INTO channels (id, server_id, name, kind, position, parent_channel_id, created_at)
             VALUES (1, 1, 'general', 'text', 0, NULL, 1000),
                    (2, 1, 'random',  'text', 1, NULL, 1000);",
        )
        .unwrap();
        drop(c);
        (dir, db)
    }

    /// Post `count` messages into `channel_id` authored by `author_id`,
    /// returning the id of the most recent one (ids are strictly increasing).
    fn post_messages(db: &Db, channel_id: i64, author_id: i64, count: i64) -> i64 {
        let mut last = 0i64;
        for i in 0..count {
            last = db
                .insert_message(channel_id, author_id, &format!("msg {i}"), 1000 + i, None)
                .unwrap();
        }
        last
    }

    /// Look up one channel's unread count in an `unread_counts` result,
    /// panicking if the channel is absent (the contract is that every member
    /// channel appears, even with count 0).
    fn count_for(counts: &[UnreadCount], channel_id: i64) -> i64 {
        counts
            .iter()
            .find(|c| c.channel_id == channel_id)
            .unwrap_or_else(|| panic!("channel {channel_id} missing from unread list: {counts:?}"))
            .count
    }

    #[test]
    fn owner_can_rename() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db);
        let res = svc.rename(1, 1, "New Name").unwrap();
        assert_eq!(res.name, "New Name");
        assert_eq!(res.owner_id, 1);
    }

    #[test]
    fn non_owner_rename_rejected() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db.clone());
        let err = svc.rename(2, 1, "Hacked").unwrap_err();
        assert!(err.contains("permission denied"), "got: {err}");
        // Name unchanged.
        assert_eq!(db.server_by_id(1).unwrap().1, "My Server");
    }

    #[test]
    fn non_member_rename_rejected() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db);
        // User 999 is not in the server at all.
        let err = svc.rename(999, 1, "Hacked").unwrap_err();
        assert!(err.contains("not a member"), "got: {err}");
    }

    #[test]
    fn empty_name_rejected() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db);
        let err = svc.rename(1, 1, "   ").unwrap_err();
        assert!(err.contains("must not be empty"), "got: {err}");
    }

    #[test]
    fn owner_can_set_icon() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db);
        let res = svc.set_icon(1, 1, Some("data:image/webp;base64,UklGRjgAAABXRUJQVlA4ICwAAADQAQCdASoIAAgAAgA0JaACdLoB+AADsAD+8FMf/yz5/DZ1Of/Cr43IWFAAAA==")).unwrap();
        assert!(res.icon_url.is_some() && res.icon_url.expect("path").starts_with("/media/"), "icon stored as media path");
    }

    #[test]
    fn owner_can_clear_icon() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db.clone());
        svc.set_icon(1, 1, Some("data:image/webp;base64,UklGRjgAAABXRUJQVlA4ICwAAADQAQCdASoIAAgAAgA0JaACdLoB+AADsAD+8FMf/yz5/DZ1Of/Cr43IWFAAAA==")).unwrap();
        let res = svc.set_icon(1, 1, None).unwrap();
        assert!(res.icon_url.is_none());
    }

    #[test]
    fn non_owner_set_icon_rejected() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db.clone());
        let err = svc.set_icon(2, 1, Some("https://evil.example.com/x.png")).unwrap_err();
        assert!(err.contains("permission denied"), "got: {err}");
        // Icon unchanged.
        assert!(db.server_by_id(1).unwrap().3.is_none());
    }

    #[test]
    fn owner_can_transfer_ownership() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db.clone());
        let res = svc.transfer_ownership(1, 1, 2).unwrap();
        assert_eq!(res.owner_id, 2, "owner should now be user 2");
        // Verify roles: old owner demoted to admin, new owner promoted.
        assert_eq!(db.membership_role(1, 1).as_deref(), Some("admin"));
        assert_eq!(db.membership_role(1, 2).as_deref(), Some("owner"));
    }

    #[test]
    fn non_owner_transfer_rejected() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db.clone());
        let err = svc.transfer_ownership(2, 1, 3).unwrap_err();
        assert!(err.contains("permission denied"), "got: {err}");
        // Ownership unchanged.
        assert_eq!(db.server_by_id(1).unwrap().2, 1, "owner should still be user 1");
    }

    #[test]
    fn transfer_to_non_member_rejected() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db.clone());
        // User 999 is not a member.
        let err = svc.transfer_ownership(1, 1, 999).unwrap_err();
        assert!(err.contains("not a member"), "got: {err}");
        // Ownership unchanged.
        assert_eq!(db.server_by_id(1).unwrap().2, 1);
    }

    #[test]
    fn transfer_to_self_rejected_by_membership_guard() {
        // Transferring to oneself: the owner is already a member, so the
        // membership guard passes. But the old-owner→admin demotion would run
        // and then the self→owner promotion would re-set it. The net effect
        // is a no-op with owner_id unchanged. Verify it doesn't error and
        // the owner stays the same.
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db);
        let res = svc.transfer_ownership(1, 1, 1).unwrap();
        assert_eq!(res.owner_id, 1);
    }

    #[test]
    fn unknown_server_rejected() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db);
        let err = svc.rename(1, 999, "X").unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
    }

    // ── member list ──────────────────────────────────────────────────────────

    /// A member can read the list; owner comes first, then admin/mod/member,
    /// then user id within a role.
    #[test]
    fn member_list_ordered_by_role_then_user_id() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        // Seed a moderator and another member with a higher user id than bob.
        let c = db.lock().unwrap();
        c.execute_batch(
            "INSERT INTO users (id, username, email, created_at) VALUES (4, 'dave', 'dave@example.com', 1000);
             UPDATE server_memberships SET role = 'moderator' WHERE server_id = 1 AND user_id = 3;
             INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES (1, 4, 'member', 1000);",
        )
        .unwrap();
        drop(c);

        let members = svc.list_members(2, 1).unwrap();
        let got: Vec<(i64, &str)> = members
            .iter()
            .map(|m| (m.user_id, m.role.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![(1, "owner"), (3, "moderator"), (2, "member"), (4, "member")],
            "owner first, then moderator, then members by user id"
        );
        // The row for alice carries her username, not an empty placeholder.
        let alice = members.iter().find(|m| m.user_id == 2).unwrap();
        assert_eq!(alice.username, "alice");
    }

    #[test]
    fn member_list_rejects_unknown_server() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db);
        let err = svc.list_members(1, 999).unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
    }

    #[test]
    fn member_list_rejects_non_member() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db);
        // User 999 has no membership in server 1.
        let err = svc.list_members(999, 1).unwrap_err();
        assert!(err.contains("not a member"), "got: {err}");
    }

    // ── member management: ban ───────────────────────────────────────────────
    //
    // A ban removes a member AND bars rejoining. It clears a higher bar than a
    // kick (admin+ vs moderator+), and the rejoin block is asserted through
    // `add_member` (the other rejoin path, `redeem_invite`, is covered by its
    // own e2e test in `invites`).

    /// Give user 3 the admin role in server 1 so we can exercise the
    /// admin-can-ban path (the default fixture only has one member, bob).
    fn promote_user_to_admin(db: &Db, user_id: i64) {
        let c = db.lock().unwrap();
        c.execute(
            "UPDATE server_memberships SET role = 'admin' WHERE server_id = 1 AND user_id = ?1",
            rusqlite::params![user_id],
        )
        .unwrap();
        drop(c);
    }

    #[test]
    fn owner_can_ban_member() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db.clone());
        // Owner (1) bans alice (2).
        svc.ban_member(1, 1, 2, Some("spam")).unwrap();
        // The membership row is gone (they're kicked out)…
        assert_eq!(db.membership_role(1, 2), None, "banned member is removed");
        // …and a ban row is recorded (rejoining is barred).
        assert!(db.is_banned(1, 2), "a ban row must be recorded");
    }

    #[test]
    fn admin_can_ban_member() {
        let (_dir, db) = setup_db();
        promote_user_to_admin(&db, 3);
        let svc = ServerSettings::new(db.clone());
        // An admin (3) may ban a fellow member (2).
        svc.ban_member(3, 1, 2, None).unwrap();
        assert!(db.is_banned(1, 2));
    }

    #[test]
    fn moderator_cannot_ban() {
        let (_dir, db) = setup_db();
        // Give bob (3) the moderator role — a kick is allowed, a ban is not.
        let c = db.lock().unwrap();
        c.execute(
            "UPDATE server_memberships SET role = 'moderator' WHERE server_id = 1 AND user_id = 3",
            [],
        )
        .unwrap();
        drop(c);
        let svc = ServerSettings::new(db.clone());
        let err = svc.ban_member(3, 1, 2, None).unwrap_err();
        assert!(
            err.contains("permission denied"),
            "a moderator may kick but not ban; got: {err}"
        );
        // alice is untouched.
        assert_eq!(db.membership_role(1, 2), Some("member".into()));
        assert!(!db.is_banned(1, 2));
    }

    #[test]
    fn cannot_ban_the_owner() {
        let (_dir, db) = setup_db();
        // Give user 3 the admin role so a ban would otherwise be permitted,
        // then have them try to ban the owner (1): refused.
        promote_user_to_admin(&db, 3);
        let svc = ServerSettings::new(db);
        let err = svc.ban_member(3, 1, 1, None).unwrap_err();
        assert!(err.contains("cannot ban the server owner"), "got: {err}");
    }

    #[test]
    fn banning_a_non_member_is_rejected() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db.clone());
        // User 999 has no membership row: a ban has nothing to remove, so it's
        // a no-op rejection rather than a phantom ban row.
        let err = svc.ban_member(1, 1, 999, None).unwrap_err();
        assert!(err.contains("member not found"), "got: {err}");
        assert!(!db.is_banned(1, 999), "no ban row is written for a non-member");
    }

    #[test]
    fn banned_member_cannot_be_readded() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db.clone());
        // Ban alice (2), then the owner tries to re-add her directly.
        svc.ban_member(1, 1, 2, Some("spam")).unwrap();
        let err = svc.add_member(1, 1, 2, Role::Member).unwrap_err();
        assert!(err.contains("banned"), "the ban blocks a direct re-add; got: {err}");
        // Still not a member.
        assert_eq!(db.membership_role(1, 2), None);
    }

    #[test]
    fn unban_restores_rejoin() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db.clone());
        svc.ban_member(1, 1, 2, None).unwrap();
        assert!(db.is_banned(1, 2));
        // Unban, then the owner may re-add.
        db.unban(1, 2).unwrap();
        assert!(!db.is_banned(1, 2));
        let role = svc.add_member(1, 1, 2, Role::Member).unwrap();
        assert_eq!(role, "member");
        assert_eq!(db.membership_role(1, 2), Some("member".into()));
    }

    // ── channel reads / unread counts ───────────────────────────────────────
    //
    // Service-level tests for the per-channel read state: marking a channel
    // read, the unread-count computation, and the membership/authz guards.
    // The e2e test that drives the real `#[server]` HTTP routes lives at the
    // bottom of this module with the other gateway round-trip tests.

    #[test]
    fn mark_read_creates_state_and_is_idempotent() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        let last = post_messages(&db, 1, 1, 3); // 3 messages in channel 1

        svc.mark_read(1, 1, last).unwrap();
        // The read state was persisted to the newest message id.
        assert_eq!(db.last_read_message_id(1, 1), Some(last));

        // Marking the same channel read again with the same id is a no-op that
        // doesn't error and doesn't move the pointer.
        svc.mark_read(1, 1, last).unwrap();
        assert_eq!(db.last_read_message_id(1, 1), Some(last));
    }

    #[test]
    fn mark_read_with_no_id_reads_up_to_the_newest_message() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        // An empty channel: nothing to read, and no error.
        svc.mark_read(1, 1, 0).unwrap();
        assert_eq!(db.last_read_message_id(1, 1), None);
        let last = post_messages(&db, 1, 1, 4);
        svc.mark_read(1, 1, 0).unwrap();
        assert_eq!(db.last_read_message_id(1, 1), Some(last));
    }

    #[test]
    fn mark_read_is_monotonic_not_backward() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        let last = post_messages(&db, 1, 1, 5);

        // Read up to the newest, then attempt to mark read at an OLDER id.
        svc.mark_read(1, 1, last).unwrap();
        svc.mark_read(1, 1, 1).unwrap(); // id 1 < last
        // The pointer must not regress.
        assert_eq!(db.last_read_message_id(1, 1), Some(last));
    }

    #[test]
    fn mark_read_per_user_isolated() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        let last = post_messages(&db, 1, 1, 3);

        svc.mark_read(1, 1, last).unwrap();
        // User 2 has not read anything yet.
        assert_eq!(db.last_read_message_id(2, 1), None);
        // User 2 marking read is independent.
        svc.mark_read(2, 1, 1).unwrap();
        assert_eq!(db.last_read_message_id(1, 1), Some(last));
        assert_eq!(db.last_read_message_id(2, 1), Some(1));
    }

    #[test]
    fn mark_read_rejects_unknown_channel() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db);
        let err = svc.mark_read(1, 999, 1).unwrap_err();
        assert!(err.contains("channel not found"), "got: {err}");
    }

    #[test]
    fn mark_read_rejects_non_member() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        // User 999 has no membership in server 1, so they can't touch its
        // channel 1 even though the channel exists.
        let err = svc.mark_read(999, 1, 1).unwrap_err();
        assert!(err.contains("not a member"), "got: {err}");
        // No read state was written.
        assert_eq!(db.last_read_message_id(999, 1), None);
    }

    #[test]
    fn unread_counts_full_when_unread() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        post_messages(&db, 1, 1, 4); // channel 1: 4 msgs
        post_messages(&db, 2, 2, 2); // channel 2: 2 msgs

        // User 1 has read nothing → every message in every channel is unread.
        let counts = svc.unread_counts(1, 1).unwrap();
        assert_eq!(counts.len(), 2, "both member channels are listed");
        assert_eq!(count_for(&counts, 1), 4);
        assert_eq!(count_for(&counts, 2), 2);
    }

    #[test]
    fn unread_counts_zero_after_read() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        let last1 = post_messages(&db, 1, 1, 4);
        let last2 = post_messages(&db, 2, 2, 2);

        svc.mark_read(1, 1, last1).unwrap();
        svc.mark_read(1, 2, last2).unwrap();
        let counts = svc.unread_counts(1, 1).unwrap();
        assert_eq!(count_for(&counts, 1), 0);
        assert_eq!(count_for(&counts, 2), 0);
    }

    #[test]
    fn unread_counts_partial_read_counts_only_newer() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        let last = post_messages(&db, 1, 1, 5); // ids 1..=5
        // Read only up to id 2 → messages 3, 4, 5 remain unread.
        svc.mark_read(1, 1, 2).unwrap();
        let counts = svc.unread_counts(1, 1).unwrap();
        assert_eq!(count_for(&counts, 1), 3);
        assert_eq!(count_for(&counts, 2), 0, "channel 2 has no messages");
        // Sanity: the read pointer really is at id 2.
        assert_eq!(db.last_read_message_id(1, 1), Some(2));
        // And marking read up to the newest zeroes it out.
        svc.mark_read(1, 1, last).unwrap();
        assert_eq!(count_for(&svc.unread_counts(1, 1).unwrap(), 1), 0);
    }

    #[test]
    fn unread_counts_empty_channel_is_zero_not_absent() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db);
        // No messages anywhere, no read state. Both channels must still appear,
        // each with count 0 — the client distinguishes "0 unread" from
        // "no such channel".
        let counts = svc.unread_counts(1, 1).unwrap();
        assert_eq!(counts.len(), 2);
        assert_eq!(count_for(&counts, 1), 0);
        assert_eq!(count_for(&counts, 2), 0);
    }

    #[test]
    fn unread_counts_rejects_unknown_server() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db);
        let err = svc.unread_counts(1, 999).unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
    }

    #[test]
    fn unread_counts_rejects_non_member() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db);
        let err = svc.unread_counts(999, 1).unwrap_err();
        assert!(err.contains("not a member"), "got: {err}");
    }

    #[test]
    fn unread_counts_includes_unread_mentions() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());

        // Three messages in channel 1 authored by user 1 (the owner); two of
        // them @mention user 2 (alice). The mention rows land via the live
        // gateway's resolver, exactly as the production path writes them.
        crate::realtime::post_channel_message(&db, 1, 1, "hello @alice and @bob", None, Vec::new()).unwrap();
        crate::realtime::post_channel_message(&db, 1, 1, "no mention here", None, Vec::new()).unwrap();
        crate::realtime::post_channel_message(&db, 1, 1, "again @alice", None, Vec::new()).unwrap();

        // User 2 (alice) has read nothing. The channel has 3 unread messages,
        // and 2 of them mention alice (the "hello" and "again" lines). The
        // "bob" mention in the same first message must NOT be counted for
        // alice, and the mention-free message contributes nothing.
        let counts = svc.unread_counts(2, 1).unwrap();
        let row = counts
            .iter()
            .find(|c| c.channel_id == 1)
            .unwrap_or_else(|| panic!("channel 1 missing: {counts:?}"));
        assert_eq!(row.count, 3, "all three messages are unread for alice");
        assert_eq!(row.unread_mentions, 2, "alice is mentioned in two of them");

        // After alice marks the channel read, both counts drop to zero.
        svc.mark_read(2, 1, 3).unwrap();
        let counts = svc.unread_counts(2, 1).unwrap();
        let row = counts.iter().find(|c| c.channel_id == 1).unwrap();
        assert_eq!(row.count, 0);
        assert_eq!(row.unread_mentions, 0, "mention count follows the read pointer");

        // bob (user 3) is mentioned exactly once (in the "hello" line).
        let counts = svc.unread_counts(3, 1).unwrap();
        let row = counts.iter().find(|c| c.channel_id == 1).unwrap();
        assert_eq!(row.unread_mentions, 1, "bob is mentioned in exactly one message");
    }

    #[test]
    fn unread_counts_isolated_per_user() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        let last = post_messages(&db, 1, 1, 3);

        svc.mark_read(1, 1, last).unwrap(); // user 1 reads up to date
        // User 2, who has not read, still sees all 3 as unread.
        let c1 = svc.unread_counts(1, 1).unwrap();
        let c2 = svc.unread_counts(2, 1).unwrap();
        assert_eq!(count_for(&c1, 1), 0);
        assert_eq!(count_for(&c2, 1), 3);
    }

    // ── message pins ───────────────────────────────────────────────────────
    //
    // Pinning a message is moderator+ (Discord "Manage Messages" parity);
    // reading the pinned view / count is any member. The pin state lives in
    // `message_pins` and the channel is resolved from the message, so these
    // tests exercise: the happy path (owner pins), the authz matrix (member
    // can read but not pin; moderator can pin), idempotence (re-pin / re-unpin
    // are no-ops), pin-order preservation, the count, and the "unknown
    // message" rejection.

    /// Seed user 3 (bob) as a moderator in server 1 so the moderator-can-pin
    /// path can be exercised (the default fixture only has one owner + two
    /// plain members).
    fn make_user3_moderator(db: &Db) {
        let c = db.lock().unwrap();
        c.execute(
            "UPDATE server_memberships SET role = 'moderator' WHERE server_id = 1 AND user_id = 3",
            [],
        )
        .unwrap();
        drop(c);
    }

    #[test]
    fn owner_can_pin_a_message() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        let msg = post_messages(&db, 1, 1, 3); // channel 1: ids 1..=3
        let res = svc.pin_message(1, msg).unwrap();
        assert_eq!(res.message_id, msg);
        assert_eq!(res.channel_id, 1, "the channel is resolved from the message");
        assert!(res.pinned, "pin reports the message as pinned");
        assert_eq!(res.pinned_count, 1, "one message is now pinned in channel 1");
        assert!(db.message_is_pinned(msg), "the pin is persisted");
    }

    #[test]
    fn pin_is_idempotent() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        let msg = post_messages(&db, 1, 1, 2);
        svc.pin_message(1, msg).unwrap();
        // Pinning the same message again is a no-op, not an error, and the
        // count stays at 1 (no duplicate pin row).
        let res = svc.pin_message(1, msg).unwrap();
        assert!(res.pinned);
        assert_eq!(res.pinned_count, 1, "a second pin must not double-count");
        assert_eq!(svc.pinned_count(1, 1).unwrap(), 1);
    }

    #[test]
    fn plain_member_cannot_pin_but_can_read() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        let msg = post_messages(&db, 1, 1, 2);
        // alice (2) is a plain member: pinning is refused…
        let err = svc.pin_message(2, msg).unwrap_err();
        assert!(err.contains("permission denied"), "got: {err}");
        assert!(!db.message_is_pinned(msg), "no pin row was written");
        // …but she may still read the (empty) pinned view and count.
        assert!(svc.list_pinned(2, 1).unwrap().is_empty());
        assert_eq!(svc.pinned_count(2, 1).unwrap(), 0);
    }

    #[test]
    fn moderator_can_pin_and_unpin() {
        let (_dir, db) = setup_db_with_channels();
        make_user3_moderator(&db);
        let svc = ServerSettings::new(db.clone());
        let a = post_messages(&db, 1, 1, 1); // id 1
        let b = post_messages(&db, 1, 1, 1); // id 2
        // bob (3, moderator) pins two messages in order a then b.
        svc.pin_message(3, a).unwrap();
        let res = svc.pin_message(3, b).unwrap();
        assert_eq!(res.pinned_count, 2);
        // The view is in pin order (oldest pin first): a, then b.
        let view = svc.list_pinned(3, 1).unwrap();
        let order: Vec<i64> = view.iter().map(|m| m.message_id).collect();
        assert_eq!(order, vec![a, b], "pinned view preserves pin order");
        // Each entry carries the author's resolved username and who pinned it.
        assert_eq!(view[0].author_username, "owner", "author username resolved");
        assert_eq!(view[0].pinned_by, 3, "pinned_by is the moderator, not the author");
        // Unpin the first: idempotent and reflected in the count + view.
        let res = svc.unpin_message(3, a).unwrap();
        assert!(!res.pinned, "after unpin the message is not pinned");
        assert_eq!(res.pinned_count, 1);
        assert!(!db.message_is_pinned(a));
        let view = svc.list_pinned(3, 1).unwrap();
        assert_eq!(view.iter().map(|m| m.message_id).collect::<Vec<_>>(), vec![b]);
        // Unpinning an already-unpinned message is a no-op, not an error.
        let res = svc.unpin_message(3, a).unwrap();
        assert!(!res.pinned);
        assert_eq!(res.pinned_count, 1);
    }

    #[test]
    fn plain_member_cannot_unpin() {
        let (_dir, db) = setup_db_with_channels();
        make_user3_moderator(&db);
        let svc = ServerSettings::new(db.clone());
        let msg = post_messages(&db, 1, 1, 1);
        svc.pin_message(3, msg).unwrap();
        assert!(db.message_is_pinned(msg));
        // alice (2, plain member) cannot unpin it.
        let err = svc.unpin_message(2, msg).unwrap_err();
        assert!(err.contains("permission denied"), "got: {err}");
        assert!(db.message_is_pinned(msg), "the pin is untouched");
    }

    #[test]
    fn pinned_view_rejects_non_member_and_unknown_channel() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        // A non-member cannot read the view or count.
        assert!(svc.list_pinned(999, 1).unwrap_err().contains("not a member"));
        assert!(svc.pinned_count(999, 1).unwrap_err().contains("not a member"));
        // An unknown channel is rejected even for a member.
        assert!(svc.list_pinned(1, 999).unwrap_err().contains("channel not found"));
        assert!(svc.pinned_count(1, 999).unwrap_err().contains("channel not found"));
    }

    #[test]
    fn pin_unknown_message_rejected() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db);
        // Even the owner cannot pin a message that does not exist.
        let err = svc.pin_message(1, 999_999).unwrap_err();
        assert!(err.contains("message not found"), "got: {err}");
    }

    #[test]
    fn pinned_view_is_per_channel() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        let m1 = post_messages(&db, 1, 1, 1); // channel 1
        let _m2 = post_messages(&db, 2, 1, 1); // channel 2
        svc.pin_message(1, m1).unwrap();
        // Pinning in one channel does not leak into the other.
        assert_eq!(svc.pinned_count(1, 1).unwrap(), 1);
        assert_eq!(svc.pinned_count(1, 2).unwrap(), 0);
        let view2 = svc.list_pinned(1, 2).unwrap();
        assert!(view2.is_empty(), "channel 2 has no pins");
        // Unpinning the channel-1 pin zeroes that channel's count only.
        svc.unpin_message(1, m1).unwrap();
        assert_eq!(svc.pinned_count(1, 1).unwrap(), 0);
    }

    #[tokio::test]
    async fn unread_counts_and_mark_read_over_http() {
        let (_dir, db) = setup_db_with_channels();
        let _guard = e2e_guard();
        server::install_state(db.clone());

        let last1 = post_messages(&db, 1, 1, 3); // channel 1: 3 msgs

        // Boot the macro-mounted router on an ephemeral port.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, server::router()).await;
        });

        // A tiny raw-HTTP client: POST a JSON args-tuple to /_srv/<fn> with the
        // x-user-id auth header, return the parsed `{"Ok":…}`/`{"Err":…}` body.
        // (A raw socket, like the gateway tests, because the `#[server]` HTTP
        // transport isn't in the vendored registry as a dev-dep.)
        let call =
            |path: String, user_id: String, body: String| -> BoxFuture<serde_json::Value> {
                Box::pin(async move {
                    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
                    let (mut reader, mut writer) = stream.into_split();
                    let req = format!(
                        "POST /_srv/{path} HTTP/1.1\r\n\
                         Host: {addr}\r\n\
                         x-user-id: {user_id}\r\n\
                         content-type: application/json\r\n\
                         content-length: {}\r\n\
                         connection: close\r\n\r\n{body}",
                        body.len()
                    );
                    writer.write_all(req.as_bytes()).await.unwrap();
                    let mut buf = Vec::new();
                    reader.read_to_end(&mut buf).await.unwrap();
                    let text = String::from_utf8(buf).unwrap();
                    // Body is everything after the first blank line.
                    let body = text
                        .split_once("\r\n\r\n")
                        .map(|(_, b)| b.to_string())
                        .unwrap_or_default();
                    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
                    v
                })
            };

        // 1. unread_counts before any read: user 1 sees 3 in channel 1, 0 in 2.
        let resp = call(
            "unread_counts".to_string(),
            "1".to_string(),
            serde_json::to_string(&[serde_json::json!({ "server_id": 1 })]).unwrap(),
        )
        .await;
        let counts: Vec<UnreadCount> = resp["Ok"]
            .as_array()
            .cloned()
            .expect("HTTP unread_counts returns Ok(array)")
            .into_iter()
            .map(|v| serde_json::from_value(v).unwrap())
            .collect();
        assert_eq!(count_for(&counts, 1), 3, "all 3 messages unread before read");
        assert_eq!(count_for(&counts, 2), 0);

        // 2. mark_read up to the newest message in channel 1.
        let resp = call(
            "mark_read".to_string(),
            "1".to_string(),
            serde_json::to_string(&[serde_json::json!({
                "channel_id": 1,
                "message_id": last1
            })])
            .unwrap(),
        )
        .await;
        let mark: MarkReadResult = serde_json::from_value(resp["Ok"].clone())
            .expect("HTTP mark_read returns Ok(MarkReadResult)");
        assert_eq!(mark.channel_id, 1);
        assert_eq!(mark.message_id, last1);

        // 3. unread_counts again: channel 1 is now 0.
        let resp = call(
            "unread_counts".to_string(),
            "1".to_string(),
            serde_json::to_string(&[serde_json::json!({ "server_id": 1 })]).unwrap(),
        )
        .await;
        let counts: Vec<UnreadCount> = resp["Ok"]
            .as_array()
            .cloned()
            .unwrap()
            .into_iter()
            .map(|v| serde_json::from_value(v).unwrap())
            .collect();
        assert_eq!(count_for(&counts, 1), 0, "channel 1 is read up to date");
        assert_eq!(count_for(&counts, 2), 0);

        // 4. Unauthenticated (no x-user-id) is rejected by the endpoint.
        let resp = {
            let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            let (mut reader, mut writer) = stream.into_split();
            let body = serde_json::to_string(&[serde_json::json!({ "server_id": 1 })]).unwrap();
            let req = format!(
                "POST /_srv/unread_counts HTTP/1.1\r\nHost: {addr}\r\n\
                 content-type: application/json\r\ncontent-length: {}\r\n\
                 connection: close\r\n\r\n{body}",
                body.len()
            );
            writer.write_all(req.as_bytes()).await.unwrap();
            let mut buf = Vec::new();
            reader.read_to_end(&mut buf).await.unwrap();
            String::from_utf8(buf).unwrap()
        };
        let v: serde_json::Value = resp
            .split_once("\r\n\r\n")
            .map(|(_, b)| b.to_string())
            .unwrap_or_default()
            .parse()
            .unwrap();
        assert!(
            v.get("Err").is_some(),
            "unauthenticated unread_counts must fail, got {v:?}"
        );
    }

    /// Acquire the e2e serialization guard; hold it for the whole test body.
    /// (Poisoned if a prior holder panicked — recover rather than re-panic.)
    /// Delegates to the crate-level [`crate::e2e_lock`] so the HTTP/WS e2e
    /// tests here share ONE lock with the DM e2e test in `dms`.
    fn e2e_guard() -> std::sync::MutexGuard<'static, ()> {
        crate::e2e_lock::guard()
    }
}
