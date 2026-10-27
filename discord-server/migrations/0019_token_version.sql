-- 0019_token_version.sql
-- Revocable sessions: every session token carries the account's
-- token_version at issue time, and a request whose token carries an older
-- version is unauthenticated. Bumping the column (a password change) logs
-- out every existing session of that account at once.
ALTER TABLE users ADD COLUMN token_version INTEGER NOT NULL DEFAULT 0;
