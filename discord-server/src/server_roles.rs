//! Named server roles and the mention grammar that covers them.
//!
//! ## Model
//!
//! A server has any number of named roles (`0020_custom_roles.sql`): a name,
//! an optional `#rrggbb` colour, a `mentionable` flag and a position (higher
//! is listed first and wins the name colour). Members hold any set of them.
//! A role also carries a permission set (see [`crate::permissions`]);
//! members with Manage Roles manage the roles ranked below their own.
//!
//! ## Mentions
//!
//! [`segments`] splits message text into plain text and mentions: `@everyone`,
//! `@here`, `@<role name>` (role names may contain spaces, matched
//! case-insensitively, longest first) and `@<username>`. The client renders
//! the segments; the server's [`resolve_mentions`] turns the same text into
//! the user ids to notify: `@everyone` is every member, `@here` the members
//! not offline, a role its holders. `@everyone`, `@here` and roles that are
//! not mentionable only notify when the author is a moderator or above.

use serde::{Deserialize, Serialize};
use server::{server, ServerError};

#[cfg(feature = "server")]
use crate::db::Db;
#[cfg(feature = "server")]
use crate::permissions::{self, Member, Permissions, ServerContext};
#[cfg(feature = "server")]
use crate::session::acting_user;

/// Most roles one server may have.
pub const MAX_ROLES: usize = 50;
/// Longest role name, in characters.
pub const MAX_NAME_CHARS: usize = 32;

/// One named role.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomRole {
    pub id: i64,
    pub server_id: i64,
    pub name: String,
    /// `#rrggbb`, or empty for no colour.
    pub color: String,
    pub mentionable: bool,
    pub position: i64,
    /// What holding the role allows (see [`crate::permissions`]).
    #[serde(default)]
    pub permissions: crate::permissions::Permissions,
}

/// A server's roles (highest position first) and who holds them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RolesSnapshot {
    pub server_id: i64,
    pub roles: Vec<CustomRole>,
    /// `(user_id, role_id)` pairs.
    pub assignments: Vec<(i64, i64)>,
    /// What @everyone may do server-wide.
    #[serde(default)]
    pub everyone_permissions: crate::permissions::Permissions,
}

impl RolesSnapshot {
    /// The roles `user_id` holds, highest position first.
    pub fn roles_of(&self, user_id: i64) -> Vec<&CustomRole> {
        self.roles
            .iter()
            .filter(|r| self.assignments.contains(&(user_id, r.id)))
            .collect()
    }

