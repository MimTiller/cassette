//! Data layer: SQLite (rusqlite, bundled) over numbered migrations under
//! `migrations/`. Compiled only into the server build (`feature = "server"`),
//! so the wasm client never pulls a native C dep.
//!
//! The schema is defined entirely by the numbered SQL files; this module only
//! opens the database, runs migrations in order, and exposes the typed
//! queries the realtime layer + `#[server]` endpoints need.
//!
//! Timestamps are UTC epoch **milliseconds** (INTEGER) throughout, matching
//! the schema convention and the client's `epoch_millis` wire format.

use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;

use rusqlite::Connection;

// ── password KDF ───────────────────────────────────────────────────────────
//
// Passwords are stored as a self-describing PBKDF2-HMAC-SHA256 secret:
//   pbkdf2-sha256$<iterations>$<salt_b64>$<dk_b64>
// The `pbkdf2` crate isn't in the local registry, so we hand-roll it over the
// vendored `hmac` + `sha2` crates. `rand` supplies the per-password salt.

#[cfg(feature = "server")]
use hmac::{Hmac, Mac};
#[cfg(feature = "server")]
use sha2::Sha256;

#[cfg(feature = "server")]
type HmacSha256 = Hmac<Sha256>;

/// PBKDF2 iterations for password storage. High enough to slow offline
/// brute-force of a leaked DB; low enough that login stays snappy (~10ms).
#[cfg(feature = "server")]
const PBKDF2_ITERATIONS: u32 = 100_000;

/// Derive a key from `password` using PBKDF2-HMAC-SHA256.
#[cfg(feature = "server")]
fn pbkdf2(password: &[u8], salt: &[u8], iterations: u32, dk_len: usize) -> Vec<u8> {
    use sha2::Digest;
    // HMAC-SHA256 output is fixed at 32 bytes (SHA256's digest size).
    let mac_len = 32usize;
    let blocks = dk_len.div_ceil(mac_len);
    let mut out = Vec::with_capacity(blocks * mac_len);
    for block in 1..=blocks as u32 {
        // U1 = PRF(password, salt || INT_32_BE(block))
        let mut mac = <HmacSha256 as Mac>::new_from_slice(password).expect("hmac key any size");
        mac.update(salt);
        mac.update(&block.to_be_bytes());
        let mut u: Vec<u8> = mac.finalize().into_bytes().to_vec();
        let mut result = u.clone();
        for _ in 1..iterations {
            let mut mac = <HmacSha256 as Mac>::new_from_slice(password).expect("hmac key any size");
            mac.update(&u);
            u = mac.finalize().into_bytes().to_vec();
            for (r, x) in result.iter_mut().zip(u.iter()) {
                *r ^= *x;
            }
        }
        out.extend_from_slice(&result);
    }
    out.truncate(dk_len);
    out
}

fn b64encode(bytes: &[u8]) -> String {
    // Minimal standard base64 (no `base64` crate in the local registry).
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = *chunk.first().unwrap_or(&0) as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        s.push(A[(n >> 18 & 63) as usize] as char);
        s.push(A[(n >> 12 & 63) as usize] as char);
        s.push(if chunk.len() > 1 { A[(n >> 6 & 63) as usize] as char } else { '=' });
        s.push(if chunk.len() > 2 { A[(n & 63) as usize] as char } else { '=' });
    }
    s
}

fn b64decode(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let chars: Vec<u8> = s.bytes().filter(|b| *b != b'=').collect();
    let mut out = Vec::with_capacity(chars.len() * 3 / 4);
    for chunk in chars.chunks(4) {
        let mut n = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            n |= val(*c)? << (18 - 6 * i);
        }
        let full = chunk.len().min(4);
        for i in 0..full - 1 {
            out.push(((n >> (16 - 8 * i)) & 0xff) as u8);
        }
    }
    Some(out)
}

