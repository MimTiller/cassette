//! Server invite links: generate a shareable invite code for a server and
//! join a server by redeeming a valid code.
//!
//! ## Design
//!
//! Mirrors [`crate::server_settings`]: the authorization + redemption logic
//! lives in a pure, DB-backed service ([`InviteService`]) that takes the DB
//! handle and the acting user id as plain arguments and returns
//! `Result<T, String>`, so it is fully unit-testable against a temp-file
//! SQLite database with no axum/tokio stack. The `#[server]` fns below are
//! thin adapters: they pull the shared `Db` out of server state, resolve the
//! acting user from the `x-user-id` request header, and map a service failure
//! to `ServerError::failed`.
//!
//! ## Model
//!
//! An *invite* is a row in `server_invites` naming a server, a globally
//! unique short `code`, the member who created it, an optional `expires_at`
//! (UTC epoch seconds; `None` = never expires), and an optional `max_uses`
//! cap.
//!
//! - **Generate** — a moderator+ (owner/admin/moderator) of a server mints a
//!   fresh code, optionally with an expiration and a use cap. The code is
//!   drawn from an unambiguous alphabet and retried on the (astronomically
//!   unlikely) global `UNIQUE` collision.
//! - **Redeem** — any authenticated user joins the server the code belongs
//!   to, as a plain member. Redemption is rejected when the code is unknown,
//!   expired, or fully spent. Re-deeding an already-member user is a
//!   no-op (their existing role is never demoted).
//!
//! ## Expiry
//!
//! Expiry is enforced at *redemption* time, never by a sweep: `redeem`
//! compares the row's `expires_at` against the current wall-clock and
//! rejects when it is in the past. The service reads "now" through an
//! injectable clock so the expiry test can pin time without sleeping.

use serde::{Deserialize, Serialize};
use server::{server, ServerError};

use crate::db::{Db, InviteRow};
use crate::roles::{Action, Role};

/// The acting user id from the `x-user-id` header (the same stand-in session
/// mechanism [`crate::server_settings`] uses); `None` when absent.
fn acting_user() -> Option<i64> {
    server::use_request_header("x-user-id")?.parse().ok()
}

/// The current wall-clock seconds since the Unix epoch.
fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(1_000)
}

// ── invite codes ────────────────────────────────────────────────────────────
//
// A readable, unambiguous alphabet: consonants + a few vowels, excluding the
// characters most easily confused (0/O, 1/l/I, etc.) and the ones that make a
// bare token look like a reserved word. 22 symbols over 4 characters is
// ~234k distinct codes — far more than any server needs, and short enough to
// type. `rand` is a server-only dep (pulled in by the `server` feature), and
// this whole module compiles only under that feature.

const CODE_ALPHABET: &[u8] = b"aeikmnoqrstuwyzbcdfgjlhpv";
const CODE_LEN: usize = 4;

fn gen_code() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let mut s = String::with_capacity(CODE_LEN);
    for _ in 0..CODE_LEN {
        let i = rng.gen_range(0..CODE_ALPHABET.len());
        s.push(CODE_ALPHABET[i] as char);
    }
    s
}

/// How many times a freshly generated code may be redeemed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UseLimit {
    /// Unlimited redemptions (the default).
    Unlimited,
    /// Redeemable at most `n` times in total.
    Max(i64),
}

impl UseLimit {
    fn to_max_uses(self) -> Option<i64> {
        match self {
            UseLimit::Unlimited => None,
            UseLimit::Max(n) => Some(n),
        }
    }
}

// ── wire types ───────────────────────────────────────────────────────────────

/// Input for [`create_invite`].
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateInviteInput {
    pub server_id: i64,
    /// Optional expiration as UTC epoch *seconds*. `None` / omitted = the
    /// invite never expires.
    #[serde(default)]
    pub expires_at: Option<i64>,
    /// Optional cap on the number of redemptions. `None` / omitted =
    /// unlimited.
    #[serde(default)]
    pub max_uses: Option<i64>,
}

/// Input for [`redeem_invite`]. The code alone identifies the server (like a
/// real Discord invite), so no `server_id` is needed.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RedeemInviteInput {
    /// The invite code to redeem.
    pub code: String,
}

