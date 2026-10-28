//! Direct messages (DMs): start a private 1:1 conversation between two users
//! and list a user's existing DMs.
//!
//! ## Design
//!
//! Mirrors [`crate::invites`]: the DM logic lives in a pure, DB-backed
//! service ([`DmService`]) that takes the DB handle and the acting user id as
//! plain arguments and returns `Result<T, String>`, so it is fully
//! unit-testable against a temp-file SQLite database with no axum/tokio
//! stack. The `#[server]` fns below are thin adapters: they pull the shared
//! `Db` out of server state, resolve the acting user from the `x-user-id`
//! request header (the same stand-in session mechanism
//! [`crate::server_settings`] uses), and map a service failure to
//! `ServerError::failed`.
//!
//! ## Model
//!
//! A *DM channel* is a row in `dm_channels`; its *participants* are the
//! `(dm_channel_id, user_id)` rows in `dm_participants` (migration
//! `0007_dm_channels.sql`). A DM always has exactly two participants —
//! enforced here, at start.
//!
//! - **Start** — a user begins a DM with another user. Starting is
//!   *idempotent*: if the pair already share a DM, that existing channel is
//!   returned rather than a duplicate (a real client re-opens the same
//!   conversation).
//! - **List** — return every DM the acting user participates in,
//!   most-recently-created first, each with its two participants. (Until DM
//!   messages land, "most recent activity" == most recent creation.)
//! - **Send message** — a participant posts a message into a DM channel they
//!   are a member of; a non-participant is rejected. The message lands in
//!   `dm_messages` (migration `0011_dm_messages.sql`), the DM-specific table
//!   that exists because the guild `messages` table's `channel_id` FKs
//!   `channels(id)` — a DM message's `channel_id` is a `dm_channels.id`, a
//!   different id space.
//! - **List messages** — the most recent messages of a DM channel,
//!   oldest-first (initial history), same participant-only authorization as
//!   sending.

use serde::{Deserialize, Serialize};
use server::{server, ServerError};

// This file is NOT feature-gated so the `#[server]` fns compile to client RPC
// stubs by default (the same shape as `servers.rs` / `auth.rs`); only the
// DB-touching pieces are gated `#[cfg(feature = "server")]`.
#[cfg(feature = "server")]
use crate::db::Db;

/// The acting user for the current request, resolved from a verified session
/// token (see [`crate::session`]); `None` when unauthenticated.
#[cfg(feature = "server")]
use crate::session::acting_user;

/// The current wall-clock milliseconds since the Unix epoch (the schema's
/// timestamp unit; see migration `0007_dm_channels.sql`).
#[cfg(feature = "server")]
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(1_000)
}

// ── wire types ───────────────────────────────────────────────────────────────

/// Input for [`start_dm`]: the id of the *other* user the DM is with.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StartDmInput {
    pub user_id: i64,
}

/// One member of a DM channel, as returned by [`start_dm`] / [`list_dms`] —
/// including their avatar (a `/media/…` path or `None`), so the
/// DM rail can show the other participant's uploaded avatar.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct DmParticipant {
    pub user_id: i64,
    pub username: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

/// The wire record of a DM channel: its id, creation time, and both
/// participants.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Dm {
    pub channel_id: i64,
    pub created_at: i64,
    /// Both members (the caller's own DM list always includes themselves).
    pub participants: Vec<DmParticipant>,
}

/// Input for [`send_dm_message`]: the DM channel to post into and the text.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SendDmMessageInput {
    pub channel_id: i64,
    pub content: String,
}

/// Input for [`list_dm_messages`]: the DM channel to fetch history from.
/// `limit` caps the number of returned messages (most recent `limit`,
/// delivered oldest-first); absent means the default of 50.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListDmMessagesInput {
    pub channel_id: i64,
    #[serde(default)]
    pub limit: Option<i64>,
}

/// One DM message on the wire: the persisted message plus the author's
/// username, so a client can render "alice: hello" without a user lookup.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct DmMessage {
    pub message_id: i64,
    pub channel_id: i64,
    pub author_id: i64,
    pub author_username: String,
    /// The author's avatar (a `/media/…` path) or `None`
    /// = default initial circle, so a DM client can render the author's
    /// uploaded avatar.
    #[serde(default)]
    pub author_avatar: Option<String>,
    pub content: String,
    /// UTC epoch milliseconds.
    pub created_at: i64,
    /// UTC epoch milliseconds, or `None` until the first edit.
    pub edited_at: Option<i64>,
}

