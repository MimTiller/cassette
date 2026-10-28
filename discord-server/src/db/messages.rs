//! Channel messages: post/edit/delete, history, mentions, attachments,
//! reactions, read state and pins.

use super::Db;
use super::{Message, Attachment};
use std::collections::HashMap;

impl Db {
    // ── messages ───────────────────────────────────────────────────────────

    /// Insert a chat message and return its new row id. `created_at` is UTC
    /// epoch milliseconds, as the schema dictates; the caller supplies it.
    ///
    /// This inserts the message row only. Resolved `@mention` rows are written
    /// separately via [`Self::insert_message_mentions`] (the caller resolves
    /// the tokens against the member list — see `realtime::resolve_mentions`).
    /// Message attachments go through [`Self::insert_message_with_attachments`]
    /// so the message row and its `message_attachments` rows land together.
    pub fn insert_message(
        &self,
        channel_id: i64,
        author_id: i64,
        content: &str,
        created_at: i64,
        reply_to_message_id: Option<i64>,
    ) -> Result<i64, rusqlite::Error> {
        let c = self.lock()?;
        c.execute(
            "INSERT INTO messages (channel_id, author_id, content, created_at, reply_to_message_id)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![channel_id, author_id, content, created_at, reply_to_message_id],
        )?;
        Ok(c.last_insert_rowid())
    }

