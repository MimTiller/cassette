//! Friendships, friend requests and user blocks.

use super::Db;
use super::now_epoch_ms;

impl Db {
    // ── friendships ────────────────────────────────────────────────────────
    //
    // The friends list (`friendships`, migration `0016_friendships.sql`).
    // One row per unordered user pair: `requester_id`/`addressee_id` are
    // directional (who asked whom) while the pair itself is unique via the
    // normalised `min_user_id` < `max_user_id` columns (SQLite has no
    // expression UNIQUE index). `status` is 'pending' (request awaiting a
    // reply) or 'accepted' (friends). All mutations here are single-row
    // updates on the pair's unique row — no new rows are ever created for an
    // existing pair, so the UNIQUE index is the duplicate-request guard.

    /// The row for the unordered pair (`None` when the two have no
    /// relationship at all). `(id, requester_id, addressee_id, status)`.
    pub fn friendship_between(&self, user_a: i64, user_b: i64) -> Option<(i64, i64, i64, String)> {
        let lo = std::cmp::min(user_a, user_b);
        let hi = std::cmp::max(user_a, user_b);
        let c = self.lock().ok()?;
        c.query_row(
            "SELECT id, requester_id, addressee_id, status FROM friendships
             WHERE min_user_id = ?1 AND max_user_id = ?2",
            rusqlite::params![lo, hi],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .ok()
    }

    /// Insert a fresh 'pending' request from `requester` to `addressee`
    /// (distinct ids, min/max precomputed by the caller). `created_at` is UTC
    /// epoch milliseconds. Returns the new row id. The UNIQUE pair index
    /// rejects a second row for the same pair (duplicate-request guard).
    pub fn insert_friend_request(
        &self,
        requester: i64,
        addressee: i64,
        created_at: i64,
    ) -> Result<i64, rusqlite::Error> {
        let lo = std::cmp::min(requester, addressee);
        let hi = std::cmp::max(requester, addressee);
        let c = self.lock()?;
        c.execute(
            "INSERT INTO friendships (requester_id, addressee_id, min_user_id, max_user_id, status, created_at)
             VALUES (?1, ?2, ?3, ?4, 'pending', ?5)",
            rusqlite::params![requester, addressee, lo, hi, created_at],
        )?;
        Ok(c.last_insert_rowid())
    }

    /// Flip the pair's row to 'accepted'. `None` when the pair has no row.
    pub fn accept_friend_request(&self, row_id: i64) -> Result<(), rusqlite::Error> {
        let c = self.lock()?;
        c.execute(
            "UPDATE friendships SET status = 'accepted' WHERE id = ?1",
            rusqlite::params![row_id],
        )?;
        Ok(())
    }

    /// Drop the pair's row (decline / cancel / remove-friend). `None` when
    /// the pair has no row.
    pub fn delete_friendship(&self, row_id: i64) -> Result<(), rusqlite::Error> {
        let c = self.lock()?;
        c.execute(
            "DELETE FROM friendships WHERE id = ?1",
            rusqlite::params![row_id],
        )?;
        Ok(())
    }

    /// The acting user's friendships, split by their side:
    /// `(their_row_id, other_user_id, status)` — incoming (they were
    /// asked), outgoing (they asked), accepted (friends). Each other user
    /// appears at most once (the pair is unique). Newest-first in each group.
    pub fn friendships_for_user(&self, user_id: i64) -> (Vec<(i64, i64, String)>, Vec<(i64, i64, String)>, Vec<(i64, i64, String)>) {
        let c = match self.lock() {
            Ok(c) => c,
            Err(_) => return (Vec::new(), Vec::new(), Vec::new()),
        };
        let mut stmt = match c.prepare(
            "SELECT id, requester_id, addressee_id, status FROM friendships
             WHERE min_user_id = ?1 OR max_user_id = ?1 ORDER BY created_at DESC, id DESC",
        ) {
            Ok(s) => s,
            Err(_) => return (Vec::new(), Vec::new(), Vec::new()),
        };
        // NOTE: `?1` appears twice on purpose (both sides of the pair) —
        // bind ONE param. rusqlite binds positionally, so a second param
        // here is `bind(2, …)` on a 1-placeholder statement and errors.
        let rows = match stmt.query_map(rusqlite::params![user_id], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?))
        }) {
            Ok(rows) => rows,
            Err(_) => return (Vec::new(), Vec::new(), Vec::new()),
        };
        let mut incoming = Vec::new();
        let mut outgoing = Vec::new();
        let mut accepted = Vec::new();
        for row in rows.filter_map(Result::ok) {
            let (id, requester, addressee, status) = row;
            if status == "accepted" {
                // Direction is irrelevant for friends: the "other" side is
                // whichever of the two is not the acting user.
                accepted.push((id, if requester == user_id { addressee } else { requester }, status));
            } else if requester == user_id {
                outgoing.push((id, addressee, status));
            } else if addressee == user_id {
                incoming.push((id, requester, status));
            }
        }
        (incoming, outgoing, accepted)
    }

    /// Delete the pair's friendship row (either direction, any status).
    /// Used by block cleanup: blocking removes any existing link between the
    /// two. Returns true when a row was deleted.
    pub fn delete_friendship_between(&self, a: i64, b: i64) -> bool {
        let Some((row_id, _, _, _)) = self.friendship_between(a, b) else {
            return false;
        };
        let c = self.lock().ok().expect("lock");
        let n = c
            .execute(
                "DELETE FROM friendships WHERE id = ?1",
                rusqlite::params![row_id],
            )
            .unwrap_or(0);
        n > 0
    }

    // ── user blocks ────────────────────────────────────────────────────────
    //
    // `user_blocks` (migration `0017_user_blocks.sql`): one row per DIRECTED
    // (blocker_id, blocked_id) pair, composite PK on both. Blocking is
    // unilateral — a pair is "blocked" when a row exists in EITHER direction,
    // so app-layer checks go through `blocks_pair` (two-way `EXISTS`). No
    // status lifecycle: a block is presence/absence, removed by deleting the
    // row. `created_at` is UTC epoch milliseconds, like the other tables.

    /// Insert a directed block row. Idempotent — `INSERT OR IGNORE` against
    /// the composite PK, so a second block by the same blocker is a no-op.
    pub fn insert_user_block(&self, blocker_id: i64, blocked_id: i64) {
        let c = self.lock().ok().expect("lock");
        let _ = c.execute(
            "INSERT OR IGNORE INTO user_blocks (blocker_id, blocked_id, created_at)
             VALUES (?1, ?2, ?3)",
            rusqlite::params![blocker_id, blocked_id, now_epoch_ms()],
        );
    }

    /// Delete the directed block row, if any (a reverse block is a separate
    /// row and survives — unblock only removes the blocker's own row).
    pub fn delete_user_block(&self, blocker_id: i64, blocked_id: i64) {
        let c = self.lock().ok().expect("lock");
        let _ = c.execute(
            "DELETE FROM user_blocks WHERE blocker_id = ?1 AND blocked_id = ?2",
            rusqlite::params![blocker_id, blocked_id],
        );
    }

    /// True when a block exists in EITHER direction between `a` and `b`. The
    /// pair is blocked regardless of who initiated it; the app layer enforces
    /// this symmetrically (friend requests and DM starts fail both ways).
    pub fn blocks_pair(&self, a: i64, b: i64) -> bool {
        let c = self.lock().ok().expect("lock");
        c.query_row(
            "SELECT EXISTS (
                 SELECT 1 FROM user_blocks WHERE blocker_id = ?1 AND blocked_id = ?2
                 UNION
                 SELECT 1 FROM user_blocks WHERE blocker_id = ?2 AND blocked_id = ?1
             )",
            rusqlite::params![a, b],
            |r| r.get::<_, bool>(0),
        )
        .unwrap_or(false)
    }

    /// The user ids the acting user has blocked (rows they initiated). Feeds
    /// the `LoginResult` startup payload so the client can collapse channel
    /// messages from blocked users (the server does not filter channel
    /// messages itself).
    pub fn blocked_ids_for_user(&self, user_id: i64) -> Vec<i64> {
        let c = self.lock().ok().expect("lock");
        let mut stmt = c
            .prepare("SELECT blocked_id FROM user_blocks WHERE blocker_id = ?1 ORDER BY blocked_id")
            .expect("prepare blocked ids");
        let rows = stmt.query_map(rusqlite::params![user_id], |r| r.get::<_, i64>(0));
        match rows {
            Ok(rows) => rows.filter_map(Result::ok).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// The full blocked-user roster for a user: the ids they blocked, joined
    /// with username + avatar for the listing surface.
    pub fn blocked_users_for_user(&self, user_id: i64) -> Vec<(i64, String, Option<String>)> {
        let c = self.lock().ok().expect("lock");
        let mut stmt = c
            .prepare(
                "SELECT ub.blocked_id, u.username, u.avatar_url
                 FROM user_blocks ub JOIN users u ON u.id = ub.blocked_id
                 WHERE ub.blocker_id = ?1 ORDER BY ub.blocked_id",
            )
            .expect("prepare blocked users");
        let rows = stmt.query_map(rusqlite::params![user_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        });
        match rows {
            Ok(rows) => rows.filter_map(Result::ok).collect(),
            Err(_) => Vec::new(),
        }
    }
}
