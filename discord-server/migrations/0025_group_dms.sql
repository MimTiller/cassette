-- 0025_group_dms.sql
-- Group DMs: a DM channel with up to 10 participants, an optional name and
-- an owner (who can remove people). 1:1 DMs keep is_group = 0.
ALTER TABLE dm_channels ADD COLUMN is_group INTEGER NOT NULL DEFAULT 0;
ALTER TABLE dm_channels ADD COLUMN name TEXT;
ALTER TABLE dm_channels ADD COLUMN owner_id INTEGER REFERENCES users (id) ON DELETE SET NULL;
