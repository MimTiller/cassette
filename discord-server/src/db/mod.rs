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

use crate::roles::{ChannelKind, Role};

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

/// Current UTC wall-clock time as epoch **milliseconds** (the schema
/// convention for every `created_at` / `edited_at` column).
#[cfg(feature = "server")]
fn now_epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
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
    ///
    /// This inserts the message row only. Resolved `@mention` rows are written
    /// separately via [`Self::insert_message_mentions`] (the caller resolves
    /// the tokens against the member list — see `realtime::resolve_mentions`).
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

    /// Record the resolved `@mention`s for `message_id` (one row per user id).
    /// Empty `user_ids` is a no-op. See migration `0012_message_mentions.sql`.
    pub fn insert_message_mentions(
        &self,
        message_id: i64,
        user_ids: &[i64],
    ) -> Result<(), rusqlite::Error> {
        if user_ids.is_empty() {
            return Ok(());
        }
        let c = self.lock()?;
        for user_id in user_ids {
            c.execute(
                "INSERT OR IGNORE INTO message_mentions (message_id, user_id)
                 VALUES (?1, ?2)",
                rusqlite::params![message_id, user_id],
            )?;
        }
        Ok(())
    }

    /// A message's `(author_id, channel_id)`, or `None` when the id is unknown.
    /// Read via `0004_messages.sql` so the gateway can authorize an edit or
    /// delete against the stored author and recover the channel for the
    /// broadcast audience.
    pub fn message_row(&self, message_id: i64) -> Option<(i64, i64)> {
        let c = self.lock().ok()?;
        c.query_row(
            "SELECT author_id, channel_id FROM messages WHERE id = ?1",
            rusqlite::params![message_id],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )
        .ok()
    }

    /// Update a message's content and stamp `edited_at` (UTC epoch ms, supplied
    /// by the caller). Returns `Ok(())` when a row was updated, `Err` (with
    /// `QueryReturnedNoRows`) when the id didn't exist — mirroring the other
    /// update helpers' "0 rows = not found" convention.
    pub fn update_message(
        &self,
        message_id: i64,
        content: &str,
        edited_at: i64,
    ) -> Result<(), rusqlite::Error> {
        let c = self.lock()?;
        let updated = c.execute(
            "UPDATE messages SET content = ?2, edited_at = ?3 WHERE id = ?1",
            rusqlite::params![message_id, content, edited_at],
        )?;
        if updated == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        Ok(())
    }

    /// Replace the stored `@mention` rows for `message_id` with `user_ids`.
    /// Used after an edit so the persisted mention list matches the new text
    /// (the old rows are removed, the new ones inserted). See migration
    /// `0012_message_mentions.sql`.
    pub fn replace_message_mentions(
        &self,
        message_id: i64,
        user_ids: &[i64],
    ) -> Result<(), rusqlite::Error> {
        let c = self.lock()?;
        c.execute(
            "DELETE FROM message_mentions WHERE message_id = ?1",
            rusqlite::params![message_id],
        )?;
        for user_id in user_ids {
            c.execute(
                "INSERT OR IGNORE INTO message_mentions (message_id, user_id)
                 VALUES (?1, ?2)",
                rusqlite::params![message_id, user_id],
            )?;
        }
        Ok(())
    }

    /// Delete a message row. `message_mentions` cascades on the message id
    /// (`0012_message_mentions.sql`); a reply pointing at it is nulled by the
    /// schema's `ON DELETE SET NULL`. Returns the number of rows deleted (0 if
    /// the id didn't exist).
    pub fn delete_message(&self, message_id: i64) -> Result<usize, rusqlite::Error> {
        let c = self.lock()?;
        Ok(c.execute("DELETE FROM messages WHERE id = ?1", rusqlite::params![message_id])?)
    }

    /// The most recent `limit` messages in a channel, oldest-first (ready to
    /// hand to a client as initial history). Each message carries its resolved
    /// `@mention` user ids (see `0012_message_mentions.sql`).
    pub fn recent_messages(&self, channel_id: i64, limit: i64) -> Vec<Message> {
        let mut out: Vec<Message> = {
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
                    mentioned_user_ids: Vec::new(),
                })
            }) {
                Ok(rows) => rows,
                Err(_) => return Vec::new(),
            };
            let mut out: Vec<Message> = rows.filter_map(Result::ok).collect();
            out.reverse();
            out
        }; // drop the connection lock before re-locking in message_mentions
        for m in &mut out {
            m.mentioned_user_ids = self.message_mentions(m.id);
        }
        out
    }

    /// The `limit` messages strictly OLDER than `before_id` (id is a monotonic
    /// AUTOINCREMENT cursor), in `channel_id`, oldest-first — the next page
    /// back from a client that already holds everything from `before_id` on.
    /// `before_id == None` returns the latest `limit` (same as
    /// [`recent_messages`]). Each message carries its resolved `@mention` user
    /// ids, like [`recent_messages`].
    pub fn older_messages(&self, channel_id: i64, before_id: Option<i64>, limit: i64) -> Vec<Message> {
        let mut out: Vec<Message> = {
            let Ok(c) = self.lock() else {
                return Vec::new();
            };
            let mut stmt = match c.prepare(
                "SELECT id, channel_id, author_id, content, created_at, edited_at
                 FROM messages WHERE channel_id = ?1 AND id < ?2
                 ORDER BY created_at DESC, id DESC LIMIT ?3",
            ) {
                Ok(s) => s,
                Err(_) => return Vec::new(),
            };
            let rows = match stmt.query_map(rusqlite::params![channel_id, before_id.unwrap_or(i64::MAX), limit], |r| {
                Ok(Message {
                    id: r.get(0)?,
                    channel_id: r.get(1)?,
                    author_id: r.get(2)?,
                    content: r.get(3)?,
                    created_at: r.get(4)?,
                    edited_at: r.get(5)?,
                    mentioned_user_ids: Vec::new(),
                })
            }) {
                Ok(rows) => rows,
                Err(_) => return Vec::new(),
            };
            let mut out: Vec<Message> = rows.filter_map(Result::ok).collect();
            out.reverse();
            out
        }; // drop the connection lock before re-locking in message_mentions
        for m in &mut out {
            m.mentioned_user_ids = self.message_mentions(m.id);
        }
        out
    }

    /// The resolved `@mention` user ids stored for `message_id`
    /// (`message_mentions`, see `0012_message_mentions.sql`). Empty when the
    /// message has none (or the id is unknown).
    pub fn message_mentions(&self, message_id: i64) -> Vec<i64> {
        let Ok(c) = self.lock() else {
            return Vec::new();
        };
        let mut stmt = match c.prepare(
            "SELECT user_id FROM message_mentions WHERE message_id = ?1 ORDER BY user_id",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map(rusqlite::params![message_id], |r| r.get::<_, i64>(0)) {
            Ok(rows) => rows,
            Err(_) => return Vec::new(),
        };
        rows.filter_map(Result::ok).collect()
    }

    // ── channel reads ──────────────────────────────────────────────────────
    //
    // Per-user, per-channel read state (`channel_reads`): the latest message id
    // a user has seen in a channel. Unread = messages whose id is greater than
    // this id (every message when there is no read row). See migration
    // `0010_channel_reads.sql`.

    /// The newest message id `user_id` has read in `channel_id`, or `None` when
    /// the user has no read state for that channel (i.e. it is entirely unread).
    pub fn last_read_message_id(&self, user_id: i64, channel_id: i64) -> Option<i64> {
        let c = self.lock().ok()?;
        c.query_row(
            "SELECT last_read_message_id FROM channel_reads WHERE user_id = ?1 AND channel_id = ?2",
            rusqlite::params![user_id, channel_id],
            |r| r.get::<_, Option<i64>>(0),
        )
        .ok()
        .flatten()
    }

    /// Record that `user_id` has read up to and including message id
    /// `message_id` in `channel_id`. Upserts: a new channel read is created, an
    /// existing one is advanced. The id is monotonic, so this never moves the
    /// read state backwards even if called with an older id.
    ///
    /// # Errors
    /// - the channel does not exist (FK violation), or the write fails.
    pub fn mark_channel_read(
        &self,
        user_id: i64,
        channel_id: i64,
        message_id: i64,
        read_at: i64,
    ) -> Result<(), rusqlite::Error> {
        let c = self.lock()?;
        c.execute(
            "INSERT INTO channel_reads (user_id, channel_id, last_read_message_id, read_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (user_id, channel_id) DO UPDATE SET
                last_read_message_id = MAX(channel_reads.last_read_message_id, excluded.last_read_message_id),
                read_at = excluded.read_at",
            rusqlite::params![user_id, channel_id, message_id, read_at],
        )?;
        Ok(())
    }

    /// Unread message counts for every channel in `server_id` that `user_id`
    /// belongs to (a membership row exists), one row per channel. A channel the
    /// user has never read comes back with count = its total message count; a
    /// channel read up to date comes back with 0. Channels with zero unread
    /// messages are still included (so the client can distinguish "0 unread"
    /// from "no channel").
    ///
    /// `unread_mentions` is the subset of those unread messages that mention
    /// `user_id` (a `message_mentions` row for the user on a message whose id is
    /// greater than the channel's `last_read_message_id`). It is always
    /// `<= unread`.
    ///
    /// Returns `(channel_id, unread_count, unread_mention_count)` triples,
    /// ordered by channel id for determinism.
    pub fn unread_counts_for_server(
        &self,
        user_id: i64,
        server_id: i64,
    ) -> Vec<(i64, i64, i64)> {
        let Ok(c) = self.lock() else {
            return Vec::new();
        };
        let sql = "SELECT ch.id AS channel_id,
                          (SELECT COUNT(*) FROM messages m
                            WHERE m.channel_id = ch.id
                              AND m.id > COALESCE(cr.last_read_message_id, -1)) AS unread,
                          (SELECT COUNT(*) FROM messages m
                            JOIN message_mentions mm ON mm.message_id = m.id
                            WHERE m.channel_id = ch.id
                              AND mm.user_id = ?1
                              AND m.id > COALESCE(cr.last_read_message_id, -1)) AS mentions
                   FROM channels ch
                   JOIN server_memberships sm ON sm.server_id = ch.server_id AND sm.user_id = ?1
                   LEFT JOIN channel_reads cr ON cr.user_id = ?1 AND cr.channel_id = ch.id
                   WHERE ch.server_id = ?2
                   ORDER BY ch.id";
        let mut stmt = match c.prepare(sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map(rusqlite::params![user_id, server_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        }) {
            Ok(rows) => rows,
            Err(_) => return Vec::new(),
        };
        rows.filter_map(Result::ok).collect()
    }

    // ── DM channels ────────────────────────────────────────────────────────
    //
    // Private 1:1 conversations (`dm_channels` + `dm_participants`, migration
    // `0007_dm_channels.sql`). A DM channel has no name — its identity is the
    // pair of participants. The app layer enforces exactly two.
    // DM *messages* are a separate follow-up (they need their own table).

    /// Create a DM channel and add `user_a` and `user_b` as its participants
    /// in one transaction. Returns the new `dm_channels.id`.
    ///
    /// Both users must exist (FK violation otherwise). `created_at` /
    /// `joined_at` are UTC epoch milliseconds, supplied by the caller.
    pub fn insert_dm_channel(
        &self,
        user_a: i64,
        user_b: i64,
        created_at: i64,
    ) -> Result<i64, rusqlite::Error> {
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        tx.execute(
            "INSERT INTO dm_channels (created_at) VALUES (?1)",
            rusqlite::params![created_at],
        )?;
        let channel_id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO dm_participants (dm_channel_id, user_id, joined_at)
             VALUES (?1, ?2, ?3), (?1, ?4, ?3)",
            rusqlite::params![channel_id, user_a, created_at, user_b],
        )?;
        tx.commit()?;
        Ok(channel_id)
    }

    /// The DM channel shared by `user_a` and `user_b`, if one exists.
    /// The `JOIN` on distinct users is the pair-existence check: exactly one
    /// row is produced when both users participate in the same channel.
    pub fn dm_channel_between(&self, user_a: i64, user_b: i64) -> Option<i64> {
        let c = self.lock().ok()?;
        c.query_row(
            "SELECT d.id FROM dm_channels d
             WHERE d.id IN (SELECT dm_channel_id FROM dm_participants WHERE user_id = ?1)
               AND d.id IN (SELECT dm_channel_id FROM dm_participants WHERE user_id = ?2)
             ORDER BY d.id DESC LIMIT 1",
            rusqlite::params![user_a, user_b],
            |r| r.get::<_, i64>(0),
        )
        .ok()
    }

    /// The participants of a DM channel (all of them, including the caller).
    /// Returns `(user_id, username)` pairs.
    pub fn dm_participants_of(&self, channel_id: i64) -> Vec<(i64, String)> {
        let c = match self.lock() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut stmt = match c.prepare(
            "SELECT p.user_id, u.username
             FROM dm_participants p
             JOIN users u ON u.id = p.user_id
             WHERE p.dm_channel_id = ?1
             ORDER BY p.user_id",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map(rusqlite::params![channel_id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        }) {
            Ok(rows) => rows,
            Err(_) => return Vec::new(),
        };
        rows.filter_map(Result::ok).collect()
    }

    /// A DM channel's `created_at` (UTC epoch milliseconds), or `None` if the
    /// id is not a DM channel.
    pub fn dm_channel_created_at(&self, channel_id: i64) -> Option<i64> {
        let c = self.lock().ok()?;
        c.query_row(
            "SELECT created_at FROM dm_channels WHERE id = ?1",
            rusqlite::params![channel_id],
            |r| r.get::<_, i64>(0),
        )
        .ok()
    }

    /// All DM channels the user participates in, newest-created first.
    /// Returns `(channel_id, created_at)` pairs. (Until DM messages land,
    /// "most recent activity" == most recent creation.)
    pub fn dm_channels_for_user(&self, user_id: i64) -> Vec<(i64, i64)> {
        let c = match self.lock() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut stmt = match c.prepare(
            "SELECT d.id, d.created_at
             FROM dm_channels d
             JOIN dm_participants p ON p.dm_channel_id = d.id AND p.user_id = ?1
             ORDER BY d.created_at DESC, d.id DESC",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map(rusqlite::params![user_id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        }) {
            Ok(rows) => rows,
            Err(_) => return Vec::new(),
        };
        rows.filter_map(Result::ok).collect()
    }

    // ── DM messages ────────────────────────────────────────────────────────
    //
    // Messages inside a DM channel (`dm_messages`, migration
    // `0011_dm_messages.sql`). Separate from the guild `messages` table: the
    // latter's `channel_id` FKs `channels(id)`, a real-server id space — a DM
    // message's `channel_id` is a `dm_channels.id`.

    /// Does `user_id` participate in DM channel `channel_id`? (`None` when the
    /// channel doesn't exist.) Authorization for DM message send/list.
    pub fn is_dm_participant(&self, channel_id: i64, user_id: i64) -> bool {
        let c = match self.lock() {
            Ok(c) => c,
            Err(_) => return false,
        };
        c.query_row(
            "SELECT COUNT(*) FROM dm_participants WHERE dm_channel_id = ?1 AND user_id = ?2",
            rusqlite::params![channel_id, user_id],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0)
            > 0
    }

    /// Insert a DM message and return its new row id. `created_at` is UTC
    /// epoch milliseconds, as the schema dictates; the caller supplies it.
    pub fn insert_dm_message(
        &self,
        channel_id: i64,
        author_id: i64,
        content: &str,
        created_at: i64,
    ) -> Result<i64, rusqlite::Error> {
        let c = self.lock()?;
        c.execute(
            "INSERT INTO dm_messages (channel_id, author_id, content, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![channel_id, author_id, content, created_at],
        )?;
        Ok(c.last_insert_rowid())
    }

    /// The most recent `limit` messages in a DM channel, oldest-first (ready
    /// to hand to a client as initial history). Mirrors [`recent_messages`].
    pub fn recent_dm_messages(&self, channel_id: i64, limit: i64) -> Vec<Message> {
        let Ok(c) = self.lock() else {
            return Vec::new();
        };
        let mut stmt = match c.prepare(
            "SELECT id, channel_id, author_id, content, created_at, edited_at
             FROM dm_messages WHERE channel_id = ?1 ORDER BY created_at DESC, id DESC LIMIT ?2",
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
                mentioned_user_ids: Vec::new(),
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

    /// Create a server owned by `owner_id`, and make the owner its first
    /// member (role='owner'). Returns the new server id.
    pub fn insert_server(&self, owner_id: i64, name: &str) -> Result<i64, rusqlite::Error> {
        let created_at = now_epoch_ms();
        let c = self.lock()?;
        c.execute(
            "INSERT INTO servers (name, owner_id, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![name, owner_id, created_at],
        )?;
        let server_id = c.last_insert_rowid();
        c.execute(
            "INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES (?1, ?2, 'owner', ?3)",
            rusqlite::params![server_id, owner_id, created_at],
        )?;
        Ok(server_id)
    }

    /// Servers `user_id` is a member of, as `(id, name, icon_url)`, excluding
    /// soft-deleted servers.
    pub fn servers_for_user(&self, user_id: i64) -> Vec<(i64, String, Option<String>)> {
        let c = match self.lock() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut stmt = match c.prepare(
            "SELECT s.id, s.name, s.icon_url FROM servers s
             JOIN server_memberships m ON m.server_id = s.id
             WHERE m.user_id = ?1 AND s.deleted_at IS NULL",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map(rusqlite::params![user_id], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?))
        });
        match rows {
            Ok(rows) => rows.filter_map(Result::ok).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Add `user_id` as a 'member' of `server_id`. Errors (via the UNIQUE
    /// constraint) if already a member.
    pub fn insert_membership(&self, server_id: i64, user_id: i64) -> Result<(), rusqlite::Error> {
        let joined_at = now_epoch_ms();
        let c = self.lock()?;
        c.execute(
            "INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES (?1, ?2, 'member', ?3)",
            rusqlite::params![server_id, user_id, joined_at],
        )?;
        Ok(())
    }

    /// Channels in `server_id` as `(id, name, kind, position, parent_channel_id)`,
    /// ordered by position.
    pub fn channels_for_server(&self, server_id: i64) -> Vec<(i64, String, String, i64, Option<i64>)> {
        let c = match self.lock() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut stmt = match c.prepare(
            "SELECT id, name, kind, position, parent_channel_id FROM channels
             WHERE server_id = ?1 ORDER BY position",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map(rusqlite::params![server_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, Option<i64>>(4)?,
            ))
        });
        match rows {
            Ok(rows) => rows.filter_map(Result::ok).collect(),
            Err(_) => Vec::new(),
        }
    }

    // ── channels: management (create / rename / delete) ────────────────────

    /// The channel kind as stored in `channels.kind` (`'text'` / `'voice'` /
    /// `'category'`).
    pub const fn channel_kind_db_str(kind: ChannelKind) -> &'static str {
        kind.db_str()
    }

    /// A channel's identity, as read by the management layer: id, server,
    /// name, kind, position, and parent (category) channel id.
    pub fn channel_full(&self, id: i64) -> Option<(i64, i64, String, String, i64, Option<i64>)> {
        let c = self.lock().ok()?;
        c.query_row(
            "SELECT id, server_id, name, kind, position, parent_channel_id
             FROM channels WHERE id = ?1",
            rusqlite::params![id],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, Option<i64>>(5)?,
                ))
            },
        )
        .ok()
    }

    /// The lowest in-category position, so a new channel appends to the end
    /// (0 when the server has no channels).
    fn next_channel_position(&self, server_id: i64) -> i64 {
        let c = match self.lock() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        c.query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM channels WHERE server_id = ?1",
            rusqlite::params![server_id],
            |r| r.get(0),
        )
        .unwrap_or(0)
    }

    /// Create a channel in `server_id`. Returns the new channel id.
    pub fn insert_channel(
        &self,
        server_id: i64,
        name: &str,
        kind: ChannelKind,
        parent_channel_id: Option<i64>,
        created_at: i64,
    ) -> Result<i64, rusqlite::Error> {
        let position = self.next_channel_position(server_id);
        let c = self.lock()?;
        c.execute(
            "INSERT INTO channels (server_id, name, kind, position, parent_channel_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![server_id, name, kind.db_str(), position, parent_channel_id, created_at],
        )?;
        Ok(c.last_insert_rowid())
    }

    /// Rename a channel. Returns the new name. `Err` if the channel doesn't
    /// exist.
    pub fn rename_channel(&self, id: i64, name: &str) -> Result<String, rusqlite::Error> {
        let c = self.lock()?;
        let updated = c.execute(
            "UPDATE channels SET name = ?2 WHERE id = ?1",
            rusqlite::params![id, name],
        )?;
        if updated == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        Ok(name.to_string())
    }

    /// Soft-delete a channel (the schema has no `deleted_at` on channels, so
    /// "soft" here means: delete the row and let SQLite cascade to its
    /// messages + child category channels). Returns the number of rows
    /// deleted (0 if the id didn't exist).
    pub fn delete_channel(&self, id: i64) -> Result<usize, rusqlite::Error> {
        let c = self.lock()?;
        Ok(c.execute("DELETE FROM channels WHERE id = ?1", rusqlite::params![id])?)
    }

    // ── members: role management ───────────────────────────────────────────

    /// Insert or update `user_id`'s membership in `server_id`, setting their
    /// role. Returns the new role string. Uses `ON CONFLICT DO UPDATE` so a
    /// re-invite / role-change is a single atomic statement.
    pub fn upsert_membership(
        &self,
        server_id: i64,
        user_id: i64,
        role: Role,
        joined_at: i64,
    ) -> Result<(), rusqlite::Error> {
        let c = self.lock()?;
        c.execute(
            "INSERT INTO server_memberships (server_id, user_id, role, joined_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (server_id, user_id) DO UPDATE SET role = ?3",
            rusqlite::params![server_id, user_id, role.db_str(), joined_at],
        )?;
        Ok(())
    }

    /// Remove a member from a server. Returns the number of rows deleted (0
    /// if they were not a member).
    pub fn remove_membership(&self, server_id: i64, user_id: i64) -> Result<usize, rusqlite::Error> {
        let c = self.lock()?;
        Ok(c.execute(
            "DELETE FROM server_memberships WHERE server_id = ?1 AND user_id = ?2",
            rusqlite::params![server_id, user_id],
        )?)
    }

    /// All members of a server with their role, ordered by role (owner first)
    /// then user id. Returns `(user_id, username, role_string)`.
    pub fn server_members(&self, server_id: i64) -> Vec<(i64, String, String)> {
        let c = match self.lock() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut stmt = match c.prepare(
            "SELECT m.user_id, u.username, m.role
             FROM server_memberships m
             JOIN users u ON u.id = m.user_id
             WHERE m.server_id = ?1
             ORDER BY CASE m.role
                     WHEN 'owner' THEN 0 WHEN 'admin' THEN 1
                     WHEN 'moderator' THEN 2 ELSE 3 END, m.user_id",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map(rusqlite::params![server_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        }) {
            Ok(rows) => rows,
            Err(_) => return Vec::new(),
        };
        rows.filter_map(Result::ok).collect()
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

    // ── invites ────────────────────────────────────────────────────────────

    /// Insert a new invite and return its full row. The `code` is generated
    /// by the caller (the service layer); a duplicate `code` trips the global
    /// `UNIQUE` constraint, which the caller maps to a retry.
    pub fn insert_invite(
        &self,
        server_id: i64,
        code: &str,
        created_by: i64,
        created_at: i64,
        expires_at: Option<i64>,
        max_uses: Option<i64>,
    ) -> Result<InviteRow, rusqlite::Error> {
        let c = self.lock()?;
        c.execute(
            "INSERT INTO server_invites
                 (server_id, code, created_by, created_at, expires_at, max_uses, uses)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
            rusqlite::params![server_id, code, created_by, created_at, expires_at, max_uses],
        )?;
        let id = c.last_insert_rowid();
        Ok(Self::read_invite_row(c.query_row(
            "SELECT id, server_id, code, created_by, created_at, expires_at, max_uses, uses
             FROM server_invites WHERE id = ?1",
            rusqlite::params![id],
            Self::map_invite,
        )?))
    }

    /// Look up an invite by its (globally unique) code.
    pub fn invite_by_code(&self, code: &str) -> Option<InviteRow> {
        let c = self.lock().ok()?;
        let row = c
            .query_row(
                "SELECT id, server_id, code, created_by, created_at, expires_at, max_uses, uses
                 FROM server_invites WHERE code = ?1",
                rusqlite::params![code],
                Self::map_invite,
            )
            .ok()?;
        Some(Self::read_invite_row(row))
    }

    /// All live invites for a server, ordered by creation. (An "expired"
    /// invite is still a row; callers filter by `expires_at` if they want
    /// only the still-valid ones.)
    pub fn invites_for_server(&self, server_id: i64) -> Vec<InviteRow> {
        let c = match self.lock() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut stmt = match c.prepare(
            "SELECT id, server_id, code, created_by, created_at, expires_at, max_uses, uses
             FROM server_invites WHERE server_id = ?1 ORDER BY created_at, id",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map(rusqlite::params![server_id], Self::map_invite) {
            Ok(rows) => rows,
            Err(_) => return Vec::new(),
        };
        rows.filter_map(|r| r.ok()).map(Self::read_invite_row).collect()
    }

    /// Atomically consume one use of an invite. Increments `uses` only if the
    /// invite still has budget (i.e. it is unlimited or `uses < max_uses`),
    /// so concurrent redemptions can't overspend a capped code. Returns the
    /// new `uses` count on success, `0` if the code was already spent.
    pub fn redeem_invite_use(&self, invite_id: i64) -> Result<i64, rusqlite::Error> {
        let c = self.lock()?;
        let updated = c
            .execute(
                "UPDATE server_invites
                 SET uses = uses + 1
                 WHERE id = ?1 AND (max_uses IS NULL OR uses < max_uses)",
                rusqlite::params![invite_id],
            )? as i64;
        if updated == 0 {
            // No row was updated: the code was already spent (or missing).
            return Ok(0);
        }
        // Return the post-increment count for the caller to report.
        Ok(c.query_row(
            "SELECT uses FROM server_invites WHERE id = ?1",
            rusqlite::params![invite_id],
            |r| r.get(0),
        )?)
    }

    /// Revoke (delete) an invite for a server. Returns the number of rows
    /// deleted (0 if the code did not exist on that server).
    pub fn revoke_invite(&self, server_id: i64, code: &str) -> Result<usize, rusqlite::Error> {
        let c = self.lock()?;
        Ok(c.execute(
            "DELETE FROM server_invites WHERE server_id = ?1 AND code = ?2",
            rusqlite::params![server_id, code],
        )?)
    }

    fn map_invite(
        row: &rusqlite::Row<'_>,
    ) -> Result<(i64, i64, String, i64, i64, Option<i64>, Option<i64>, i64), rusqlite::Error> {
        Ok((
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
            row.get(6)?,
            row.get(7)?,
        ))
    }

    fn read_invite_row(t: (i64, i64, String, i64, i64, Option<i64>, Option<i64>, i64)) -> InviteRow {
        InviteRow {
            id: t.0,
            server_id: t.1,
            code: t.2,
            created_by: t.3,
            created_at: t.4,
            expires_at: t.5,
            max_uses: t.6,
            uses: t.7,
        }
    }
}

