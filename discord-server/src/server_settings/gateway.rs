//! The realtime message gateway: session-token auth, frame types, and the
//! full-duplex `message_gateway` channel handler.

use serde::{Deserialize, Serialize};

#[cfg(feature = "server")]
use crate::permissions::Permissions;
#[cfg(feature = "server")]
use server::ServerError;

#[cfg(feature = "server")]
use crate::server_settings::db_from_state;

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
    /// Pick the user's status: "online", "idle", "dnd" or "invisible".
    SetStatus { status: String },
}

/// The next frame sequence number. EVERY outbound `WsServerMsg` carries a
/// unique `seq` so no two frames delivered to one socket are ever equal: the
/// client watches frames with `switch(move || gateway.incoming().get(), …)`,
/// and `switch` skips a key equal to the last one, so a frame identical to
/// the previous frame (e.g. a second `RolesChanged` for the same server)
/// would be silently dropped and its change never applied. Stamping every
/// frame from one shared counter fixes that for every variant at once;
/// older clients ignore the field through `#[serde(default)]`.
pub fn next_frame_seq() -> u64 {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Outbound frames the server sends.
///
/// Every variant carries `seq`: the frame's position in the process-wide
/// frame stream from [`crate::server_settings::next_frame_seq`], so two
/// frames for the same socket always compare unequal (see the note there).
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
        /// The channel's server, so a client can apply server-wide
        /// notification settings to channels it hasn't loaded.
        #[serde(default)]
        server_id: i64,
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
        /// Link preview cards. A fresh post has none yet; they follow in a
        /// `MessageEmbeds` frame.
        #[serde(default)]
        embeds: Vec<crate::link_previews::Embed>,
        #[serde(default)]
        seq: u64,
    },
    /// A message's link preview cards, after they were fetched or after an
    /// edit changed its links (empty = remove them).
    MessageEmbeds {
        message_id: i64,
        channel_id: i64,
        embeds: Vec<crate::link_previews::Embed>,
        #[serde(default)]
        seq: u64,
    },
    /// A server's custom emoji changed; members refetch them.
    EmojiChanged {
        server_id: i64,
        #[serde(default)]
        seq: u64,
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
    History {
        channel_id: i64,
        has_more: bool,
        #[serde(default)]
        seq: u64,
    },
    /// A direct message pushed to a participant of the DM (always `new`).
    DmMessage {
        message_id: i64,
        dm_id: i64,
        author_id: i64,
        author_username: String,
        #[serde(default)]
        author_avatar: Option<String>,
        content: String,
        created_at: i64,
        #[serde(default)]
        seq: u64,
    },
    /// A DM's members or name changed (or the DM was created). Sent to its
    /// members, including anyone just removed; clients refetch `list_dms`.
    DmChanged {
        dm_id: i64,
        #[serde(default)]
        seq: u64,
    },
    /// A presence status change for a user: 'online' / 'away' / 'offline'.
    Presence {
        user_id: i64,
        status: String,
        #[serde(default)]
        seq: u64,
    },
    /// The user's own picked status ("online" / "idle" / "dnd" /
    /// "invisible"): sent on connect, and to each of their sockets when it
    /// changes.
    StatusChoice {
        status: String,
        #[serde(default)]
        seq: u64,
    },
    /// A channel member started typing. The client shows this transiently
    /// (a few seconds) and does not need an explicit stop event.
    Typing {
        channel_id: i64,
        user_id: i64,
        #[serde(default)]
        seq: u64,
    },
    /// A channel message was edited. The client replaces the stored content
    /// (and its timestamp) for `message_id` with the new values;
    /// `mentioned_user_ids` is the mention list recomputed for the new text.
    MessageEdited {
        message_id: i64,
        channel_id: i64,
        content: String,
        edited_at: i64,
        #[serde(default)]
        mentioned_user_ids: Vec<i64>,
        #[serde(default)]
        seq: u64,
    },
    /// A server's named roles or their holders changed; clients refetch
    /// `server_roles::server_roles`. `seq` differs on every frame: the
    /// client's frame `switch` skips a value equal to the last one, which
    /// would drop a second change to the same server.
    RolesChanged {
        server_id: i64,
        #[serde(default)]
        seq: u64,
    },
    /// A server was renamed or its icon set/cleared; its members refetch the
    /// server list so the rail and sidebar show the new name/icon live.
    ServerSettingsChanged {
        server_id: i64,
        #[serde(default)]
        seq: u64,
    },
    /// A channel message was deleted. The client drops its stored entry for
    /// `message_id`.
    MessageDeleted {
        message_id: i64,
        channel_id: i64,
        #[serde(default)]
        seq: u64,
    },
    /// A member joined (`joined: true`) or left (`joined: false`) the server.
    /// Fired by the join (invite redemption) and leave (kick/ban) paths so the
    /// server's *other* members see the roster change live — the client
    /// re-fetches `list_members` for the active server on receipt (the same
    /// re-fetch the local kick path already triggers). No roster data rides on
    /// the frame; the re-fetch supplies it.
    MemberChanged {
        server_id: i64,
        user_id: i64,
        joined: bool,
        #[serde(default)]
        seq: u64,
    },
    /// A user changed their display name (`None` = show the username).
    /// Sent to everyone who shares a server with them.
    ProfileChanged {
        user_id: i64,
        username: String,
        display_name: Option<String>,
        #[serde(default)]
        seq: u64,
    },
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
        #[serde(default)]
        seq: u64,
    },
    ReactionRemove {
        message_id: i64,
        channel_id: i64,
        user_id: i64,
        emoji: String,
        count: u32,
        #[serde(default)]
        seq: u64,
    },
    /// The friends list changed for the socket's owner: a friend request was
    /// sent / auto-accepted (`action` = 'requested' / 'accepted'), declined or
    /// cancelled, or a friend was removed ('removed'). `friend_id` is the other
    /// user in the pair. The client re-fetches `list_friends` on receipt (the
    /// same re-fetch pattern as `MemberChanged`) — no relationship data rides
    /// on the frame; the re-fetch supplies it.
    FriendChanged {
        friend_id: i64,
        action: String,
        #[serde(default)]
        seq: u64,
    },
    /// A gateway error for a specific operation.
    Error {
        reason: String,
        #[serde(default)]
        seq: u64,
    },
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
    use crate::realtime::{announce_presence, history_for, hub, post_channel_message, channel_audience, refresh_presence};

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

    // Presence: what others see follows the user's picked status and whether
    // they are active (`realtime::shown_presence`); active the moment the
    // socket is authenticated. The client learns its picked status here.
    let mut active = true;
    refresh_presence(&db, user_id, active);
    let _ = ch.send(WsServerMsg::StatusChoice { status: db.status_choice(user_id), seq: next_frame_seq() }).await;

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
            _ = tokio::time::sleep_until(idle_at), if active => {
                active = false;
                refresh_presence(&db, user_id, active);
                continue;
            }
        };
        let Some(Ok(msg)) = frame else {
            break;
        };
        last_activity = tokio::time::Instant::now();
        if !active {
            active = true;
            refresh_presence(&db, user_id, active);
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
                    let _ = ch.send(WsServerMsg::Error { reason, seq: next_frame_seq() }).await;
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
                            seq: next_frame_seq(),
                        })
                        .await;
                    let server_id = db.channel_full(channel_id).map(|c| c.1).unwrap_or(0);
                    for m in messages {
                        let author_username = db
                            .user_by_id(m.author_id)
                            .map(|(_, n)| n)
                            .unwrap_or_default();
                        let _ = ch
                            .send(WsServerMsg::Message {
                                message_id: m.id,
                                channel_id: m.channel_id,
                                server_id,
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
                                embeds: crate::link_previews::embeds_for(&db, m.id),
                                seq: next_frame_seq(),
                                })
                            .await;
                    }
                }
                Err(reason) => {
                    let _ = ch
                        .send(WsServerMsg::Error {
                            reason: reason.to_string(),
                            seq: next_frame_seq(),
                        })
                        .await;
                }
            },
            WsClientMsg::Typing { channel_id } => {
                // Only someone who could post here may say they're typing.
                if crate::realtime::authorize_text_channel(&db, user_id, channel_id, Permissions::SEND_MESSAGES).is_err() {
                    continue;
                }
                let audience: Vec<i64> = channel_audience(&db, channel_id).into_iter().filter(|&id| id != user_id).collect();
                hub.publish(
                    &audience,
                    &WsServerMsg::Typing { channel_id, user_id, seq: next_frame_seq() },
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
                    let _ = ch.send(WsServerMsg::Error { reason, seq: next_frame_seq() }).await;
                }
            }
            WsClientMsg::DeleteMessage { message_id } => {
                if let Err(reason) =
                    crate::realtime::delete_channel_message(&db, user_id, message_id)
                {
                    let _ = ch.send(WsServerMsg::Error { reason, seq: next_frame_seq() }).await;
                }
            }
            WsClientMsg::SetStatus { status } => {
                if db.set_status_choice(user_id, &status).is_ok() {
                    // The user's other tabs show the new pick too.
                    hub.publish(&[user_id], &WsServerMsg::StatusChoice { status, seq: next_frame_seq() });
                    refresh_presence(&db, user_id, active);
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

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    #[test]
    fn seq_default_keeps_old_client_frames_parsing() {
        // A frame JSON as an older server sent it (no `seq` on the wire)
        // must still deserialize for this client: `#[serde(default)]` on
        // every variant's `seq` is what keeps old servers/clients working.
        let legacy = r#"{"MessageDeleted":{"message_id":4,"channel_id":2}}"#;
        let parsed = serde_json::from_str::<WsServerMsg>(legacy).unwrap();
        assert_eq!(parsed, WsServerMsg::MessageDeleted { message_id: 4, channel_id: 2, seq: 0 });
    }

    #[test]
    fn stamped_frames_never_compare_equal() {
        // The client's frame `switch` skips a key equal to the last one, so
        // two consecutive frames must never be `==`: stamping both from the
        // shared counter makes them differ even when every payload field is
        // the same (the second RolesChanged/ProfileChanged for the same
        // target used to be silently dropped).
        let first = WsServerMsg::RolesChanged { server_id: 7, seq: next_frame_seq() };
        let second = WsServerMsg::RolesChanged { server_id: 7, seq: next_frame_seq() };
        assert_ne!(first, second, "same payload, distinct seq");
        let profile_a = WsServerMsg::ProfileChanged {
            user_id: 3,
            username: "alice".to_string(),
            display_name: Some("Ali".to_string()),
            seq: next_frame_seq(),
        };
        let profile_b = WsServerMsg::ProfileChanged {
            user_id: 3,
            username: "alice".to_string(),
            display_name: Some("Ali".to_string()),
            seq: next_frame_seq(),
        };
        assert_ne!(profile_a, profile_b, "identical ProfileChanged frames differ by seq");
    }
}
