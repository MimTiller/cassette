//! Account signup: create a new user with a salted PBKDF2-HMAC-SHA256
//! password hash.
//!
//! ## Design
//!
//! Mirrors [`crate::invites`]: the password + persistence logic lives in a
//! pure, DB-backed service ([`AuthService`]) that takes the `Db` handle as a
//! plain argument and returns `Result<T, String>`, so it is fully
//! unit-testable against a temp-file SQLite database with no axum/tokio
//! stack. The `#[server]` fn below is a thin adapter: it pulls the shared
//! `Db` out of server state and maps a service failure to
//! `ServerError::failed`.
//!
//! The password KDF itself already lives in the data layer —
//! [`crate::db::hash_password`] produces the self-describing
//! `pbkdf2-sha256$<iterations>$<salt-b64>$<hash-b64>` form documented in
//! `migrations/0002_user_creds.sql`, and [`crate::db::insert_user`] persists
//! a row, erroring on the `users.username` `UNIQUE` constraint. This module
//! composes those two primitives; it does not re-implement the KDF.

use serde::{Deserialize, Serialize};
use server::{server, ServerError};

// The `Db` handle + password KDF are server-only (native SQLite / crypto),
// pulled in only under the `server` feature. The wire types (`LoginInput` /
// `SignupInput` / `LoginResult` / `SignupResult`) and the `#[server]` fns are
// shared: on the default-features wasm client build the fns compile to RPC
// stubs, so this import is only needed for the server body.
#[cfg(feature = "server")]
use crate::db::{hash_password, Db};

/// The wire input for [`signup`].
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SignupInput {
    /// The @handle to claim. Globally unique (case-insensitive); a second
    /// signup with the same handle is rejected.
    pub username: String,
    /// The plaintext password. Hashed (never stored) before persistence.
    pub password: String,
}

/// The result of a successful [`signup`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SignupResult {
    /// The id of the newly created user.
    pub user_id: i64,
    /// The username exactly as claimed.
    pub username: String,
}

// ── service ──────────────────────────────────────────────────────────────────
//
// The `AuthService` + its two `impl` blocks are server-only: they take the
// `Db` handle and run the persistence / KDF logic, none of which exists in the
// wasm client build. The `#[server]` fns below are the shared surface.

#[cfg(feature = "server")]
/// The auth service. Wraps a `Db` handle.
pub struct AuthService {
    db: Db,
}

#[cfg(feature = "server")]
impl AuthService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Create a new account for `username` with the plaintext `password`.
    ///
    /// The password is hashed (salted PBKDF2-HMAC-SHA256) and the row is
    /// inserted. Returns the new user id.
    ///
    /// # Errors
    /// - `username '…' is already taken` — another account already claims the
    ///   handle (the `users.username` `UNIQUE` constraint fired).
    /// - `failed to create user: …` — any other persistence failure.
    pub fn signup(&self, username: &str, password: &str) -> Result<SignupResult, String> {
        let hash = hash_password(password);
        match self.db.insert_user(username, &hash) {
            Ok(id) => Ok(SignupResult {
                user_id: id,
                username: username.to_string(),
            }),
            Err(e) if e.to_string().contains("UNIQUE") => {
                Err(format!("username '{username}' is already taken"))
            }
            Err(e) => Err(format!("failed to create user: {e}")),
        }
    }
}

// ── #[server] endpoint ───────────────────────────────────────────────────────

/// Create a new account. The password is salted + hashed server-side and is
/// never stored in plaintext. A second signup with an already-claimed handle
/// is rejected with a clear error.
#[server]
pub async fn signup(input: SignupInput) -> Result<SignupResult, ServerError> {
    let db = db_from_state()?;
    AuthService::new(db)
        .signup(&input.username, &input.password)
        .map_err(ServerError::failed)
}

// ── login ───────────────────────────────────────────────────────────────────

/// The per-account login-failure policy: after `LOGIN_MAX_FAILURES` wrong
/// passwords, the account is locked for `LOGIN_LOCKOUT_SECS`. Server-only —
/// the wire types below are shared with the client build.
#[cfg(feature = "server")]
const LOGIN_MAX_FAILURES: i64 = 5;
#[cfg(feature = "server")]
const LOGIN_LOCKOUT_SECS: i64 = 300;

/// The wire input for [`login`].
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoginInput {
    pub username: String,
    pub password: String,
}

/// The result of a successful [`login`]: the account's numeric id and a
/// signed, expiring session [`token`]. The client sends the token back as
/// `Authorization: Bearer <token>` on every subsequent request — identity
/// comes only from a token whose HMAC-SHA256 signature verifies against the
/// server secret (see [`crate::session`]).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct LoginResult {
    pub user_id: i64,
    pub token: String,
    /// The account's username, echoed back so the client can label the signed-in
    /// identity (the profile strip) with the real name rather than an id or the
    /// mock "you" placeholder.
    pub username: String,
}

