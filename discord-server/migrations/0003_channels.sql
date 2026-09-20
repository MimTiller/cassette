-- 0003_channels.sql
-- Text, voice, and category channels. One table, discriminated by `kind`
-- (the canonical Discord shape — channels are polymorphic).
--
-- `parent_channel_id` self-references the owning *category* channel
-- (parent.kind = 'category'), giving the grouped sidebar layout. Top-level
-- channels have parent_channel_id NULL. `position` is the in-category sort
-- order (float-free: a plain integer, reordered by the app on drag).
--
-- A server is not directly deletable while it still has channels: the FK
-- is RESTRICT and the app soft-deletes via servers.deleted_at.

CREATE TABLE channels (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    server_id         INTEGER NOT NULL REFERENCES servers (id) ON DELETE RESTRICT,
    name              TEXT    NOT NULL,
    kind              TEXT    NOT NULL CHECK (kind IN ('text', 'voice', 'category')),
    position          INTEGER NOT NULL DEFAULT 0,
    parent_channel_id INTEGER REFERENCES channels (id) ON DELETE CASCADE,
    created_at        INTEGER NOT NULL
);

-- One server + one parent (or NULL) can have at most one channel of a given
-- name — the constraint that makes channel rename / create deterministic.
-- (SQLite has no partial-unique-on-NULL, so NULL parents are exempt; the
-- app layer treats the "top-level" group per server as the NULL-parent set.)
CREATE UNIQUE INDEX idx_channels_server_parent_name
    ON channels (server_id, COALESCE(parent_channel_id, 0), name);

CREATE INDEX idx_channels_server   ON channels (server_id);
CREATE INDEX idx_channels_parent   ON channels (parent_channel_id);
