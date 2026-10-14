-- 0014_message_pins.sql
-- Per-message pin state for a channel. A row means "this message is pinned in
-- its channel". `message_id` is UNIQUE (a message can be pinned at most once)
-- and an FK to `messages(id)` with ON DELETE CASCADE, so deleting a message
-- (or its channel, which cascades the messages) clears the pin with it.
--
-- `pinned_by` records who pinned it (informational, shown in the pinned view);
-- it is deliberately NOT an FK to users, so the pin outlives the user who set
-- it (mirrors `channel_reads` keeping read state independent of the user row's
-- lifetime in the read path). `pinned_at` is UTC epoch milliseconds, matching
-- the messages.created_at convention, and is the ordering key for the view.
--
-- The dominant queries are "is this message pinned?" (by message id) and
-- "what is pinned in this channel?" (by joining to the message's channel), so
-- the single index on `message_id` serves the first and the join uses it.

CREATE TABLE message_pins (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id INTEGER NOT NULL UNIQUE REFERENCES messages (id) ON DELETE CASCADE,
    pinned_by  INTEGER NOT NULL,
    pinned_at  INTEGER NOT NULL
);

CREATE INDEX idx_message_pins_message ON message_pins (message_id);