#[cfg(feature = "server")]
impl AuthService {
    /// Authenticate `username` / `password` against the stored PBKDF2 hash.
    ///
    /// Per-account lockout: once `LOGIN_MAX_FAILURES` failures are recorded
    /// within the trailing `LOGIN_LOCKOUT_SECS` window, every further attempt
    /// is rejected until the window slides past the oldest recorded failure —
    /// regardless of whether the password is right. A wrong password records
    /// one failure; a successful login clears the account's failure log.
    ///
    /// # Errors
    /// - `too many failed login attempts — account locked for N seconds` —
    ///   the per-account lockout window is open.
    /// - `invalid username or password` — no such account, or the password
    ///   does not verify. Both cases are reported identically (and both
    ///   record a failure when the account exists) so the endpoint never
    ///   leaks which usernames are registered.
    pub fn login(&self, username: &str, password: &str) -> Result<LoginResult, String> {
        let (user_id, stored_username, stored_hash) = self
            .db
            .user_by_username_creds(username)
            .ok_or_else(|| "invalid username or password".to_string())?;

        // Lockout is checked BEFORE the password is verified: a locked
        // account rejects everything, even the right password, so probing
        // during the window is useless.
        let failures = self.db.login_failures(user_id, LOGIN_LOCKOUT_SECS);
        if failures >= LOGIN_MAX_FAILURES {
            let retry_after = self.retry_after_secs(user_id);
            return Err(format!(
                "too many failed login attempts — account locked for {retry_after} seconds"
            ));
        }

        if !crate::db::verify_password(&stored_hash, password) {
            self.db
                .record_login_failure(user_id)
                .map_err(|e| format!("failed to record login failure: {e}"))?;
            return Err("invalid username or password".to_string());
        }

        // Success: clear the failure log so the next brute-force burst
        // starts from a clean slate (the floor resets).
        self.db
            .clear_login_failures(user_id)
            .map_err(|e| format!("failed to clear login failures: {e}"))?;
        Ok(LoginResult { user_id, token: crate::session::issue_token(user_id), username: stored_username })
    }

    /// Seconds until the account's lockout window slides past its oldest
    /// recorded failure (i.e. the failure count drops back under the
    /// threshold). Used only in the lockout error message.
    fn retry_after_secs(&self, user_id: i64) -> u64 {
        let oldest: Option<i64> = self
            .db
            .lock()
            .ok()
            .and_then(|c| {
                c.query_row(
                    "SELECT MIN(at) FROM login_failures WHERE user_id = ?1",
                    rusqlite::params![user_id],
                    |r| r.get::<_, i64>(0),
                )
                .ok()
            });
        let Some(oldest) = oldest else { return 1 };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        (oldest + LOGIN_LOCKOUT_SECS - now).max(1).unsigned_abs()
    }
}

