//! The friends list: send a friend request by username, list the acting
//! user's incoming / outgoing / accepted relationships, accept an incoming
//! request, decline or cancel a pending request, and remove an accepted
//! friend.
//!
//! ## Design
//!
//! Mirrors [`crate::dms`]: the authorization + state-machine logic lives in a
//! pure, DB-backed service ([`FriendService`]) that takes the DB handle and the
//! acting user id as plain arguments and returns `Result<T, String>`, so it is
//! fully unit-testable against a temp-file SQLite database with no axum/tokio
//! stack. The `#[server]` fns below are thin adapters: they pull the shared
//! `Db` out of server state, resolve the acting user from a verified session
//! token (the same mechanism [`crate::dms`] / [`crate::session`] use), push the
//! realtime `FriendChanged` event to the *other* user (the DM-push pattern),
//! and map a service failure to `ServerError::failed`.
//!
//! ## Model
//!
//! A *friendship* is one row per **unordered** user pair (`friendships`,
//! migration `0016_friendships.sql`). The pair is unique via the normalised
//! `min_user_id` < `max_user_id` columns (SQLite has no expression UNIQUE
//! index); `requester_id` / `addressee_id` keep the direction so the lifecycle
//! is testable. `status` is `'pending'` (a request awaiting a reply) or
//! `'accepted'` (friends).
//!
//! The lifecycle, all operating on the pair's single row:
//!
//! - **Send request** — name the addressee by `username`. A fresh row is
//!   created when the pair has no relationship. If the addressee already has a
//!   *pending* request to the caller (crossed requests), the caller's send
//!   *accepts* that existing row instead of erroring. A second request from the
//!   same caller, or a request between already-friends, is an error.
//! - **List** — the acting user's relationships split into `incoming` (they
//!   were asked), `outgoing` (they asked), and `accepted` (friends); each
//!   other user appears once with their username + avatar.
//! - **Accept** — the addressee of a pending request flips the row to
//!   `'accepted'`. Only the addressee (the incoming side) may accept.
//! - **Decline / cancel** — the row is dropped: the addressee declines an
//!   incoming request, or the requester cancels their own outgoing one. Either
//!   way the relationship is gone.
//! - **Remove friend** — an `'accepted'` relationship is dropped.

use serde::{Deserialize, Serialize};
use server::{server, ServerError};

// This file is NOT feature-gated so the `#[server]` fns compile to client RPC
// stubs by default (the same shape as `dms.rs` / `servers.rs`); only the
// DB-touching pieces are gated `#[cfg(feature = "server")]`.
#[cfg(feature = "server")]
use crate::db::Db;

/// The acting user for the current request, resolved from a verified session
/// token (see [`crate::session`]); `None` when unauthenticated.
#[cfg(feature = "server")]
use crate::session::acting_user;

/// The current wall-clock milliseconds since the Unix epoch (the schema's
/// timestamp unit; see migration `0016_friendships.sql`).
#[cfg(feature = "server")]
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(1_000)
}

// ── wire types ───────────────────────────────────────────────────────────────

/// One user in a friendship group, as returned by [`list_friends`] and the
/// accept / decline / remove fns — including their avatar (a
/// `data:image/…;base64,` URL or `None`), so the friends list can render the
/// other participant's uploaded avatar.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct FriendUser {
    pub user_id: i64,
    pub username: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

/// The acting user's friends list: `incoming` (pending requests addressed to
/// them), `outgoing` (pending requests they sent), and `accepted` (friends).
/// Each other user appears in exactly one group (the pair is unique).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct FriendList {
    pub incoming: Vec<FriendUser>,
    pub outgoing: Vec<FriendUser>,
    pub accepted: Vec<FriendUser>,
}

/// Input for [`send_friend_request`]: the addressee's `@username`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SendFriendRequestInput {
    pub username: String,
}

/// The result of [`send_friend_request`]: the addressee's id and the
/// resulting status — `'pending'` (a new request was sent) or `'accepted'`
/// (a crossed request was auto-accepted, so the pair is now friends).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SendFriendRequestResult {
    pub user_id: i64,
    pub status: String,
}

/// Input for [`accept_friend_request`]: the id of the user whose incoming
/// request is being accepted.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AcceptFriendRequestInput {
    pub user_id: i64,
}

/// Input for [`decline_or_cancel`]: the id of the user the pending request
/// with is being declined (incoming) or cancelled (outgoing).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeclineOrCancelInput {
    pub user_id: i64,
}

