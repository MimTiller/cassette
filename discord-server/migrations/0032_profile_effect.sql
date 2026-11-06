-- 0032_profile_effect.sql
-- The catalog id of the effect drawn over a user's profile card (NULL = none).
ALTER TABLE users ADD COLUMN profile_effect TEXT;
