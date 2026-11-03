//! The identity-provider seam in front of login and signup.
//!
//! ## Design
//!
//! `login` will one day authenticate against an external identity service — a
//! central authority that certifies each user's key, which community servers
//! trust instead of storing passwords. Today it authenticates username +
//! password against the stored PBKDF2 hash. This module is the seam that lets
//! that service plug in later with no other restructure: the credential check
//! lives behind [`IdentityProvider`], which turns credentials into a verified
//! [`VerifiedIdentity`] (`issuer` names who certified it, `subject` is the
//! provider's account id, `suggested_username` the handle to claim).
//! [`crate::auth`]'s `login` asks the provider, maps the identity to a local
//! account, and issues the session token; [`crate::session`] is untouched.
//!
//! [`PasswordProvider`] is the only implementation today and keeps the exact
//! policy `login` used to carry inline: per-account lockout checked BEFORE
//! verification, a timing-burning miss for unknown usernames, failure
//! recording/clearing, and hash upgrades. The provider choice lives in the
//! ONE place [`login_provider`] names; a second provider implements the trait
//! and swaps in there.
//!
//! Like `auth`, the module is server-only: the `#[server]` fns in `auth.rs`
//! compile to client RPC stubs on the wasm build and never reference this.

// The password KDF + the user table, the same server-only data layer `auth`
// uses.
#[cfg(feature = "server")]
use crate::db::{hash_password, verify_password, burn_password_check, password_needs_rehash, Db};

// The per-account login-failure policy the password provider enforces;
// declared here since the provider owns it, and `crate::auth` names it too.
#[cfg(feature = "server")]
pub const LOGIN_MAX_FAILURES: i64 = 5;
#[cfg(feature = "server")]
pub const LOGIN_LOCKOUT_SECS: i64 = 300;

#[cfg(feature = "server")]
/// Credentials handed to an [`IdentityProvider`]: what the client sent. An
/// external IdP will carry more (an assertion, a key); the provider decides
/// what to read.
pub struct Credentials {
    pub username: String,
    pub password: String,
}

#[cfg(feature = "server")]
/// A credential check that passed, as the provider sees the account: `issuer`
/// names who certified it ("password" for the local scheme, later an IdP URL),
/// `subject` is the provider's account id (opaque string), and
/// `suggested_username` is the handle to claim for it.
/// [`crate::auth`] maps this pair to a local user id.
#[derive(Debug, Clone)]
pub struct VerifiedIdentity {
    pub issuer: String,
    pub subject: String,
    pub suggested_username: String,
}

#[cfg(feature = "server")]
/// A credential verifier: `authenticate` returns a verified identity or an
/// error string (`#[server]` fns map it to `ServerError::failed`).
pub trait IdentityProvider {
    fn authenticate(&self, credentials: Credentials) -> Result<VerifiedIdentity, String>;
}

#[cfg(feature = "server")]
/// THE one place login's provider is chosen: a second implementation of
/// [`IdentityProvider`] (the future external IdP) implements the trait and
/// swaps in here.
pub fn login_provider(db: Db) -> PasswordProvider {
    PasswordProvider::new(db)
}

#[cfg(feature = "server")]
/// The built-in username/password provider: verifies against the stored
/// salted PBKDF2 hash in `users.password_hash`.
pub struct PasswordProvider {
    db: Db,
}

#[cfg(feature = "server")]
impl PasswordProvider {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Create a new account for the password scheme: the plaintext is hashed
    /// (never stored) and the row inserted. Returns the new user id.
    ///
    /// # Errors
    /// - `username '…' is already taken` — the `users.username` `UNIQUE`
    ///   constraint fired.
    /// - `failed to create user: …` — any other persistence failure.
    pub fn register_account(&self, username: &str, password: &str) -> Result<i64, String> {
        let hash = hash_password(password);
        match self.db.insert_user(username, &hash) {
            Ok(id) => Ok(id),
            Err(e) if e.to_string().contains("UNIQUE") => {
                Err(format!("username '{username}' is already taken"))
            }
            Err(e) => Err(format!("failed to create user: {e}")),
        }
    }