/// The result of [`send_dm_message`]: the persisted message.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SendDmMessageResult {
    pub message: DmMessage,
}

// ── service ──────────────────────────────────────────────────────────────────

/// The DM service. Wraps a `Db` handle. Every method takes the acting user id
/// explicitly so the authorization is testable without any request context.
#[cfg(feature = "server")]
pub struct DmService {
    db: Db,
}

#[cfg(feature = "server")]
impl DmService {
    /// A service backed by `db`.
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Start a DM between `caller_id` and `other_id`.
    ///
    /// Idempotent: if the two already share a DM, the existing channel is
    /// returned (a fresh row is not created).
    ///
    /// # Errors
    /// - `user not found` — `other_id` (or the caller) is not a user.
    /// - `cannot start a DM with yourself` — `other_id == caller_id`.
    /// - `blocked` — a user block exists between the pair (either direction).
    pub fn start_dm(&self, caller_id: i64, other_id: i64) -> Result<Dm, String> {
        if caller_id == other_id {
            return Err("cannot start a DM with yourself".into());
        }
        // Both users must exist (the participants FK would reject a ghost).
        self.db
            .user_by_id(caller_id)
            .ok_or("user not found")?;
        self.db
            .user_by_id(other_id)
            .ok_or("user not found")?;

        // Idempotent: reuse the pair's existing DM if one exists. A block does
        // NOT make an existing DM unreadable — only STARTING a new one is
        // refused below.
        if let Some(id) = self.db.dm_channel_between(caller_id, other_id) {
            return Ok(self.dm(id));
        }

        // A block in either direction forbids opening a new DM.
        if self.db.blocks_pair(caller_id, other_id) {
            return Err("you cannot message this user: they are blocked".into());
        }

        let now = now_ms();
        let channel_id = self
            .db
            .insert_dm_channel(caller_id, other_id, now)
            .map_err(|e| format!("failed to create DM channel: {e}"))?;
        Ok(self.dm(channel_id))
    }

    /// List every DM `user_id` participates in, most-recently-created first,
    /// each with its two participants.
    pub fn list_dms(&self, user_id: i64) -> Result<Vec<Dm>, String> {
        let channels = self.db.dm_channels_for_user(user_id);
        Ok(channels
            .into_iter()
            .map(|(id, _)| self.dm(id))
            .collect())
    }

    /// Send a message into DM channel `channel_id` as `user_id`.
    ///
    /// # Errors
    /// - `user not found` — the caller is not a user.
    /// - `dm channel not found` — `channel_id` is not a DM channel.
    /// - `not a participant of this DM` — the caller is not one of the two
    ///   members (only participants may read or write the conversation).
    /// - `message content is empty` — blank messages are rejected.
    pub fn send_message(
        &self,
        user_id: i64,
        channel_id: i64,
        content: &str,
    ) -> Result<DmMessage, String> {
        if content.trim().is_empty() {
            return Err("message content is empty".into());
        }
        self.db
            .user_by_id(user_id)
            .ok_or("user not found")?;
        if self.db.dm_channel_created_at(channel_id).is_none() {
            return Err("dm channel not found".into());
        }
        if !self.db.is_dm_participant(channel_id, user_id) {
            return Err("not a participant of this DM".into());
        }
        let created_at = now_ms();
        let message_id = self
            .db
            .insert_dm_message(channel_id, user_id, content, created_at)
            .map_err(|e| format!("failed to insert DM message: {e}"))?;
        // The row was just written by the insert above — assemble the wire
        // record directly (no re-read).
        Ok(self.with_username(DmMessage {
            message_id,
            channel_id,
            author_id: user_id,
            author_username: String::new(),
            author_avatar: self.db.user_avatar(user_id),
            content: content.to_string(),
            created_at,
            edited_at: None,
        }))
    }