/// Input for [`remove_friend`]: the id of the accepted friend being removed.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RemoveFriendInput {
    pub user_id: i64,
}

// ── service ──────────────────────────────────────────────────────────────────

/// The friends-list service. Wraps a `Db` handle. Every method takes the
/// acting user id explicitly so the authorization is testable without any
/// request context. Pure (no realtime) — the `#[server]` adapters push the
/// `FriendChanged` event after a successful call.
#[cfg(feature = "server")]
pub struct FriendService {
    db: Db,
}

#[cfg(feature = "server")]
impl FriendService {
    /// A service backed by `db`.
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Send a friend request from `caller` to the user named by `username`.
    ///
    /// Returns the addressee's id and the resulting status: `'pending'` when a
    /// new request row was created, `'accepted'` when a crossed pending request
    /// (the addressee had already asked the caller) was auto-accepted.
    ///
    /// # Errors
    /// - `user not found` — the caller or the named username is not a user.
    /// - `cannot friend yourself` — the username resolves to the caller.
    /// - `blocked` — a user block exists between the pair in either direction.
    /// - `already friends` — the pair is already `'accepted'`.
    /// - `a friend request is already pending` — the caller's own pending
    ///   request to that user already exists (a duplicate).
    pub fn send_request(&self, caller: i64, username: &str) -> Result<SendFriendRequestResult, String> {
        self.db
            .user_by_id(caller)
            .ok_or("user not found")?;
        let (other_id, _name) = self
            .db
            .user_by_username(username)
            .ok_or("user not found")?;
        if caller == other_id {
            return Err("cannot friend yourself".into());
        }
        if self.db.blocks_pair(caller, other_id) {
            return Err("you cannot friend this user: they are blocked".into());
        }
        match self.db.friendship_between(caller, other_id) {
            None => {
                let created_at = now_ms();
                self.db
                    .insert_friend_request(caller, other_id, created_at)
                    .map_err(|e| format!("failed to send friend request: {e}"))?;
                Ok(SendFriendRequestResult {
                    user_id: other_id,
                    status: "pending".to_string(),
                })
            }
            Some((row_id, requester, _addressee, status)) => {
                if status == "accepted" {
                    return Err("already friends".into());
                }
                // status == "pending": the row is one of the caller's two sides.
                if requester == caller {
                    // The caller already sent a pending request to this user —
                    // a duplicate. (The addressee's reply lives in `accept`.)
                    return Err("a friend request is already pending".into());
                }
                // requester == other: the addressee had ALREADY sent a pending
                // request to the caller (crossed requests). The caller's send
                // accepts it rather than erroring.
                self.db
                    .accept_friend_request(row_id)
                    .map_err(|e| format!("failed to accept friend request: {e}"))?;
                Ok(SendFriendRequestResult {
                    user_id: other_id,
                    status: "accepted".to_string(),
                })
            }
        }
    }

    /// List `user_id`'s relationships, split into incoming / outgoing /
    /// accepted. Each group's users carry their username + avatar.
    pub fn list(&self, user_id: i64) -> Result<FriendList, String> {
        self.db
            .user_by_id(user_id)
            .ok_or("user not found")?;
        let (incoming, outgoing, accepted) = self.db.friendships_for_user(user_id);
        Ok(FriendList {
            incoming: incoming.into_iter().map(|(_, other, _)| self.friend(other)).collect(),
            outgoing: outgoing.into_iter().map(|(_, other, _)| self.friend(other)).collect(),
            accepted: accepted.into_iter().map(|(_, other, _)| self.friend(other)).collect(),
        })
    }

    /// Accept the pending request from `user_id` addressed to `caller`.
    /// `caller` must be the *addressee* (the incoming side) of a pending row;
    /// the row is flipped to `'accepted'`. Returns the newly-accepted friend.
    ///
    /// # Errors
    /// - `user not found` — the caller is not a user.
    /// - `no pending friend request` — the pair has no pending row, or the
    ///   caller is not its addressee (a request the caller *sent* is cancelled
    ///   via [`Self::decline_or_cancel`], not accepted).
    pub fn accept(&self, caller: i64, user_id: i64) -> Result<FriendUser, String> {
        self.db
            .user_by_id(caller)
            .ok_or("user not found")?;
        let Some((row_id, requester, _addressee, status)) =
            self.db.friendship_between(caller, user_id)
        else {
            return Err("no pending friend request".into());
        };
        if status != "pending" || requester != user_id {
            return Err("no pending friend request".into());
        }
        self.db
            .accept_friend_request(row_id)
            .map_err(|e| format!("failed to accept friend request: {e}"))?;
        Ok(self.friend(user_id))
    }

