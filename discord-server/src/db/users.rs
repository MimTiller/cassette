//! User-account queries: login failures, credentials, token
//! revocation, avatars and presence.

use super::Db;
use super::now_epoch_ms;
use super::sqlite_err;

impl Db {
    // ── users ──────────────────────────────────────────────────────────────

    /// Number of failed logins within the lockout window. `window_secs` is the
    /// account-level lockout horizon (kept in sync with the per-IP limiter in
    /// `server_settings`). Returns 0 when the user has no recent failures.
    #[cfg(feature = "server")]
    pub fn login_failures(&self, user_id: i64, window_secs: i64) -> i64 {
        let Ok(c) = self.lock() else { return 0 };
        let cutoff = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
            - window_secs;
        c.query_row(
            "SELECT COUNT(*) FROM login_failures WHERE user_id = ?1 AND at > ?2",
            rusqlite::params![user_id, cutoff],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0)
    }

    /// Record a failed login for a user (drives the per-account lockout floor).
    #[cfg(feature = "server")]
    pub fn record_login_failure(&self, user_id: i64) -> Result<(), rusqlite::Error> {
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let c = self.lock()?;
        c.execute(
            "INSERT INTO login_failures (user_id, at) VALUES (?1, ?2)",
            rusqlite::params![user_id, at],
        )?;
        Ok(())
    }

    /// Clear all recorded login failures for a user (called after a success so
    /// the floor resets).
    #[cfg(feature = "server")]
    pub fn clear_login_failures(&self, user_id: i64) -> Result<(), rusqlite::Error> {
        let c = self.lock()?;
        c.execute(
            "DELETE FROM login_failures WHERE user_id = ?1",
            rusqlite::params![user_id],
        )?;
        Ok(())
    }

    /// Create a user and return the new id. Returns `Err` on a duplicate
    /// username (the `UNIQUE` constraint), which the endpoint maps to a
    /// domain error.
    #[cfg(feature = "server")]
    pub fn insert_user(&self, username: &str, password_hash: &str) -> Result<i64, rusqlite::Error> {
        // `users.created_at` is NOT NULL (UTC epoch millis, the schema
        // convention) — the data layer stamps it so app-layer callers
        // (signup) don't have to.
        let created_at = now_epoch_ms();
        let c = self.lock()?;
        c.execute(
            "INSERT INTO users (username, password_hash, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![username, password_hash, created_at],
        )?;
        Ok(c.last_insert_rowid())
    }