/// The wire record of an invite, as returned by `create_invite`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Invite {
    pub id: i64,
    pub server_id: i64,
    /// The shareable code (the redeemable handle).
    pub code: String,
    pub created_by: i64,
    pub created_at: i64,
    /// Expiry in UTC epoch seconds, or `None` if it never expires.
    pub expires_at: Option<i64>,
    /// The redemption cap, or `None` if unlimited.
    pub max_uses: Option<i64>,
    /// How many times the code has been redeemed so far.
    pub uses: i64,
}

impl From<InviteRow> for Invite {
    fn from(r: InviteRow) -> Self {
        Self {
            id: r.id,
            server_id: r.server_id,
            code: r.code,
            created_by: r.created_by,
            created_at: r.created_at,
            expires_at: r.expires_at,
            max_uses: r.max_uses,
            uses: r.uses,
        }
    }
}

/// The result of [`redeem_invite`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct RedeemResult {
    pub server_id: i64,
    /// The user who redeemed (the acting user).
    pub user_id: i64,
    /// `true` when the user was already a member and nothing changed;
    /// `false` when this redemption added them.
    pub already_member: bool,
    /// The member's role after the op (always `'member'`).
    pub role: String,
}

// ── service ──────────────────────────────────────────────────────────────────

/// The invite service. Wraps a `Db` handle. Every method takes the acting
/// user id explicitly so the authorization is testable without any request
/// context.
pub struct InviteService {
    db: Db,
    /// Injectable "now" in UTC epoch seconds. Production passes
/// [`now_secs`]; tests pass a fixed value to pin expiry.
    clock: Box<dyn Fn() -> i64>,
}

impl InviteService {
    /// A service that reads the real wall-clock.
    pub fn new(db: Db) -> Self {
        Self {
            db,
            clock: Box::new(now_secs),
        }
    }

    /// A service with a fixed "now" — used by the expiry tests.
    #[cfg(test)]
    pub fn with_clock(db: Db, now: i64) -> Self {
        Self {
            db,
            clock: Box::new(move || now),
        }
    }

    /// Generate a new invite for `server_id`, created by `caller_id`.
    ///
    /// `expires_at` is an optional expiration in UTC epoch seconds (ignored /
    /// `None` = never expires); `max_uses` is an optional redemption cap
    /// (`None` = unlimited).
    ///
    /// # Errors
    /// - `not found` — the server id doesn't exist.
    /// - `permission denied` — the caller's role is below moderator.
    /// - `expiration must be in the future` — `expires_at` is at or before now.
    /// - `max_uses must be positive`.
    pub fn create_invite(
        &self,
        caller_id: i64,
        server_id: i64,
        expires_at: Option<i64>,
        max_uses: Option<i64>,
    ) -> Result<Invite, String> {
        self.db
            .server_by_id(server_id)
            .ok_or("server not found")?;
        self.require(caller_id, server_id, Action::AddMember)?;
        let now = (self.clock)();
        if let Some(exp) = expires_at {
            if exp <= now {
                return Err("expiration must be in the future".into());
            }
        }
        if let Some(n) = max_uses {
            if n <= 0 {
                return Err("max_uses must be positive".into());
            }
        }
        // Retry on a global code collision (the UNIQUE constraint).
        for _ in 0..100 {
            let code = gen_code();
            match self.db.insert_invite(
                server_id,
                &code,
                caller_id,
                now,
                expires_at,
                max_uses,
            ) {
                Ok(row) => return Ok(Invite::from(row)),
                Err(e) if e.to_string().contains("UNIQUE") => continue,
                Err(e) => return Err(format!("failed to create invite: {e}")),
            }
        }
        Err("could not allocate a unique invite code".into())
    }