    /// The colour of `user_id`'s highest coloured role.
    pub fn color_of(&self, user_id: i64) -> Option<&str> {
        self.roles_of(user_id)
            .into_iter()
            .find(|r| !r.color.is_empty())
            .map(|r| r.color.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerRolesInput {
    pub server_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoleInput {
    pub server_id: i64,
    pub name: String,
    pub color: String,
    pub mentionable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRoleInput {
    pub role_id: i64,
    pub name: String,
    pub color: String,
    pub mentionable: bool,
    /// The role's new permissions; `None` leaves them as they are.
    #[serde(default)]
    pub permissions: Option<crate::permissions::Permissions>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveRoleInput {
    pub role_id: i64,
    /// The new position; higher ranks above.
    pub position: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteRoleInput {
    pub role_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignRoleInput {
    pub role_id: i64,
    pub user_id: i64,
    /// `true` gives the role, `false` takes it away.
    pub assigned: bool,
}

// ── mention grammar (shared by client and server) ───────────────────────────

/// A piece of message text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MentionSeg {
    Text(String),
    /// `@everyone` or `@here` (the text without the `@`).
    Everyone(String),
    Role { id: i64, name: String, color: String },
    User { id: i64, name: String },
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The byte length of `name` at the start of `rest` (case-insensitive) when it
/// is followed by a non-word character or the end, else `None`.
fn match_name(rest: &str, name: &str) -> Option<usize> {
    let mut rest_chars = rest.char_indices();
    for n in name.chars() {
        let (_, r) = rest_chars.next()?;
        if !r.to_lowercase().eq(n.to_lowercase()) {
            return None;
        }
    }
    let end = rest_chars.next().map(|(i, _)| i).unwrap_or(rest.len());
    match rest[end..].chars().next() {
        Some(c) if is_word(c) => None,
        _ => Some(end),
    }
}

/// Split `text` into plain runs and mentions. `users` are the server's
/// `(user_id, username)`; an `@` only starts a mention at the start of the
/// text or after a non-word character (so an e-mail address stays text).
pub fn segments(text: &str, users: &[(i64, String)], roles: &[CustomRole]) -> Vec<MentionSeg> {
    let mut by_len: Vec<&CustomRole> = roles.iter().filter(|r| !r.name.is_empty()).collect();
    by_len.sort_by_key(|r| std::cmp::Reverse(r.name.chars().count()));
    let mut out = Vec::new();
    let mut plain = String::new();
    let mut i = 0;
    let mut prev: Option<char> = None;
    while i < text.len() {
        let c = text[i..].chars().next().unwrap();
        if c == '@' && !prev.map(is_word).unwrap_or(false) {
            let rest = &text[i + 1..];
            let found: Option<(MentionSeg, usize)> = by_len
                .iter()
                .find_map(|r| {
                    match_name(rest, &r.name)
                        .map(|n| (MentionSeg::Role { id: r.id, name: r.name.clone(), color: r.color.clone() }, n))
                })
                .or_else(|| {
                    ["everyone", "here"].iter().find_map(|w| {
                        (rest.starts_with(w) && !rest[w.len()..].chars().next().map(is_word).unwrap_or(false))
                            .then(|| (MentionSeg::Everyone(w.to_string()), w.len()))
                    })
                })
                .or_else(|| {
                    let n = rest.find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_')).unwrap_or(rest.len());
                    let word = &rest[..n];
                    users
                        .iter()
                        .find(|(_, name)| !word.is_empty() && name.eq_ignore_ascii_case(word))
                        .map(|(id, name)| (MentionSeg::User { id: *id, name: name.clone() }, n))
                });
            if let Some((seg, n)) = found {
                if !plain.is_empty() {
                    out.push(MentionSeg::Text(std::mem::take(&mut plain)));
                }
                out.push(seg);
                i += 1 + n;
                prev = text[..i].chars().next_back();
                continue;
            }
        }
        plain.push(c);
        i += c.len_utf8();
        prev = Some(c);
    }
    if !plain.is_empty() {
        out.push(MentionSeg::Text(plain));
    }
    out
}

/// Check a role name: trimmed, 1–32 characters, no `@` or control
/// characters, and not `everyone` / `here`.
pub fn validate_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    let n = name.chars().count();
    if n == 0 {
        return Err("role name can't be empty".into());
    }
    if n > MAX_NAME_CHARS {
        return Err(format!("role name too long (max {MAX_NAME_CHARS} characters)"));
    }
    if name.contains('@') || name.chars().any(char::is_control) {
        return Err("role names can't contain @ or line breaks".into());
    }
    if name.eq_ignore_ascii_case("everyone") || name.eq_ignore_ascii_case("here") {
        return Err("that name is reserved".into());
    }
    Ok(name.to_string())
}

/// Check a colour: empty (no colour) or `#rrggbb`, returned lower-case.
pub fn validate_color(color: &str) -> Result<String, String> {
    let c = color.trim();
    if c.is_empty() {
        return Ok(String::new());
    }
    let hex = c.strip_prefix('#').unwrap_or("");
    if hex.len() == 6 && hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Ok(format!("#{}", hex.to_ascii_lowercase()))
    } else {
        Err("colour must look like #a1b2c3".into())
    }
}

// ── service ─────────────────────────────────────────────────────────────────

/// Role management over the DB; `caller` is the acting user.
#[cfg(feature = "server")]
pub struct RoleService {
    db: Db,
}

#[cfg(feature = "server")]
impl RoleService {
    pub fn new(db: Db) -> Self {
        RoleService { db }
    }

    /// The caller, who must hold Manage Roles in `server_id`.
    fn require_manage(&self, caller: i64, server_id: i64) -> Result<(ServerContext, Member), String> {
        let (ctx, member) = permissions::member_in(&self.db, server_id, caller)?;
        if !member.server.contains(Permissions::MANAGE_ROLES) {
            return Err(permissions::missing(Permissions::MANAGE_ROLES));
        }
        Ok((ctx, member))
    }

    /// The caller, who must hold Manage Roles and rank above `role`.
    fn require_manage_role(&self, caller: i64, role: &CustomRole) -> Result<(ServerContext, Member), String> {
        let (ctx, member) = self.require_manage(caller, role.server_id)?;
        if !member.rank.outranks(permissions::Rank::Role(role.position)) {
            return Err("you can only manage roles below your highest role".into());
        }
        Ok((ctx, member))
    }

    /// The server's roles and assignments (any member may read them).
    pub fn snapshot(&self, caller: i64, server_id: i64) -> Result<RolesSnapshot, String> {
        permissions::member_in(&self.db, server_id, caller)?;
        Ok(load_snapshot(&self.db, server_id))
    }

    pub fn create(&self, caller: i64, server_id: i64, name: &str, color: &str, mentionable: bool) -> Result<CustomRole, String> {
        self.require_manage(caller, server_id)?;
        let name = validate_name(name)?;
        let color = validate_color(color)?;
        let snap = load_snapshot(&self.db, server_id);
        if snap.roles.len() >= MAX_ROLES {
            return Err(format!("a server can have at most {MAX_ROLES} roles"));
        }
        if snap.roles.iter().any(|r| r.name.eq_ignore_ascii_case(&name)) {
            return Err("a role with that name already exists".into());
        }
        // New roles start at the bottom, below every role (and so below the
        // creator's), where the creator can manage them.
        let position = snap.roles.iter().map(|r| r.position).min().map_or(0, |p| p - 1);
        let c = self.db.lock().map_err(|e| e.to_string())?;
        c.execute(
            "INSERT INTO server_roles (server_id, name, color, mentionable, position, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![server_id, name, color, mentionable, position, now_secs()],
        )
        .map_err(|e| format!("failed to create role: {e}"))?;
        let id = c.last_insert_rowid();
        Ok(CustomRole { id, server_id, name, color, mentionable, position, permissions: Permissions::NONE })
    }

    pub fn update(
        &self,
        caller: i64,
        role_id: i64,
        name: &str,
        color: &str,
        mentionable: bool,
        perms: Option<Permissions>,
    ) -> Result<CustomRole, String> {
        let role = self.role(role_id).ok_or("role not found")?;
        let (_, member) = self.require_manage_role(caller, &role)?;
        let perms = perms.map(Permissions::known).unwrap_or(role.permissions);
        refuse_unheld_change(&member, role.permissions, perms)?;
        let name = validate_name(name)?;
        let color = validate_color(color)?;
        let snap = load_snapshot(&self.db, role.server_id);
        if snap.roles.iter().any(|r| r.id != role_id && r.name.eq_ignore_ascii_case(&name)) {
            return Err("a role with that name already exists".into());
        }
        let c = self.db.lock().map_err(|e| e.to_string())?;
        c.execute(
            "UPDATE server_roles SET name = ?2, color = ?3, mentionable = ?4, permissions = ?5 WHERE id = ?1",
            rusqlite::params![role_id, name, color, mentionable, perms.0 as i64],
        )
        .map_err(|e| format!("failed to update role: {e}"))?;
        Ok(CustomRole { name, color, mentionable, permissions: perms, ..role })
    }

    /// Move a role to `position`. The caller must outrank the role both where
    /// it is and where it goes.
    pub fn move_to(&self, caller: i64, role_id: i64, position: i64) -> Result<CustomRole, String> {
        let role = self.role(role_id).ok_or("role not found")?;
        let (_, member) = self.require_manage_role(caller, &role)?;
        if !member.rank.outranks(permissions::Rank::Role(position)) {
            return Err("you can't move a role up to or above your highest role".into());
        }
        let c = self.db.lock().map_err(|e| e.to_string())?;
        c.execute("UPDATE server_roles SET position = ?2 WHERE id = ?1", rusqlite::params![role_id, position])
            .map_err(|e| format!("failed to move role: {e}"))?;
        Ok(CustomRole { position, ..role })
    }

    /// Delete a role; returns its server id.
    pub fn delete(&self, caller: i64, role_id: i64) -> Result<i64, String> {
        let role = self.role(role_id).ok_or("role not found")?;
        self.require_manage_role(caller, &role)?;
        let c = self.db.lock().map_err(|e| e.to_string())?;
        c.execute("DELETE FROM member_roles WHERE role_id = ?1", rusqlite::params![role_id])
            .and_then(|_| c.execute("DELETE FROM server_roles WHERE id = ?1", rusqlite::params![role_id]))
            .map_err(|e| format!("failed to delete role: {e}"))?;
        Ok(role.server_id)
    }

    /// Give `user_id` the role (or take it away); returns the server id.
    pub fn assign(&self, caller: i64, role_id: i64, user_id: i64, assigned: bool) -> Result<i64, String> {
        let role = self.role(role_id).ok_or("role not found")?;
        let (ctx, _) = self.require_manage_role(caller, &role)?;
        if ctx.member(user_id).is_none() {
            return Err("member not found".into());
        }
        let c = self.db.lock().map_err(|e| e.to_string())?;
        let sql = if assigned {
            "INSERT OR IGNORE INTO member_roles (role_id, user_id) VALUES (?1, ?2)"
        } else {
            "DELETE FROM member_roles WHERE role_id = ?1 AND user_id = ?2"
        };
        c.execute(sql, rusqlite::params![role_id, user_id])
            .map_err(|e| format!("failed to update the member's roles: {e}"))?;
        Ok(role.server_id)
    }

    fn role(&self, role_id: i64) -> Option<CustomRole> {
        let c = self.db.lock().ok()?;
        c.query_row(
            "SELECT id, server_id, name, color, mentionable, position, permissions FROM server_roles WHERE id = ?1",
            rusqlite::params![role_id],
            row_to_role,
        )
        .ok()
    }
}

#[cfg(feature = "server")]
fn row_to_role(r: &rusqlite::Row<'_>) -> rusqlite::Result<CustomRole> {
    Ok(CustomRole {
        id: r.get(0)?,
        server_id: r.get(1)?,
        name: r.get(2)?,
        color: r.get(3)?,
        mentionable: r.get(4)?,
        position: r.get(5)?,
        permissions: Permissions(r.get::<_, i64>(6)? as u64),
    })
}

/// Refuse changing bits of a permission set that `member` doesn't hold
/// themself (the owner and administrators hold every bit).
#[cfg(feature = "server")]
pub fn refuse_unheld_change(member: &Member, old: Permissions, new: Permissions) -> Result<(), String> {
    let changed = Permissions(old.0 ^ new.0);
    let unheld = changed.minus(member.server);
    match unheld.names().first() {
        None => Ok(()),
        Some(name) => Err(format!("you can't grant or remove {name}, because you don't have it yourself")),
    }
}

/// A server's roles (highest position first) and their current holders.
#[cfg(feature = "server")]
pub fn load_snapshot(db: &Db, server_id: i64) -> RolesSnapshot {
    let mut snap = RolesSnapshot { server_id, ..Default::default() };
    let Ok(c) = db.lock() else {
        return snap;
    };
    if let Ok(v) = c.query_row(
        "SELECT everyone_permissions FROM servers WHERE id = ?1",
        rusqlite::params![server_id],
        |r| r.get::<_, i64>(0),
    ) {
        snap.everyone_permissions = crate::permissions::Permissions(v as u64);
    }
    if let Ok(mut stmt) = c.prepare(
        "SELECT id, server_id, name, color, mentionable, position, permissions FROM server_roles
         WHERE server_id = ?1 ORDER BY position DESC, id",
    ) {
        if let Ok(rows) = stmt.query_map(rusqlite::params![server_id], row_to_role) {
            snap.roles = rows.flatten().collect();
        }
    }
    if let Ok(mut stmt) = c.prepare(
        "SELECT mr.user_id, mr.role_id FROM member_roles mr
         JOIN server_roles r ON r.id = mr.role_id
         JOIN server_memberships m ON m.server_id = r.server_id AND m.user_id = mr.user_id
         WHERE r.server_id = ?1 ORDER BY mr.user_id, mr.role_id",
    ) {
        if let Ok(rows) = stmt.query_map(rusqlite::params![server_id], |r| Ok((r.get(0)?, r.get(1)?))) {
            snap.assignments = rows.flatten().collect();
        }
    }
    snap
}

/// The user ids a message from `author_id` in `channel_id` notifies: named
/// users, plus everyone / online members / role holders for `@everyone`,
/// `@here` and role mentions. Only people who can see the channel are
/// notified. The group mentions need Mention @everyone in the channel, except
/// roles marked mentionable. The author is never notified by a group mention.
/// Sorted and de-duplicated.
#[cfg(feature = "server")]
pub fn resolve_mentions(db: &Db, channel_id: i64, author_id: i64, content: &str) -> Vec<i64> {
    if !content.contains('@') {
        return Vec::new();
    }
    let Some(server_id) = db.channel_full(channel_id).map(|c| c.1) else {
        return Vec::new();
    };
    let Some(ctx) = ServerContext::load(db, server_id) else {
        return Vec::new();
    };
    let viewers = ctx.members_with(channel_id, Permissions::VIEW_CHANNEL);
    let users: Vec<(i64, String)> = db
        .server_members(server_id)
        .into_iter()
        .filter(|(id, _, _)| viewers.contains(id))
        .map(|(id, name, _)| (id, name))
        .collect();
    let snap = load_snapshot(db, server_id);
    let elevated = ctx
        .member(author_id)
        .map(|m| ctx.in_channel(&m, channel_id).contains(Permissions::MENTION_EVERYONE))
        .unwrap_or(false);
    let mut out: Vec<i64> = Vec::new();
    let mut group: Vec<i64> = Vec::new();
    for seg in segments(content, &users, &snap.roles) {
        match seg {
            MentionSeg::User { id, .. } => out.push(id),
            MentionSeg::Everyone(w) if elevated => {
                let online_only = w == "here";
                group.extend(users.iter().map(|u| u.0).filter(|id| {
                    !online_only || db.presence(*id).map(|p| p != "offline").unwrap_or(false)
                }));
            }
            MentionSeg::Role { id, .. } => {
                if elevated || snap.roles.iter().any(|r| r.id == id && r.mentionable) {
                    group.extend(
                        snap.assignments
                            .iter()
                            .filter(|(uid, rid)| *rid == id && viewers.contains(uid))
                            .map(|(uid, _)| *uid),
                    );
                }
            }
            _ => {}
        }
    }
    out.extend(group.into_iter().filter(|id| *id != author_id));
    out.sort_unstable();
    out.dedup();
    out
}

#[cfg(feature = "server")]
fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ── server functions ────────────────────────────────────────────────────────

#[cfg(feature = "server")]
fn db_from_state() -> Result<Db, ServerError> {
    server::use_state::<Db>().ok_or_else(|| ServerError::failed("database not installed"))
}

/// Tell the server's connected members to refetch its roles, and with them
/// their permissions and the channels they can see.
#[cfg(feature = "server")]
pub fn announce(db: &Db, server_id: i64) {
    crate::realtime::hub().publish(
        &crate::realtime::server_audience(db, server_id),
        &crate::server_settings::WsServerMsg::RolesChanged { server_id },
    );
}

/// The server's roles and who holds them. Any member may call.
#[server]
pub async fn server_roles(input: ServerRolesInput) -> Result<RolesSnapshot, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    RoleService::new(db).snapshot(caller, input.server_id).map_err(ServerError::failed)
}

/// Create a role, at the bottom of the list. Needs Manage Roles.
#[server]
pub async fn create_role(input: CreateRoleInput) -> Result<CustomRole, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let role = RoleService::new(db.clone())
        .create(caller, input.server_id, &input.name, &input.color, input.mentionable)
        .map_err(ServerError::failed)?;
    announce(&db, role.server_id);
    Ok(role)
}

/// Rename or recolour a role, change whether anyone may mention it, or change
/// its permissions. Needs Manage Roles and a higher rank than the role.
#[server]
pub async fn update_role(input: UpdateRoleInput) -> Result<CustomRole, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let role = RoleService::new(db.clone())
        .update(caller, input.role_id, &input.name, &input.color, input.mentionable, input.permissions)
        .map_err(ServerError::failed)?;
    announce(&db, role.server_id);
    Ok(role)
}

/// Move a role up or down the list (which sets who outranks whom).
#[server]
pub async fn move_role(input: MoveRoleInput) -> Result<CustomRole, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let role = RoleService::new(db.clone())
        .move_to(caller, input.role_id, input.position)
        .map_err(ServerError::failed)?;
    announce(&db, role.server_id);
    Ok(role)
}

/// Delete a role (its holders simply lose it).
#[server]
pub async fn delete_role(input: DeleteRoleInput) -> Result<(), ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let server_id = RoleService::new(db.clone()).delete(caller, input.role_id).map_err(ServerError::failed)?;
    announce(&db, server_id);
    Ok(())
}

/// Give a member a role, or take it away.
#[server]
pub async fn assign_role(input: AssignRoleInput) -> Result<(), ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let server_id = RoleService::new(db.clone())
        .assign(caller, input.role_id, input.user_id, input.assigned)
        .map_err(ServerError::failed)?;
    announce(&db, server_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn role(id: i64, name: &str, mentionable: bool) -> CustomRole {
        CustomRole {
            id,
            server_id: 10,
            name: name.into(),
            color: "#e91e63".into(),
            mentionable,
            position: id,
            permissions: crate::permissions::Permissions::NONE,
        }
    }

    fn users() -> Vec<(i64, String)> {
        vec![(1, "alice".into()), (2, "bob".into()), (3, "night".into())]
    }

    #[test]
    fn segments_split_users_roles_and_everyone() {
        let roles = vec![role(7, "Night Owls", true)];
        let segs = segments("hey @Alice and @night owls, @everyone!", &users(), &roles);
        assert_eq!(
            segs,
            vec![
                MentionSeg::Text("hey ".into()),
                MentionSeg::User { id: 1, name: "alice".into() },
                MentionSeg::Text(" and ".into()),
                MentionSeg::Role { id: 7, name: "Night Owls".into(), color: "#e91e63".into() },
                MentionSeg::Text(", ".into()),
                MentionSeg::Everyone("everyone".into()),
                MentionSeg::Text("!".into()),
            ]
        );
    }

    #[test]
    fn longer_role_names_win_and_words_must_end() {
        let roles = vec![role(1, "Mod", true), role(2, "Mod Squad", true)];
        let segs = segments("@mod squad @mods @Mod", &users(), &roles);
        assert!(matches!(&segs[0], MentionSeg::Role { id: 2, .. }));
        assert_eq!(segs[1], MentionSeg::Text(" @mods ".into()));
        assert!(matches!(&segs[2], MentionSeg::Role { id: 1, .. }));
    }

    #[test]
    fn plain_text_and_emails_stay_text() {
        let segs = segments("mail bob@example.com or @nobody @ here @heres", &users(), &[]);
        assert_eq!(segs, vec![MentionSeg::Text("mail bob@example.com or @nobody @ here @heres".into())]);
        assert_eq!(segments("@here", &users(), &[]), vec![MentionSeg::Everyone("here".into())]);
        assert_eq!(segments("é@bob", &users(), &[]), vec![MentionSeg::Text("é@bob".into())]);
    }

    #[test]
    fn names_and_colours_are_checked() {
        assert_eq!(validate_name("  Mods "), Ok("Mods".into()));
        assert!(validate_name("").is_err());
        assert!(validate_name("Everyone").is_err());
        assert!(validate_name("a@b").is_err());
        assert!(validate_name(&"x".repeat(33)).is_err());
        assert_eq!(validate_color("#ABCDEF"), Ok("#abcdef".into()));
        assert_eq!(validate_color(""), Ok(String::new()));
        assert!(validate_color("red").is_err());
        assert!(validate_color("#12345").is_err());
    }

    #[test]
    fn colour_comes_from_the_highest_coloured_role() {
        let mut low = role(1, "Low", true);
        low.color = "#111111".into();
        let mut plain = role(5, "Plain", true);
        plain.color = String::new();
        let snap = RolesSnapshot {
            server_id: 10,
            roles: vec![plain, low],
            assignments: vec![(2, 5), (2, 1)],
            ..Default::default()
        };
        assert_eq!(snap.color_of(2), Some("#111111"));
        assert_eq!(snap.color_of(3), None);
    }

    #[cfg(feature = "server")]
    mod db_tests {
        use super::super::*;
        use std::path::Path;

        /// alice (owner), bob (member, online), carol (a moderator role at
        /// position 5, offline), dave (member, offline) in server 10, which
        /// has text channel 20.
        fn setup() -> (tempfile::TempDir, Db) {
            let dir = tempfile::tempdir().unwrap();
            let migrations = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
            let db = Db::migrate(&dir.path().join("t.db"), &migrations).unwrap();
            db.lock()
                .unwrap()
                .execute_batch(
                    "INSERT INTO users (id, username, email, created_at) VALUES
                        (1, 'alice', 'a@x', 1), (2, 'bob', 'b@x', 1), (3, 'carol', 'c@x', 1), (4, 'dave', 'd@x', 1);
                     INSERT INTO servers (id, name, owner_id, created_at) VALUES (10, 'Hangout', 1, 1);
                     INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES
                        (10, 1, 'owner', 1), (10, 2, 'member', 1), (10, 3, 'member', 1), (10, 4, 'member', 1);
                     INSERT INTO channels (id, server_id, name, kind, position, created_at) VALUES (20, 10, 'general', 'text', 0, 1);",
                )
                .unwrap();
            let _ = crate::permissions::grant(&db, 10, 3, crate::permissions::MODERATOR_LEVEL, 5);
            db.set_presence(2, "online").unwrap();
            db.set_presence(3, "offline").unwrap();
            db.set_presence(4, "offline").unwrap();
            (dir, db)
        }

        #[test]
        fn managing_roles_needs_the_permission() {
            let (_d, db) = setup();
            let svc = RoleService::new(db);
            let refusal = "you need the Manage Roles permission to do that";
            assert_eq!(svc.create(2, 10, "Mods", "", true).unwrap_err(), refusal);
            assert_eq!(svc.create(3, 10, "Mods", "", true).unwrap_err(), refusal);
            let r = svc.create(1, 10, "Mods", "#FF0000", true).unwrap();
            assert_eq!((r.color.as_str(), r.position), ("#ff0000", 4), "new roles go below the rest");
            assert!(svc.create(1, 10, "mods", "", true).unwrap_err().contains("already exists"));
            assert!(svc.assign(2, r.id, 2, true).is_err());
            svc.assign(1, r.id, 2, true).unwrap();
            assert!(svc.assign(1, r.id, 99, true).unwrap_err().contains("member not found"));
            let snap = svc.snapshot(4, 10).unwrap();
            assert!(snap.assignments.contains(&(2, r.id)));
            assert!(svc.snapshot(99, 10).is_err());
            svc.update(1, r.id, "Moderators", "", false, None).unwrap();
            svc.delete(1, r.id).unwrap();
            assert_eq!(svc.snapshot(1, 10).unwrap().roles.len(), 1, "only carol's role is left");
        }

        #[test]
        fn the_hierarchy_limits_which_roles_someone_manages() {
            let (_d, db) = setup();
            // bob manages roles, but ranks below carol's role (5).
            let bobs = crate::permissions::grant(&db, 10, 2, crate::permissions::ADMIN_LEVEL, 3);
            let carols = load_snapshot(&db, 10).roles.iter().find(|r| r.position == 5).unwrap().id;
            let svc = RoleService::new(db.clone());
            let low = svc.create(2, 10, "Low", "", true).unwrap();
            assert_eq!(low.position, 2);
            svc.update(2, low.id, "Lower", "", true, Some(Permissions::KICK_MEMBERS)).unwrap();
            let below = "you can only manage roles below your highest role";
            assert_eq!(svc.update(2, carols, "Mine", "", true, None).unwrap_err(), below);
            assert_eq!(svc.update(2, bobs, "Self", "", true, None).unwrap_err(), below, "not even his own");
            assert_eq!(svc.delete(2, carols).unwrap_err(), below);
            assert_eq!(svc.assign(2, carols, 4, true).unwrap_err(), below);
            assert!(svc.move_to(2, low.id, 3).unwrap_err().contains("up to or above"));
            assert_eq!(svc.move_to(2, low.id, 1).unwrap().position, 1);
            assert!(
                svc.update(2, low.id, "Lower", "", true, Some(Permissions::ADMINISTRATOR)).unwrap_err().contains("Administrator"),
                "can't grant what he doesn't hold"
            );
            // The owner outranks every role.
            svc.update(1, carols, "Head Mod", "", true, Some(Permissions::ADMINISTRATOR)).unwrap();
            svc.move_to(1, low.id, 99).unwrap();
        }

        #[test]
        fn leaving_a_server_drops_its_roles() {
            let (_d, db) = setup();
            let svc = RoleService::new(db.clone());
            let r = svc.create(1, 10, "Crew", "", true).unwrap();
            svc.assign(1, r.id, 4, true).unwrap();
            db.lock().unwrap().execute("DELETE FROM server_memberships WHERE user_id = 4", []).unwrap();
            let n: i64 = db
                .lock()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM member_roles WHERE user_id = 4", [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0);
        }

        #[test]
        fn group_mentions_expand_for_moderators_only() {
            let (_d, db) = setup();
            let svc = RoleService::new(db.clone());
            let open = svc.create(1, 10, "Crew", "", true).unwrap();
            let locked = svc.create(1, 10, "Staff", "", false).unwrap();
            svc.assign(1, open.id, 4, true).unwrap();
            svc.assign(1, locked.id, 2, true).unwrap();
            // A plain member: @everyone/@here and the locked role do nothing.
            assert_eq!(resolve_mentions(&db, 20, 4, "@everyone @here @Staff"), Vec::<i64>::new());
            // ...but a mentionable role and a named user still work.
            assert_eq!(resolve_mentions(&db, 20, 2, "@crew @carol"), vec![3, 4]);
            // A moderator: everyone but themself; @here is the online ones.
            assert_eq!(resolve_mentions(&db, 20, 3, "@everyone"), vec![1, 2, 4]);
            assert_eq!(resolve_mentions(&db, 20, 3, "@here"), vec![2]);
            assert_eq!(resolve_mentions(&db, 20, 3, "@Staff"), vec![2]);
            // Naming yourself still counts (as before).
            assert_eq!(resolve_mentions(&db, 20, 3, "@carol"), vec![3]);
        }
    }
}