/// Hash `password` into the stored `pbkdf2-sha256$iter$salt$dk` form.
#[cfg(feature = "server")]
pub fn hash_password(password: &str) -> String {
    use rand::RngCore;
    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    let dk = pbkdf2(password.as_bytes(), &salt, PBKDF2_ITERATIONS, 32);
    format!("pbkdf2-sha256${}${}${}", PBKDF2_ITERATIONS, b64encode(&salt), b64encode(&dk))
}

/// Constant-time compare a candidate password against a stored hash. Returns
/// `false` for any malformed stored value (defence against schema drift).
#[cfg(feature = "server")]
pub fn verify_password(stored: &str, password: &str) -> bool {
    // Split: prefix$iterations$salt$dk
    let parts: Vec<&str> = stored.split('$').collect();
    let [prefix, iterations, salt_b64, dk_b64] = parts.as_slice() else {
        return false;
    };
    if *prefix != "pbkdf2-sha256" {
        return false;
    }
    let iterations: u32 = match iterations.parse() {
        Ok(i) => i,
        Err(_) => return false,
    };
    let salt = match b64decode(salt_b64) {
        Some(s) => s,
        None => return false,
    };
    let expected = match b64decode(dk_b64) {
        Some(d) => d,
        None => return false,
    };
    let actual = pbkdf2(password.as_bytes(), &salt, iterations, expected.len());
    constant_time_eq(&actual, &expected)
}

/// Constant-time byte comparison (avoids a length/timing oracle on the hash).
#[cfg(feature = "server")]
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// A handle to the open SQLite connection. Cloning is cheap — the connection
/// itself lives in a shared `Mutex` and is handed to every query on demand.
///
/// `rusqlite::Connection` is not `Sync`, so the raw connection is wrapped in
/// a `Mutex`; the wrapper itself is `Clone + Send + Sync` so it can be stored
/// in server state (`server::install_state`) and captured by `#[server]` /
/// `#[channel]` handlers.
#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

/// Wrap a non-SQL failure (I/O, a poisoned lock, …) as a `rusqlite::Error`
/// carrying a generic `SQLITE_ERROR` code, so callers can surface it through
/// the same `Result<_, rusqlite::Error>` the query layer already uses.
fn sqlite_err(msg: impl Into<String>) -> rusqlite::Error {
    // 1 == SQLITE_ERROR (generic/unknown failure); `ffi::Error::new` maps the
    // raw code into the structured error `SqliteFailure` carries.
    rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(1), Some(msg.into()))
}

impl Db {
    /// Open (or create) the database at `path` and apply every unapplied
    /// migration in `migrations_dir`, in filename order. A migration is a
    /// single `.sql` file; it is applied inside a transaction and recorded in
    /// the `schema_migrations` bookkeeping table so it runs exactly once.
    ///
    /// `PRAGMA journal_mode = WAL` gives the concurrent reader/writer
    /// throughput the realtime fan-out relies on; `foreign_keys = ON` enforces
    /// the schema's referential rules on every connection.
    pub fn migrate(path: &Path, migrations_dir: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;

        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };

