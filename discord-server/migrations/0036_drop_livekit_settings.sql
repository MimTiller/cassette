-- 0036_drop_livekit_settings.sql
-- LiveKit is no longer a call provider anywhere: idea-chat is the only one.
-- The instance store (0035) may still hold livekit_* keys saved before the
-- switch; the settings code reads only 'call_provider', 'chat_backend',
-- 'idea_chat_url', 'instance_name' and the idea-chat relay keys, so delete
-- the livekit rows and leave no stale credentials behind.

DELETE FROM instance_settings WHERE key LIKE 'livekit_%';
