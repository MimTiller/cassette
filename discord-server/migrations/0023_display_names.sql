-- 0023_display_names.sql
-- A display name shown in place of the username (NULL = show the username)
-- and a short "about me" for the profile card.
ALTER TABLE users ADD COLUMN display_name TEXT;
ALTER TABLE users ADD COLUMN about TEXT NOT NULL DEFAULT '';