        // Bookkeeping table: one row per applied migration, keyed by filename
        // (deterministic ordering, no hash to drift).
        {
            let c = db.lock()?;
            c.execute_batch(
                "CREATE TABLE IF NOT EXISTS schema_migrations (
                    name TEXT PRIMARY KEY,
                    applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
                );",
            )?;
        }

        // Apply each pending migration in lexicographic (numeric-prefix) order.
        let mut files: Vec<std::fs::DirEntry> = std::fs::read_dir(migrations_dir)
            .map_err(|e| sqlite_err(format!("read migrations dir: {e}")))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| sqlite_err(format!("read migrations dir: {e}")))?
            .into_iter()
            .filter(|e| e.path().extension().map(|x| x == "sql").unwrap_or(false))
            .collect();
        files.sort_by_key(|e| e.file_name());

        for entry in files {
            let name = entry.file_name().to_string_lossy().into_owned();
            let pending = {
                let c = db.lock()?;
                let applied: Option<String> = c
                    .query_row(
                        "SELECT name FROM schema_migrations WHERE name = ?1",
                        rusqlite::params![name],
                        |row| row.get(0),
                    )
                    .ok();
                applied.is_none()
            };
            if !pending {
                continue;
            }
            let sql = std::fs::read_to_string(entry.path())
                .map_err(|e| sqlite_err(format!("read migration {}: {e}", name)))?;
            let mut c = db.lock()?;
            let tx = c.transaction()?;
            tx.execute_batch(&sql)?;
            tx.execute(
                "INSERT INTO schema_migrations (name) VALUES (?1)",
                rusqlite::params![name],
            )?;
            tx.commit()?;
        }

        Ok(db)
    }

    /// Open a pre-migrated database (skips the migration run).
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Lock the connection for a single synchronous query. The returned
    /// guard keeps the lock for its lifetime — do not hold it across an
    /// `.await`.
    ///
    /// `pub(crate)` so in-crate tests (e.g. `server_settings`) can seed
    /// fixtures via raw SQL without the app-layer create APIs.
    pub(crate) fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, rusqlite::Error> {
        self.conn.lock().map_err(|e| sqlite_err(format!("db lock poisoned: {e}")))
    }

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
        let c = self.lock()?;
        c.execute(
            "INSERT INTO users (username, password_hash) VALUES (?1, ?2)",
            rusqlite::params![username, password_hash],
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

    // ── channels ───────────────────────────────────────────────────────────

    /// A channel row as read by the realtime layer: id + name.
    pub fn channel_by_id(&self, id: i64) -> Option<(i64, String)> {
        let c = self.lock().ok()?;
        c.query_row(
            "SELECT id, name FROM channels WHERE id = ?1",
            rusqlite::params![id],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
        )
        .ok()
    }

    // ── messages ───────────────────────────────────────────────────────────

    /// Insert a chat message and return its new row id. `created_at` is UTC
    /// epoch milliseconds, as the schema dictates; the caller supplies it.
    pub fn insert_message(
        &self,
        channel_id: i64,
        author_id: i64,
        content: &str,
        created_at: i64,
    ) -> Result<i64, rusqlite::Error> {
        let c = self.lock()?;
        c.execute(
            "INSERT INTO messages (channel_id, author_id, content, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![channel_id, author_id, content, created_at],
        )?;
        Ok(c.last_insert_rowid())
    }

    /// The most recent `limit` messages in a channel, oldest-first (ready to
    /// hand to a client as initial history).
    pub fn recent_messages(&self, channel_id: i64, limit: i64) -> Vec<Message> {
        let Ok(c) = self.lock() else {
            return Vec::new();
        };
        let mut stmt = match c.prepare(
            "SELECT id, channel_id, author_id, content, created_at, edited_at
             FROM messages WHERE channel_id = ?1 ORDER BY created_at DESC, id DESC LIMIT ?2",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map(rusqlite::params![channel_id, limit], |r| {
            Ok(Message {
                id: r.get(0)?,
                channel_id: r.get(1)?,
                author_id: r.get(2)?,
                content: r.get(3)?,
                created_at: r.get(4)?,
                edited_at: r.get(5)?,
            })
        }) {
            Ok(rows) => rows,
            Err(_) => return Vec::new(),
        };
        let mut out: Vec<Message> = rows.filter_map(Result::ok).collect();
        out.reverse();
        out
    }

    // ── servers ────────────────────────────────────────────────────────────

    /// Look up a server by id. Returns `(id, name, owner_id, icon_url)`.
    pub fn server_by_id(&self, id: i64) -> Option<(i64, String, i64, Option<String>)> {
        let c = self.lock().ok()?;
        c.query_row(
            "SELECT id, name, owner_id, icon_url FROM servers WHERE id = ?1 AND deleted_at IS NULL",
            rusqlite::params![id],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .ok()
    }

    /// The role of `user_id`'s membership in `server_id` (`'owner'`, `'admin'`,
    /// or `'member'`), or `None` if the user is not a member.
    pub fn membership_role(&self, server_id: i64, user_id: i64) -> Option<String> {
        let c = self.lock().ok()?;
        c.query_row(
            "SELECT role FROM server_memberships WHERE server_id = ?1 AND user_id = ?2",
            rusqlite::params![server_id, user_id],
            |r| r.get::<_, String>(0),
        )
        .ok()
    }

    /// Rename the server with `id`. Returns the new name.
    pub fn rename_server(&self, id: i64, name: &str) -> Result<String, rusqlite::Error> {
        let c = self.lock()?;
        let updated = c.execute(
            "UPDATE servers SET name = ?2 WHERE id = ?1 AND deleted_at IS NULL",
            rusqlite::params![id, name],
        )?;
        if updated == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        Ok(name.to_string())
    }

    /// Set (or clear) the server icon. `icon` is `None` to clear the icon,
    /// `Some(url)` to set it to that URL string. Returns the stored icon URL.
    pub fn set_server_icon(
        &self,
        id: i64,
        icon: Option<&str>,
    ) -> Result<Option<String>, rusqlite::Error> {
        let c = self.lock()?;
        let updated = c.execute(
            "UPDATE servers SET icon_url = ?2 WHERE id = ?1 AND deleted_at IS NULL",
            rusqlite::params![id, icon],
        )?;
        if updated == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        Ok(icon.map(|s| s.to_string()))
    }

    /// Transfer server ownership from `from_id` to `to_id`.
    ///
    /// The transfer happens atomically inside a single transaction:
    /// 1. `servers.owner_id` is updated (guarded by `owner_id = from_id`, so a
    ///    concurrent rename of ownership fails the guard and rolls back).
    /// 2. The old owner's membership role is demoted from `'owner'` to `'admin'`.
    /// 3. The new owner's membership role is promoted to `'owner'`.
    ///
    /// Returns `Ok(())` on success; `Err` if the server doesn't exist, the
    /// caller is not the current owner, or the new owner is not a member.
    pub fn transfer_server_ownership(
        &self,
        server_id: i64,
        from_id: i64,
        to_id: i64,
    ) -> Result<(), rusqlite::Error> {
        let mut c = self.lock()?;
        let tx = c.transaction()?;

        // Guard 1: server exists and `from_id` is still the owner.
        let owner_ok: i64 = tx.query_row(
            "SELECT COUNT(*) FROM servers WHERE id = ?1 AND owner_id = ?2 AND deleted_at IS NULL",
            rusqlite::params![server_id, from_id],
            |r| r.get(0),
        )?;
        if owner_ok == 0 {
            tx.rollback()?;
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }

        // Guard 2: the new owner must already be a member of the server.
        let is_member: i64 = tx.query_row(
            "SELECT COUNT(*) FROM server_memberships WHERE server_id = ?1 AND user_id = ?2",
            rusqlite::params![server_id, to_id],
            |r| r.get(0),
        )?;
        if is_member == 0 {
            tx.rollback()?;
            return Err(sqlite_err(
                "new owner is not a member of this server",
            ));
        }

        // Demote old owner → admin.
        tx.execute(
            "UPDATE server_memberships SET role = 'admin' WHERE server_id = ?1 AND user_id = ?2 AND role = 'owner'",
            rusqlite::params![server_id, from_id],
        )?;

        // Promote new owner.
        tx.execute(
            "UPDATE server_memberships SET role = 'owner' WHERE server_id = ?1 AND user_id = ?2",
            rusqlite::params![server_id, to_id],
        )?;

        // Update the canonical owner_id column.
        tx.execute(
            "UPDATE servers SET owner_id = ?2 WHERE id = ?1 AND owner_id = ?3 AND deleted_at IS NULL",
            rusqlite::params![server_id, to_id, from_id],
        )?;

        tx.commit()?;
        Ok(())
    }
}

/// A single chat message, as persisted.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub id: i64,
    pub channel_id: i64,
    pub author_id: i64,
    pub content: String,
    /// UTC epoch milliseconds.
    pub created_at: i64,
    /// UTC epoch milliseconds, or `None` until the first edit.
    pub edited_at: Option<i64>,
}
