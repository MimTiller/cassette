-- 0022_permissions.sql
-- Discord-style permissions (see src/permissions.rs for the bit values).
--
-- * servers.everyone_permissions: what @everyone may do. 31 = view channels,
--   send messages, add reactions, attach files, embed links.
-- * server_roles.permissions: what a named role adds.
-- * channel_overwrites: per-channel allow/deny for @everyone (target_id 0),
--   a role, or one member. Private channels deny VIEW_CHANNEL to @everyone.

ALTER TABLE servers ADD COLUMN everyone_permissions INTEGER NOT NULL DEFAULT 31;
ALTER TABLE server_roles ADD COLUMN permissions INTEGER NOT NULL DEFAULT 0;

CREATE TABLE channel_overwrites (
    channel_id  INTEGER NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    target_kind TEXT    NOT NULL CHECK (target_kind IN ('everyone', 'role', 'member')),
    target_id   INTEGER NOT NULL,
    allow       INTEGER NOT NULL DEFAULT 0,
    deny        INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (channel_id, target_kind, target_id)
);

-- A deleted role's overwrites go with it.
CREATE TRIGGER channel_overwrites_on_role_delete AFTER DELETE ON server_roles
BEGIN
    DELETE FROM channel_overwrites WHERE target_kind = 'role' AND target_id = OLD.id;
END;

-- So do a member's when they leave (or are kicked or banned).
CREATE TRIGGER channel_overwrites_on_leave AFTER DELETE ON server_memberships
BEGIN
    DELETE FROM channel_overwrites
     WHERE target_kind = 'member'
       AND target_id = OLD.user_id
       AND channel_id IN (SELECT id FROM channels WHERE server_id = OLD.server_id);
END;

-- Carry the old membership levels over as roles with the same powers, ranked
-- above every existing role (which granted nothing until now). Moderator is
-- inserted first so Admin lands above it. A server that already has a role
-- with the name gets "<name> (level)" instead.
--   Moderator 480   = mention @everyone, manage messages, create invite, kick
--   Admin     16352 = Moderator + ban, timeout, manage channels, manage roles,
--                     manage server
INSERT INTO server_roles (server_id, name, color, mentionable, position, created_at, permissions)
SELECT m.server_id,
       CASE WHEN EXISTS (SELECT 1 FROM server_roles r
                          WHERE r.server_id = m.server_id AND r.name = 'Moderator' COLLATE NOCASE)
            THEN 'Moderator (level)' ELSE 'Moderator' END,
       '', 0,
       (SELECT COALESCE(MAX(r.position), 0) + 1 FROM server_roles r WHERE r.server_id = m.server_id),
       CAST(strftime('%s', 'now') AS INTEGER) * 1000,
       480
  FROM server_memberships m
 WHERE m.role = 'moderator'
 GROUP BY m.server_id;

INSERT INTO server_roles (server_id, name, color, mentionable, position, created_at, permissions)
SELECT m.server_id,
       CASE WHEN EXISTS (SELECT 1 FROM server_roles r
                          WHERE r.server_id = m.server_id AND r.name = 'Admin' COLLATE NOCASE)
            THEN 'Admin (level)' ELSE 'Admin' END,
       '', 0,
       (SELECT COALESCE(MAX(r.position), 0) + 1 FROM server_roles r WHERE r.server_id = m.server_id),
       CAST(strftime('%s', 'now') AS INTEGER) * 1000,
       16352
  FROM server_memberships m
 WHERE m.role = 'admin'
 GROUP BY m.server_id;

INSERT INTO member_roles (role_id, user_id)
SELECT r.id, m.user_id
  FROM server_memberships m
  JOIN server_roles r ON r.server_id = m.server_id
                     AND r.permissions = CASE m.role WHEN 'admin' THEN 16352 ELSE 480 END
                     AND r.name IN ('Admin', 'Admin (level)', 'Moderator', 'Moderator (level)')
 WHERE m.role IN ('admin', 'moderator');
