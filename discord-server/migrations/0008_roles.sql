-- 0008_roles.sql
-- Extend the membership role set to include 'moderator'.
--
-- `0005_server_memberships.sql` committed to three roles (owner / admin /
-- member) and noted that "a full role *system* (named roles + per-role
-- permissions) is a later layer". This migration is that first step: it
-- widens the CHECK to a four-role hierarchy
--   owner  >  admin  >  moderator  >  member
-- so a server can carry a moderator standing between admin and the default
-- member. The new value is purely additive — no existing row changes its
-- stored value; 'moderator' starts with zero rows.
--
-- SQLite cannot edit a CHECK constraint in place, so the column is rebuilt
-- with the documented add / copy / drop / rename dance. Data is preserved
-- verbatim (each row's `role` is copied into `role_new` before the drop).
-- `DROP COLUMN` requires SQLite 3.35+ (2021); rusqlite's bundled libsqlite3
-- is far newer, so this is safe. The `UNIQUE (server_id, user_id)` constraint
-- does not reference `role`, so dropping the column leaves it intact.

ALTER TABLE server_memberships ADD COLUMN role_new
    TEXT NOT NULL DEFAULT 'member'
    CHECK (role_new IN ('owner', 'admin', 'moderator', 'member'));

UPDATE server_memberships SET role_new = role;

ALTER TABLE server_memberships DROP COLUMN role;

ALTER TABLE server_memberships RENAME COLUMN role_new TO role;

-- Permission lookups filter by (server, role); give them an index.
CREATE INDEX idx_memberships_server_role ON server_memberships (server_id, role);
