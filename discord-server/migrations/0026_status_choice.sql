-- 0026_status_choice.sql
-- The status each user picks in the user panel: 'online', 'idle', 'dnd' or
-- 'invisible'. `presence_status` stays what others see; the gateway derives
-- it from this choice and whether the user is active (see
-- realtime::shown_presence).
ALTER TABLE users ADD COLUMN status_choice TEXT NOT NULL DEFAULT 'online';
