-- 0002_user_creds.sql
-- Auth layer: password verification.
--
-- `password_hash` holds a self-describing PBKDF2-HMAC-SHA256 secret:
--   pbkdf2-sha256$<iterations>$<salt-b64>$<hash-b64>
-- so a future rotation to a stronger KDF only needs to re-hash on next
-- login (we detect the prefix and re-derive). `login_failures` tracks
-- recent failed logins per user id for per-account lockout; the per-IP
-- window limiter (see src/rate_limit.rs) is the primary throttle.

ALTER TABLE users ADD COLUMN password_hash TEXT;

CREATE TABLE login_failures (
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    at          INTEGER NOT NULL
);
CREATE INDEX idx_login_failures_user ON login_failures(user_id, at);
