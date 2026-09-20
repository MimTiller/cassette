-- 0001_users.sql
-- The people in the app. One row per account.
--
-- `username` is the @handle (case-insensitive in real Discord; enforced
-- case-sensitively here and normalised by the app layer). `discriminator`
-- is the legacy #1234 suffix — nullable now that username is globally
-- unique, but kept for API shape parity. `avatar_url` is the remote image
-- (or null for the default avatar).
--
-- Auth columns (password_hash / totp / etc.) are intentionally ABSENT:
-- authentication is a later layer. The data layer stays focused on the
-- social-graph entities the task calls out.

CREATE TABLE users (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    username      TEXT    NOT NULL UNIQUE,
    discriminator TEXT,
    email         TEXT    UNIQUE,
    avatar_url    TEXT,
    created_at    INTEGER NOT NULL
);
