//! Server rows, memberships, named-role updates, bans and timeouts.

use super::Db;
use super::{now_epoch_ms, sqlite_err};
use crate::roles::Role;

impl Db {
    // ── servers ────────────────────────────────────────────────────────────

    /// Look up a server by id. Returns `(id, name, owner_id, icon_url)`.
    pub fn server_by_id(&self, id: i64) -> Option<(i64, String, i64, Option<String>)> {
        let c = self.lock().ok()?;
        c.query_row(
            "SELECT id, name, owner_id, icon_url FROM servers WHERE id = ?1 AND deleted_at IS NULL",
            rusqlite::params![id],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .ok()
    }

    /// Create a server owned by `owner_id`, and make the owner its first
    /// member (role='owner'). Returns the new server id.
    pub fn insert_server(&self, owner_id: i64, name: &str) -> Result<i64, rusqlite::Error> {
        let created_at = now_epoch_ms();
        let c = self.lock()?;
        c.execute(
            "INSERT INTO servers (name, owner_id, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![name, owner_id, created_at],
        )?;
        let server_id = c.last_insert_rowid();
        c.execute(
            "INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES (?1, ?2, 'owner', ?3)",
            rusqlite::params![server_id, owner_id, created_at],
        )?;
        Ok(server_id)
    }

    /// Servers `user_id` is a member of, as `(id, name, icon_url)`, excluding
    /// soft-deleted servers.
    pub fn servers_for_user(&self, user_id: i64) -> Vec<(i64, String, Option<String>)> {
        let c = match self.lock() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut stmt = match c.prepare(
            "SELECT s.id, s.name, s.icon_url FROM servers s
             JOIN server_memberships m ON m.server_id = s.id
             WHERE m.user_id = ?1 AND s.deleted_at IS NULL",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map(rusqlite::params![user_id], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?))
        });
        match rows {
            Ok(rows) => rows.filter_map(Result::ok).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Add `user_id` as a 'member' of `server_id`. Errors (via the UNIQUE
    /// constraint) if already a member.
    pub fn insert_membership(&self, server_id: i64, user_id: i64) -> Result<(), rusqlite::Error> {
        let joined_at = now_epoch_ms();
        let c = self.lock()?;
        c.execute(
            "INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES (?1, ?2, 'member', ?3)",
            rusqlite::params![server_id, user_id, joined_at],
        )?;
        Ok(())
    }


    // ── members: role management ───────────────────────────────────────────

    /// Insert or update `user_id`'s membership in `server_id`, setting their
    /// role. Returns the new role string. Uses `ON CONFLICT DO UPDATE` so a
    /// re-invite / role-change is a single atomic statement.
    pub fn upsert_membership(
        &self,
        server_id: i64,
        user_id: i64,
        role: Role,
        joined_at: i64,
    ) -> Result<(), rusqlite::Error> {
        let c = self.lock()?;
        c.execute(
            "INSERT INTO server_memberships (server_id, user_id, role, joined_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (server_id, user_id) DO UPDATE SET role = ?3",
            rusqlite::params![server_id, user_id, role.db_str(), joined_at],
        )?;
        Ok(())
    }

    /// Remove a member from a server. Returns the number of rows deleted (0
    /// if they were not a member).
    pub fn remove_membership(&self, server_id: i64, user_id: i64) -> Result<usize, rusqlite::Error> {
        let c = self.lock()?;
        Ok(c.execute(
            "DELETE FROM server_memberships WHERE server_id = ?1 AND user_id = ?2",
            rusqlite::params![server_id, user_id],
        )?)
    }

    /// All members of a server with their role, ordered by role (owner first)
    /// then user id. Returns `(user_id, username, role_string)`.
    pub fn server_members(&self, server_id: i64) -> Vec<(i64, String, String)> {
        let c = match self.lock() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut stmt = match c.prepare(
            "SELECT m.user_id, u.username, m.role
             FROM server_memberships m
             JOIN users u ON u.id = m.user_id
             WHERE m.server_id = ?1
             ORDER BY CASE m.role
                     WHEN 'owner' THEN 0 WHEN 'admin' THEN 1
                     WHEN 'moderator' THEN 2 ELSE 3 END, m.user_id",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map(rusqlite::params![server_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        }) {
            Ok(rows) => rows,
            Err(_) => return Vec::new(),
        };
        rows.filter_map(Result::ok).collect()
    }

    /// The role of `user_id`'s membership in `server_id` (`'owner'`, `'admin'`,
    /// or `'member'`), or `None` if the user is not a member.
    pub fn membership_role(&self, server_id: i64, user_id: i64) -> Option<String> {
        let c = self.lock().ok()?;
        c.query_row(
            "SELECT role FROM server_memberships WHERE server_id = ?1 AND user_id = ?2",
            rusqlite::params![server_id, user_id],
            |r| r.get::<_, String>(0),
        )
        .ok()
    }

    // ── members: bans ──────────────────────────────────────────────────────

    /// Whether `user_id` is banned from `server_id`. A banned user is barred
    /// from rejoining (the rejoin paths consult this before granting
    /// membership).
    pub fn is_banned(&self, server_id: i64, user_id: i64) -> bool {
        let c = match self.lock() {
            Ok(c) => c,
            Err(_) => return false,
        };
        c.query_row(
            "SELECT 1 FROM server_bans WHERE server_id = ?1 AND user_id = ?2",
            rusqlite::params![server_id, user_id],
            |_| Ok(()),
        )
        .is_ok()
    }

    // ── members: timeouts ──────────────────────────────────────────────────

    /// `user_id`'s current timeout deadline in `server_id` (UTC epoch seconds),
    /// or `None` when they are not timed out. The membership is *not* removed
    /// on a timeout (unlike a kick or ban) — this is just the instant after
    /// which the member may post again.
    pub fn timeout_until(&self, server_id: i64, user_id: i64) -> Option<i64> {
        let c = self.lock().ok()?;
        c.query_row(
            "SELECT timeout_until FROM server_memberships
             WHERE server_id = ?1 AND user_id = ?2",
            rusqlite::params![server_id, user_id],
            |r| r.get(0),
        )
        .ok()
        .flatten()
    }

    /// Whether `user_id` is currently timed out in `server_id`: true when they
    /// have a `timeout_until` that is still in the future relative to `now`
    /// (UTC epoch seconds). The enforcement point (`post_channel_message`)
    /// consults this before persisting a message. A `NULL` deadline (never
    /// timed out) and a past deadline (expired) both return false.
    pub fn is_timed_out(&self, server_id: i64, user_id: i64, now: i64) -> bool {
        matches!(
            self.timeout_until(server_id, user_id),
            Some(deadline) if deadline > now
        )
    }

    /// Set (or refresh) `user_id`'s timeout in `server_id` to expire at
    /// `timeout_until` (UTC epoch seconds). A re-timeout overwrites the column
    /// in place — the membership row is untouched. Returns the number of rows
    /// updated (`0` when the user is not a member, in which case nothing is
    /// changed).
    pub fn set_timeout(
        &self,
        server_id: i64,
        user_id: i64,
        timeout_until: i64,
    ) -> Result<usize, rusqlite::Error> {
        let c = self.lock()?;
        Ok(c.execute(
            "UPDATE server_memberships SET timeout_until = ?3
             WHERE server_id = ?1 AND user_id = ?2",
            rusqlite::params![server_id, user_id, timeout_until],
        )?)
    }

    /// Clear `user_id`'s timeout in `server_id` (set the column back to `NULL`)
    /// — the manual "release early" a moderator may want. Returns the number of
    /// rows updated (`0` when they were not a member or not timed out).
    pub fn clear_timeout(&self, server_id: i64, user_id: i64) -> Result<usize, rusqlite::Error> {
        let c = self.lock()?;
        Ok(c.execute(
            "UPDATE server_memberships SET timeout_until = NULL
             WHERE server_id = ?1 AND user_id = ?2",
            rusqlite::params![server_id, user_id],
        )?)
    }

    /// Record that `user_id` is banned from `server_id` by `banned_by`, with an
    /// optional `reason`. The `UNIQUE (server_id, user_id)` constraint makes a
    /// re-ban a single upsert (a re-ban refreshes the row rather than
    /// duplicating it). Returns the number of rows written (always 1).
    pub fn ban(
        &self,
        server_id: i64,
        user_id: i64,
        banned_by: i64,
        reason: Option<&str>,
        banned_at: i64,
    ) -> Result<usize, rusqlite::Error> {
        let c = self.lock()?;
        c.execute(
            "INSERT INTO server_bans (server_id, user_id, banned_by, reason, banned_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (server_id, user_id) DO UPDATE SET
                 banned_by = excluded.banned_by,
                 reason = excluded.reason,
                 banned_at = excluded.banned_at",
            rusqlite::params![server_id, user_id, banned_by, reason, banned_at],
        )
    }

    /// Remove a ban (unban) for `user_id` in `server_id`. Returns the number of
    /// rows deleted (0 if they were not banned).
    pub fn unban(&self, server_id: i64, user_id: i64) -> Result<usize, rusqlite::Error> {
        let c = self.lock()?;
        Ok(c.execute(
            "DELETE FROM server_bans WHERE server_id = ?1 AND user_id = ?2",
            rusqlite::params![server_id, user_id],
        )?)
    }

    /// Ban `user_id` from `server_id`: atomically remove their membership row
    /// AND record the ban. This is the single transaction a `ban` goes through,
    /// so a failure can never leave the user kicked-but-not-banned (or
    /// banned-but-still-present). Returns the number of memberships removed —
    /// `0` means the user was not a member and nothing was written (the ban
    /// insert is skipped in that case, so a ban only ever records someone who
    /// actually left).
    pub fn ban_and_remove(
        &self,
        server_id: i64,
        user_id: i64,
        banned_by: i64,
        reason: Option<&str>,
        banned_at: i64,
    ) -> Result<usize, rusqlite::Error> {
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        let removed = tx.execute(
            "DELETE FROM server_memberships WHERE server_id = ?1 AND user_id = ?2",
            rusqlite::params![server_id, user_id],
        )?;
        if removed == 0 {
            // They were not a member: rolling back leaves nothing changed.
            tx.rollback()?;
            return Ok(0);
        }
        tx.execute(
            "INSERT INTO server_bans (server_id, user_id, banned_by, reason, banned_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (server_id, user_id) DO UPDATE SET
                 banned_by = excluded.banned_by,
                 reason = excluded.reason,
                 banned_at = excluded.banned_at",
            rusqlite::params![server_id, user_id, banned_by, reason, banned_at],
        )?;
        tx.commit()?;
        Ok(removed)
    }

    /// Rename the server with `id`. Returns the new name.
    pub fn rename_server(&self, id: i64, name: &str) -> Result<String, rusqlite::Error> {
        let c = self.lock()?;
        let updated = c.execute(
            "UPDATE servers SET name = ?2 WHERE id = ?1 AND deleted_at IS NULL",
            rusqlite::params![id, name],
        )?;
        if updated == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        Ok(name.to_string())
    }

    /// Set (or clear) the server icon. `icon` is `None` to clear the icon,
    /// `Some(url)` to set it to that URL string. Returns the stored icon URL.
    pub fn set_server_icon(
        &self,
        id: i64,
        icon: Option<&str>,
    ) -> Result<Option<String>, rusqlite::Error> {
        let c = self.lock()?;
        let updated = c.execute(
            "UPDATE servers SET icon_url = ?2 WHERE id = ?1 AND deleted_at IS NULL",
            rusqlite::params![id, icon],
        )?;
        if updated == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        Ok(icon.map(|s| s.to_string()))
    }

    /// Transfer server ownership from `from_id` to `to_id`.
    ///
    /// The transfer happens atomically inside a single transaction:
    /// 1. `servers.owner_id` is updated (guarded by `owner_id = from_id`, so a
    ///    concurrent rename of ownership fails the guard and rolls back).
    /// 2. The old owner's membership role is demoted from `'owner'` to `'admin'`.
    /// 3. The new owner's membership role is promoted to `'owner'`.
    ///
    /// Returns `Ok(())` on success; `Err` if the server doesn't exist, the
    /// caller is not the current owner, or the new owner is not a member.
    pub fn transfer_server_ownership(
        &self,
        server_id: i64,
        from_id: i64,
        to_id: i64,
    ) -> Result<(), rusqlite::Error> {
        let mut c = self.lock()?;
        let tx = c.transaction()?;

        // Guard 1: server exists and `from_id` is still the owner.
        let owner_ok: i64 = tx.query_row(
            "SELECT COUNT(*) FROM servers WHERE id = ?1 AND owner_id = ?2 AND deleted_at IS NULL",
            rusqlite::params![server_id, from_id],
            |r| r.get(0),
        )?;
        if owner_ok == 0 {
            tx.rollback()?;
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }

        // Guard 2: the new owner must already be a member of the server.
        let is_member: i64 = tx.query_row(
            "SELECT COUNT(*) FROM server_memberships WHERE server_id = ?1 AND user_id = ?2",
            rusqlite::params![server_id, to_id],
            |r| r.get(0),
        )?;
        if is_member == 0 {
            tx.rollback()?;
            return Err(sqlite_err(
                "new owner is not a member of this server",
            ));
        }

        // Demote old owner → admin.
        tx.execute(
            "UPDATE server_memberships SET role = 'admin' WHERE server_id = ?1 AND user_id = ?2 AND role = 'owner'",
            rusqlite::params![server_id, from_id],
        )?;

        // Promote new owner.
        tx.execute(
            "UPDATE server_memberships SET role = 'owner' WHERE server_id = ?1 AND user_id = ?2",
            rusqlite::params![server_id, to_id],
        )?;

        // Update the canonical owner_id column.
        tx.execute(
            "UPDATE servers SET owner_id = ?2 WHERE id = ?1 AND owner_id = ?3 AND deleted_at IS NULL",
            rusqlite::params![server_id, to_id, from_id],
        )?;

        tx.commit()?;
        Ok(())
    }
}
