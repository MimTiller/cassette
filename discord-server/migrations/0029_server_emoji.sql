-- 0029_server_emoji.sql
-- Each server's custom emoji (see server_emoji.rs). Names are unique per
-- server, ignoring case.
CREATE TABLE server_emoji (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    server_id  INTEGER NOT NULL REFERENCES servers (id) ON DELETE CASCADE,
    name       TEXT    NOT NULL,
    image_path TEXT    NOT NULL,
    created_by INTEGER REFERENCES users (id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX idx_server_emoji_name ON server_emoji (server_id, name COLLATE NOCASE);
