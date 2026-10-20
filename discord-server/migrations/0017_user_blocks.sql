-- 0017_user_blocks.sql
-- User blocks: one row per DIRECTED (blocker, blocked) pair. Blocking is a
-- unilateral act — the pair is only "blocked" when a block exists in EITHER
-- direction, which the app layer checks as a pair (see `blocks_pair`).
--
-- The composite PRIMARY KEY (blocker_id, blocked_id) makes a second block by
-- the same blocker a no-op (the app inserts with `INSERT OR IGNORE`) and a
-- reverse block a separate row. Both columns FK to `users` with ON DELETE
-- CASCADE: deleting either user drops their block rows, like every other
-- users-FK table. `created_at` is UTC epoch milliseconds (the schema's
-- timestamp unit).
--
-- A block has no status lifecycle (unlike `friendships`): it is a plain
-- presence/absence fact, removed by deleting the row.

CREATE TABLE user_blocks (
    blocker_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    blocked_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (blocker_id, blocked_id)
);

CREATE INDEX idx_user_blocks_blocked ON user_blocks (blocked_id);
