//! Member management endpoints: add, kick, leave, ban, timeout, list.

use serde::Deserialize;
use server::{server, ServerError};

#[cfg(feature = "server")]
use crate::permissions::{self, Permissions};
#[cfg(feature = "server")]
use crate::roles::{Role};

/// The acting user for the current request, resolved from a verified session
/// token (see [`crate::session`]); `None` when unauthenticated.
#[cfg(feature = "server")]
use crate::session::acting_user;
#[cfg(feature = "server")]
use crate::server_settings::{ServerSettings, db_from_state};

#[cfg(feature = "server")]
impl ServerSettings {
    // ── member management ──────────────────────────────────────────────────

    /// Add a user to `server_id` as a plain member (roles are handed out
    /// with `server_roles`). Needs Create Invite.
    pub fn add_member(&self, caller_id: i64, server_id: i64, user_id: i64) -> Result<String, String> {
        let role = Role::Member;
        self.require(caller_id, server_id, Permissions::CREATE_INVITE)?;
        // A banned user can't be re-added even by an admin — the ban table is
        // authoritative over a direct add (an unban must come first).
        if self.db.is_banned(server_id, user_id) {
            return Err("member is banned and cannot be re-added (unban first)".into());
        }
        // The upsert would overwrite an existing member's row (the owner's
        // included), so an existing member is refused.
        if self.db.membership_role(server_id, user_id).is_some() {
            return Err("already a member".into());
        }
        self.db
            .upsert_membership(server_id, user_id, role, Self::now_secs())
            .map_err(|e| format!("failed to add member: {e}"))?;
        crate::membership::access_changed(&self.db, server_id, crate::membership::AccessChange::MemberJoined { user_id });
        Ok(role.db_str().to_string())
    }

    /// Remove (kick) a member from `server_id`. Needs Kick Members and a
    /// higher rank than the member, so the owner can never be kicked.
    ///
    /// # Errors
    /// - `server not found` — the server id doesn't exist.
    /// - `you need the Kick Members permission to do that`.
    /// - `you can't kick the server owner` / `you can only kick someone whose
    ///   highest role is below yours`.
    /// - `member not found` — `user_id` has no membership row.
    pub fn remove_member(&self, caller_id: i64, server_id: i64, user_id: i64) -> Result<(), String> {
        let (ctx, caller) = self.require(caller_id, server_id, Permissions::KICK_MEMBERS)?;
        Self::require_outranks(&ctx, &caller, user_id, "kick")?;
        let removed = self
            .db
            .remove_membership(server_id, user_id)
            .map_err(|e| format!("failed to remove member: {e}"))?;
        if removed == 0 {
            return Err("member not found".into());
        }
        // Tell the server's remaining members a member just left so their
        // live roster (and `MEMBERS — N`) updates without a reload.
        crate::membership::access_changed(&self.db, server_id, crate::membership::AccessChange::MemberRemoved { user_id });
        Ok(())
    }

    /// `caller_id` leaves `server_id`. Any member may leave except the owner,
    /// who must transfer ownership first (the server would be orphaned).
    ///
    /// # Errors
    /// - `server not found`, `not a member of this server`
    /// - `the owner can't leave (transfer ownership first)`
    pub fn leave(&self, caller_id: i64, server_id: i64) -> Result<(), String> {
        let (_sid, _name, owner_id, _icon_url) = self
            .db
            .server_by_id(server_id)
            .ok_or("server not found")?;
        if caller_id == owner_id {
            return Err("the owner can't leave (transfer ownership first)".into());
        }
        let removed = self
            .db
            .remove_membership(server_id, caller_id)
            .map_err(|e| format!("failed to leave: {e}"))?;
        if removed == 0 {
            return Err("not a member of this server".into());
        }
        crate::membership::access_changed(&self.db, server_id, crate::membership::AccessChange::MemberLeft { user_id: caller_id });
        Ok(())
    }

    /// Ban a member from `server_id`: remove them AND bar rejoining. Needs
    /// Ban Members and a higher rank than the member, so the owner can never
    /// be banned.
    ///
    /// The membership removal and the ban record happen atomically (a single
    /// DB transaction), so a failure can never leave the user kicked-but-not-
    /// banned. Rejoining is blocked in both join paths (`redeem_invite` and
    /// `add_member`) by consulting the ban table — see those functions.
    ///
    /// # Errors
    /// - `not found` — the server id doesn't exist.
    /// - `you need the … permission to do that` — see above.
    /// - `cannot ban the server owner` — `user_id` is the owner.
    /// - `member not found` — `user_id` has no membership row.
    pub fn ban_member(
        &self,
        caller_id: i64,
        server_id: i64,
        user_id: i64,
        reason: Option<&str>,
    ) -> Result<(), String> {
        let (ctx, caller) = self.require(caller_id, server_id, Permissions::BAN_MEMBERS)?;
        Self::require_outranks(&ctx, &caller, user_id, "ban")?;
        let removed = self
            .db
            .ban_and_remove(server_id, user_id, caller_id, reason, Self::now_secs())
            .map_err(|e| format!("failed to ban member: {e}"))?;
        if removed == 0 {
            return Err("member not found".into());
        }
        // A ban is a leave too: tell the remaining members the roster shrank.
        crate::membership::access_changed(&self.db, server_id, crate::membership::AccessChange::MemberBanned { user_id });
        Ok(())
    }

