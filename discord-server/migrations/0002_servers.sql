-- 0002_servers.sql
-- The guilds / "servers". One row per server.
--
-- `owner_id` names the creator (the owner's membership row is created with
-- role = 'owner' by the application layer on server creation). A server is
-- never deleted outright in the app — `deleted_at` soft-removes it so
-- channels and memberships can be cascaded without losing message history.

CREATE TABLE servers (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT    NOT NULL,
    owner_id   INTEGER NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    icon_url   TEXT,
    deleted_at INTEGER,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_servers_owner ON servers (owner_id);