    /// The most recent `limit` messages of DM channel `channel_id`, oldest-first.
    /// `user_id` must participate in the channel.
    ///
    /// # Errors
    /// - `user not found` — the caller is not a user.
    /// - `dm channel not found` — `channel_id` is not a DM channel.
    /// - `not a participant of this DM` — the caller is not a member.
    pub fn list_messages(
        &self,
        user_id: i64,
        channel_id: i64,
        limit: Option<i64>,
    ) -> Result<Vec<DmMessage>, String> {
        let limit = match limit {
            Some(l) if l > 0 => l,
            _ => 50,
        };
        self.db
            .user_by_id(user_id)
            .ok_or("user not found")?;
        if self.db.dm_channel_created_at(channel_id).is_none() {
            return Err("dm channel not found".into());
        }
        if !self.db.is_dm_participant(channel_id, user_id) {
            return Err("not a participant of this DM".into());
        }
        Ok(self
            .db
            .recent_dm_messages(channel_id, limit)
            .into_iter()
            .map(|m| {
                let username = self
                    .db
                    .user_by_id(m.author_id)
                    .map(|(_, n)| n)
                    .unwrap_or_default();
                DmMessage {
                    message_id: m.id,
                    channel_id: m.channel_id,
                    author_id: m.author_id,
                    author_username: username,
                    author_avatar: self.db.user_avatar(m.author_id),
                    content: m.content,
                    created_at: m.created_at,
                    edited_at: m.edited_at,
                }
            })
            .collect())
    }

    /// Attach the author's username to a freshly-assembled wire record
    /// (the send path builds the record from the insert rather than a re-read).
    fn with_username(&self, mut message: DmMessage) -> DmMessage {
        message.author_username = self
            .db
            .user_by_id(message.author_id)
            .map(|(_, n)| n)
            .unwrap_or_default();
        message
    }

    /// Assemble the wire record for a DM channel id.
    fn dm(&self, channel_id: i64) -> Dm {
        let created_at = self
            .db
            .dm_channel_created_at(channel_id)
            .unwrap_or(0);
        let participants = self
            .db
            .dm_participants_of(channel_id)
            .into_iter()
            .map(|(user_id, username)| DmParticipant {
                user_id,
                username,
                display_name: self.db.user_display_name(user_id),
                avatar_url: self.db.user_avatar(user_id),
            })
            .collect();
        Dm {
            channel_id,
            created_at,
            participants,
        }
    }
}

// ── #[server] endpoints ─────────────────────────────────────────────────────

/// Start a DM with the user named by `input.user_id`. Idempotent — if the
/// caller and that user already share a DM, the existing channel is returned.
///
/// The acting user is read from the `x-user-id` request header.
#[server]
pub async fn start_dm(input: StartDmInput) -> Result<Dm, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    DmService::new(db)
        .start_dm(caller, input.user_id)
        .map_err(ServerError::failed)
}

/// List the acting user's existing DMs, most-recently-created first.
///
/// The acting user is read from the `x-user-id` request header.
#[server]
pub async fn list_dms() -> Result<Vec<Dm>, ServerError> {
    let db = db_from_state()?;
    let user = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    DmService::new(db)
        .list_dms(user)
        .map_err(ServerError::failed)
}

/// Send a message into the DM channel named by `input.channel_id`.
/// Only a participant of that DM may post into it.
///
/// The acting user is read from the `x-user-id` request header.
#[server]
pub async fn send_dm_message(input: SendDmMessageInput) -> Result<SendDmMessageResult, ServerError> {
    let db = db_from_state()?;
    let user = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let message = DmService::new(db.clone())
        .send_message(user, input.channel_id, &input.content)
        .map_err(ServerError::failed)?;
    // Push it to every connected participant (the sender's other sockets too).
    let audience: Vec<i64> = db
        .dm_participants_of(message.channel_id)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    crate::realtime::hub().publish(
        &audience,
        &crate::server_settings::WsServerMsg::DmMessage {
            message_id: message.message_id,
            dm_id: message.channel_id,
            author_id: message.author_id,
            author_username: message.author_username.clone(),
            author_avatar: db.user_avatar(message.author_id),
            content: message.content.clone(),
            created_at: message.created_at,
        },
    );
    Ok(SendDmMessageResult { message })
}

/// List the recent message history of the DM channel named by
/// `input.channel_id`, oldest-first. Only a participant may read it.
///
/// The acting user is read from the `x-user-id` request header.
#[server]
pub async fn list_dm_messages(input: ListDmMessagesInput) -> Result<Vec<DmMessage>, ServerError> {
    let db = db_from_state()?;
    let user = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    DmService::new(db)
        .list_messages(user, input.channel_id, input.limit)
        .map_err(ServerError::failed)
}

/// Pull the shared `Db` out of server state (installed at startup by the
/// server bin via `server::install_state(db)`). `Db` is cheap to clone.
#[cfg(feature = "server")]
fn db_from_state() -> Result<Db, ServerError> {
    server::use_state::<Db>()
        .ok_or_else(|| ServerError::failed("database not installed"))
}

