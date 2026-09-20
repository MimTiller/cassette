-- 0004_messages.sql
-- Messages, one per row, in a text channel.
--
-- `author_id` is the sending user. `reply_to_message_id` self-references the
-- message being replied to (NULL for non-replies) — the lightweight reply
-- thread Discord uses. `edited_at` is NULL until the first edit.
--
-- Timestamps are UTC epoch milliseconds (INTEGER) to line up with the
-- client's `runtime_shared::time::epoch_millis` and the wire format.

CREATE TABLE messages (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    channel_id          INTEGER NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    author_id           INTEGER NOT NULL REFERENCES users (id)    ON DELETE CASCADE,
    content             TEXT    NOT NULL,
    reply_to_message_id INTEGER REFERENCES messages (id)          ON DELETE SET NULL,
    created_at          INTEGER NOT NULL,
    edited_at           INTEGER
);

-- The dominant query is "give me the latest N messages in this channel":
-- a per-channel, descending-time index.
CREATE INDEX idx_messages_channel_time ON messages (channel_id, created_at DESC);
CREATE INDEX idx_messages_author       ON messages (author_id);
