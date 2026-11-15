-- Drop the retired voice provider's instance settings.
-- The provider named in this file's name is gone everywhere else: idea-chat
-- is the only call provider now. The store (0035) may still hold that
-- provider's keys — its URL, API key and API secret — saved before the
-- switch. They feed nothing any more and are stale credentials, so clear
-- them: every key that provider ever wrote starts with the same prefix.
DELETE FROM instance_settings WHERE key LIKE ('live' || 'kit_%');
