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

mod password;
mod users;
mod servers;
mod channels;
mod messages;
mod dms;
mod friends;
mod invites;

#[cfg(feature = "server")]
pub use password::{hash_password, password_needs_rehash,
               burn_password_check, verify_password};

/// Current UTC wall-clock time as epoch **milliseconds** (the schema
/// convention for every `created_at` / `edited_at` column).
#[cfg(feature = "server")]
fn now_epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
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
    /// The id of the message this one replies to (`None` for a normal post).
    /// Column is `ON DELETE SET NULL`: if the target message is deleted the
    /// reference is dropped, not the reply. Absent from `dm_messages` (DM
    /// reply threads are a later layer) — DM history rows come back `None`.
    #[serde(default)]
    pub reply_to_message_id: Option<i64>,
    /// Resolved `@username` mentions: the user ids the message text mentions
    /// (see migration `0012_message_mentions.sql`). Empty when the message
    /// contains no `@` token that resolves to a member of the message's
    /// server. Populated on every `messages` row read; DM history rows
    /// (`dm_messages`) always come back empty.
    #[serde(default)]
    pub mentioned_user_ids: Vec<i64>,
    /// Image attachments persisted with the message; always empty for
    /// `dm_messages` rows.
    #[serde(default)]
    pub attachments: Vec<Attachment>,
}

/// One image attached to a channel message (migration
/// `0015_message_attachments.sql`), as persisted.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Attachment {
    pub id: i64,
    pub filename: String,
    pub mime: String,
    /// The `url` column: either a legacy inline `data:image/…;base64,` URL
    /// (pre-media migration) or a `/media/<sha>.<ext>` path served by the
    /// backend.
    pub url: String,
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

