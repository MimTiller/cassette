-- 0013_server_bans.sql
-- Permanent ban per (server, user): a member who has been *banned* from a
-- server (as opposed to merely kicked) is recorded here and is barred from
-- rejoining it.
--
-- A ban is two effects the app layer performs together: the membership row is
-- removed (so the user is out of the server immediately, exactly like a kick)
-- and a row is written here. The rejoin block lives in the two join paths —
-- `redeem_invite` and `add_member` — which both consult this table before
-- granting membership. `kick` does NOT write here, so a kicked user can still
-- be re-invited; only a banned user is refused.
--
-- `server_id` cascades with the server (a soft-deleted server takes its bans
-- with it); `user_id` cascades with the user. `UNIQUE (server_id, user_id)`
-- makes a re-ban a single upsert (re-banning a still-banned user is a no-op,
-- not a duplicate row). `banned_by` is denormalised — not a foreign key — so
-- the row still names the moderator who banned the member even after that
-- moderator leaves the server (the same rationale as `server_invites.created_by`).
-- `reason` is an optional free-text note the moderator may attach.
--
-- `banned_at` is UTC epoch seconds, matching the `created_at` convention.

CREATE TABLE server_bans (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    server_id  INTEGER NOT NULL REFERENCES servers (id) ON DELETE CASCADE,
    user_id    INTEGER NOT NULL REFERENCES users   (id) ON DELETE CASCADE,
    banned_by  INTEGER NOT NULL,
    reason     TEXT,
    banned_at  INTEGER NOT NULL,
    UNIQUE (server_id, user_id)
);

CREATE INDEX idx_server_bans_user   ON server_bans (user_id);
CREATE INDEX idx_server_bans_server ON server_bans (server_id);
