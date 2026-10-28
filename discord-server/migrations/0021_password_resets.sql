-- One-time password reset links. The operator issues one with
-- `server reset-link <username>`; only the SHA-256 of the link's token is
-- stored. A user has at most one live token: issuing a new one or using
-- one deletes the rest.
CREATE TABLE password_resets (
    token_hash TEXT    PRIMARY KEY,
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at INTEGER NOT NULL
);

CREATE INDEX password_resets_user ON password_resets(user_id);
