//! DM channels and DM history.

use super::Db;
use super::Message;

impl Db {
    // ── DM channels ────────────────────────────────────────────────────────
    //
    // Private conversations (`dm_channels` + `dm_participants`, migrations
    // `0007_dm_channels.sql` and `0025_group_dms.sql`). A 1:1 DM has exactly
    // two participants and no name; a group DM (`is_group`) has up to ten,
    // an optional name and an owner. The service layer enforces the sizes.

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

    /// The 1:1 DM channel shared by `user_a` and `user_b`, if one exists
    /// (group DMs they are both in don't count).
    pub fn dm_channel_between(&self, user_a: i64, user_b: i64) -> Option<i64> {
        let c = self.lock().ok()?;
        c.query_row(
            "SELECT d.id FROM dm_channels d
             WHERE d.is_group = 0
               AND d.id IN (SELECT dm_channel_id FROM dm_participants WHERE user_id = ?1)
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

    /// Create a group DM owned by `owner` with `members` (the owner
    /// included) and an optional name. Returns the new channel id.
    pub fn insert_group_dm(
        &self,
        owner: i64,
        members: &[i64],
        name: Option<&str>,
        created_at: i64,
    ) -> Result<i64, rusqlite::Error> {
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        tx.execute(
            "INSERT INTO dm_channels (created_at, is_group, name, owner_id) VALUES (?1, 1, ?2, ?3)",
            rusqlite::params![created_at, name, owner],
        )?;
        let channel_id = tx.last_insert_rowid();
        for user in members {
            tx.execute(
                "INSERT INTO dm_participants (dm_channel_id, user_id, joined_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![channel_id, user, created_at],
            )?;
        }
        tx.commit()?;
        Ok(channel_id)
    }

    /// `(is_group, name, owner_id)` of a DM channel, or `None` if it doesn't
    /// exist.
    pub fn dm_channel_info(&self, channel_id: i64) -> Option<(bool, Option<String>, Option<i64>)> {
        let c = self.lock().ok()?;
        c.query_row(
            "SELECT is_group, name, owner_id FROM dm_channels WHERE id = ?1",
            rusqlite::params![channel_id],
            |r| Ok((r.get::<_, i64>(0)? != 0, r.get(1)?, r.get(2)?)),
        )
        .ok()
    }

    pub fn add_dm_participant(&self, channel_id: i64, user_id: i64, joined_at: i64) -> Result<usize, rusqlite::Error> {
        let c = self.lock()?;
        c.execute(
            "INSERT OR IGNORE INTO dm_participants (dm_channel_id, user_id, joined_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![channel_id, user_id, joined_at],
        )
    }

    /// Remove `user_id` from a group DM. If they owned it, the member who
    /// joined earliest becomes the owner; the last one out deletes the group.
    pub fn remove_dm_participant(&self, channel_id: i64, user_id: i64) -> Result<(), rusqlite::Error> {
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        tx.execute(
            "DELETE FROM dm_participants WHERE dm_channel_id = ?1 AND user_id = ?2",
            rusqlite::params![channel_id, user_id],
        )?;
        let next: Option<i64> = tx
            .query_row(
                "SELECT user_id FROM dm_participants WHERE dm_channel_id = ?1 ORDER BY joined_at, id LIMIT 1",
                rusqlite::params![channel_id],
                |r| r.get(0),
            )
            .ok();
        match next {
            None => {
                tx.execute("DELETE FROM dm_channels WHERE id = ?1", rusqlite::params![channel_id])?;
            }
            Some(next) => {
                tx.execute(
                    "UPDATE dm_channels SET owner_id = ?2 WHERE id = ?1 AND (owner_id = ?3 OR owner_id IS NULL)",
                    rusqlite::params![channel_id, next, user_id],
                )?;
            }
        }
        tx.commit()
    }

    pub fn set_dm_name(&self, channel_id: i64, name: Option<&str>) -> Result<usize, rusqlite::Error> {
        let c = self.lock()?;
        c.execute("UPDATE dm_channels SET name = ?2 WHERE id = ?1", rusqlite::params![channel_id, name])
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
