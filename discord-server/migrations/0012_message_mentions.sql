-- 0012_message_mentions.sql
-- Resolved @mentions per message: one row per (message, mentioned user).
--
-- Written by the live gateway when a channel message is posted: the message
-- text is parsed for `@username` tokens, each token is resolved against the
-- channel's server members, and the resolved user ids land here. The list is
-- broadcast to clients inside the realtime `Message` payload
-- (`mentioned_user_ids`) and feeds the per-channel unread-mention count
-- (message id > the viewer's `channel_reads.last_read_message_id`).
--
-- `message_id` cascades with the message; `user_id` cascades with the user.
-- The UNIQUE key guards against duplicate rows for the same (message, user).
-- The query shape is "which of my unread messages mentioned me in this
-- channel" — indexed on both the message and the mentioned user.

CREATE TABLE message_mentions (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id INTEGER NOT NULL REFERENCES messages (id) ON DELETE CASCADE,
    user_id    INTEGER NOT NULL REFERENCES users    (id) ON DELETE CASCADE,
    UNIQUE (message_id, user_id)
);

CREATE INDEX idx_message_mentions_message ON message_mentions (message_id);
CREATE INDEX idx_message_mentions_user    ON message_mentions (user_id);