    /// Drop the pending request between `caller` and `user_id` — declining an
    /// incoming one (caller is the addressee) or cancelling an outgoing one
    /// (caller is the requester). The relationship is gone. Returns the user
    /// the request with was dropped.
    ///
    /// # Errors
    /// - `user not found` — the caller is not a user.
    /// - `no pending friend request` — the pair has no pending row.
    pub fn decline_or_cancel(&self, caller: i64, user_id: i64) -> Result<FriendUser, String> {
        self.db
            .user_by_id(caller)
            .ok_or("user not found")?;
        let Some((row_id, _requester, _addressee, status)) =
            self.db.friendship_between(caller, user_id)
        else {
            return Err("no pending friend request".into());
        };
        if status != "pending" {
            return Err("no pending friend request".into());
        }
        self.db
            .delete_friendship(row_id)
            .map_err(|e| format!("failed to drop friend request: {e}"))?;
        Ok(self.friend(user_id))
    }

    /// Remove an accepted friend: `user_id` is dropped from `caller`'s
    /// friends. The row is deleted (a fresh request can be sent again later).
    /// Returns the removed friend.
    ///
    /// # Errors
    /// - `user not found` — the caller is not a user.
    /// - `not friends` — the pair has no `'accepted'` row.
    pub fn remove_friend(&self, caller: i64, user_id: i64) -> Result<FriendUser, String> {
        self.db
            .user_by_id(caller)
            .ok_or("user not found")?;
        let Some((row_id, _requester, _addressee, status)) =
            self.db.friendship_between(caller, user_id)
        else {
            return Err("not friends".into());
        };
        if status != "accepted" {
            return Err("not friends".into());
        }
        self.db
            .delete_friendship(row_id)
            .map_err(|e| format!("failed to remove friend: {e}"))?;
        Ok(self.friend(user_id))
    }

    /// Assemble the wire record for a user id (username + avatar).
    fn friend(&self, user_id: i64) -> FriendUser {
        let username = self
            .db
            .user_by_id(user_id)
            .map(|(_, n)| n)
            .unwrap_or_default();
        FriendUser {
            user_id,
            username,
            avatar_url: self.db.user_avatar(user_id),
        }
    }
}

// ── #[server] endpoints ─────────────────────────────────────────────────────
//
// Each mutating endpoint pushes a `FriendChanged` event to the *other* user
// (the non-actor) after the DB write succeeds — the DM-push pattern: the actor
// re-syncs from the RPC return, the other user is told live. The gateway's
// `WsServerMsg` variant carries only the other user's id + an action; the
// receiver re-fetches `list_friends` (no relationship data rides on the frame).

/// Send a friend request to the user named by `input.username`. A crossed
/// pending request is auto-accepted (see [`FriendService::send_request`]).
/// Pushes a `FriendChanged` event to the addressee.
///
/// The acting user is resolved from the verified session token.
#[server]
pub async fn send_friend_request(
    input: SendFriendRequestInput,
) -> Result<SendFriendRequestResult, ServerError> {
    let db = db_from_state()?;
    let user = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let result = FriendService::new(db.clone())
        .send_request(user, &input.username)
        .map_err(ServerError::failed)?;
    let action = if result.status == "accepted" { "accepted" } else { "requested" };
    crate::realtime::hub().publish(
        &[result.user_id],
        &crate::server_settings::WsServerMsg::FriendChanged {
            friend_id: result.user_id,
            action: action.to_string(),
        },
    );
    Ok(result)
}

/// List the acting user's friends: `incoming`, `outgoing`, `accepted`.
///
/// The acting user is resolved from the verified session token.
#[server]
pub async fn list_friends() -> Result<FriendList, ServerError> {
    let db = db_from_state()?;
    let user = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    FriendService::new(db).list(user).map_err(ServerError::failed)
}

