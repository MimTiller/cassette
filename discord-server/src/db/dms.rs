//! DM channels and DM history.

use super::Db;
use super::Message;

impl Db {
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
                reply_to_message_id: None,
                mentioned_user_ids: Vec::new(),
                attachments: Vec::new(),
            })
        }) {
            Ok(rows) => rows,
            Err(_) => return Vec::new(),
        };
        let mut out: Vec<Message> = rows.filter_map(Result::ok).collect();
        out.reverse();
        out
    }
}
