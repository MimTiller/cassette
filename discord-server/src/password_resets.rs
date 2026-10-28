//! Password reset links, issued by whoever runs the server.
//!
//! There is no email, so a forgotten password is reset through a link the
//! operator creates on the server machine (`server reset-link <username>`,
//! see `bin/server.rs`) and passes on. The link carries a random token; only
//! its SHA-256 is stored (`0021_password_resets.sql`). A token works once and
//! expires after [`TOKEN_TTL_MS`]. Using it sets the new password, logs out
//! every session and clears the account's failed-login lockout. Issuing a
//! new link revokes the user's older ones.

use serde::{Deserialize, Serialize};
use server::{server, ServerError};

#[cfg(feature = "server")]
use crate::db::{hash_password, Db};

/// How long a reset link stays valid: a day, since the operator hands it
/// over by hand.
pub const TOKEN_TTL_MS: i64 = 24 * 60 * 60 * 1000;

/// The query parameter a reset link carries its token in (`/?reset=<token>`).
pub const URL_PARAM: &str = "reset";

/// The wire input for [`reset_password`].
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResetPasswordInput {
    pub token: String,
    pub new_password: String,
}

/// What [`reset_password`] returns: the account's username, so the login
/// form can be filled in.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResetPasswordResult {
    pub username: String,
}

#[cfg(feature = "server")]
const INVALID_LINK: &str = "this reset link is invalid or has expired; ask for a new one";

#[cfg(feature = "server")]
pub struct ResetService {
    db: Db,
}

#[cfg(feature = "server")]
impl ResetService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Create a reset token for `username` and return it. Any earlier tokens
    /// for the account stop working.
    pub fn issue(&self, username: &str, now_ms: i64) -> Result<String, String> {
        let (user_id, _) = self
            .db
            .user_by_username(username)
            .ok_or_else(|| format!("no account named '{username}'"))?;
        let token = new_token();
        let c = self.db.lock().map_err(|e| e.to_string())?;
        c.execute("DELETE FROM password_resets WHERE user_id = ?1", [user_id])
            .map_err(|e| e.to_string())?;
        c.execute(
            "INSERT INTO password_resets (token_hash, user_id, expires_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![token_hash(&token), user_id, now_ms + TOKEN_TTL_MS],
        )
        .map_err(|e| e.to_string())?;
        Ok(token)
    }

    /// Set a new password with a reset token and return the account's
    /// username. The token is used up even if it turns out to be expired.
    pub fn redeem(&self, token: &str, new_password: &str, now_ms: i64) -> Result<String, String> {
        use rusqlite::OptionalExtension;
        if new_password.trim().is_empty() {
            return Err("the new password can't be empty".to_string());
        }
        let (user_id, username) = {
            let c = self.db.lock().map_err(|e| e.to_string())?;
            let row: Option<(i64, i64, String)> = c
                .query_row(
                    "SELECT r.user_id, r.expires_at, u.username
                       FROM password_resets r JOIN users u ON u.id = r.user_id
                      WHERE r.token_hash = ?1",
                    [token_hash(token.trim())],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            let Some((user_id, expires_at, username)) = row else {
                return Err(INVALID_LINK.to_string());
            };
            c.execute("DELETE FROM password_resets WHERE user_id = ?1", [user_id])
                .map_err(|e| e.to_string())?;
            if expires_at <= now_ms {
                return Err(INVALID_LINK.to_string());
            }
            (user_id, username)
        };
        let fail = |e: rusqlite::Error| format!("failed to reset password: {e}");
        self.db.set_password_hash(user_id, &hash_password(new_password)).map_err(fail)?;
        self.db.bump_token_version(user_id).map_err(fail)?;
        self.db.clear_login_failures(user_id).map_err(fail)?;
        Ok(username)
    }
}

