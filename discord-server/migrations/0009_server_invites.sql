-- 0009_server_invites.sql
-- Invite links: a shareable code that lets a user join a server.
--
-- One row per generated invite. `code` is the short, unambiguous,
-- URL-safe token a member hands to someone (Discord uses a base-62-ish
-- token; here we draw from a consonant-vowel alphabet to keep codes
-- readable and to avoid obvious lookalikes). The code is GLOBALLY unique
-- — like a real Discord invite, the code alone identifies the server it
-- belongs to, so a "join by code" endpoint needs nothing but the code.
--
-- `expires_at` is the invite's expiry in UTC epoch *seconds* and is
-- NULL for an invite that never expires (the common case). Expiry is
-- checked at redemption time, never enforced by a background sweep —
-- an expired row is simply rejected, and rows are deleted only when a
-- member explicitly revokes them.
--
-- `max_uses` caps how many times a code may be redeemed before it is
-- spent. `NULL` means unlimited uses (the default). `uses` counts the
-- successful redemptions so far.
--
-- `created_by` is the member who generated the invite. It is denormalised
-- here (rather than derived through `server_memberships`) so an invite
-- still names its creator even if that member later leaves the server;
-- it is NOT a foreign key, for exactly that reason.

CREATE TABLE server_invites (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    server_id   INTEGER NOT NULL REFERENCES servers (id) ON DELETE CASCADE,
    code        TEXT    NOT NULL,
    created_by  INTEGER NOT NULL REFERENCES users (id)   ON DELETE RESTRICT,
    created_at  INTEGER NOT NULL,
    expires_at  INTEGER,
    max_uses    INTEGER,
    uses        INTEGER NOT NULL DEFAULT 0,
    UNIQUE (code)
);

CREATE INDEX idx_invites_server ON server_invites (server_id);