/// Authenticate with a username + password. On success returns the account's
/// numeric id and a signed, expiring session token, which the client must
/// send back as `Authorization: Bearer <token>` on every subsequent request
/// (identity is resolved from the token's HMAC-SHA256 signature — see
/// [`crate::session`] — rather than from a client-supplied id).
///
/// Wrong passwords are rejected with a uniform error (the account is never
/// named) and record a per-account failure; after `LOGIN_MAX_FAILURES`
/// failures within `LOGIN_LOCKOUT_SECS` the account is locked and even a
/// correct password is rejected until the window passes. A successful login
/// clears the account's failure log.
#[server]
pub async fn login(input: LoginInput) -> Result<LoginResult, ServerError> {
    let db = db_from_state()?;
    AuthService::new(db)
        .login(&input.username, &input.password)
        .map_err(ServerError::failed)
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
// The tests exercise [`AuthService`] directly against a temp-file database
// (no HTTP / no server state needed), the same way [`crate::invites::tests`]
// does.

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use std::path::Path;

    /// Build a temp-file database with all migrations applied (empty tables).
    fn setup_db() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let db = Db::migrate(&db_path, &migrations_dir).unwrap();
        (dir, db)
    }

    #[test]
    fn signup_succeeds() {
        let (_dir, db) = setup_db();
        let svc = AuthService::new(db.clone());

        let res = svc
            .signup("nova", "hunter2-secure")
            .expect("a fresh username signs up");
        assert!(res.user_id > 0, "a new id is minted");
        assert_eq!(res.username, "nova");

        // The user is really in the table, and the password is stored hashed
        // (never plaintext) in the documented `pbkdf2-sha256$…` form.
        let (id, username, hash) = db
            .user_by_username_creds("nova")
            .expect("the row is readable back");
        assert_eq!(id, res.user_id);
        assert_eq!(username, "nova");
        assert!(
            hash.starts_with("pbkdf2-sha256$"),
            "stored hash uses the documented prefix, got: {hash}"
        );
        assert!(
            !hash.contains("hunter2-secure"),
            "the plaintext password is never stored"
        );
        // The stored hash verifies against the original password.
        assert!(crate::db::verify_password(&hash, "hunter2-secure"));
    }

    #[test]
    fn signup_rejects_duplicate_username() {
        let (_dir, db) = setup_db();
        let svc = AuthService::new(db.clone());

        // First signup succeeds.
        assert!(svc.signup("taken", "pw-one").is_ok());

        // A second signup with the same handle is rejected.
        let err = svc
            .signup("taken", "pw-two")
            .expect_err("a duplicate username is rejected");
        assert!(
            err.contains("already taken"),
            "the error names the duplicate, got: {err}"
        );

        // Only one user exists for that handle, and it still verifies against
        // the FIRST password (the second attempt changed nothing).
        let creds = db.user_by_username_creds("taken").expect("still readable");
        assert!(crate::db::verify_password(&creds.2, "pw-one"));
        assert!(!crate::db::verify_password(&creds.2, "pw-two"), "the password was not overwritten");
    }

    // ── login ─────────────────────────────────────────────────────────────

    #[test]
    fn login_succeeds_with_correct_credentials() {
        let (_dir, db) = setup_db();
        let svc = AuthService::new(db.clone());

        svc.signup("nova", "hunter2-secure").expect("signup succeeds");

        // A correct username + password authenticates and returns the user's
        // numeric id (the value the client sends back as `x-user-id`).
        let res = svc
            .login("nova", "hunter2-secure")
            .expect("a correct password logs in");
        let (id, _username, _hash) = db
            .user_by_username_creds("nova")
            .expect("the row is readable back");
        assert_eq!(res.user_id, id, "the returned id is the user's numeric id");
        assert!(res.user_id > 0);

        // A successful login clears any recorded failures.
        db.record_login_failure(id).expect("seed a failure");
        assert_eq!(
            db.login_failures(id, LOGIN_LOCKOUT_SECS),
            1,
            "a failure is recorded before the login"
        );
        svc.login("nova", "hunter2-secure").expect("login clears the floor");
        assert_eq!(
            db.login_failures(id, LOGIN_LOCKOUT_SECS),
            0,
            "a successful login resets the failure count"
        );
    }

    #[test]
    fn login_fails_with_wrong_password_and_records_failure() {
        let (_dir, db) = setup_db();
        let svc = AuthService::new(db.clone());

        svc.signup("nova", "hunter2-secure").expect("signup succeeds");
        let (id, _username, _hash) = db
            .user_by_username_creds("nova")
            .expect("the row is readable back");

        // A wrong password is rejected with the uniform credential error and
        // records one per-account failure.
        let err = svc
            .login("nova", "totally-wrong")
            .expect_err("a wrong password is rejected");
        assert!(
            err.contains("invalid username or password"),
            "the error is the uniform credential message, got: {err}"
        );
        assert_eq!(
            db.login_failures(id, LOGIN_LOCKOUT_SECS),
            1,
            "a wrong password records one failure"
        );

        // The stored hash is unchanged — the account still verifies against
        // the ORIGINAL password (the failed attempt did not overwrite it).
        let (_id2, _u, hash) = db.user_by_username_creds("nova").unwrap();
        assert!(crate::db::verify_password(&hash, "hunter2-secure"));
    }

    #[test]
    fn repeated_wrong_passwords_trigger_lockout() {
        let (_dir, db) = setup_db();
        let svc = AuthService::new(db.clone());

        svc.signup("nova", "hunter2-secure").expect("signup succeeds");
        let (id, _username, _hash) = db
            .user_by_username_creds("nova")
            .expect("the row is readable back");

        // Hammer the account with wrong passwords until the failure count
        // reaches the threshold.
        for _ in 0..LOGIN_MAX_FAILURES {
            let err = svc
                .login("nova", "wrong-guess")
                .expect_err("each wrong password is rejected");
            assert!(
                err.contains("invalid username or password"),
                "below the threshold the error is the credential message, got: {err}"
            );
        }
        assert_eq!(
            db.login_failures(id, LOGIN_LOCKOUT_SECS),
            LOGIN_MAX_FAILURES,
            "the threshold number of failures is recorded"
        );

        // The account is now locked: even the CORRECT password is rejected
        // with a clear, distinct lockout error (not the credential error).
        let err = svc
            .login("nova", "hunter2-secure")
            .expect_err("a locked account rejects even the right password");
        assert!(
            err.contains("too many failed login attempts") && err.contains("locked"),
            "the lockout error is clear and distinct, got: {err}"
        );

        // The lockout is per-account: a different account with a clean
        // failure log can still log in.
        svc.signup("other", "another-pw").expect("second account signs up");
        assert!(
            svc.login("other", "another-pw").is_ok(),
            "lockout is per-account, not global"
        );
    }
}