    /// Seconds until the account's lockout window slides past its oldest
    /// recorded failure; used only in the lockout error message.
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

#[cfg(feature = "server")]
impl IdentityProvider for PasswordProvider {
    /// Verify username + password against the stored hash, with the exact
    /// policy `login` carried inline before: unknown usernames burn a real
    /// check so timing never reveals which handles exist, lockout is checked
    /// BEFORE verification (a locked account rejects even the right
    /// password), a wrong password records one failure, and success clears
    /// the failure log and upgrades a hash made with fewer iterations.
    ///
    /// # Errors
    /// - `too many failed login attempts — account locked for N seconds`
    /// - `invalid username or password` — uniform for both unknown handles
    ///   and wrong passwords, so the endpoint never leaks which usernames
    ///   are registered.
    fn authenticate(&self, credentials: Credentials) -> Result<VerifiedIdentity, String> {
        let Some((user_id, stored_username, stored_hash)) =
            self.db.user_by_username_creds(&credentials.username)
        else {
            // Take as long as a real check, so timing doesn't reveal which
            // usernames exist.
            burn_password_check(&credentials.password);
            return Err("invalid username or password".to_string());
        };

        let failures = self.db.login_failures(user_id, LOGIN_LOCKOUT_SECS);
        if failures >= LOGIN_MAX_FAILURES {
            let retry_after = self.retry_after_secs(user_id);
            return Err(format!(
                "too many failed login attempts — account locked for {retry_after} seconds"
            ));
        }

        if !verify_password(&stored_hash, &credentials.password) {
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
        // Upgrade a hash made with fewer iterations than new ones use.
        if password_needs_rehash(&stored_hash) {
            let _ = self.db.set_password_hash(user_id, &hash_password(&credentials.password));
        }
        Ok(VerifiedIdentity {
            issuer: "password".to_string(),
            subject: user_id.to_string(),
            suggested_username: stored_username,
        })
    }
}

// ── tests ────────────────────────────────────────────────────────────────────

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    /// Build a temp-file database with all migrations applied (empty tables).
    fn setup_db() -> (TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let db = Db::migrate(&db_path, &migrations_dir).unwrap();
        (dir, db)
    }

    #[test]
    fn register_account_creates_and_rejects_duplicates() {
        let (_dir, db) = setup_db();
        let svc = PasswordProvider::new(db.clone());

        let id = svc.register_account("nova", "hunter2-secure").expect("a fresh handle signs up");
        assert!(id > 0, "a new id is minted");
        let (id2, username, hash) = db.user_by_username_creds("nova").expect("the row is readable back");
        assert_eq!(id2, id);
        assert_eq!(username, "nova");
        assert!(hash.starts_with("pbkdf2-sha256$"), "stored hash uses the documented prefix, got: {hash}");
        assert!(verify_password(&hash, "hunter2-secure"));

        let err = svc.register_account("nova", "other-pw").expect_err("a claimed handle is rejected");
        assert!(err.contains("already taken"), "got: {err}");
        assert!(verify_password(&db.user_by_username_creds("nova").unwrap().2, "hunter2-secure"),
            "the first password still verifies");
    }

    #[test]
    fn authenticate_returns_a_verified_identity() {
        let (_dir, db) = setup_db();
        let id = PasswordProvider::new(db.clone())
            .register_account("nova", "hunter2-secure")
            .expect("signup");

        let provider = login_provider(db.clone());
        let got = provider.authenticate(Credentials {
            username: "nova".to_string(),
            password: "hunter2-secure".to_string(),
        }).expect("the right credentials verify");
        assert_eq!(got.issuer, "password".to_string(), "the issuer names the local scheme");
        assert_eq!(got.subject, id.to_string(), "the subject is the local account id");
        assert_eq!(got.suggested_username, "nova");

        // Renaming changes what a later login suggests, since the lookup is
        // by credential-time handle.
        db.rename_user(id, "renamed").unwrap();
        let got = provider.authenticate(Credentials {
            username: "renamed".to_string(),
            password: "hunter2-secure".to_string(),
        }).expect("renamed handle still logs in");
        assert_eq!(got.suggested_username, "renamed");
    }

    #[test]
    fn authenticate_policy_is_unchanged_through_the_seam() {
        let (_dir, db) = setup_db();
        let svc = PasswordProvider::new(db.clone());
        svc.register_account("nova", "hunter2-secure").expect("signup");
        let (id, _username, _hash) = db.user_by_username_creds("nova").expect("the row is readable back");

        // A wrong password keeps the uniform error and records one failure.
        let err = svc.authenticate(Credentials { username: "nova".to_string(), password: "totally-wrong".to_string() }).expect_err("a wrong password is rejected");
        assert!(err.contains("invalid username or password"), "got: {err}");
        assert_eq!(db.login_failures(id, LOGIN_LOCKOUT_SECS), 1, "one failure recorded");

        // An unknown handle burns time and shares the error.
        let err = svc.authenticate(Credentials { username: "ghost".to_string(), password: "x".to_string() }).expect_err("an unknown handle is rejected");
        assert!(err.contains("invalid username or password"), "got: {err}");

        // Hammer to the threshold, then even the right password is locked out.
        // (One failure is already recorded above, so four more reach five.)
        for _ in 0..LOGIN_MAX_FAILURES - 1 {
            let err = svc.authenticate(Credentials { username: "nova".to_string(), password: "wrong-guess".to_string() }).expect_err("each wrong password is rejected");
            assert!(err.contains("invalid username or password"), "got: {err}");
        }
        assert_eq!(db.login_failures(id, LOGIN_LOCKOUT_SECS), LOGIN_MAX_FAILURES, "threshold recorded");
        let err = svc.authenticate(Credentials { username: "nova".to_string(), password: "hunter2-secure".to_string() }).expect_err("the lockout rejects the right password too");
        assert!(err.contains("too many failed login attempts") && err.contains("locked"), "got: {err}");

        // A clean account can still authenticate.
        svc.register_account("other", "another-pw").expect("second account signs up");
        assert!(svc.authenticate(Credentials { username: "other".to_string(), password: "another-pw".to_string() }).is_ok(), "lockout is per-account");
    }
}
