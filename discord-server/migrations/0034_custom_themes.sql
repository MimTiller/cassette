-- 0034_custom_themes.sql
-- Idempotent: dev databases that already ran this under an earlier number
-- apply it a second time, so the table and index guards keep it a no-op.
-- The colour themes a person makes for themselves: an imported or
-- customised palette, kept per account so it follows them to another device.
-- `base` is the built-in palette it started from ('dark' or 'light'), and
-- `colors_json` is an array of hex colours in the app's role order.
-- Deleting the user leaves no orphans.

CREATE TABLE IF NOT EXISTS custom_themes (
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name        TEXT    NOT NULL,
    base        TEXT    NOT NULL,
    colors_json TEXT    NOT NULL,
    created_at  INTEGER NOT NULL,
    PRIMARY KEY (user_id, name)
);

CREATE INDEX IF NOT EXISTS idx_custom_themes_user ON custom_themes (user_id);
