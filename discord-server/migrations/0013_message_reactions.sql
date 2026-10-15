-- 0013_message_reactions.sql
-- Emoji reactions per channel message: one row per (message, user, emoji).
--
-- A reaction is (message, user, emoji): the user chose that emoji on that
-- message. `message_id` cascades with the message (a deleted message takes its
-- reactions with it) and `user_id` cascades with the user; the
-- UNIQUE (message_id, user_id, emoji) key makes a re-add an upsert and guards
-- against duplicate rows. `emoji` is the raw emoji string the client picked
-- (no FK — it is user-chosen text, like message content).
--
-- The live gateway writes these rows when a client sends a
-- `ReactionAdd`/`ReactionRemove` frame (see `realtime::add_channel_reaction` /
-- `remove_channel_reaction`), then fans out a `Reaction` frame carrying the
-- new per-emoji count so every connected member of the channel's server sees
-- it appear (or drop) live. History reads fold them back in via
-- `db.reactions_for_message` (see `Message.reactions`).
--
-- The dominant query is "which reactions does this message have" — one index
-- on the message id.

CREATE TABLE message_reactions (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id INTEGER NOT NULL REFERENCES messages (id) ON DELETE CASCADE,
    user_id    INTEGER NOT NULL REFERENCES users    (id) ON DELETE CASCADE,
    emoji      TEXT    NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (message_id, user_id, emoji)
);

CREATE INDEX idx_message_reactions_message ON message_reactions (message_id);
CREATE INDEX idx_message_reactions_user    ON message_reactions (user_id);
