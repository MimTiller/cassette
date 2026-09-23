-- 0007_dm_channels.sql
-- Direct messages: the (user, user) DM channel and its participants.
--
-- A DM is a private 1:1 conversation. It is modelled as its own entity
-- (`dm_channels`) rather than reusing the guild `channels` table because that
-- table's `server_id` is NOT NULL and FKs `servers` — a DM belongs to no
-- guild. The participants live in `dm_participants`, one row per member; a
-- real DM always has exactly two, so the app layer enforces that on start.
--
-- `dm_channels` carries no name (a DM's identity is its two participants) and
-- no position — the list is ordered by most-recent activity, which the app
-- derives from the latest message (see `dm_messages.last_message_at`).
--
-- `dm_messages` is a separate table from guild `messages`: the latter's
-- `channel_id` is NOT NULL and FKs `channels(id)`, which requires a real
-- server. A DM message's `channel_id` is a `dm_channels.id` — a different id
-- space — so it needs its own row type with an FK to `dm_channels`.
--
-- Timestamps are UTC epoch milliseconds (INTEGER), matching the schema
-- convention and the client's `epoch_millis` wire format.

CREATE TABLE dm_channels (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at INTEGER NOT NULL
);

-- A participant edge of the social graph: (dm_channel, user). One row per
-- member. The pair is UNIQUE — a user can be in a given DM channel only once.
CREATE TABLE dm_participants (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    dm_channel_id  INTEGER NOT NULL REFERENCES dm_channels (id) ON DELETE CASCADE,
    user_id        INTEGER NOT NULL REFERENCES users       (id) ON DELETE CASCADE,
    joined_at      INTEGER NOT NULL,
    UNIQUE (dm_channel_id, user_id)
);

-- A DM message. Same column shape as a guild message (author, content,
-- created/edited), but `channel_id` is a `dm_channels.id` and there is no
-- reply-to (DM reply threads are a later layer).
CREATE TABLE dm_messages (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    channel_id INTEGER NOT NULL REFERENCES dm_channels (id) ON DELETE CASCADE,
    author_id  INTEGER NOT NULL REFERENCES users       (id) ON DELETE CASCADE,
    content    TEXT    NOT NULL,
    created_at INTEGER NOT NULL,
    edited_at  INTEGER
);

-- The dominant DM queries, indexed:
--   "my DM channels, newest activity first"  → participants.user_id
--   "latest N messages in this DM, newest first" → channel + time desc
CREATE INDEX idx_dm_participants_user      ON dm_participants (user_id);
CREATE INDEX idx_dm_participants_channel   ON dm_participants (dm_channel_id);
CREATE INDEX idx_dm_messages_channel_time  ON dm_messages (channel_id, created_at DESC);
CREATE INDEX idx_dm_messages_author        ON dm_messages (author_id);
