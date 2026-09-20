-- 0005_server_memberships.sql
-- The (user, server) edge of the social graph — who is in which server,
-- and at what role.
--
-- `user_id` / `server_id` form a UNIQUE pair (you can only be in a server
-- once). `role` is the membership's top role on that server:
--   'owner'  — the creator (exactly one per server, set on creation)
--   'admin'  — a server admin
--   'member' — default
-- A full role *system* (named roles + per-role permissions) is a later
-- layer; a single per-membership role column is enough for the data layer
-- and keeps the schema to the five tables the task calls out.

CREATE TABLE server_memberships (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    server_id  INTEGER NOT NULL REFERENCES servers (id) ON DELETE CASCADE,
    user_id    INTEGER NOT NULL REFERENCES users (id)   ON DELETE CASCADE,
    role       TEXT    NOT NULL DEFAULT 'member'
               CHECK (role IN ('owner', 'admin', 'member')),
    joined_at  INTEGER NOT NULL,
    UNIQUE (server_id, user_id)
);

CREATE INDEX idx_memberships_user   ON server_memberships (user_id);
CREATE INDEX idx_memberships_server ON server_memberships (server_id);
