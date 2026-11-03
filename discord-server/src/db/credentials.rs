//! Credential queries: how an account signs in (see `0030_credentials.sql`).
//!
//! A `users` row says WHO someone is; a `credentials` row says HOW they sign
//! in. `kind = 'password'` carries the salted PBKDF2 secret locally; `kind
//! = 'external'` links an account to an identity certified by a service
//! elsewhere (`issuer` names the authority, `subject` its account id, no
//! `secret`). Password access never reads the legacy `users.password_hash`
//! column.

use super::Db;
use super::now_epoch_ms;
use super::sqlite_err;

impl Db {
    // ── password credentials ───────────────────────────────────────────────

    /// The stored password hash for user `id`, or `None` if the account has
    /// no password credential (unknown id, or an account that signs in
    /// another way). The hash is the PBKDF2 form from [`hash_password`].
    #[cfg(feature = "server")]
    pub fn password_hash_for_id(&self, user_id: i64) -> Option<String> {
        let c = self.lock().ok()?;
        c.query_row(
            "SELECT cr.secret FROM credentials cr WHERE cr.user_id = ?1 AND cr.kind = 'password'",
            rusqlite::params![user_id],
            |r| r.get::<_, String>(0),
        )
        .ok()
    }

    /// Replace the stored password hash for user `id`, written through the
    /// account's password credential — creating that credential when the
    /// account has none yet (signup, or a user provisioned another way who
    /// later sets a password). Returns the number of credential rows touched.
    /// Fails when `user_id` has no user row.
    #[cfg(feature = "server")]
    pub fn set_password_hash(
        &self,
        user_id: i64,
        password_hash: &str,
    ) -> Result<usize, rusqlite::Error> {
        let c = self.lock()?;
        let rows = c.execute(
            "UPDATE credentials SET secret = ?2 WHERE user_id = ?1 AND kind = 'password'",
            rusqlite::params![user_id, password_hash],
        )?;
        if rows > 0 {
            return Ok(rows)
        }
        // No password credential yet: give the account one, but only if the
        // user row is there (a credential must never orphan a ghost account).
        let exists: i64 = c.query_row(
            "SELECT COUNT(*) FROM users WHERE id = ?1",
            rusqlite::params![user_id],
            |r| r.get::<_, i64>(0),
        )?;
        if exists == 0 {
            return Err(sqlite_err(format!("no user {user_id}")))
        }
        c.execute(
            "INSERT INTO credentials (user_id, kind, secret, issuer, subject, created_at)
             VALUES (?1, 'password', ?2, NULL, NULL, ?3)",
            rusqlite::params![user_id, password_hash, now_epoch_ms()],
        )?;
        Ok(1)
    }

    // ── identity linking ───────────────────────────────────────────────────
    //
    // A future external identity service certifies each user by (issuer,
    // subject); these queries map such an identity to a local account.

    /// The local account already linked to the `(issuer, subject)` identity,
    /// or `None` when that identity has not been seen before.
    #[cfg(feature = "server")]
    pub fn user_for_identity(&self, issuer: &str, subject: &str) -> Option<i64> {
        let c = self.lock().ok()?;
        c.query_row(
            "SELECT cr.user_id FROM credentials cr WHERE cr.issuer = ?1 AND cr.subject = ?2",
            rusqlite::params![issuer, subject],
            |r| r.get::<_, i64>(0),
        )
        .ok()
    }

    /// The account for `(issuer, subject)`, or — when the identity has not
    /// been seen before — a fresh user plus its credential, claiming
    /// `suggested_username` (a free numbered variant when the handle is
    /// already taken). A provisioned account carries only the external
    /// credential; linking an external identity to an EXISTING account is a
    /// later task. Returns `(user_id, username)` of the account the identity
    /// now maps to.
    #[cfg(feature = "server")]
    pub fn find_or_provision(
        &self,
        issuer: &str,
        subject: &str,
        suggested_username: &str,
    ) -> Result<(i64, String), rusqlite::Error> {
        match self.user_for_identity(issuer, subject) {
            Some(user_id) =>
                Ok(self.user_by_id(user_id).unwrap_or((user_id, suggested_username.to_string()))),
            None => {
                let username = self.pick_free_username(suggested_username)?;
                let c = self.lock()?;
                c.execute(
                    "INSERT INTO users (username, created_at) VALUES (?1, ?2)",
                    rusqlite::params![username, now_epoch_ms()],
                )?;
                let user_id = c.last_insert_rowid();
                c.execute(
                    "INSERT INTO credentials (user_id, kind, secret, issuer, subject, created_at)
                     VALUES (?1, 'external', NULL, ?2, ?3, ?4)",
                    rusqlite::params![user_id, issuer, subject, now_epoch_ms()],
                )?;
                Ok((user_id, username))
            }
        }
    }

    /// A handle nobody claims yet: `suggested` itself when free, else
    /// `suggested_2`, `suggested_3`, … (case-insensitive like login lookups;
    /// a blank suggestion falls back to `user`).
    #[cfg(feature = "server")]
    fn pick_free_username(&self, suggested: &str) -> Result<String, rusqlite::Error> {
        let base = if suggested.trim().is_empty() { "user" } else { suggested.trim() };
        for n in 0..1000 {
            let suffix = if n == 0 { "".to_string() } else { let m = n + 1; format!("_{m}") };
            let candidate = format!("{base}{suffix}");
            let c = self.lock()?;
            let taken: i64 = c.query_row(
                "SELECT COUNT(*) FROM users WHERE username = ?1 COLLATE NOCASE",
                rusqlite::params![candidate],
                |r| r.get::<_, i64>(0),
            )?;
            if taken == 0 {
                return Ok(candidate)
            }
        }
        Err(sqlite_err(format!("no free username near '{base}'")))
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    fn setup_db() -> (TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let db = Db::migrate(&db_path, &migrations_dir).unwrap();
        (dir, db)
    }

    #[test]
    fn signup_backfills_and_reads_the_password() {
        let (_dir, db) = setup_db();
        let id = db.insert_user("probe").unwrap();
        let n = db.set_password_hash(id, "pbkdf2-sha256$abc")
            .unwrap_or_else(|e| panic!("set failed: {e}"));
        assert_eq!(n, 1, "one credential touched");
        let row = db.user_by_username_creds("probe");
        assert_eq!(
            row.as_ref().map(|r| r.2.clone()),
            Some("pbkdf2-sha256$abc".to_string()),
            "readback: {row:?}"
        );
        assert_eq!(
            db.password_hash_for_id(id),
            Some("pbkdf2-sha256$abc".to_string()),
            "the provider reads it back"
        );
    }

    #[test]
    fn provision_finds_the_same_user_twice() {
        let (_dir, db) = setup_db();
        let (id1, name1) = db.find_or_provision("idp", "alice", "alice").unwrap();
        let (id2, name2) = db.find_or_provision("idp", "alice", "bob").unwrap();
        assert_eq!(id1, id2, "the same identity provisions once");
        assert_eq!(
            (name1, name2),
            ("alice".to_string(), "alice".to_string()),
            "the first handle wins"
        );
        assert!(
            db.user_for_identity("idp", "alice").is_some(),
            "the credential is linked"
        );
    }

    #[test]
    fn provision_resolves_a_username_clash() {
        let (_dir, db) = setup_db();
        db.insert_user("alice");
        let (id, name) = db.find_or_provision("idp", "alice", "alice").unwrap();
        assert_ne!(id, 1, "the existing alice is not the provisioned account");
        assert_eq!(name, "alice_2", "got: id {id}, name {name}");
    }
}
