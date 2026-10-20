-- 0015_message_attachments.sql
-- Image attachments on channel messages: one row per image attached to a
-- message. `data` holds the image inline as a `data:image/…;base64,` URL, so
-- the backend never fetches remote bytes (same posture as `users.avatar_url`).
--
-- `message_id` is an FK to `messages(id)` with ON DELETE CASCADE: deleting a
-- message (or its channel, which cascades the messages) clears its
-- attachments with it. Unlike `message_pins`, a message may carry MANY
-- attachments, so `message_id` is deliberately NOT UNIQUE; the post path
-- enforces the per-message count cap instead.
--
-- Limits are enforced at the post boundary (`realtime::post_channel_message`),
-- not by SQL: mime in {png,jpeg,webp,gif}, ≤ 2 MB decoded per image, ≤ 4 per
-- message.
--
-- The dominant queries are "what is attached to this message?" (a history
-- page joining by message id), so the single index on `message_id` serves it.

CREATE TABLE message_attachments (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id   INTEGER NOT NULL REFERENCES messages (id) ON DELETE CASCADE,
    filename     TEXT    NOT NULL,
    mime         TEXT    NOT NULL,
    data         TEXT    NOT NULL,
    created_at   INTEGER NOT NULL
);

CREATE INDEX idx_message_attachments_message ON message_attachments (message_id);