/// 32 random bytes as lowercase hex.
#[cfg(feature = "server")]
fn new_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(feature = "server")]
fn token_hash(token: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(token.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(feature = "server")]
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Set a new password with the token from a reset link. Needs no session.
#[server]
pub async fn reset_password(input: ResetPasswordInput) -> Result<ResetPasswordResult, ServerError> {
    crate::rate_limit::throttle_ip(reset_limiter(), &crate::session::client_ip())
        .map_err(ServerError::failed)?;
    let db = server::use_state::<Db>().ok_or_else(|| ServerError::failed("database not installed"))?;
    let username = ResetService::new(db)
        .redeem(&input.token, &input.new_password, now_ms())
        .map_err(ServerError::failed)?;
    Ok(ResetPasswordResult { username })
}

/// Reset attempts per IP, so tokens can't be guessed by brute force.
#[cfg(feature = "server")]
fn reset_limiter() -> &'static crate::rate_limit::RateLimiter {
    static L: std::sync::OnceLock<crate::rate_limit::RateLimiter> = std::sync::OnceLock::new();
    L.get_or_init(|| crate::rate_limit::RateLimiter::new(10, std::time::Duration::from_secs(3600)))
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use std::path::Path;

    fn setup() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let migrations = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let db = Db::migrate(&dir.path().join("t.db"), &migrations).unwrap();
        crate::auth::AuthService::new(db.clone()).signup("alice", "old-pass").unwrap();
        (dir, db)
    }

    fn alice(db: &Db) -> i64 {
        db.user_by_username("alice").unwrap().0
    }

    #[test]
    fn reset_sets_the_password_and_revokes_sessions() {
        let (_dir, db) = setup();
        let id = alice(&db);
        let version = db.token_version(id);
        let svc = ResetService::new(db.clone());
        let token = svc.issue("alice", 1_000).unwrap();
        assert_eq!(token.len(), 64);
        assert_eq!(svc.redeem(&token, "new-pass", 2_000).unwrap(), "alice");
        let hash = db.password_hash_for_id(id).unwrap();
        assert!(crate::db::verify_password(&hash, "new-pass"));
        assert!(!crate::db::verify_password(&hash, "old-pass"));
        assert!(db.token_version(id) > version);
    }

    #[test]
    fn a_token_works_once() {
        let (_dir, db) = setup();
        let svc = ResetService::new(db);
        let token = svc.issue("alice", 1_000).unwrap();
        svc.redeem(&token, "new-pass", 2_000).unwrap();
        assert_eq!(svc.redeem(&token, "again", 3_000).unwrap_err(), INVALID_LINK);
    }

    #[test]
    fn an_expired_token_is_refused_and_used_up() {
        let (_dir, db) = setup();
        let svc = ResetService::new(db);
        let token = svc.issue("alice", 1_000).unwrap();
        assert_eq!(svc.redeem(&token, "new-pass", 1_000 + TOKEN_TTL_MS).unwrap_err(), INVALID_LINK);
        assert_eq!(svc.redeem(&token, "new-pass", 2_000).unwrap_err(), INVALID_LINK);
    }

    #[test]
    fn issuing_again_revokes_the_older_link() {
        let (_dir, db) = setup();
        let svc = ResetService::new(db);
        let first = svc.issue("alice", 1_000).unwrap();
        let second = svc.issue("alice", 1_000).unwrap();
        assert_eq!(svc.redeem(&first, "new-pass", 2_000).unwrap_err(), INVALID_LINK);
        assert_eq!(svc.redeem(&second, "new-pass", 2_000).unwrap(), "alice");
    }

    #[test]
    fn unknown_tokens_users_and_empty_passwords_are_refused() {
        let (_dir, db) = setup();
        let svc = ResetService::new(db);
        assert_eq!(svc.redeem("not-a-token", "new-pass", 1_000).unwrap_err(), INVALID_LINK);
        assert!(svc.issue("nobody", 1_000).unwrap_err().contains("no account named"));
        let token = svc.issue("alice", 1_000).unwrap();
        assert!(svc.redeem(&token, "  ", 2_000).is_err());
        // The refused attempt didn't use the token up.
        assert_eq!(svc.redeem(&token, "new-pass", 2_000).unwrap(), "alice");
    }

    #[test]
    fn a_reset_clears_the_login_lockout() {
        let (_dir, db) = setup();
        let id = alice(&db);
        for _ in 0..5 {
            db.record_login_failure(id).unwrap();
        }
        let svc = ResetService::new(db.clone());
        let token = svc.issue("alice", 1_000).unwrap();
        svc.redeem(&token, "new-pass", 2_000).unwrap();
        assert!(crate::auth::AuthService::new(db).login("alice", "new-pass").is_ok());
    }
}
