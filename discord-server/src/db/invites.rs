//! Invite codes: create, look up, redeem, revoke.

use super::Db;
use super::InviteRow;

impl Db {
    // ── invites ────────────────────────────────────────────────────────────

    /// Insert a new invite and return its full row. The `code` is generated
    /// by the caller (the service layer); a duplicate `code` trips the global
    /// `UNIQUE` constraint, which the caller maps to a retry.
    pub fn insert_invite(
        &self,
        server_id: i64,
        code: &str,
        created_by: i64,
        created_at: i64,
        expires_at: Option<i64>,
        max_uses: Option<i64>,
    ) -> Result<InviteRow, rusqlite::Error> {
        let c = self.lock()?;
        c.execute(
            "INSERT INTO server_invites
                 (server_id, code, created_by, created_at, expires_at, max_uses, uses)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
            rusqlite::params![server_id, code, created_by, created_at, expires_at, max_uses],
        )?;
        let id = c.last_insert_rowid();
        Ok(Self::read_invite_row(c.query_row(
            "SELECT id, server_id, code, created_by, created_at, expires_at, max_uses, uses
             FROM server_invites WHERE id = ?1",
            rusqlite::params![id],
            Self::map_invite,
        )?))
    }

    /// Look up an invite by its (globally unique) code.
    pub fn invite_by_code(&self, code: &str) -> Option<InviteRow> {
        let c = self.lock().ok()?;
        let row = c
            .query_row(
                "SELECT id, server_id, code, created_by, created_at, expires_at, max_uses, uses
                 FROM server_invites WHERE code = ?1",
                rusqlite::params![code],
                Self::map_invite,
            )
            .ok()?;
        Some(Self::read_invite_row(row))
    }

    /// All live invites for a server, ordered by creation. (An "expired"
    /// invite is still a row; callers filter by `expires_at` if they want
    /// only the still-valid ones.)
    pub fn invites_for_server(&self, server_id: i64) -> Vec<InviteRow> {
        let c = match self.lock() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut stmt = match c.prepare(
            "SELECT id, server_id, code, created_by, created_at, expires_at, max_uses, uses
             FROM server_invites WHERE server_id = ?1 ORDER BY created_at, id",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map(rusqlite::params![server_id], Self::map_invite) {
            Ok(rows) => rows,
            Err(_) => return Vec::new(),
        };
        rows.filter_map(|r| r.ok()).map(Self::read_invite_row).collect()
    }

    /// Atomically consume one use of an invite. Increments `uses` only if the
    /// invite still has budget (i.e. it is unlimited or `uses < max_uses`),
    /// so concurrent redemptions can't overspend a capped code. Returns the
    /// new `uses` count on success, `0` if the code was already spent.
    pub fn redeem_invite_use(&self, invite_id: i64) -> Result<i64, rusqlite::Error> {
        let c = self.lock()?;
        let updated = c
            .execute(
                "UPDATE server_invites
                 SET uses = uses + 1
                 WHERE id = ?1 AND (max_uses IS NULL OR uses < max_uses)",
                rusqlite::params![invite_id],
            )? as i64;
        if updated == 0 {
            // No row was updated: the code was already spent (or missing).
            return Ok(0);
        }
        // Return the post-increment count for the caller to report.
        Ok(c.query_row(
            "SELECT uses FROM server_invites WHERE id = ?1",
            rusqlite::params![invite_id],
            |r| r.get(0),
        )?)
    }

    /// Revoke (delete) an invite for a server. Returns the number of rows
    /// deleted (0 if the code did not exist on that server).
    pub fn revoke_invite(&self, server_id: i64, code: &str) -> Result<usize, rusqlite::Error> {
        let c = self.lock()?;
        Ok(c.execute(
            "DELETE FROM server_invites WHERE server_id = ?1 AND code = ?2",
            rusqlite::params![server_id, code],
        )?)
    }

    fn map_invite(
        row: &rusqlite::Row<'_>,
    ) -> Result<(i64, i64, String, i64, i64, Option<i64>, Option<i64>, i64), rusqlite::Error> {
        Ok((
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
            row.get(6)?,
            row.get(7)?,
        ))
    }

    fn read_invite_row(t: (i64, i64, String, i64, i64, Option<i64>, Option<i64>, i64)) -> InviteRow {
        InviteRow {
            id: t.0,
            server_id: t.1,
            code: t.2,
            created_by: t.3,
            created_at: t.4,
            expires_at: t.5,
            max_uses: t.6,
            uses: t.7,
        }
    }
}