    /// Insert a chat message AND its image attachments in ONE SQLite
    /// transaction, so a message never exists without its attachments (and
    /// vice versa) if the process dies mid-post. `attachments` is a list of
    /// `(filename, mime, url)` triples, `url` a `/media/…` path (column renamed
    /// from `data` by migration `0018_attachment_media_paths.sql`);
    /// attachment rows share the message's `created_at`. Returns
    /// `(message_id, attachment_ids)` in input order.
    pub fn insert_message_with_attachments(
        &self,
        channel_id: i64,
        author_id: i64,
        content: &str,
        created_at: i64,
        reply_to_message_id: Option<i64>,
        attachments: &[(String, String, String)],
    ) -> Result<(i64, Vec<i64>), rusqlite::Error> {
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        tx.execute(
            "INSERT INTO messages (channel_id, author_id, content, created_at, reply_to_message_id)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![channel_id, author_id, content, created_at, reply_to_message_id],
        )?;
        let message_id = tx.last_insert_rowid();
        let mut attachment_ids: Vec<i64> = Vec::new();
        for (filename, mime, data) in attachments {
            tx.execute(
                "INSERT INTO message_attachments (message_id, filename, mime, url, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![message_id, filename, mime, data, created_at],
            )?;
            attachment_ids.push(tx.last_insert_rowid());
        }
        tx.commit()?;
        Ok((message_id, attachment_ids))
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
    /// `@mention` user ids (see `0012_message_mentions.sql`) and its image
    /// attachments (see `0015_message_attachments.sql`).
    pub fn recent_messages(&self, channel_id: i64, limit: i64) -> Vec<Message> {
        let mut out: Vec<Message> = {
            let Ok(c) = self.lock() else {
                return Vec::new();
            };
            let mut stmt = match c.prepare(
                "SELECT id, channel_id, author_id, content, created_at, edited_at, reply_to_message_id
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
                    reply_to_message_id: r.get(6)?,
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
        }; // drop the connection lock before re-locking in message_mentions
        for m in &mut out {
            m.mentioned_user_ids = self.message_mentions(m.id);
            m.attachments = self.message_attachments(m.id);
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
                "SELECT id, channel_id, author_id, content, created_at, edited_at, reply_to_message_id
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
                    reply_to_message_id: r.get(6)?,
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
        }; // drop the connection lock before re-locking in message_mentions
        for m in &mut out {
            m.mentioned_user_ids = self.message_mentions(m.id);
            m.attachments = self.message_attachments(m.id);
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

    /// The image attachments stored for `message_id` (`message_attachments`,
    /// see migration `0015_message_attachments.sql`), ordered by id. Empty
    /// when the message has none (or the id is unknown).
    pub fn message_attachments(&self, message_id: i64) -> Vec<Attachment> {
        let Ok(c) = self.lock() else {
            return Vec::new();
        };
        let mut stmt = match c.prepare(
            "SELECT id, filename, mime, url FROM message_attachments WHERE message_id = ?1 ORDER BY id",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map(rusqlite::params![message_id], |r| {
            Ok(Attachment {
                id: r.get(0)?,
                filename: r.get(1)?,
                mime: r.get(2)?,
                url: r.get(3)?,
            })
        }) {
            Ok(rows) => rows,
            Err(_) => return Vec::new(),
        };
        rows.filter_map(Result::ok).collect()
    }

    // ── message reactions ──────────────────────────────────────────────────
    //
    // Emoji reactions per channel message (`message_reactions`): one row per
    // (message, user, emoji). Written by `realtime::add_channel_reaction` /
    // `remove_channel_reaction` — the bodies behind the `#[server]` `add_reaction`
    // / `remove_reaction` endpoints — which also broadcast a
    // `WsServerMsg::ReactionAdd` / `ReactionRemove` frame so connected members
    // see the pill live. History loads fold them back per message via
    // `reactions_for_message` (the gateway attaches them to
    // `WsServerMsg::Message.reactions`). See migration `0013_message_reactions.sql`.

    /// Record that `user_id` reacted with `emoji` to `message_id`. Upsert: a
    /// re-add of the same (message, user, emoji) is a no-op (the UNIQUE key
    /// guards it). `created_at` is UTC epoch ms supplied by the caller.
    ///
    /// # Errors
    /// - the message or user does not exist (FK violation), or the write fails.
    pub fn add_reaction(
        &self,
        message_id: i64,
        user_id: i64,
        emoji: &str,
        created_at: i64,
    ) -> Result<(), rusqlite::Error> {
        let c = self.lock()?;
        c.execute(
            "INSERT OR IGNORE INTO message_reactions
             (message_id, user_id, emoji, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![message_id, user_id, emoji, created_at],
        )?;
        Ok(())
    }

    /// Remove `user_id`'s `emoji` reaction from `message_id`. Returns the
    /// number of rows removed (0 if the reaction did not exist) — mirroring the
    /// other helpers' "0 rows" convention rather than erroring on a no-op.
    ///
    /// # Errors
    /// - the write fails.
    pub fn remove_reaction(
        &self,
        message_id: i64,
        user_id: i64,
        emoji: &str,
    ) -> Result<usize, rusqlite::Error> {
        let c = self.lock()?;
        Ok(c.execute(
            "DELETE FROM message_reactions
             WHERE message_id = ?1 AND user_id = ?2 AND emoji = ?3",
            rusqlite::params![message_id, user_id, emoji],
        )?)
    }

    /// The reactions stored for `message_id`, grouped by emoji: one
    /// `(emoji, count, reacted_by_me)` tuple per distinct emoji, where
    /// `count` is the number of users who reacted with that emoji and
    /// `reacted_by_me` is whether `user_id` is among them. Ordered by count
    /// (desc) then emoji so the busiest reactions lead, like Discord. Empty
    /// when the message has no reactions (or the id is unknown).
    pub fn reactions_for_message(&self, message_id: i64, user_id: i64) -> Vec<(String, u32, bool)> {
        let Ok(c) = self.lock() else {
            return Vec::new();
        };
        let mut stmt = match c.prepare(
            "SELECT emoji, user_id FROM message_reactions WHERE message_id = ?1",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map(rusqlite::params![message_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        }) {
            Ok(rows) => rows,
            Err(_) => return Vec::new(),
        };
        // Group by emoji, tracking count and whether `user_id` reacted.
        let mut by_emoji: HashMap<String, (u32, bool)> = HashMap::new();
        for (emoji, uid) in rows.filter_map(Result::ok) {
            let e = by_emoji.entry(emoji).or_insert((0, false));
            e.0 += 1;
            if uid == user_id {
                e.1 = true;
            }
        }
        let mut out: Vec<(String, u32, bool)> =
            by_emoji.into_iter().map(|(e, (c, m))| (e, c, m)).collect();
        // Busiest first; ties broken by emoji for a stable order.
        out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        out
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

    /// The newest message id in `channel_id` (`None` for an empty channel).
    pub fn latest_message_id(&self, channel_id: i64) -> Option<i64> {
        let c = self.lock().ok()?;
        c.query_row(
            "SELECT MAX(id) FROM messages WHERE channel_id = ?1",
            rusqlite::params![channel_id],
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

    // ── message pins ───────────────────────────────────────────────────────
    //
    // Per-message pin state (`message_pins`, migration
    // `0014_message_pins.sql`). A row means "this message is pinned in its
    // channel"; `pinned_by` + `pinned_at` record who and when (shown in the
    // pinned view). The message's `channel_id` comes from the `messages` join,
    // so pinning is always scoped to a real guild message.

    /// Record that `message_id` is pinned in its channel by `pinned_by`.
    /// `INSERT OR IGNORE` against the `message_id` UNIQUE key makes a
    /// double-pin a no-op rather than an error. `pinned_at` is UTC epoch
    /// milliseconds, supplied by the caller.
    pub fn pin_message(
        &self,
        message_id: i64,
        pinned_by: i64,
        pinned_at: i64,
    ) -> Result<(), rusqlite::Error> {
        let c = self.lock()?;
        c.execute(
            "INSERT OR IGNORE INTO message_pins (message_id, pinned_by, pinned_at)
             VALUES (?1, ?2, ?3)",
            rusqlite::params![message_id, pinned_by, pinned_at],
        )?;
        Ok(())
    }

    /// Remove the pin on `message_id`. Returns the number of rows deleted
    /// (0 if it was not pinned).
    pub fn unpin_message(&self, message_id: i64) -> Result<usize, rusqlite::Error> {
        let c = self.lock()?;
        Ok(c.execute(
            "DELETE FROM message_pins WHERE message_id = ?1",
            rusqlite::params![message_id],
        )?)
    }

    /// Whether `message_id` is pinned (a `message_pins` row exists).
    pub fn message_is_pinned(&self, message_id: i64) -> bool {
        let c = match self.lock() {
            Ok(c) => c,
            Err(_) => return false,
        };
        c.query_row(
            "SELECT 1 FROM message_pins WHERE message_id = ?1",
            rusqlite::params![message_id],
            |_| Ok(()),
        )
        .is_ok()
    }

    /// The number of pinned messages in `channel_id`.
    pub fn pinned_count(&self, channel_id: i64) -> i64 {
        let c = match self.lock() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        c.query_row(
            "SELECT COUNT(*) FROM message_pins p
             JOIN messages m ON m.id = p.message_id
             WHERE m.channel_id = ?1",
            rusqlite::params![channel_id],
            |r| r.get(0),
        )
        .unwrap_or(0)
    }

    /// Every pinned message in `channel_id`, in the order they were pinned
    /// (oldest pin first — the ordering the pinned view uses). Each message
    /// carries its resolved `@mention` ids (populated like
    /// [`recent_messages`]) plus the `pinned_by` user id.
    pub fn pinned_messages(&self, channel_id: i64) -> Vec<(Message, i64)> {
        let mut out: Vec<(Message, i64)> = {
            let Ok(c) = self.lock() else {
                return Vec::new();
            };
            let mut stmt = match c.prepare(
                "SELECT m.id, m.channel_id, m.author_id, m.content, m.created_at, m.edited_at, p.pinned_by
                 FROM message_pins p
                 JOIN messages m ON m.id = p.message_id
                 WHERE m.channel_id = ?1
                 ORDER BY p.pinned_at, p.id",
            ) {
                Ok(s) => s,
                Err(_) => return Vec::new(),
            };
            let rows = match stmt.query_map(rusqlite::params![channel_id], |r| {
                let pinned_by: i64 = r.get(6)?;
                Ok((
                    Message {
                        id: r.get(0)?,
                        channel_id: r.get(1)?,
                        author_id: r.get(2)?,
                        content: r.get(3)?,
                        created_at: r.get(4)?,
                        edited_at: r.get(5)?,
                        reply_to_message_id: None,
                        mentioned_user_ids: Vec::new(),
                        attachments: Vec::new(),
                    },
                    pinned_by,
                ))
            }) {
                Ok(rows) => rows,
                Err(_) => return Vec::new(),
            };
            rows.filter_map(Result::ok).collect()
        }; // drop the connection lock before re-locking in message_mentions
        for (m, _) in &mut out {
            m.mentioned_user_ids = self.message_mentions(m.id);
        }
        out
    }
}
