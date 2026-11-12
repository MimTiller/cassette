//! The instance-wide store: settings keys with their values (migration
//! `0035_instance_settings.sql`) and the instance-admin roster.
//!
//! Env vars stay the fallback (see `crate::instance`): a key present here
//! wins over its env var. Secrets in here are stored raw; the endpoints
//! above never hand them back in full.

use super::Db;

impl Db {
    /// A stored instance setting, or `None` when the key was never saved
    /// (the effective value then comes from the environment).
    pub fn instance_setting(&self, key: String) -> Option<String> {
        let c = self.lock().ok()?;
        c.query_row(
            "SELECT value FROM instance_settings WHERE key = ?1",
            rusqlite::params![key],
            |r| r.get::<_, String>(0),
        )
            .ok()
    }

    /// Every stored instance setting as `(key, value)`, ordered by key.
    pub fn instance_settings_all(&self) -> Vec<(String, String)> {
        let c = match self.lock() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut stmt = match c.prepare(
            "SELECT key, value FROM instance_settings ORDER BY key",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map(rusqlite::params![], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        }) {
            Ok(rows) => rows,
            Err(_) => return Vec::new(),
        };
        rows.filter_map(Result::ok).collect()
    }

    /// Save an instance setting (replacing any old value for the key).
    pub fn set_instance_setting(&self, key: String, value: String) -> Result<(), rusqlite::Error> {
        let c = self.lock()?;
        c.execute(
            "REPLACE INTO instance_settings (key, value) VALUES (?1, ?2)",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }

    /// Drop a stored instance setting so the key falls back to its env
    /// value. Returns whether a row was removed.
    pub fn clear_instance_setting(&self, key: String) -> Result<bool, rusqlite::Error> {
        let c = self.lock()?;
        Ok(c.execute("DELETE FROM instance_settings WHERE key = ?1", rusqlite::params![key])? > 0)
    }

    /// Is `user_id` an instance admin?
    pub fn instance_admin(&self, user_id: i64) -> bool {
        let c = match self.lock() {
            Ok(c) => c,
            Err(_) => return false,
        };
        let n = c.query_row(
            "SELECT COUNT(user_id) FROM instance_admins WHERE user_id = ?1",
            rusqlite::params![user_id],
            |r| r.get::<_, i64>(0),
        );
        n.unwrap_or_else(|e| panic!("instance_admin: {e}")) > 0
    }

    /// How many instance admins the instance has (the setup-complete check
    /// needs it even for a caller who is not one).
    pub fn instance_admin_count(&self) -> i64 {
        let c = match self.lock() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        let n = c.query_row(
            "SELECT COUNT(user_id) FROM instance_admins",
            rusqlite::params![],
            |r| r.get::<_, i64>(0),
        );
        n.unwrap_or_else(|e| panic!("instance_admin_count: {e}"))
    }

    /// Mark an existing account an instance admin by id. Idempotent.
    pub fn grant_instance_admin(&self, user_id: i64) -> Result<(), rusqlite::Error> {
        let c = self.lock()?;
        c.execute("INSERT INTO instance_admins (user_id) VALUES (?1)", rusqlite::params![user_id])?;
        Ok(())
    }
}
