-- 0036_drop_livekit_settings.sql
-- LiveKit is no longer a call provider. Its stored credentials (set through
-- the instance admin page or the environment) are removed from the settings
-- store. The table itself stays: it still holds call_provider, chat_backend,
-- idea_chat_url and instance_name.
DELETE FROM instance_settings
 WHERE key IN ('livekit_url', 'livekit_api_key', 'livekit_api_secret');
