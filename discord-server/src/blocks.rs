//! User blocking: block / unblock / list-blocked for the acting user.
//!
//! ## Design
//!
//! Mirrors [`crate::friends`]: the authorization + state-machine logic lives
//! in a pure, DB-backed service ([`BlockService`]) that takes the DB handle
//! and the acting user id as plain arguments and returns `Result<T, String>`,
//! so it is fully unit-testable against a temp-file SQLite database with no
//! axum/tokio stack. The `#[server]` fns below are thin adapters: they pull
//! the shared `Db` out of server state, resolve the acting user from a
//! verified session token (the same mechanism [`crate::friends`] /
//! [`crate::session`] use), push a realtime `FriendChanged` event to the
//! *other* user (so their friend list refreshes), and map a service failure
//! to `ServerError::failed`.
//!
//! ## Model
//!
//! A *block* is one row per **directed** `(blocker, blocked)` pair in
//! `user_blocks` (migration `0017_user_blocks.sql`). The pair is "blocked"
//! when a row exists in EITHER direction (`db.blocks_pair`). Blocking has no
//! status lifecycle — it is a presence/absence fact.
//!
//! **Effects of blocking:**
//! - Any existing friendship or pending request between the two is deleted
//!   (either direction).
//! - A `FriendChanged { action: "removed" }` event is pushed to the blocked
//!   user so their friend list refreshes.
//! - While the block exists (either direction), `send_friend_request` and
//!   `start_dm` between the pair are rejected with a clear error.
//! - Existing DMs remain readable (the DM channel is not deleted).
//! - Channel messages are NOT filtered server-side; the client receives the
//!   `blocked_ids` list in the `LoginResult` startup payload and collapses
//!   blocked users' messages locally.

use serde::{Deserialize, Serialize};
use server::{server, ServerError};

#[cfg(feature = "server")]
use crate::db::Db;

#[cfg(feature = "server")]
use crate::session::acting_user;

// ── wire types ───────────────────────────────────────────────────────────────

/// Input for [`block_user`] / [`unblock_user`]: the id of the target user.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BlockUserInput {
    pub user_id: i64,
}

/// One user in the blocked list, as returned by [`list_blocked`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct BlockedUser {
    pub user_id: i64,
    pub username: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

// ── service ──────────────────────────────────────────────────────────────────

/// The user-blocking service. Wraps a `Db` handle. Every method takes the
/// acting user id explicitly so the authorization is testable without any
/// request context. Pure (no realtime) — the `#[server]` adapters push the
/// `FriendChanged` event after a successful call.
#[cfg(feature = "server")]
pub struct BlockService {
    db: Db,
}

#[cfg(feature = "server")]
impl BlockService {
    /// A service backed by `db`.
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Block `target` from `caller`.
    ///
    /// - Inserts a directed block row (idempotent: `INSERT OR IGNORE`).
    /// - Deletes any friendship or pending request between the two (either
    ///   direction, either status).
    ///
    /// # Errors
    /// - `user not found` — the caller or target is not a user.
    /// - `cannot block yourself` — `target == caller`.
    pub fn block(&self, caller: i64, target: i64) -> Result<BlockedUser, String> {
        if caller == target {
            return Err("cannot block yourself".into());
        }
        self.db
            .user_by_id(caller)
            .ok_or("user not found")?;
        self.db
            .user_by_id(target)
            .ok_or("user not found")?;

        // Insert the block row (no-op if already present).
        self.db.insert_user_block(caller, target);

        // Remove any existing friendship or pending request between the two.
        self.db.delete_friendship_between(caller, target);

        Ok(self.blocked(target))
    }

    /// Unblock `target` for `caller`. Deletes the directed block row
    /// `(caller, target)` if it exists. A reverse block `(target, caller)` is
    /// a separate row and survives.
    ///
    /// # Errors
    /// - `user not found` — the caller or target is not a user.
    pub fn unblock(&self, caller: i64, target: i64) -> Result<BlockedUser, String> {
        self.db
            .user_by_id(caller)
            .ok_or("user not found")?;
        self.db
            .user_by_id(target)
            .ok_or("user not found")?;

        self.db.delete_user_block(caller, target);
        Ok(self.blocked(target))
    }

