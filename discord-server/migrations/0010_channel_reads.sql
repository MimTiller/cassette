-- 0010_channel_reads.sql
-- Per-user, per-channel read state: the latest message id the user has seen
-- in each channel. This is the primitive behind the unread-count badge the
-- client draws next to each channel — a message is "unread" for a user when
-- its id is greater than the user's `last_read_message_id` for that channel.
--
-- `last_read_message_id` is a plain message id (the messages.id of the newest
-- message the user has seen). NULL = never read, i.e. every message is unread.
-- It is intentionally NOT an FK to messages(id): the client may mark a
-- channel read at any message id, and a channel's messages cascade away with
-- the channel (messages ON DELETE CASCADE), so a dangling id must stay legal —
-- it simply reads as "everything is unread" once the messages are gone.
--
-- `read_at` records when the read was last advanced (UTC epoch milliseconds,
-- matching the messages.created_at convention); it is informational and is not
-- used by the unread-count query.
--
-- The UNIQUE (user_id, channel_id) key is the whole point: one read row per
-- (user, channel), written with an upsert. Indexes mirror the two dominant
-- queries — "what is my read state for this channel?" and "which of this
-- user's channels do I have read state for?".

CREATE TABLE channel_reads (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id               INTEGER NOT NULL REFERENCES users    (id) ON DELETE CASCADE,
    channel_id            INTEGER NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    last_read_message_id  INTEGER,
    read_at               INTEGER NOT NULL,
    UNIQUE (user_id, channel_id)
);

CREATE INDEX idx_channel_reads_user    ON channel_reads (user_id);
CREATE INDEX idx_channel_reads_channel ON channel_reads (channel_id);
