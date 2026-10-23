-- Move image storage out of SQLite: `message_attachments.data` held the image
-- bytes inline as a `data:image/…;base64,` URL. The media layer
-- (`discord-server/src/media.rs`) now keeps the bytes on disk content-
-- addressed and stores only the path (`/media/<sha256>.<ext>`), so this
-- migration renames the column to `url` (a `/media/…` path or a legacy
-- inline `data:` URL until the one-time `migrate_legacy` pass converts it).
--
-- No new SQL: same FK/cascade posture as `0015_message_attachments.sql`;
-- `users.avatar_url` / `servers.icon_url` keep their TEXT type — their VALUES
-- move to paths via the Rust pass, not the schema.

ALTER TABLE message_attachments RENAME COLUMN data TO url;