    /// List the users `user_id` has blocked (rows they initiated).
    ///
    /// # Errors
    /// - `user not found` — the caller is not a user.
    pub fn list(&self, user_id: i64) -> Result<Vec<BlockedUser>, String> {
        self.db
            .user_by_id(user_id)
            .ok_or("user not found")?;
        let rows = self.db.blocked_users_for_user(user_id);
        Ok(rows
            .into_iter()
            .map(|(id, username, avatar_url)| BlockedUser {
                user_id: id,
                username,
                avatar_url,
            })
            .collect())
    }

    /// Assemble the wire record for a user id (username + avatar).
    fn blocked(&self, user_id: i64) -> BlockedUser {
        let username = self
            .db
            .user_by_id(user_id)
            .map(|(_, n)| n)
            .unwrap_or_default();
        BlockedUser {
            user_id,
            username,
            avatar_url: self.db.user_avatar(user_id),
        }
    }
}

// ── #[server] endpoints ─────────────────────────────────────────────────────

/// Block `input.user_id`. Removes any existing friendship or pending request
/// between the caller and the target. Pushes a `FriendChanged` event to the
/// target so their friend list refreshes.
///
/// The acting user is resolved from the verified session token.
#[server]
pub async fn block_user(input: BlockUserInput) -> Result<BlockedUser, ServerError> {
    let db = db_from_state()?;
    let user = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let blocked = BlockService::new(db.clone())
        .block(user, input.user_id)
        .map_err(ServerError::failed)?;
    crate::realtime::hub().publish(
        &[blocked.user_id],
        &crate::server_settings::WsServerMsg::FriendChanged {
            friend_id: blocked.user_id,
            action: "removed".to_string(),
        },
    );
    Ok(blocked)
}

/// Unblock `input.user_id`. Only removes the caller's own directed block row;
/// a reverse block (they blocked the caller) is a separate row and survives.
/// Pushes a `FriendChanged` event to the target so their friend list refreshes.
///
/// The acting user is resolved from the verified session token.
#[server]
pub async fn unblock_user(input: BlockUserInput) -> Result<BlockedUser, ServerError> {
    let db = db_from_state()?;
    let user = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let blocked = BlockService::new(db.clone())
        .unblock(user, input.user_id)
        .map_err(ServerError::failed)?;
    crate::realtime::hub().publish(
        &[blocked.user_id],
        &crate::server_settings::WsServerMsg::FriendChanged {
            friend_id: blocked.user_id,
            action: "removed".to_string(),
        },
    );
    Ok(blocked)
}

/// List the users the acting user has blocked (with username + avatar).
///
/// The acting user is resolved from the verified session token.
#[server]
pub async fn list_blocked() -> Result<Vec<BlockedUser>, ServerError> {
    let db = db_from_state()?;
    let user = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    BlockService::new(db)
        .list(user)
        .map_err(ServerError::failed)
}

/// Pull the shared `Db` out of server state. `Db` is cheap to clone.
#[cfg(feature = "server")]
fn db_from_state() -> Result<Db, ServerError> {
    server::use_state::<Db>()
        .ok_or_else(|| ServerError::failed("database not installed"))
}