/// The fields of one invite row, as read by the invite layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InviteRow {
    pub id: i64,
    pub server_id: i64,
    pub code: String,
    pub created_by: i64,
    pub created_at: i64,
    /// Expiry in UTC epoch seconds; `None` = never expires.
    pub expires_at: Option<i64>,
    /// Maximum redemptions; `None` = unlimited.
    pub max_uses: Option<i64>,
    pub uses: i64,
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
    /// Resolved `@username` mentions: the user ids the message text mentions
    /// (see migration `0012_message_mentions.sql`). Empty when the message
    /// contains no `@` token that resolves to a member of the message's
    /// server. Populated on every `messages` row read; DM history rows
    /// (`dm_messages`) always come back empty.
    #[serde(default)]
    pub mentioned_user_ids: Vec<i64>,
}

/// The `@mention` token grammar, for messages: `@` followed by a run of ASCII
/// letters, digits, or underscores — `@[A-Za-z0-9_]+`. A token is one
/// contiguous run: `@alice_bob` is the single token `alice_bob` (not two
/// mentions), and `@alice@bob` is the two tokens `alice` and `bob` (the
/// character class excludes `@`). This helper is the single source of truth
/// for the grammar; username resolution against the member list happens in
/// `realtime::resolve_mentions` (case-insensitive, first match per token).
pub fn mention_tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_token = false;
    for ch in text.chars() {
        let is_word = ch.is_ascii_alphanumeric() || ch == '_';
        if in_token {
            if is_word {
                current.push(ch);
            } else {
                // A bare `@` with no following word char (e.g. `email@ x`, or
                // a trailing `@`) leaves `current` empty — skip it so no empty
                // token is emitted.
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
                in_token = false;
                // A non-word char immediately after the token still starts a
                // new token when it is `@` (e.g. `@a@b`).
                if ch == '@' {
                    in_token = true;
                }
            }
        } else if ch == '@' {
            in_token = true;
        }
    }
    if in_token && !current.is_empty() {
        out.push(current);
    }
    out
}
