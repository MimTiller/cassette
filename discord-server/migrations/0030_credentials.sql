-- 0030_credentials.sql
-- Separate HOW someone signs in from WHO they are: an account (a `users`
-- row) can carry several credentials, and one credential may later certify
-- an identity issued by an external service (`issuer` names the authority,
-- `subject` its id) instead of a local password.
--
-- `kind` is 'password' for the local salted PBKDF2 hash (kept in `secret`)
-- and, later, e.g. 'external' for an IdP-linked identity (no `secret`;
-- `issuer`/`subject` identify it). At most one password credential per
-- account, and at most one credential per (issuer, subject) pair.
--
-- The old `users.password_hash` column stays in the schema for now — every
-- existing row is moved into `credentials` below — but nothing reads it
-- anymore.

CREATE TABLE credentials (
    id         INTEGER PRIMARY KEY,
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind       TEXT    NOT NULL,
    secret     TEXT,
    issuer     TEXT,
    subject    TEXT,
    created_at INTEGER NOT NULL
);

-- One credential per external identity (NULL issuer/subject on password
-- rows never collide: SQLite UNIQUE ignores NULLs).
CREATE UNIQUE INDEX credentials_issuer_subject
    ON credentials(issuer, subject);

-- At most one 'password' credential per user: the indexed key is NULL for
-- every other kind, so those rows are unconstrained.
CREATE UNIQUE INDEX credentials_one_password_per_user
    ON credentials(CASE WHEN kind = 'password' THEN user_id ELSE NULL END);

-- Backfill: every existing user's stored hash becomes a password credential.
-- The old column stays in the schema (one more migration would drop it) but
-- nothing reads it anymore.
INSERT INTO credentials (user_id, kind, secret, created_at)
    SELECT u.id, 'password', u.password_hash, u.created_at FROM users AS u
    WHERE u.password_hash IS NOT NULL
;