// ── tests ────────────────────────────────────────────────────────────────────

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

    /// Make two users friends (request + accept).
    fn make_friends(db: &Db, a: i64, b: i64) {
        let svc = crate::friends::FriendService::new(db.clone());
        // We need the target's username.
        let b_name = db.user_by_id(b).unwrap().1;
        svc.send_request(a, &b_name).unwrap();
        let (row_id, requester, _addressee, _status) = db.friendship_between(a, b).unwrap();
        if requester == a {
            svc.accept(b, a).unwrap();
        } else {
            svc.accept(a, b).unwrap();
        }
        let _ = row_id;
    }

    // ── block basics ────────────────────────────────────────────────────────

    #[test]
    fn block_removes_friendship() {
        let (_dir, db) = setup_db();
        let svc = BlockService::new(db.clone());
        let alice = db.user_by_username("alice").unwrap().0;
        let bob = db.user_by_username("bob").unwrap().0;

        make_friends(&db, alice, bob);
        assert!(
            db.friendship_between(alice, bob).is_some(),
            "precondition: they are friends"
        );

        let blocked = svc.block(alice, bob).expect("alice blocks bob");
        assert_eq!(blocked.user_id, bob);
        assert_eq!(blocked.username, "bob");

        // The friendship row is gone.
        assert!(
            db.friendship_between(alice, bob).is_none(),
            "blocking deletes the friendship"
        );
        // Both sides see no relationship.
        let fs = crate::friends::FriendService::new(db.clone());
        assert!(fs.list(alice).unwrap().accepted.is_empty());
        assert!(fs.list(bob).unwrap().accepted.is_empty());
        // The block row exists.
        assert!(db.blocks_pair(alice, bob), "pair is blocked");
    }

    #[test]
    fn block_removes_pending_request() {
        let (_dir, db) = setup_db();
        let fs = crate::friends::FriendService::new(db.clone());
        let alice = db.user_by_username("alice").unwrap().0;
        let bob = db.user_by_username("bob").unwrap().0;

        // Alice sends a pending request to bob (not yet accepted).
        fs.send_request(alice, "bob").unwrap();
        assert_eq!(
            db.friendship_between(alice, bob).unwrap().3,
            "pending"
        );

        let bs = BlockService::new(db.clone());
        bs.block(alice, bob).expect("alice blocks bob");

        // The pending request row is gone.
        assert!(
            db.friendship_between(alice, bob).is_none(),
            "blocking deletes the pending request"
        );
        assert!(fs.list(alice).unwrap().outgoing.is_empty());
        assert!(fs.list(bob).unwrap().incoming.is_empty());
    }

    #[test]
    fn blocked_user_cannot_friend_request_either_direction() {
        let (_dir, db) = setup_db();
        let fs = crate::friends::FriendService::new(db.clone());
        let bs = BlockService::new(db.clone());
        let alice = db.user_by_username("alice").unwrap().0;
        let bob = db.user_by_username("bob").unwrap().0;

        // Alice blocks bob.
        bs.block(alice, bob).unwrap();

        // Bob cannot send a friend request to alice.
        let err = fs
            .send_request(bob, "alice")
            .expect_err("blocked user cannot friend-request");
        assert!(
            err.contains("blocked"),
            "error should mention block, got: {err}"
        );

        // Alice cannot send a friend request to bob either.
        let err = fs
            .send_request(alice, "bob")
            .expect_err("blocker also cannot friend-request");
        assert!(
            err.contains("blocked"),
            "error should mention block, got: {err}"
        );
    }

    #[test]
    fn blocked_user_cannot_start_dm_either_direction() {
        let (_dir, db) = setup_db();
        let ds = crate::dms::DmService::new(db.clone());
        let bs = BlockService::new(db.clone());
        let alice = db.user_by_username("alice").unwrap().0;
        let bob = db.user_by_username("bob").unwrap().0;

        bs.block(alice, bob).unwrap();

        // Bob cannot start a DM with alice.
        let err = ds
            .start_dm(bob, alice)
            .expect_err("blocked user cannot start DM");
        assert!(
            err.contains("blocked"),
            "error should mention block, got: {err}"
        );

        // Alice cannot start a DM with bob either.
        let err = ds
            .start_dm(alice, bob)
            .expect_err("blocker also cannot start DM");
        assert!(
            err.contains("blocked"),
            "error should mention block, got: {err}"
        );
    }

    #[test]
    fn unblock_restores_friend_request_ability() {
        let (_dir, db) = setup_db();
        let fs = crate::friends::FriendService::new(db.clone());
        let bs = BlockService::new(db.clone());
        let alice = db.user_by_username("alice").unwrap().0;
        let bob = db.user_by_username("bob").unwrap().0;

        bs.block(alice, bob).unwrap();
        // Confirm blocked: request fails.
        fs.send_request(bob, "alice")
            .expect_err("blocked while block active");

        bs.unblock(alice, bob).unwrap();
        assert!(!db.blocks_pair(alice, bob), "block removed after unblock");

        // Now bob CAN send a friend request.
        let sent = fs
            .send_request(bob, "alice")
            .expect("unblocked user can friend-request again");
        assert_eq!(sent.status, "pending");
    }

    #[test]
    fn unblock_restores_dm_ability() {
        let (_dir, db) = setup_db();
        let ds = crate::dms::DmService::new(db.clone());
        let bs = BlockService::new(db.clone());
        let alice = db.user_by_username("alice").unwrap().0;
        let bob = db.user_by_username("bob").unwrap().0;

        bs.block(alice, bob).unwrap();
        ds.start_dm(bob, alice)
            .expect_err("blocked while block active");

        bs.unblock(alice, bob).unwrap();
        let dm = ds
            .start_dm(bob, alice)
            .expect("unblocked user can start DM again");
        assert!(dm.channel_id > 0);
    }

    #[test]
    fn existing_dm_stays_readable_after_block() {
        let (_dir, db) = setup_db();
        let ds = crate::dms::DmService::new(db.clone());
        let bs = BlockService::new(db.clone());
        let alice = db.user_by_username("alice").unwrap().0;
        let bob = db.user_by_username("bob").unwrap().0;

        // Start a DM first.
        let dm = ds.start_dm(alice, bob).unwrap();
        let channel_id = dm.channel_id;

        // Alice blocks bob.
        bs.block(alice, bob).unwrap();

        // Both can still read the existing DM channel.
        let listed_alice = ds.list_dms(alice).unwrap();
        assert!(
            listed_alice.iter().any(|d| d.channel_id == channel_id),
            "alice still sees the existing DM"
        );
        let listed_bob = ds.list_dms(bob).unwrap();
        assert!(
            listed_bob.iter().any(|d| d.channel_id == channel_id),
            "bob still sees the existing DM"
        );
    }

    // ── failure modes ───────────────────────────────────────────────────────

    #[test]
    fn cannot_block_self() {
        let (_dir, db) = setup_db();
        let svc = BlockService::new(db.clone());
        let alice = db.user_by_username("alice").unwrap().0;
        let err = svc
            .block(alice, alice)
            .expect_err("a user cannot block themselves");
        assert!(err.contains("yourself"), "got: {err}");
    }

    #[test]
    fn double_block_is_noop() {
        let (_dir, db) = setup_db();
        let svc = BlockService::new(db.clone());
        let alice = db.user_by_username("alice").unwrap().0;
        let bob = db.user_by_username("bob").unwrap().0;

        svc.block(alice, bob).unwrap();
        // Second block: no error, no duplicate row.
        let again = svc.block(alice, bob).expect("double block is a no-op, not an error");
        assert_eq!(again.user_id, bob);

        let count: i64 = db
            .lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM user_blocks WHERE blocker_id = ?1 AND blocked_id = ?2",
                rusqlite::params![alice, bob],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "exactly one block row (INSERT OR IGNORE)");
    }

    #[test]
    fn block_unknown_user_is_rejected() {
        let (_dir, db) = setup_db();
        let svc = BlockService::new(db.clone());
        let alice = db.user_by_username("alice").unwrap().0;
        let err = svc
            .block(alice, 999)
            .expect_err("blocking a non-existent user is rejected");
        assert_eq!(err, "user not found");
    }

    // ── list_blocked ────────────────────────────────────────────────────────

    #[test]
    fn list_blocked_contents() {
        let (_dir, db) = setup_db();
        let svc = BlockService::new(db.clone());
        let alice = db.user_by_username("alice").unwrap().0;
        let bob = db.user_by_username("bob").unwrap().0;
        let carol = db.user_by_username("carol").unwrap().0;

        // Empty initially.
        assert!(svc.list(alice).unwrap().is_empty());

        svc.block(alice, bob).unwrap();
        svc.block(alice, carol).unwrap();

        let listed = svc.list(alice).unwrap();
        assert_eq!(listed.len(), 2, "two blocked users");
        // Sorted by blocked_id (bob=2, carol=3).
        assert_eq!(listed[0].user_id, bob);
        assert_eq!(listed[0].username, "bob");
        assert_eq!(listed[1].user_id, carol);
        assert_eq!(listed[1].username, "carol");
    }

    #[test]
    fn unblock_only_removes_own_row() {
        let (_dir, db) = setup_db();
        let svc = BlockService::new(db.clone());
        let alice = db.user_by_username("alice").unwrap().0;
        let bob = db.user_by_username("bob").unwrap().0;

        // Both block each other (two separate directed rows).
        svc.block(alice, bob).unwrap();
        svc.block(bob, alice).unwrap();
        assert!(db.blocks_pair(alice, bob));

        // Alice unblocks bob: removes only (alice, bob). (bob, alice) survives.
        svc.unblock(alice, bob).unwrap();
        assert!(
            db.blocks_pair(alice, bob),
            "pair still blocked (reverse row survives)"
        );

        // Bob unblocks alice: now the pair is unblocked.
        svc.unblock(bob, alice).unwrap();
        assert!(!db.blocks_pair(alice, bob), "pair unblocked after both unblock");
    }
}