    /// Fetch a user's id, username, and password hash by username (case-insensitive,
    /// matching the `UNIQUE` collation of the `users` table).
    #[cfg(feature = "server")]
    pub fn user_by_username_creds(&self, username: &str) -> Option<(i64, String, String)> {
        let c = self.lock().ok()?;
        c.query_row(
            "SELECT id, username, COALESCE(password_hash, '') FROM users WHERE username = ?1 COLLATE NOCASE",
            rusqlite::params![username],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .ok()
    }

    /// Look up a user by id. Returns `(id, username)`.
    pub fn user_by_id(&self, id: i64) -> Option<(i64, String)> {
        let c = self.lock().ok()?;
        c.query_row(
            "SELECT id, username FROM users WHERE id = ?1",
            rusqlite::params![id],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
        )
        .ok()
    }

    /// Look up a user by (unique) username.
    pub fn user_by_username(&self, username: &str) -> Option<(i64, String)> {
        let c = self.lock().ok()?;
        c.query_row(
            "SELECT id, username FROM users WHERE username = ?1",
            rusqlite::params![username],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok()
    }

    /// The stored password hash for user `id`, or `None` if there is no such
    /// row. Used by the change-password flow to verify the current password
    /// before replacing it (the hash is the PBKDF2 form from
    /// [`hash_password`]).
    pub fn password_hash_for_id(&self, user_id: i64) -> Option<String> {
        let c = self.lock().ok()?;
        c.query_row(
            "SELECT COALESCE(password_hash, '') FROM users WHERE id = ?1",
            rusqlite::params![user_id],
            |r| r.get::<_, String>(0),
        )
        .ok()
    }

    /// The account's current session-token version, or `None` when there is
    /// no such account (a deleted account's tokens stop verifying).
    pub fn token_version(&self, user_id: i64) -> Option<i64> {
        let c = self.lock().ok()?;
        c.query_row(
            "SELECT token_version FROM users WHERE id = ?1",
            rusqlite::params![user_id],
            |r| r.get::<_, i64>(0),
        )
        .ok()
    }

    /// Invalidate every session token issued to `user_id` so far.
    pub fn bump_token_version(&self, user_id: i64) -> Result<usize, rusqlite::Error> {
        let c = self.lock()?;
        c.execute(
            "UPDATE users SET token_version = token_version + 1 WHERE id = ?1",
            rusqlite::params![user_id],
        )
    }

    /// Replace the stored password hash for user `id`. Returns the number of
    /// rows updated (1 on success, 0 if there is no such row).
    pub fn set_password_hash(
        &self,
        user_id: i64,
        password_hash: &str,
    ) -> Result<usize, rusqlite::Error> {
        let c = self.lock()?;
        c.execute(
            "UPDATE users SET password_hash = ?2 WHERE id = ?1",
            rusqlite::params![user_id, password_hash],
        )
    }

    /// Rename user `id` to `new_username`. Returns the number of rows updated
    /// (1 on success). The `users.username` `UNIQUE` (COLLATE NOCASE)
    /// constraint fires a `UNIQUE` error when the handle is already claimed by
    /// a different account — the caller maps that to a domain message.
    pub fn rename_user(&self, user_id: i64, new_username: &str) -> Result<usize, rusqlite::Error> {
        let c = self.lock()?;
        c.execute(
            "UPDATE users SET username = ?2 WHERE id = ?1",
            rusqlite::params![user_id, new_username],
        )
    }

    // ── avatars ────────────────────────────────────────────────────────────
    //
    // The `users.avatar_url` column (declared in `0001_users.sql`: "the remote
    // image (or null for the default avatar)") stores a `/media/…` path:
    // the app layer validates the image and moves its bytes into the media
    // store before persisting, so the data layer only moves strings.

    /// The user's stored avatar URL (a `data:image/…` URL), or `None` for
    /// "no avatar" (the NULL column) or an unknown id.
    pub fn user_avatar(&self, user_id: i64) -> Option<String> {
        let c = self.lock().ok()?;
        c.query_row(
            "SELECT avatar_url FROM users WHERE id = ?1",
            rusqlite::params![user_id],
            |r| r.get::<_, Option<String>>(0),
        )
        .ok()
        .flatten()
    }

    /// Set (or clear, with `None`) the stored avatar URL for `user_id`.
    pub fn set_user_avatar(
        &self,
        user_id: i64,
        avatar_url: Option<String>,
    ) -> Result<usize, rusqlite::Error> {
        let c = self.lock()?;
        c.execute(
            "UPDATE users SET avatar_url = ?2 WHERE id = ?1",
            rusqlite::params![user_id, avatar_url],
        )
    }

    // ── presence ───────────────────────────────────────────────────────────
    //
    // One status per user: 'online' / 'away' / 'offline', driven entirely by
    // the realtime gateway (set on connect, idle-timeout, and disconnect).

    /// The user's current presence status ('online' / 'away' / 'offline').
    /// `None` if the user row doesn't exist.
    pub fn presence(&self, user_id: i64) -> Option<String> {
        let c = self.lock().ok()?;
        c.query_row(
            "SELECT presence_status FROM users WHERE id = ?1",
            rusqlite::params![user_id],
            |r| r.get::<_, String>(0),
        )
        .ok()
    }

    /// Set the user's presence status. `Ok(true)` when the value changed
    /// (callers use that to decide whether to broadcast an update),
    /// `Ok(false)` when it was already the given status. `Err` only on a
    /// non-status string or a DB failure.
    pub fn set_presence(&self, user_id: i64, status: &str) -> Result<bool, rusqlite::Error> {
        match status {
            "online" | "away" | "offline" => {}
            other => return Err(sqlite_err(format!("invalid presence status: {other}"))),
        }
        let c = self.lock()?;
        // Read the current status first (QueryReturnedNoRows if the user row
        // doesn't exist — surfaced as Err rather than a silent no-op).
        let old: String = c.query_row(
            "SELECT presence_status FROM users WHERE id = ?1",
            rusqlite::params![user_id],
            |r| r.get(0),
        )?;
        c.execute(
            "UPDATE users SET presence_status = ?2 WHERE id = ?1",
            rusqlite::params![user_id, status],
        )?;
        Ok(old != status)
    }
}
