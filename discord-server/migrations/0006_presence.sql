-- 0006_presence.sql
-- One presence status per user, maintained by the realtime gateway:
-- 'online' while a socket is open, 'away' after the idle timeout,
-- 'offline' once every socket has closed. Stored as a plain TEXT column
-- (no CHECK constraint — the gateway is the only writer and validates the
-- value in the db layer).

ALTER TABLE users ADD COLUMN presence_status TEXT NOT NULL DEFAULT 'offline';