/// Accept the pending request from `input.user_id` addressed to the caller.
/// Pushes a `FriendChanged` event to `input.user_id`.
///
/// The acting user is resolved from the verified session token.
#[server]
pub async fn accept_friend_request(
    input: AcceptFriendRequestInput,
) -> Result<FriendUser, ServerError> {
    let db = db_from_state()?;
    let user = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let friend = FriendService::new(db.clone())
        .accept(user, input.user_id)
        .map_err(ServerError::failed)?;
    crate::realtime::hub().publish(
        &[friend.user_id],
        &crate::server_settings::WsServerMsg::FriendChanged {
            friend_id: friend.user_id,
            action: "accepted".to_string(),
        },
    );
    Ok(friend)
}

/// Decline an incoming request from `input.user_id`, or cancel the caller's
/// own outgoing request to them. Pushes a `FriendChanged` event to
/// `input.user_id`.
///
/// The acting user is resolved from the verified session token.
#[server]
pub async fn decline_or_cancel(input: DeclineOrCancelInput) -> Result<FriendUser, ServerError> {
    let db = db_from_state()?;
    let user = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let friend = FriendService::new(db.clone())
        .decline_or_cancel(user, input.user_id)
        .map_err(ServerError::failed)?;
    crate::realtime::hub().publish(
        &[friend.user_id],
        &crate::server_settings::WsServerMsg::FriendChanged {
            friend_id: friend.user_id,
            action: "removed".to_string(),
        },
    );
    Ok(friend)
}

/// Remove the accepted friend `input.user_id`. Pushes a `FriendChanged` event
/// to `input.user_id`.
///
/// The acting user is resolved from the verified session token.
#[server]
pub async fn remove_friend(input: RemoveFriendInput) -> Result<FriendUser, ServerError> {
    let db = db_from_state()?;
    let user = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let friend = FriendService::new(db.clone())
        .remove_friend(user, input.user_id)
        .map_err(ServerError::failed)?;
    crate::realtime::hub().publish(
        &[friend.user_id],
        &crate::server_settings::WsServerMsg::FriendChanged {
            friend_id: friend.user_id,
            action: "removed".to_string(),
        },
    );
    Ok(friend)
}

/// Pull the shared `Db` out of server state (installed at startup by the
/// server bin via `server::install_state(db)`). `Db` is cheap to clone.
/// Server-only — the client RPC stub never executes a body.
#[cfg(feature = "server")]
fn db_from_state() -> Result<Db, ServerError> {
    server::use_state::<Db>()
        .ok_or_else(|| ServerError::failed("database not installed"))
}

