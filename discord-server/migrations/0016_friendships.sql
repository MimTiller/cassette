-- 0016_friendships.sql
-- The friends list: one row per (unordered) pair of users, with a lifecycle
-- status of 'pending' (a request awaiting a reply) or 'accepted' (friends).
--
-- The row is DIRECTIONAL in its id columns — `requester_id` sent the request,
-- `addressee_id` must answer it — but the PAIR is what must be unique: a
-- second request between the same two users is a duplicate. SQLite has no
-- expression UNIQUE index, so the pair is normalised into `min_user_id` /
-- `max_user_id` and the UNIQUE constraint rides on those: the app layer
-- always stores min(id) in `min_user_id`, and the CHECK below keeps the
-- normalisation honest. Swapping the ids of an existing request is a
-- status change in the app layer, never a new row.
--
-- `requester_id`/`addressee_id` also keep both users FK-referenced for
-- ON DELETE CASCADE: deleting either user drops their friendship rows
-- (and their DM rows, like every other users-FK table).

CREATE TABLE friendships (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    requester_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    addressee_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    min_user_id  INTEGER NOT NULL,
    max_user_id  INTEGER NOT NULL,
    status       TEXT    NOT NULL CHECK (status IN ('pending', 'accepted')),
    created_at   INTEGER NOT NULL,
    CHECK (min_user_id < max_user_id)
);

CREATE UNIQUE INDEX uq_friendships_pair ON friendships (min_user_id, max_user_id);
CREATE INDEX idx_friendships_min ON friendships (min_user_id);
CREATE INDEX idx_friendships_max ON friendships (max_user_id);