// ── tests ────────────────────────────────────────────────────────────────────
//
// The unit tests exercise [`DmService`] directly against a temp-file database
// (no HTTP / no server state needed). One HTTP e2e test boots the
// macro-mounted router and drives `start_dm` + `list_dms` through the real
// `x-user-id` session header, mirroring `server_settings`'s raw-HTTP e2e.

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Build a temp-file database, run migrations, and seed three users.
    fn setup_db() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let db = Db::migrate(&db_path, &migrations_dir).unwrap();

        let c = db.lock().unwrap();
        c.execute_batch(
            "INSERT INTO users (id, username, email, created_at) VALUES
                (1, 'alice', 'alice@example.com', 1000),
                (2, 'bob',   'bob@example.com',   1000),
                (3, 'carol', 'carol@example.com', 1000);",
        )
        .unwrap();
        drop(c);
        (dir, db)
    }

    // ── start_dm ────────────────────────────────────────────────────────────

    #[test]
    fn start_dm_creates_channel_with_both_participants() {
        let (_dir, db) = setup_db();
        let svc = DmService::new(db.clone());

        let dm = svc.start_dm(1, 2).expect("a valid pair starts a DM");
        assert!(dm.channel_id > 0);
        assert_eq!(dm.participants.len(), 2, "a DM has exactly two members");
        // The two participants are the pair, in a stable (by id) order.
        let ids: Vec<i64> = dm.participants.iter().map(|p| p.user_id).collect();
        assert_eq!(ids, vec![1, 2]);
        let names: Vec<&str> =
            dm.participants.iter().map(|p| p.username.as_str()).collect();
        assert_eq!(names, vec!["alice", "bob"]);
        // The row really exists in the DB with both participant edges.
        assert_eq!(
            db.dm_channel_between(1, 2),
            Some(dm.channel_id),
            "the pair resolves to the new channel"
        );
        assert_eq!(db.dm_participants_of(dm.channel_id).len(), 2);
    }

    #[test]
    fn start_dm_is_idempotent() {
        let (_dir, db) = setup_db();
        let svc = DmService::new(db.clone());

        let first = svc.start_dm(1, 2).unwrap();
        let second = svc.start_dm(1, 2).unwrap();
        assert_eq!(
            first.channel_id, second.channel_id,
            "restarting a DM returns the SAME channel, not a duplicate"
        );
        // ...and only ONE channel exists for the pair.
        let c = db.lock().unwrap();
        let count: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM dm_channels",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "exactly one DM channel for the pair");
        drop(c);
        // Symmetric: starting from the other side also finds the same DM.
        let from_bob = svc.start_dm(2, 1).unwrap();
        assert_eq!(from_bob.channel_id, first.channel_id);
    }

    #[test]
    fn start_dm_rejects_self() {
        let (_dir, db) = setup_db();
        let svc = DmService::new(db);
        let err = svc
            .start_dm(1, 1)
            .expect_err("a user cannot DM themselves");
        assert!(err.contains("yourself"), "got: {err}");
    }

    #[test]
    fn start_dm_rejects_unknown_user() {
        let (_dir, db) = setup_db();
        let svc = DmService::new(db);
        let err = svc
            .start_dm(1, 999)
            .expect_err("a DM with a non-existent user is rejected");
        assert_eq!(err, "user not found");
    }

    // ── list_dms ────────────────────────────────────────────────────────────

    #[test]
    fn list_dms_empty_for_fresh_user() {
        let (_dir, db) = setup_db();
        let svc = DmService::new(db);
        assert!(
            svc.list_dms(3).unwrap().is_empty(),
            "a user with no DMs lists none"
        );
    }

    #[test]
    fn list_dms_returns_each_dm_once_newest_first() {
        let (_dir, db) = setup_db();
        let svc = DmService::new(db.clone());

        let ab = svc.start_dm(1, 2).unwrap(); // alice <-> bob
        let ac = svc.start_dm(1, 3).unwrap(); // alice <-> carol
        assert_ne!(ab.channel_id, ac.channel_id);

        let listed = svc.list_dms(1).expect("user 1 lists their DMs");
        assert_eq!(listed.len(), 2, "user 1 is in exactly two DMs");
        // Newest-created first: `ac` was created after `ab`.
        assert_eq!(listed[0].channel_id, ac.channel_id);
        assert_eq!(listed[1].channel_id, ab.channel_id);
        // Each listed DM carries its two participants.
        for dm in &listed {
            assert_eq!(dm.participants.len(), 2);
            assert!(
                dm.participants.iter().any(|p| p.user_id == 1),
                "user 1 is a participant in each of their DMs"
            );
        }
        // The other party of each DM is correct.
        let dm_with_bob = &listed[1];
        let other = dm_with_bob.participants.iter().find(|p| p.user_id != 1).unwrap();
        assert_eq!(other.user_id, 2);
        let dm_with_carol = &listed[0];
        let other = dm_with_carol.participants.iter().find(|p| p.user_id != 1).unwrap();
        assert_eq!(other.user_id, 3);
    }

    #[test]
    fn list_dms_is_per_user() {
        let (_dir, db) = setup_db();
        let svc = DmService::new(db.clone());
        svc.start_dm(1, 2).unwrap(); // alice <-> bob

        // Bob sees the same DM; carol (not in it) sees none.
        let bob = svc.list_dms(2).unwrap();
        assert_eq!(bob.len(), 1);
        assert_eq!(bob[0].channel_id, db.dm_channel_between(1, 2).unwrap());
        let carol = svc.list_dms(3).unwrap();
        assert!(carol.is_empty(), "carol is not a participant, lists none");
    }

    // ── send_message ────────────────────────────────────────────────────────

    #[test]
    fn send_message_stores_message_in_dm() {
        let (_dir, db) = setup_db();
        let svc = DmService::new(db.clone());
        let dm = svc.start_dm(1, 2).unwrap();

        let msg = svc
            .send_message(1, dm.channel_id, "hello bob")
            .expect("a participant can send into their DM");
        assert!(msg.message_id > 0);
        assert_eq!(msg.channel_id, dm.channel_id);
        assert_eq!(msg.author_id, 1);
        assert_eq!(msg.author_username, "alice");
        assert_eq!(msg.content, "hello bob");
        assert_eq!(msg.edited_at, None);
        // The row really landed in `dm_messages` (the DM-specific table, NOT
        // the guild `messages` table).
        let c = db.lock().unwrap();
        let in_dm_messages: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM dm_messages WHERE channel_id = ?1",
                rusqlite::params![dm.channel_id],
                |r| r.get(0),
            )
            .unwrap();
        let in_guild_messages: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE channel_id = ?1",
                rusqlite::params![dm.channel_id],
                |r| r.get(0),
            )
            .unwrap();
        drop(c);
        assert_eq!(in_dm_messages, 1, "exactly one row in dm_messages");
        assert_eq!(in_guild_messages, 0, "DM messages do not touch guild messages");
    }

    #[test]
    fn send_message_rejects_non_participant() {
        let (_dir, db) = setup_db();
        let svc = DmService::new(db.clone());
        let dm = svc.start_dm(1, 2).unwrap();
        let err = svc
            .send_message(3, dm.channel_id, "intruder")
            .expect_err("carol is not a participant");
        assert!(
            err.contains("not a participant"),
            "got: {err}"
        );
    }

    #[test]
    fn send_message_rejects_empty_content_and_unknowns() {
        let (_dir, db) = setup_db();
        let svc = DmService::new(db.clone());
        let dm = svc.start_dm(1, 2).unwrap();

        assert_eq!(
            svc.send_message(1, dm.channel_id, "   ")
                .unwrap_err(),
            "message content is empty",
            "whitespace-only content is empty"
        );
        assert_eq!(
            svc.send_message(1, 9999, "hi").unwrap_err(),
            "dm channel not found",
            "unknown DM channel id"
        );
        assert_eq!(
            svc.send_message(999, dm.channel_id, "hi")
                .unwrap_err(),
            "user not found",
            "unknown caller"
        );
    }

    // ── list_messages ───────────────────────────────────────────────────────

    #[test]
    fn list_messages_returns_oldest_first_with_author() {
        let (_dir, db) = setup_db();
        let svc = DmService::new(db.clone());
        let dm = svc.start_dm(1, 2).unwrap();

        // bob replies to alice, so both authors appear in the history.
        svc.send_message(1, dm.channel_id, "one").unwrap();
        svc.send_message(2, dm.channel_id, "two").unwrap();
        svc.send_message(1, dm.channel_id, "three").unwrap();

        let msgs = svc
            .list_messages(2, dm.channel_id, None)
            .expect("a participant can list the history");
        assert_eq!(msgs.len(), 3);
        let contents: Vec<&str> = msgs.iter().map(|m| m.content.as_str()).collect();
        assert_eq!(contents, vec!["one", "two", "three"], "oldest-first order");
        let authors: Vec<(i64, &str)> =
            msgs.iter().map(|m| (m.author_id, m.author_username.as_str())).collect();
        assert_eq!(authors, vec![(1, "alice"), (2, "bob"), (1, "alice")]);
        // Monotonic ids.
        assert!(msgs[0].message_id < msgs[1].message_id);
        assert!(msgs[1].message_id < msgs[2].message_id);
        // Visible to both participants (checked from bob's side above); alice
        // sees the same history.
        let alice_view = svc.list_messages(1, dm.channel_id, None).unwrap();
        assert_eq!(
            alice_view.len(),
            3,
            "the other participant sees the same history"
        );
    }

    #[test]
    fn list_messages_honours_limit_and_rejects_outsiders() {
        let (_dir, db) = setup_db();
        let svc = DmService::new(db.clone());
        let dm = svc.start_dm(1, 2).unwrap();
        for i in 0..5 {
            svc.send_message(1, dm.channel_id, &format!("m{i}")).unwrap();
        }

        // limit=2 → the TWO most recent, still oldest-first.
        let last2 = svc.list_messages(1, dm.channel_id, Some(2)).unwrap();
        let contents: Vec<&str> = last2.iter().map(|m| m.content.as_str()).collect();
        assert_eq!(contents, vec!["m3", "m4"]);

        // A non-participant cannot read the history.
        let err = svc
            .list_messages(3, dm.channel_id, None)
            .expect_err("carol is not a participant");
        assert!(err.contains("not a participant"), "got: {err}");

        // Unknown DM channel.
        assert_eq!(
            svc.list_messages(1, 9999, None).unwrap_err(),
            "dm channel not found"
        );
        // Empty history for a fresh DM.
        let fresh = svc.start_dm(1, 3).unwrap();
        assert!(
            svc.list_messages(1, fresh.channel_id, None)
                .unwrap()
                .is_empty()
        );
    }

    // ── HTTP e2e (start + list round-trip) ───────────────────────────────────
    //
    // Boots the macro-mounted router on an ephemeral port and drives both
    // endpoints over raw HTTP with the `x-user-id` session header — the same
    // pattern as `server_settings::tests::unread_counts_and_mark_read_over_http`.
    // Serialized with the crate-level e2e lock (all e2e tests share the one
    // process-wide `install_state` slot).

    #[tokio::test]
    async fn start_dm_and_list_dms_over_http() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let (_dir, db) = setup_db();
        let _guard = crate::e2e_lock::guard();
        server::install_state(db.clone());

        // Boot the macro-mounted router on an ephemeral port.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, server::router()).await;
        });

        /// A tiny raw-HTTP client: POST a JSON args-tuple to `/_srv/<fn>` with
        /// the `x-user-id` auth header, return the parsed `{"Ok":…}`/`{"Err":…}`
        /// body. (A raw socket, like the gateway tests, because the `#[server]`
        /// HTTP transport isn't in the vendored registry as a dev-dep.)
        async fn call(
            addr: std::net::SocketAddr,
            path: &str,
            user_id: Option<&str>,
            body: &str,
        ) -> serde_json::Value {
            let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            let (mut reader, mut writer) = stream.into_split();
            let auth = match user_id {
                Some(id) => format!("x-user-id: {id}\r\n"),
                None => String::new(),
            };
            let req = format!(
                "POST /_srv/{path} HTTP/1.1\r\n\
                 Host: {addr}\r\n\
                 {auth}\
                 content-type: application/json\r\n\
                 content-length: {}\r\n\
                 connection: close\r\n\r\n{body}",
                body.len()
            );
            writer.write_all(req.as_bytes()).await.unwrap();
            let mut buf = Vec::new();
            reader.read_to_end(&mut buf).await.unwrap();
            let text = String::from_utf8(buf).unwrap();
            let body = text
                .split_once("\r\n\r\n")
                .map(|(_, b)| b.to_string())
                .unwrap_or_default();
            serde_json::from_str(&body).unwrap()
        }

        // 1. start_dm: alice (1) opens a DM with bob (2).
        let resp = call(
            addr,
            "start_dm",
            Some("1"),
            &serde_json::to_string(&[serde_json::json!({ "user_id": 2 })]).unwrap(),
        )
        .await;
        let dm: Dm = serde_json::from_value(resp["Ok"].clone())
            .expect("HTTP start_dm returns Ok(Dm)");
        assert!(dm.channel_id > 0);
        assert_eq!(dm.participants.len(), 2);
        let ids: Vec<i64> = dm.participants.iter().map(|p| p.user_id).collect();
        assert_eq!(ids, vec![1, 2]);

        // 2. list_dms: alice now sees exactly that DM.
        let resp = call(addr, "list_dms", Some("1"), "null").await;
        let listed: Vec<Dm> = resp["Ok"]
            .as_array()
            .cloned()
            .expect("HTTP list_dms returns Ok(array)")
            .into_iter()
            .map(|v| serde_json::from_value(v).unwrap())
            .collect();
        assert_eq!(listed.len(), 1, "alice has one DM");
        assert_eq!(listed[0].channel_id, dm.channel_id);

        // 3. Idempotent over the wire: restarting the same pair returns the
        //    same channel and does NOT grow the list.
        let resp = call(
            addr,
            "start_dm",
            Some("1"),
            &serde_json::to_string(&[serde_json::json!({ "user_id": 2 })]).unwrap(),
        )
        .await;
        let again: Dm = serde_json::from_value(resp["Ok"].clone()).unwrap();
        assert_eq!(again.channel_id, dm.channel_id, "restart is idempotent");
        let resp = call(addr, "list_dms", Some("1"), "null").await;
        let listed: Vec<Dm> = resp["Ok"]
            .as_array()
            .cloned()
            .unwrap()
            .into_iter()
            .map(|v| serde_json::from_value(v).unwrap())
            .collect();
        assert_eq!(listed.len(), 1, "still exactly one DM");

        // 4. bob's side: he sees the same DM, with alice as the other party.
        let resp = call(addr, "list_dms", Some("2"), "null").await;
        let bob_list: Vec<Dm> = resp["Ok"]
            .as_array()
            .cloned()
            .unwrap()
            .into_iter()
            .map(|v| serde_json::from_value(v).unwrap())
            .collect();
        assert_eq!(bob_list.len(), 1);
        assert_eq!(bob_list[0].channel_id, dm.channel_id);
        let other = bob_list[0]
            .participants
            .iter()
            .find(|p| p.user_id != 2)
            .unwrap();
        assert_eq!(other.user_id, 1);

        // 5. Unauthenticated (no x-user-id) is rejected by both endpoints.
        let resp = call(
            addr,
            "start_dm",
            None,
            &serde_json::to_string(&[serde_json::json!({ "user_id": 2 })]).unwrap(),
        )
        .await;
        assert!(
            resp.get("Err").is_some(),
            "unauthenticated start_dm must fail, got {resp:?}"
        );
        let resp = call(addr, "list_dms", None, "null").await;
        assert!(
            resp.get("Err").is_some(),
            "unauthenticated list_dms must fail, got {resp:?}"
        );
    }

    // ── HTTP e2e (send + list message round-trip) ───────────────────────────
    //
    // Same raw-HTTP pattern as the start/list e2e above: start a DM, send
    // messages through `send_dm_message`, then read the history back through
    // `list_dm_messages` — covering the full send + list path over the wire.

    #[tokio::test]
    async fn send_dm_message_and_list_dm_messages_over_http() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let (_dir, db) = setup_db();
        let _guard = crate::e2e_lock::guard();
        server::install_state(db.clone());

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, server::router()).await;
        });

        /// Raw-HTTP POST of a JSON args-tuple to `/_srv/<fn>` with the
        /// `x-user-id` auth header (see the start/list e2e above).
        async fn call(
            addr: std::net::SocketAddr,
            path: &str,
            user_id: Option<&str>,
            body: &str,
        ) -> serde_json::Value {
            let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            let (mut reader, mut writer) = stream.into_split();
            let auth = match user_id {
                Some(id) => format!("x-user-id: {id}\r\n"),
                None => String::new(),
            };
            let req = format!(
                "POST /_srv/{path} HTTP/1.1\r\n\
                 Host: {addr}\r\n\
                 {auth}\
                 content-type: application/json\r\n\
                 content-length: {}\r\n\
                 connection: close\r\n\r\n{body}",
                body.len()
            );
            writer.write_all(req.as_bytes()).await.unwrap();
            let mut buf = Vec::new();
            reader.read_to_end(&mut buf).await.unwrap();
            let text = String::from_utf8(buf).unwrap();
            let body = text
                .split_once("\r\n\r\n")
                .map(|(_, b)| b.to_string())
                .unwrap_or_default();
            serde_json::from_str(&body).unwrap()
        }

        // 1. alice (1) starts a DM with bob (2).
        let resp = call(
            addr,
            "start_dm",
            Some("1"),
            &serde_json::to_string(&[serde_json::json!({ "user_id": 2 })]).unwrap(),
        )
        .await;
        let dm: Dm = serde_json::from_value(resp["Ok"].clone()).unwrap();
        let channel_id = dm.channel_id;

        // 2. Send: alice posts, bob replies.
        let resp = call(
            addr,
            "send_dm_message",
            Some("1"),
            &serde_json::to_string(&[serde_json::json!({
                "channel_id": channel_id,
                "content": "hello bob"
            })]).unwrap(),
        )
        .await;
        let sent: SendDmMessageResult =
            serde_json::from_value(resp["Ok"].clone())
                .expect("HTTP send_dm_message returns Ok(SendDmMessageResult)");
        assert_eq!(sent.message.channel_id, channel_id);
        assert_eq!(sent.message.author_id, 1);
        assert_eq!(sent.message.author_username, "alice");
        assert_eq!(sent.message.content, "hello bob");

        let resp = call(
            addr,
            "send_dm_message",
            Some("2"),
            &serde_json::to_string(&[serde_json::json!({
                "channel_id": channel_id,
                "content": "hi alice"
            })]).unwrap(),
        )
        .await;
        let sent2: SendDmMessageResult =
            serde_json::from_value(resp["Ok"].clone()).unwrap();
        assert_eq!(sent2.message.content, "hi alice");
        assert_eq!(sent2.message.author_id, 2);
        assert_eq!(sent2.message.author_username, "bob");

        // 3. List: bob reads the full history — both messages, oldest first.
        let resp = call(
            addr,
            "list_dm_messages",
            Some("2"),
            &serde_json::to_string(&[serde_json::json!({ "channel_id": channel_id })]).unwrap(),
        )
        .await;
        let msgs: Vec<DmMessage> = resp["Ok"]
            .as_array()
            .cloned()
            .expect("HTTP list_dm_messages returns Ok(array)")
            .into_iter()
            .map(|v| serde_json::from_value(v).unwrap())
            .collect();
        assert_eq!(msgs.len(), 2, "the DM has exactly two messages");
        assert_eq!(msgs[0].content, "hello bob");
        assert_eq!(msgs[1].content, "hi alice");
        let authors: Vec<i64> = msgs.iter().map(|m| m.author_id).collect();
        assert_eq!(authors, vec![1, 2]);

        // 4. Limit over the wire: limit=1 returns only the most recent.
        let resp = call(
            addr,
            "list_dm_messages",
            Some("2"),
            &serde_json::to_string(&[serde_json::json!({
                "channel_id": channel_id,
                "limit": 1
            })]).unwrap(),
        )
        .await;
        let last1: Vec<DmMessage> = resp["Ok"]
            .as_array()
            .cloned()
            .unwrap()
            .into_iter()
            .map(|v| serde_json::from_value(v).unwrap())
            .collect();
        assert_eq!(last1.len(), 1);
        assert_eq!(last1[0].content, "hi alice");

        // 5. Non-participant (carol, 3) cannot send into or read the DM.
        let resp = call(
            addr,
            "send_dm_message",
            Some("3"),
            &serde_json::to_string(&[serde_json::json!({
                "channel_id": channel_id,
                "content": "intruder"
            })]).unwrap(),
        )
        .await;
        assert!(
            resp["Err"]["Failed"]
                .as_str()
                .map(|e| e.contains("not a participant"))
                .unwrap_or(false),
            "non-participant send must fail, got {resp:?}"
        );
        let resp = call(
            addr,
            "list_dm_messages",
            Some("3"),
            &serde_json::to_string(&[serde_json::json!({ "channel_id": channel_id })]).unwrap(),
        )
        .await;
        assert!(
            resp["Err"]["Failed"]
                .as_str()
                .map(|e| e.contains("not a participant"))
                .unwrap_or(false),
            "non-participant list must fail, got {resp:?}"
        );

        // 6. Unauthenticated calls are rejected.
        let resp = call(
            addr,
            "send_dm_message",
            None,
            &serde_json::to_string(&[serde_json::json!({
                "channel_id": channel_id,
                "content": "anon"
            })]).unwrap(),
        )
        .await;
        assert!(
            resp.get("Err").is_some(),
            "unauthenticated send must fail, got {resp:?}"
        );
        let resp = call(
            addr,
            "list_dm_messages",
            None,
            &serde_json::to_string(&[serde_json::json!({ "channel_id": channel_id })]).unwrap(),
        )
        .await;
        assert!(
            resp.get("Err").is_some(),
            "unauthenticated list must fail, got {resp:?}"
        );
    }
}
