-- 0028_notification_settings.sql
-- Each user's notification level and mute per server and per channel.
-- level NULL = inherit; muted_until NULL = not muted, -1 = until turned
-- off, else an epoch-ms deadline (see notification_settings.rs).
CREATE TABLE notification_settings (
    user_id     INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    scope       TEXT    NOT NULL CHECK (scope IN ('server', 'channel')),
    scope_id    INTEGER NOT NULL,
    level       TEXT    CHECK (level IN ('all', 'mentions', 'nothing')),
    muted_until INTEGER,
    PRIMARY KEY (user_id, scope, scope_id)
);
