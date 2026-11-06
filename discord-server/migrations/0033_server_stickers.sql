-- 0033_server_stickers.sql
-- A server's stickers: bigger than custom emoji, sent as whole messages.
--
-- `related_emoji` is one unicode emoji or a `:custom:` token: a search
-- chip for the sticker, never part of how the sticker renders. The creator
-- link is SET NULL, not CASCADE: deleting a user keeps the server's stickers
-- and the messages that carry their copies.
--
-- `ALTER`-added columns on the message tables carry the sticker copied at
-- SEND time (id, name, image path) so a message still renders after the
-- sticker itself is deleted — these columns deliberately have no FK into
-- `stickers`. NULL = a text message.

CREATE TABLE stickers (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    server_id    INTEGER NOT NULL REFERENCES servers (id) ON DELETE CASCADE,
    name         TEXT    NOT NULL,
    description  TEXT    DEFAULT '',
    related_emoji TEXT   NOT NULL,
    image_path   TEXT    NOT NULL,
    created_by   INTEGER REFERENCES users (id) ON DELETE SET NULL,
    created_at   INTEGER NOT NULL
);

CREATE UNIQUE INDEX idx_sticker_name ON stickers (server_id, name COLLATE NOCASE);

ALTER TABLE messages ADD COLUMN sticker_id INTEGER;
ALTER TABLE messages ADD COLUMN sticker_name TEXT;
ALTER TABLE messages ADD COLUMN sticker_image TEXT;

ALTER TABLE dm_messages ADD COLUMN sticker_id INTEGER;
ALTER TABLE dm_messages ADD COLUMN sticker_name TEXT;
ALTER TABLE dm_messages ADD COLUMN sticker_image TEXT;