    /// Time out a member of `server_id`: silence them from posting messages
    /// for `duration_secs` without removing their membership (a kick/ban drop
    /// the row; a timeout only sets a deadline). Needs Timeout Members and a
    /// higher rank than the member. Re-timing overwrites the existing
    /// deadline in place (`db.set_timeout`), so a member can be extended.
    ///
    /// Enforcement is at the post boundary: [`crate::realtime::post_channel_message`]
    /// consults `db.is_timed_out` before persisting, comparing `timeout_until`
    /// against the current epoch.
    ///
    /// # Errors
    /// - `not found` — the server id doesn't exist.
    /// - `you need the … permission to do that` — see above.
    /// - `cannot time out the server owner` — `user_id` is the owner.
    /// - `member not found` — `user_id` has no membership row.
    pub fn timeout_member(
        &self,
        caller_id: i64,
        server_id: i64,
        user_id: i64,
        duration_secs: i64,
    ) -> Result<(), String> {
        let (ctx, caller) = self.require(caller_id, server_id, Permissions::MODERATE_MEMBERS)?;
        // A timeout of zero or less would be instantly expired — reject it at
        // the boundary rather than silently no-op.
        if duration_secs <= 0 {
            return Err("timeout duration must be positive".into());
        }
        Self::require_outranks(&ctx, &caller, user_id, "time out")?;
        let timeout_until = Self::now_secs() + duration_secs;
        let updated = self
            .db
            .set_timeout(server_id, user_id, timeout_until)
            .map_err(|e| format!("failed to time out member: {e}"))?;
        if updated == 0 {
            return Err("member not found".into());
        }
        Ok(())
    }

    /// All members of `server_id` with their role, ordered by role (owner
    /// first) then user id. Any member of the server may read the list.
    ///
    /// # Errors
    /// - `server not found` — the id doesn't exist.
    /// - `you're not a member of this server` — no membership.
    pub fn list_members(&self, caller_id: i64, server_id: i64) -> Result<Vec<MemberInfo>, String> {
        permissions::member_in(&self.db, server_id, caller_id)?;
        Ok(self
            .db
            .server_members(server_id)
            .into_iter()
            .map(|(user_id, username, role)| MemberInfo {
                user_id,
                display_name: self.db.user_display_name(user_id),
                username,
                role,
                avatar_url: self.db.user_avatar(user_id),
                presence: self.db.presence(user_id),
            })
            .collect())
    }
}

// ── member management endpoints ─────────────────────────────────────────────

/// Add a user to a server as a plain member. Needs Create Invite.
#[server]
pub async fn add_member(input: AddMemberInput) -> Result<MemberResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let role_str = ServerSettings::new(db)
        .add_member(caller, input.server_id, input.user_id)
        .map_err(ServerError::failed)?;
    Ok(MemberResult { server_id: input.server_id, user_id: input.user_id, role: role_str })
}

/// Remove (kick) a member from a server. Needs Kick Members and a higher
/// rank than the member.
#[server]
pub async fn remove_member(input: RemoveMemberInput) -> Result<MemberResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .remove_member(caller, input.server_id, input.user_id)
        .map_err(ServerError::failed)?;
    Ok(MemberResult { server_id: input.server_id, user_id: input.user_id, role: String::new() })
}

/// Leave a server. Any member except the owner may.
#[server]
pub async fn leave_server(input: LeaveServerInput) -> Result<(), ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .leave(caller, input.server_id)
        .map_err(ServerError::failed)
}

/// Ban a member from a server: remove them AND bar rejoining (invite
/// redemption or a direct add). Owner or admin may call (a ban clears a
/// higher bar than a kick). The server owner can't be banned. `reason` is an
/// optional free-text note attached to the ban record.
#[server]
pub async fn ban_member(input: BanMemberInput) -> Result<MemberResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .ban_member(caller, input.server_id, input.user_id, input.reason.as_deref())
        .map_err(ServerError::failed)?;
    Ok(MemberResult { server_id: input.server_id, user_id: input.user_id, role: String::new() })
}

