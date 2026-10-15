-- 0014_server_timeouts.sql
-- Transient timeout per member: a timed-out member stays in the server (unlike
-- a kick or ban, which remove the membership row) but is blocked from posting
-- messages until `timeout_until` passes.
--
-- A single nullable column on `server_memberships` (not a separate table) is
-- the smallest shape that carries it: a timeout is a property of the
-- membership itself (there is no independent timeout row to upsert), `NULL`
-- means "not timed out", and a re-timeout is a plain overwrite of the column.
-- The enforcement point (`post_channel_message`) consults it via
-- `db.is_timed_out`, comparing against the current epoch.
--
-- `timeout_until` is UTC epoch seconds (Unix timestamp), matching the
-- `joined_at` / `banned_at` convention — `ALTER TABLE ADD COLUMN` needs no
-- value for existing rows, which all default to `NULL` (never timed out).

ALTER TABLE server_memberships ADD COLUMN timeout_until INTEGER;