    /// Redeem `code`: the acting user (`user_id`) joins the server the code
    /// belongs to, as a plain member.
    ///
    /// # Errors
    /// - `unauthenticated` is handled by the adapter (a missing `x-user-id`);
    ///   here `user_id` is always present.
    /// - `invite not found` — no live invite carries this code.
    /// - `invite expired` — the code's `expires_at` is at or before now.
    /// - `invite has been fully redeemed` — a capped code hit its `max_uses`.
    pub fn redeem_invite(&self, user_id: i64, code: &str) -> Result<RedeemResult, String> {
        let invite = self
            .db
            .invite_by_code(code)
            .ok_or("invite not found")?;
        let server_id = invite.server_id;
        let now = (self.clock)();

        // Expiry gate (before spending a use): a lapsed code is dead.
        if let Some(exp) = invite.expires_at {
            if now >= exp {
                return Err("invite expired".into());
            }
        }

        // Already a member? Redeeming is a no-op, and we must NOT spend a use
        // nor touch their (possibly elevated) role.
        if self.db.membership_role(server_id, user_id).is_some() {
            return Ok(RedeemResult {
                server_id,
                user_id,
                already_member: true,
                role: "member".to_string(),
            });
        }

        // Spend a redemption. The DB layer only increments when budget
        // remains (unlimited, or `uses < max_uses`), so a fully-spent capped
        // code yields 0 and is rejected here — no use is lost.
        match self.db.redeem_invite_use(invite.id) {
            Ok(0) => return Err("invite has been fully redeemed".into()),
            Ok(_) => {}
            Err(e) => return Err(format!("failed to redeem invite: {e}")),
        }

        // Add the member as a plain member. (The user is guaranteed NOT to be
        // a member here, so the upsert's DO-UPDATE arm can't fire; an
        // explicit role is the only way an invite would demote, and it can't
        // be a member yet.)
        self.db
            .upsert_membership(server_id, user_id, Role::Member, now)
            .map_err(|e| format!("failed to add member: {e}"))?;

        Ok(RedeemResult {
            server_id,
            user_id,
            already_member: false,
            role: "member".to_string(),
        })
    }

    /// The caller's role in `server_id` (a membership is required).
    fn caller_role(&self, server_id: i64, caller_id: i64) -> Result<Role, String> {
        let role_str = self
            .db
            .membership_role(server_id, caller_id)
            .ok_or_else(|| "permission denied: not a member of this server".to_string())?;
        Role::from_db_str(&role_str)
            .ok_or_else(|| format!("permission denied: unknown stored role {role_str:?}"))
    }

    /// Gate the caller on `action` in `server_id` (membership + policy).
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
}

// ── #[server] endpoints ─────────────────────────────────────────────────────

/// Generate an invite code for a server. Moderator+ (owner, admin, or
/// moderator) may create invites. `expires_at` / `max_uses` are optional.
///
/// The acting user is read from the `x-user-id` request header.
#[server]
pub async fn create_invite(input: CreateInviteInput) -> Result<Invite, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    InviteService::new(db)
        .create_invite(caller, input.server_id, input.expires_at, input.max_uses)
        .map_err(ServerError::failed)
}

/// Join a server by redeeming a valid invite code. Any authenticated user may
/// redeem; the code alone names the server.
///
/// The acting user is read from the `x-user-id` request header.
#[server]
pub async fn redeem_invite(input: RedeemInviteInput) -> Result<RedeemResult, ServerError> {
    let db = db_from_state()?;
    let user = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    InviteService::new(db)
        .redeem_invite(user, &input.code)
        .map_err(ServerError::failed)
}

/// Pull the shared `Db` out of server state (installed at startup by the
/// server bin via `server::install_state(db)`). `Db` is cheap to clone.
fn db_from_state() -> Result<Db, ServerError> {
    server::use_state::<Db>()
        .ok_or_else(|| ServerError::failed("database not installed"))
}

