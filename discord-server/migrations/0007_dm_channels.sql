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
-- derives from the latest message (a follow-up layer; until DM messages land
-- the list falls back to the channel's `created_at`).
--
-- DM *messages* are a separate follow-up: they need their own table
-- (`dm_messages`) because the guild `messages` table's `channel_id` is NOT
-- NULL and FKs `channels(id)`, which requires a real server — a DM message's
-- `channel_id` would be a `dm_channels.id`, a different id space. That table
-- is added together with the message send/list endpoints.
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

-- The dominant DM queries, indexed:
--   "my DM channels, newest first"        → participants.user_id
--   "who is the other side of this DM?"   → participants.dm_channel_id
CREATE INDEX idx_dm_participants_user    ON dm_participants (user_id);
CREATE INDEX idx_dm_participants_channel ON dm_participants (dm_channel_id);
