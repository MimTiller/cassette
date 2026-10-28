-- 0020_custom_roles.sql
-- Named server roles (Discord's "roles"): a name, an optional display colour,
-- whether any member may @mention it, and a position (higher = listed first
-- and wins the name colour). Members can hold any number of them.
--
-- These sit beside `server_memberships.role` (owner/admin/moderator/member),
-- which stays the permission level; a named role is identity, colour and a
-- mention group.

CREATE TABLE server_roles (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    server_id   INTEGER NOT NULL REFERENCES servers (id) ON DELETE CASCADE,
    name        TEXT    NOT NULL,
    color       TEXT    NOT NULL DEFAULT '',
    mentionable INTEGER NOT NULL DEFAULT 1,
    position    INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL
);

CREATE UNIQUE INDEX idx_server_roles_name ON server_roles (server_id, name COLLATE NOCASE);

CREATE TABLE member_roles (
    role_id INTEGER NOT NULL REFERENCES server_roles (id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    PRIMARY KEY (role_id, user_id)
);

CREATE INDEX idx_member_roles_user ON member_roles (user_id);

-- Leaving (or being kicked/banned from) a server drops that server's roles.
CREATE TRIGGER member_roles_on_leave AFTER DELETE ON server_memberships
BEGIN
    DELETE FROM member_roles
     WHERE user_id = OLD.user_id
       AND role_id IN (SELECT id FROM server_roles WHERE server_id = OLD.server_id);
END;
