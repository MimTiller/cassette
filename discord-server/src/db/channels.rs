//! Channel lookups and channel management (create / rename / delete).

use super::Db;
use crate::roles::ChannelKind;

impl Db {
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
}
