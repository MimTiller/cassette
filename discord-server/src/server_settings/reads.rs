//! Channel read state and unread counts endpoints.

use serde::Deserialize;
use server::{server, ServerError};

#[cfg(feature = "server")]
use crate::permissions::{self, Permissions};

/// The acting user for the current request, resolved from a verified session
/// token (see [`crate::session`]); `None` when unauthenticated.
#[cfg(feature = "server")]
use crate::session::acting_user;
#[cfg(feature = "server")]
use crate::server_settings::{ServerSettings, db_from_state};

#[cfg(feature = "server")]
impl ServerSettings {
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
    /// - `you're not a member of this server` — no membership.
    /// - `failed to mark channel read: …` — the write failed.
    pub fn mark_read(
        &self,
        caller_id: i64,
        channel_id: i64,
        message_id: i64,
    ) -> Result<(), String> {
        // Resolve the owning server from the channel (a channel id maps to
        // exactly one server); membership is then checked against that server.
        self.require_in_channel(caller_id, channel_id, Permissions::VIEW_CHANNEL)?;
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
    /// - `you're not a member of this server` — no membership.
    pub fn unread_counts(
        &self,
        caller_id: i64,
        server_id: i64,
    ) -> Result<Vec<UnreadCount>, String> {
        let (ctx, member) = permissions::member_in(&self.db, server_id, caller_id)?;
        let counts = self.db.unread_counts_for_server(caller_id, server_id);
        Ok(counts
            .into_iter()
            .filter(|(channel_id, _, _)| ctx.in_channel(&member, *channel_id).contains(Permissions::VIEW_CHANNEL))
            .map(|(channel_id, count, unread_mentions)| UnreadCount {
                channel_id,
                count,
                unread_mentions,
            })
            .collect())
    }
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

/// Confirmation that a channel was marked read: the channel and the message id
/// the read state was advanced to.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct MarkReadResult {
    pub channel_id: i64,
    pub message_id: i64,
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

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use crate::server_settings::test_helpers::{setup_db, setup_db_with_channels, post_messages, count_for, e2e_guard, BoxFuture};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
}
