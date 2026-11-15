-- 0035_instance_settings.sql
-- Instance-wide settings the host's own admin area configures: a key/value
-- store (keys like 'call_provider', 'chat_backend', 'idea_chat_url',
-- 'instance_name', plus the retired voice provider's credentials, which
-- 0036 clears) so the admin page can grow without new columns, and the
-- instance-admin roster.
--
-- Secrets in here (the voice provider's secret, idea-chat credentials) are
-- write-only from the client: the server stores the plaintext and hands back
-- only secret_set + the last 4 characters.
--
-- Env vars stay the fallback: a key present in the table wins over its
-- env var; the admin page labels which source each effective value came
-- from. `instance_admins` marks WHO may read/change this store, per account
-- (deleting the user leaves no orphans).

CREATE TABLE instance_settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE instance_admins (
    user_id INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_instance_settings_key ON instance_settings (key);
