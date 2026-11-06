-- 0031_avatar_decoration.sql
-- The catalog id of the decoration a user wears on their avatar (NULL = none).
ALTER TABLE users ADD COLUMN avatar_decoration TEXT;