/// Time out a member of a server: silence them from posting messages for
/// `duration_secs` without removing their membership (a kick/ban drop the row;
/// a timeout only sets a deadline they can post again after). Needs Timeout
/// Members and a higher rank than the member. A re-timeout overwrites the
/// existing deadline in place.
#[server]
pub async fn timeout_member(input: TimeoutMemberInput) -> Result<MemberResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .timeout_member(caller, input.server_id, input.user_id, input.duration_secs)
        .map_err(ServerError::failed)?;
    Ok(MemberResult { server_id: input.server_id, user_id: input.user_id, role: String::new() })
}

/// List the members of a server with their roles. Any member of the server
/// may call; the list is ordered by role (owner first), then user id.
#[server]
pub async fn list_members(input: ListMembersInput) -> Result<Vec<MemberInfo>, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .list_members(caller, input.server_id)
        .map_err(ServerError::failed)
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct AddMemberInput {
    pub server_id: i64,
    pub user_id: i64,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct LeaveServerInput {
    pub server_id: i64,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct RemoveMemberInput {
    pub server_id: i64,
    pub user_id: i64,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct BanMemberInput {
    pub server_id: i64,
    pub user_id: i64,
    /// Optional free-text note stored on the ban record.
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct TimeoutMemberInput {
    pub server_id: i64,
    pub user_id: i64,
    /// How long to silence the member for, in seconds from the moment the
    /// timeout is applied. Must be positive; the server rejects non-positive
    /// values at the service boundary.
    pub duration_secs: i64,
}

/// The server whose member list the caller wants.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct ListMembersInput {
    pub server_id: i64,
}

/// One member of a server: their id, display name, stored role string
/// (`'owner'` / `'admin'` / `'moderator'` / `'member'`), and avatar URL
/// (a `data:image/…;base64,` string, `None` = default initial circle).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct MemberInfo {
    pub user_id: i64,
    pub username: String,
    /// Shown in place of the username when set.
    #[serde(default)]
    pub display_name: Option<String>,
    pub role: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    /// What others currently see: 'online' / 'away' / 'dnd' / 'offline'. The
    /// member panel seeds a row's dot from this so a client that connected
    /// after the member came online (or reloaded) still shows them online;
    /// `Presence` frames then override it live.
    #[serde(default)]
    pub presence: Option<String>,
}

/// The result of a member-management operation. `role` is the member's role
/// after the op (empty string when the member was removed).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct MemberResult {
    pub server_id: i64,
    pub user_id: i64,
    pub role: String,
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use crate::server_settings::test_helpers::{setup_db, setup_db_with_channels};
    use crate::db::Db;

    #[test]
    fn a_member_can_leave_but_the_owner_cannot() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db.clone());
        svc.leave(2, 1).unwrap();
        assert_eq!(db.membership_role(1, 2), None);
        assert_eq!(svc.leave(2, 1).unwrap_err(), "not a member of this server");
        assert_eq!(svc.leave(1, 1).unwrap_err(), "the owner can't leave (transfer ownership first)");
        assert_eq!(db.membership_role(1, 1).as_deref(), Some("owner"));
        assert_eq!(svc.leave(3, 99).unwrap_err(), "server not found");
    }

    // ── member list ──────────────────────────────────────────────────────────

    /// A member can read the list; owner comes first, then admin/mod/member,
    /// then user id within a role.
    #[test]
    fn member_list_ordered_by_role_then_user_id() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        // Seed a moderator and another member with a higher user id than bob.
        let c = db.lock().unwrap();
        c.execute_batch(
            "INSERT INTO users (id, username, email, created_at) VALUES (4, 'dave', 'dave@example.com', 1000);
             UPDATE server_memberships SET role = 'moderator' WHERE server_id = 1 AND user_id = 3;
             INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES (1, 4, 'member', 1000);",
        )
        .unwrap();
        drop(c);

        let members = svc.list_members(2, 1).unwrap();
        let got: Vec<(i64, &str)> = members
            .iter()
            .map(|m| (m.user_id, m.role.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![(1, "owner"), (3, "moderator"), (2, "member"), (4, "member")],
            "owner first, then moderator, then members by user id"
        );
        // The row for alice carries her username, not an empty placeholder.
        let alice = members.iter().find(|m| m.user_id == 2).unwrap();
        assert_eq!(alice.username, "alice");
    }

    /// The roster carries each member's stored presence so a client that
    /// connected after they came online still dots them online; members
    /// never seen by the gateway read as offline (the DB default).
    #[test]
    fn member_list_carries_presence() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        db.lock().unwrap().execute(
            "UPDATE users SET presence_status = 'online' WHERE id = 2",
            (),
        ).unwrap();

        let members = svc.list_members(2, 1).unwrap();
        let by_id = members.iter().find(|m| m.user_id == 2).unwrap();
        assert_eq!(by_id.presence, Some("online".to_string()));
        let other = members.iter().find(|m| m.user_id == 3).unwrap();
        assert_eq!(other.presence, Some("offline".to_string()));
    }

    #[test]
    fn member_list_rejects_unknown_server() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db);
        let err = svc.list_members(1, 999).unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
    }

    #[test]
    fn member_list_rejects_non_member() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db);
        // User 999 has no membership in server 1.
        let err = svc.list_members(999, 1).unwrap_err();
        assert!(err.contains("not a member"), "got: {err}");
    }


    // ── member management: ban ───────────────────────────────────────────────
    //
    // A ban removes a member AND bars rejoining (Ban Members, while a kick
    // needs Kick Members), and the rejoin block is asserted through
    // `add_member` (the other rejoin path, `redeem_invite`, is covered by its
    // own e2e test in `invites`).

    /// Give `user_id` a role with the old admin level's powers in server 1.
    fn promote_user_to_admin(db: &Db, user_id: i64) {
        let _ = crate::permissions::grant(db, 1, user_id, crate::permissions::ADMIN_LEVEL, 2);
    }

    #[test]
    fn owner_can_ban_member() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db.clone());
        // Owner (1) bans alice (2).
        svc.ban_member(1, 1, 2, Some("spam")).unwrap();
        // The membership row is gone (they're kicked out)…
        assert_eq!(db.membership_role(1, 2), None, "banned member is removed");
        // …and a ban row is recorded (rejoining is barred).
        assert!(db.is_banned(1, 2), "a ban row must be recorded");
    }

    #[test]
    fn admin_can_ban_member() {
        let (_dir, db) = setup_db();
        promote_user_to_admin(&db, 3);
        let svc = ServerSettings::new(db.clone());
        // An admin (3) may ban a fellow member (2).
        svc.ban_member(3, 1, 2, None).unwrap();
        assert!(db.is_banned(1, 2));
    }

    #[test]
    fn moderator_cannot_ban() {
        let (_dir, db) = setup_db();
        // Give bob (3) the moderator powers — a kick is allowed, a ban is not.
        let _ = crate::permissions::grant(&db, 1, 3, crate::permissions::MODERATOR_LEVEL, 1);
        let svc = ServerSettings::new(db.clone());
        let err = svc.ban_member(3, 1, 2, None).unwrap_err();
        assert_eq!(err, "you need the Ban Members permission to do that");
        // alice is untouched.
        assert_eq!(db.membership_role(1, 2), Some("member".into()));
        assert!(!db.is_banned(1, 2));
    }

    #[test]
    fn cannot_ban_the_owner() {
        let (_dir, db) = setup_db();
        // Give user 3 the admin role so a ban would otherwise be permitted,
        // then have them try to ban the owner (1): refused.
        promote_user_to_admin(&db, 3);
        let svc = ServerSettings::new(db);
        let err = svc.ban_member(3, 1, 1, None).unwrap_err();
        assert_eq!(err, "you can't ban the server owner");
    }

    #[test]
    fn banning_a_non_member_is_rejected() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db.clone());
        // User 999 has no membership row: a ban has nothing to remove, so it's
        // a no-op rejection rather than a phantom ban row.
        let err = svc.ban_member(1, 1, 999, None).unwrap_err();
        assert!(err.contains("member not found"), "got: {err}");
        assert!(!db.is_banned(1, 999), "no ban row is written for a non-member");
    }

    #[test]
    fn banned_member_cannot_be_readded() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db.clone());
        // Ban alice (2), then the owner tries to re-add her directly.
        svc.ban_member(1, 1, 2, Some("spam")).unwrap();
        let err = svc.add_member(1, 1, 2).unwrap_err();
        assert!(err.contains("banned"), "the ban blocks a direct re-add; got: {err}");
        // Still not a member.
        assert_eq!(db.membership_role(1, 2), None);
    }

    #[test]
    fn unban_restores_rejoin() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db.clone());
        svc.ban_member(1, 1, 2, None).unwrap();
        assert!(db.is_banned(1, 2));
        // Unban, then the owner may re-add.
        db.unban(1, 2).unwrap();
        assert!(!db.is_banned(1, 2));
        let role = svc.add_member(1, 1, 2).unwrap();
        assert_eq!(role, "member");
        assert_eq!(db.membership_role(1, 2), Some("member".into()));
    }
}