// ── tests ────────────────────────────────────────────────────────────────────
//
// The tests exercise [`InviteService`] directly against a temp-file database
// (no HTTP / no server state needed). Expiry is pinned by a fixed clock. A
// rejection case is included for each failure mode.

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Build a temp-file database, run migrations, seed one server with an
    /// owner (user 1), a moderator (user 2), and a plain member (user 3), and
    /// a *second* user (user 4) who is a member of no server (the joiner).
    fn setup_db() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let db = Db::migrate(&db_path, &migrations_dir).unwrap();

        let c = db.lock().unwrap();
        c.execute_batch(
            "INSERT INTO users (id, username, email, created_at) VALUES
                (1, 'owner',  'owner@example.com',  1000),
                (2, 'mod',    'mod@example.com',    1000),
                (3, 'bob',    'bob@example.com',    1000),
                (4, 'joiner', 'joiner@example.com', 1000);
             INSERT INTO servers (id, name, owner_id, icon_url, created_at)
                VALUES (1, 'My Server', 1, NULL, 1000);
             INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES
                (1, 1, 'owner',     1000),
                (1, 2, 'moderator', 1000),
                (1, 3, 'member',    1000);",
        )
        .unwrap();
        drop(c);
        (dir, db)
    }

    // ── generation ──────────────────────────────────────────────────────────

    #[test]
    fn create_invite_succeeds_for_moderator() {
        let (_dir, db) = setup_db();
        let svc = InviteService::new(db.clone());
        // A moderator may mint an invite (the common case).
        let inv = svc
            .create_invite(2, 1, None, None)
            .expect("moderator can create an invite");
        assert_eq!(inv.server_id, 1);
        assert_eq!(inv.created_by, 2);
        assert_eq!(inv.uses, 0);
        assert_eq!(inv.max_uses, None, "no cap requested");
        assert_eq!(inv.expires_at, None, "no expiry requested");
        assert_eq!(inv.code.len(), CODE_LEN);
        // The code is globally unique (not reused by a second generation).
        let inv2 = svc.create_invite(2, 1, None, None).unwrap();
        assert_ne!(inv.code, inv2.code, "codes are unique");
    }

    #[test]
    fn create_invite_rejects_plain_member() {
        let (_dir, db) = setup_db();
        let svc = InviteService::new(db);
        // A plain member (user 3) is below moderator.
        let err = svc
            .create_invite(3, 1, None, None)
            .expect_err("plain member cannot create invites");
        assert!(err.starts_with("permission denied"), "got: {err}");
    }

    #[test]
    fn create_invite_rejects_past_expiration() {
        let (_dir, db) = setup_db();
        // Pin the clock at 1_000_000; an expiry at or before it is rejected.
        let svc = InviteService::with_clock(db.clone(), 1_000_000);
        assert!(
            svc.create_invite(2, 1, Some(1_000_000), None)
                .is_err(),
            "an expiry equal to now is not in the future"
        );
        assert!(
            svc.create_invite(2, 1, Some(500_000), None)
                .is_err(),
            "a past expiry is rejected"
        );
        // A future expiry is accepted.
        assert!(svc
            .create_invite(2, 1, Some(2_000_000), None)
            .is_ok(), "a future expiry is accepted");
    }

    #[test]
    fn create_invite_rejects_nonpositive_max_uses() {
        let (_dir, db) = setup_db();
        let svc = InviteService::new(db.clone());
        assert!(
            svc.create_invite(2, 1, None, Some(0)).is_err(),
            "max_uses of 0 is rejected"
        );
        assert!(svc
            .create_invite(2, 1, None, Some(5))
            .is_ok(), "a positive max_uses is accepted");
    }

    #[test]
    fn create_invite_rejects_unknown_server() {
        let (_dir, db) = setup_db();
        let svc = InviteService::new(db);
        assert!(
            svc.create_invite(1, 999, None, None).is_err(),
            "unknown server id is rejected"
        );
    }

    // ── redemption ──────────────────────────────────────────────────────────

    #[test]
    fn redeem_invite_adds_member() {
        let (_dir, db) = setup_db();
        let svc = InviteService::new(db.clone());
        let inv = svc.create_invite(2, 1, None, None).unwrap();

        // User 4 (the joiner) is not yet a member.
        assert!(db.membership_role(1, 4).is_none());
        let res = svc
            .redeem_invite(4, &inv.code)
            .expect("a valid invite redeems");
        assert_eq!(res.server_id, 1);
        assert_eq!(res.user_id, 4);
        assert!(!res.already_member, "this redemption added the user");
        assert_eq!(res.role, "member");
        // They are really a member now, as a plain member.
        assert_eq!(db.membership_role(1, 4).as_deref(), Some("member"));
    }

    #[test]
    fn redeem_invite_unknown_code_fails() {
        let (_dir, db) = setup_db();
        let svc = InviteService::new(db);
        let err = svc
            .redeem_invite(4, "nope")
            .expect_err("unknown code is rejected");
        assert_eq!(err, "invite not found");
    }

    #[test]
    fn redeem_invite_already_member_is_noop() {
        let (_dir, db) = setup_db();
        let svc = InviteService::new(db.clone());
        let inv = svc.create_invite(2, 1, None, None).unwrap();

        // User 3 is already a member of server 1.
        assert!(db.membership_role(1, 3).is_some());
        let res = svc
            .redeem_invite(3, &inv.code)
            .expect("redeeming as an existing member is a no-op");
        assert!(res.already_member, "already a member");
        // Their role is untouched (not demoted / not re-stamped).
        assert_eq!(db.membership_role(1, 3).as_deref(), Some("member"));
        // And no use was spent (a no-op must not burn the cap).
        assert_eq!(
            db.invite_by_code(&inv.code).unwrap().uses,
            0,
            "no-op does not consume a use"
        );
    }

    #[test]
    fn redeem_invite_respects_max_uses() {
        let (_dir, db) = setup_db();
        let svc = InviteService::new(db.clone());
        // Two separate joiners (5, 6) so each redemption is a real join.
        let c = db.lock().unwrap();
        c.execute_batch(
            "INSERT INTO users (id, username, email, created_at) VALUES
                (5, 'a', 'a@example.com', 1000),
                (6, 'b', 'b@example.com', 1000);",
        )
        .unwrap();
        drop(c);

        let inv = svc
            .create_invite(2, 1, None, Some(2))
            .expect("capped invite");
        assert_eq!(inv.max_uses, Some(2));

        assert!(svc.redeem_invite(5, &inv.code).is_ok());
        assert!(svc.redeem_invite(6, &inv.code).is_ok());
        // The third redemption is over the cap.
        let err = svc
            .redeem_invite(4, &inv.code)
            .expect_err("a spent code is rejected");
        assert_eq!(err, "invite has been fully redeemed");
        // User 4 was NOT added.
        assert!(db.membership_role(1, 4).is_none());
        // Uses is exactly 2.
        assert_eq!(db.invite_by_code(&inv.code).unwrap().uses, 2);
    }

    // ── expiry (the key requirement) ────────────────────────────────────────

    #[test]
    fn redeem_invite_expired_rejected() {
        let (_dir, db) = setup_db();
        // Create an invite that expires at t = 1_000_000 (created at t = 999_999,
        // i.e. just under its expiry so it is valid at creation time).
        let svc_before = InviteService::with_clock(db.clone(), 999_999);
        let inv = svc_before
            .create_invite(2, 1, Some(1_000_000), None)
            .expect("invite created just before its expiry");
        assert_eq!(inv.expires_at, Some(1_000_000));

        // Still valid at t = 999_999 (now < expires_at).
        let ok_svc = InviteService::with_clock(db.clone(), 999_999);
        assert!(
            ok_svc.redeem_invite(4, &inv.code).is_ok(),
            "valid at creation time"
        );
        // ...and user 4 was added.
        assert!(db.membership_role(1, 4).is_some());

        // Now jump the clock past the expiry. The same code is now dead.
        let expired_svc = InviteService::with_clock(db.clone(), 1_000_000);
        let err = expired_svc
            .redeem_invite(4, &inv.code)
            .expect_err("an expired invite is rejected");
        assert_eq!(err, "invite expired");

        // A *fresh* non-member can't use it either.
        let c = db.lock().unwrap();
        c.execute_batch("INSERT INTO users (id, username, email, created_at) VALUES
                (7, 'c', 'c@example.com', 1000);").unwrap();
        drop(c);
        assert_eq!(
            expired_svc
                .redeem_invite(7, &inv.code)
                .unwrap_err(),
            "invite expired"
        );
        assert!(db.membership_role(1, 7).is_none(), "expired code adds nobody");
    }

    #[test]
    fn redeem_invite_never_expires() {
        let (_dir, db) = setup_db();
        // An invite with no expiry is redeemable at any "now".
        let svc_now = InviteService::with_clock(db.clone(), 1_000);
        let inv = svc_now
            .create_invite(2, 1, None, None)
            .expect("no-expiry invite");
        assert_eq!(inv.expires_at, None);

        // Jump the clock a year forward; still valid.
        let svc_later = InviteService::with_clock(db.clone(), 1_000_000_000);
        assert!(
            svc_later
                .redeem_invite(4, &inv.code)
                .is_ok(),
            "a never-expiring invite stays valid"
        );
    }
}