// ── tests ────────────────────────────────────────────────────────────────────
//
// The tests exercise [`FriendService`] directly against a temp-file database
// (no HTTP / no server state / no realtime needed) — the same shape as
// `dms`'s unit tests. Each listed lifecycle case gets a rejection test for its
// failure mode.

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

    /// The user id of `username` in the seeded DB (for assert convenience).
    fn id_of(db: &Db, username: &str) -> i64 {
        db.user_by_username(username).unwrap().0
    }

    // ── request -> accept ───────────────────────────────────────────────────

    #[test]
    fn request_then_accept_is_accepted_for_both() {
        let (_dir, db) = setup_db();
        let svc = FriendService::new(db.clone());
        let alice = id_of(&db, "alice");
        let bob = id_of(&db, "bob");

        // Alice requests bob by username.
        let sent = svc.send_request(alice, "bob").expect("alice sends to bob");
        assert_eq!(sent.user_id, bob);
        assert_eq!(sent.status, "pending", "a fresh request is pending");

        // Alice's list: bob is OUTGOING. Bob's list: alice is INCOMING.
        let alice_list = svc.list(alice).unwrap();
        assert!(alice_list.outgoing.iter().any(|u| u.user_id == bob));
        assert!(alice_list.incoming.is_empty());
        assert!(alice_list.accepted.is_empty());
        let bob_list = svc.list(bob).unwrap();
        assert!(bob_list.incoming.iter().any(|u| u.user_id == alice));
        assert!(bob_list.outgoing.is_empty());
        assert!(bob_list.accepted.is_empty());

        // Bob accepts alice's incoming request.
        let accepted = svc.accept(bob, alice).expect("bob accepts alice");
        assert_eq!(accepted.user_id, alice);
        assert_eq!(accepted.username, "alice");

        // Now the pair is accepted for BOTH sides (and in neither request list).
        let alice_list = svc.list(alice).unwrap();
        assert!(
            alice_list.accepted.iter().any(|u| u.user_id == bob),
            "alice sees bob as accepted"
        );
        assert!(alice_list.outgoing.is_empty());
        let bob_list = svc.list(bob).unwrap();
        assert!(
            bob_list.accepted.iter().any(|u| u.user_id == alice),
            "bob sees alice as accepted"
        );
        assert!(bob_list.incoming.is_empty());
        // Exactly one relationship row remains, now accepted.
        let row = db.friendship_between(alice, bob).expect("one row remains");
        assert_eq!(row.3, "accepted");
        let count: i64 = db
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM friendships", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "exactly one friendship row for the pair");
    }

    #[test]
    fn accept_only_the_addressee_may_accept() {
        let (_dir, db) = setup_db();
        let svc = FriendService::new(db.clone());
        let alice = id_of(&db, "alice");
        let bob = id_of(&db, "bob");

        svc.send_request(alice, "bob").unwrap(); // alice -> bob pending
        // Bob is the addressee; alice (the requester) cannot "accept" her own
        // request — that is cancelled, not accepted.
        let err = svc
            .accept(alice, bob)
            .expect_err("the requester cannot accept their own request");
        assert_eq!(err, "no pending friend request");
        // And the pair is still pending (not flipped by the failed accept).
        assert_eq!(db.friendship_between(alice, bob).unwrap().3, "pending");
    }

    // ── decline / cancel ────────────────────────────────────────────────────

    #[test]
    fn decline_drops_the_incoming_request() {
        let (_dir, db) = setup_db();
        let svc = FriendService::new(db.clone());
        let alice = id_of(&db, "alice");
        let bob = id_of(&db, "bob");

        svc.send_request(alice, "bob").unwrap(); // alice -> bob pending
        // Bob declines alice's incoming request.
        let dropped = svc.decline_or_cancel(bob, alice).expect("bob declines");
        assert_eq!(dropped.user_id, alice);

        assert!(
            db.friendship_between(alice, bob).is_none(),
            "declining drops the row entirely"
        );
        // Both sides see no relationship.
        assert!(svc.list(alice).unwrap().incoming.is_empty());
        assert!(svc.list(alice).unwrap().outgoing.is_empty());
        assert!(svc.list(alice).unwrap().accepted.is_empty());
        assert!(svc.list(bob).unwrap().incoming.is_empty());
        assert!(svc.list(bob).unwrap().accepted.is_empty());
    }

    #[test]
    fn cancel_drops_the_outgoing_request() {
        let (_dir, db) = setup_db();
        let svc = FriendService::new(db.clone());
        let alice = id_of(&db, "alice");
        let bob = id_of(&db, "bob");

        svc.send_request(alice, "bob").unwrap(); // alice -> bob pending
        // Alice cancels her own outgoing request.
        let dropped = svc.decline_or_cancel(alice, bob).expect("alice cancels");
        assert_eq!(dropped.user_id, bob);
        assert!(db.friendship_between(alice, bob).is_none());
    }

    #[test]
    fn decline_or_cancel_requires_a_pending_row() {
        let (_dir, db) = setup_db();
        let svc = FriendService::new(db.clone());
        let alice = id_of(&db, "alice");
        let bob = id_of(&db, "bob");

        // No relationship at all -> nothing to decline/cancel.
        let err = svc
            .decline_or_cancel(alice, bob)
            .expect_err("no relationship, nothing to drop");
        assert_eq!(err, "no pending friend request");
        // An accepted pair is not a pending request either.
        svc.send_request(alice, "bob").unwrap();
        svc.accept(bob, alice).unwrap(); // now friends
        let err = svc
            .decline_or_cancel(alice, bob)
            .expect_err("friends is not a pending request");
        assert_eq!(err, "no pending friend request");
    }

    // ── remove friend ───────────────────────────────────────────────────────

    #[test]
    fn remove_friend_drops_an_accepted_pair() {
        let (_dir, db) = setup_db();
        let svc = FriendService::new(db.clone());
        let alice = id_of(&db, "alice");
        let bob = id_of(&db, "bob");

        svc.send_request(alice, "bob").unwrap();
        svc.accept(bob, alice).unwrap(); // friends
        assert!(db.friendship_between(alice, bob).is_some());

        let removed = svc.remove_friend(alice, bob).expect("alice removes bob");
        assert_eq!(removed.user_id, bob);
        assert!(
            db.friendship_between(alice, bob).is_none(),
            "removing drops the row"
        );
        assert!(svc.list(alice).unwrap().accepted.is_empty());
        assert!(svc.list(bob).unwrap().accepted.is_empty());
    }

    #[test]
    fn remove_friend_requires_friends() {
        let (_dir, db) = setup_db();
        let svc = FriendService::new(db.clone());
        let alice = id_of(&db, "alice");
        let bob = id_of(&db, "bob");

        // No relationship -> not friends.
        let err = svc
            .remove_friend(alice, bob)
            .expect_err("no relationship is not removable");
        assert_eq!(err, "not friends");
        // A pending (unaccepted) request is not a friend either.
        svc.send_request(alice, "bob").unwrap();
        let err = svc
            .remove_friend(alice, bob)
            .expect_err("a pending request is not a friend");
        assert_eq!(err, "not friends");
    }

    // ── failure modes ───────────────────────────────────────────────────────

    #[test]
    fn cannot_friend_yourself() {
        let (_dir, db) = setup_db();
        let svc = FriendService::new(db.clone());
        let alice = id_of(&db, "alice");
        let err = svc
            .send_request(alice, "alice")
            .expect_err("a user cannot friend themselves");
        assert!(err.contains("yourself"), "got: {err}");
    }

    #[test]
    fn unknown_username_is_rejected() {
        let (_dir, db) = setup_db();
        let svc = FriendService::new(db.clone());
        let alice = id_of(&db, "alice");
        let err = svc
            .send_request(alice, "ghost")
            .expect_err("an unknown username is rejected");
        assert_eq!(err, "user not found");
    }

    #[test]
    fn duplicate_request_is_rejected() {
        let (_dir, db) = setup_db();
        let svc = FriendService::new(db.clone());
        let alice = id_of(&db, "alice");

        svc.send_request(alice, "bob").unwrap(); // alice -> bob pending
        // Alice sends a second request to bob before bob replies: duplicate.
        let err = svc
            .send_request(alice, "bob")
            .expect_err("a duplicate pending request is rejected");
        assert!(err.contains("pending"), "got: {err}");
        // Exactly one row for the pair (no duplicate row was created).
        let count: i64 = db
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM friendships", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "exactly one friendship row for the pair");
    }

    #[test]
    fn sending_to_an_existing_friend_is_rejected() {
        let (_dir, db) = setup_db();
        let svc = FriendService::new(db.clone());
        let alice = id_of(&db, "alice");
        let bob = id_of(&db, "bob");

        svc.send_request(alice, "bob").unwrap();
        svc.accept(bob, alice).unwrap(); // friends
        let err = svc
            .send_request(alice, "bob")
            .expect_err("requesting an existing friend is rejected");
        assert!(err.contains("already friends"), "got: {err}");
    }

    #[test]
    fn crossed_requests_auto_accept() {
        let (_dir, db) = setup_db();
        let svc = FriendService::new(db.clone());
        let alice = id_of(&db, "alice");
        let bob = id_of(&db, "bob");

        // Alice requests bob (pending). Bob then requests alice — this is the
        // crossed case: it must accept the existing row, not error or
        // duplicate.
        svc.send_request(alice, "bob").unwrap();
        let crossed = svc.send_request(bob, "alice").expect("bob's send auto-accepts");
        assert_eq!(crossed.user_id, alice);
        assert_eq!(crossed.status, "accepted", "crossed request auto-accepts");

        // The pair is accepted for both, and there is exactly ONE row.
        assert!(svc.list(alice).unwrap().accepted.iter().any(|u| u.user_id == bob));
        assert!(svc.list(bob).unwrap().accepted.iter().any(|u| u.user_id == alice));
        let count: i64 = db
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM friendships", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "crossed requests collapse to a single row");
        assert_eq!(db.friendship_between(alice, bob).unwrap().3, "accepted");
    }

    #[test]
    fn re_request_after_removal_succeeds() {
        let (_dir, db) = setup_db();
        let svc = FriendService::new(db.clone());
        let alice = id_of(&db, "alice");
        let bob = id_of(&db, "bob");

        svc.send_request(alice, "bob").unwrap();
        svc.accept(bob, alice).unwrap();
        svc.remove_friend(alice, bob).unwrap(); // friends gone
        // A fresh request after removal is allowed (the row was deleted).
        let again = svc.send_request(alice, "bob").expect("re-request after removal");
        assert_eq!(again.status, "pending");
    }
}
