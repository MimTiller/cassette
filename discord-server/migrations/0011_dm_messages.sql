-- 0011_dm_messages.sql
-- DM messages: the messages exchanged inside a `dm_channels` row.
--
-- A separate table from the guild `messages` table: that table's
-- `channel_id` is NOT NULL and FKs `channels(id)`, which requires a real
-- server. A DM message's `channel_id` is a `dm_channels.id` — a different id
-- space — so it gets its own row type with an FK to `dm_channels` (the same
-- reason documented in `0007_dm_channels.sql`).
--
-- Same column shape as a guild message (author, content, created/edited),
-- minus `reply_to_message_id` (DM reply threads are a later layer).
--
-- Timestamps are UTC epoch milliseconds (INTEGER), matching the schema
-- convention and the client's `epoch_millis` wire format.

CREATE TABLE dm_messages (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    channel_id INTEGER NOT NULL REFERENCES dm_channels (id) ON DELETE CASCADE,
    author_id  INTEGER NOT NULL REFERENCES users       (id) ON DELETE CASCADE,
    content    TEXT    NOT NULL,
    created_at INTEGER NOT NULL,
    edited_at  INTEGER
);

-- The dominant DM query is "the latest N messages in this channel":
-- a per-channel, descending-time index (mirrors `idx_messages_channel_time`).
CREATE INDEX idx_dm_messages_channel_time ON dm_messages (channel_id, created_at DESC);
