//! Discord-style permissions: per-role permission sets, the implicit
//! @everyone role, and per-channel overwrites (private channels).
//!
//! ## Model
//!
//! - **Server permissions.** Each server stores what @everyone may do
//!   (`servers.everyone_permissions`), and each named role carries its own
//!   set (`server_roles.permissions`). A member has @everyone's set plus the
//!   sets of every role they hold. The owner, and anyone whose roles include
//!   [`Permissions::ADMINISTRATOR`], has every permission everywhere.
//! - **Channel overwrites.** A channel can allow or deny bits for @everyone,
//!   for a role, or for one member (`channel_overwrites`). A channel inside a
//!   category inherits the category's overwrites and applies its own on top.
//!   Within one channel the order is Discord's: @everyone, then all of the
//!   member's roles (denies, then allows), then the member themself. Only
//!   [`Permissions::CHANNEL_SCOPED`] bits can be overwritten.
//! - **Visibility.** Without [`Permissions::VIEW_CHANNEL`] a channel is
//!   invisible and every other bit in it is off. A private channel is one
//!   that denies VIEW_CHANNEL to @everyone and allows it to some roles or
//!   members.
//! - **Hierarchy.** A member's [`Rank`] is the highest position among their
//!   roles. Managing a role, or kicking, banning or timing out a member,
//!   needs a strictly higher rank. The owner outranks everyone.
//!
//! The compute functions are pure; [`ServerContext`] loads one server's
//! roles, members and overwrites in one go and answers questions from them.

use serde::{Deserialize, Serialize};
use server::{server, ServerError};

#[cfg(feature = "server")]
use crate::db::Db;

/// A set of permission bits, stored as an `INTEGER` column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Permissions(pub u64);

impl Permissions {
    pub const NONE: Self = Self(0);
    pub const VIEW_CHANNEL: Self = Self(1 << 0);
    pub const SEND_MESSAGES: Self = Self(1 << 1);
    pub const ADD_REACTIONS: Self = Self(1 << 2);
    pub const ATTACH_FILES: Self = Self(1 << 3);
    pub const EMBED_LINKS: Self = Self(1 << 4);
    /// Notify with @everyone, @here, or a role that isn't mentionable.
    pub const MENTION_EVERYONE: Self = Self(1 << 5);
    /// Pin and unpin messages.
    pub const MANAGE_MESSAGES: Self = Self(1 << 6);
    pub const CREATE_INVITE: Self = Self(1 << 7);
    pub const KICK_MEMBERS: Self = Self(1 << 8);
    pub const BAN_MEMBERS: Self = Self(1 << 9);
    /// Time members out.
    pub const MODERATE_MEMBERS: Self = Self(1 << 10);
    /// Create, rename and delete channels, and edit their overwrites.
    pub const MANAGE_CHANNELS: Self = Self(1 << 11);
    /// Create, edit, delete and hand out roles ranked below one's own.
    pub const MANAGE_ROLES: Self = Self(1 << 12);
    /// Rename the server, change its icon and @everyone's permissions.
    pub const MANAGE_SERVER: Self = Self(1 << 13);
    /// Every permission, and channel overwrites don't apply.
    pub const ADMINISTRATOR: Self = Self(1 << 14);

    pub const ALL: Self = Self((1 << 15) - 1);

    /// What @everyone gets in a new server: read, write, react, attach and
    /// embed. Matches the column default in `0022_permissions.sql`.
    pub const EVERYONE_DEFAULT: Self = Self(
        Self::VIEW_CHANNEL.0
            | Self::SEND_MESSAGES.0
            | Self::ADD_REACTIONS.0
            | Self::ATTACH_FILES.0
            | Self::EMBED_LINKS.0,
    );

    /// The bits a channel overwrite can change.
    pub const CHANNEL_SCOPED: Self = Self(
        Self::VIEW_CHANNEL.0
            | Self::SEND_MESSAGES.0
            | Self::ADD_REACTIONS.0
            | Self::ATTACH_FILES.0
            | Self::EMBED_LINKS.0
            | Self::MENTION_EVERYONE.0
            | Self::MANAGE_MESSAGES.0
            | Self::CREATE_INVITE.0
            | Self::MANAGE_CHANNELS.0,
    );

    /// Bits that do nothing without SEND_MESSAGES.
    const NEEDS_SEND: Self =
        Self(Self::ATTACH_FILES.0 | Self::EMBED_LINKS.0 | Self::MENTION_EVERYONE.0);

    /// Every permission with its display name and a one-line description,
    /// in the order a settings screen lists them.
    pub const FLAGS: [(Self, &'static str, &'static str); 15] = [
        (Self::ADMINISTRATOR, "Administrator", "Every permission, in every channel. Grant with care."),
        (Self::MANAGE_SERVER, "Manage Server", "Rename the server, change its icon and what everyone can do."),
        (Self::MANAGE_ROLES, "Manage Roles", "Create, edit and hand out roles below their own."),
        (Self::MANAGE_CHANNELS, "Manage Channels", "Create, rename and delete channels and set who can see them."),
        (Self::KICK_MEMBERS, "Kick Members", "Remove members, who can rejoin with an invite."),
        (Self::BAN_MEMBERS, "Ban Members", "Remove members and stop them rejoining."),
        (Self::MODERATE_MEMBERS, "Timeout Members", "Stop members posting for a while."),
        (Self::CREATE_INVITE, "Create Invite", "Make invite links to the server."),
        (Self::VIEW_CHANNEL, "View Channels", "See channels and read their messages."),
        (Self::SEND_MESSAGES, "Send Messages", "Post in text channels."),
        (Self::ADD_REACTIONS, "Add Reactions", "React to messages with emoji."),
        (Self::ATTACH_FILES, "Attach Files", "Upload images with messages."),
        (Self::EMBED_LINKS, "Embed Links", "Links they post show a preview."),
        (Self::MENTION_EVERYONE, "Mention @everyone", "Use @everyone, @here and roles that aren't mentionable."),
        (Self::MANAGE_MESSAGES, "Manage Messages", "Pin and unpin messages."),
    ];

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
    pub const fn minus(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }
    pub const fn intersect(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }
    /// Drop unknown bits (from a newer client or a bad request).
    pub const fn known(self) -> Self {
        self.intersect(Self::ALL)
    }
    /// The [`FLAGS`](Self::FLAGS) display names of the bits in `self`.
    pub fn names(self) -> Vec<&'static str> {
        Self::FLAGS.iter().filter(|(p, _, _)| self.contains(*p)).map(|(_, n, _)| *n).collect()
    }
}

/// Who a channel overwrite applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Target {
    Everyone,
    Role(i64),
    Member(i64),
}

impl Target {
    /// The stored `(target_kind, target_id)` pair.
    pub const fn db_parts(self) -> (&'static str, i64) {
        match self {
            Target::Everyone => ("everyone", 0),
            Target::Role(id) => ("role", id),
            Target::Member(id) => ("member", id),
        }
    }

    pub fn from_db_parts(kind: &str, id: i64) -> Option<Self> {
        match kind {
            "everyone" => Some(Target::Everyone),
            "role" => Some(Target::Role(id)),
            "member" => Some(Target::Member(id)),
            _ => None,
        }
    }
}

/// One channel's allow/deny for one target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Overwrite {
    pub target: Target,
    pub allow: Permissions,
    pub deny: Permissions,
}

/// Standing in a server's hierarchy. Ordered: anyone without roles is lowest,
/// role holders by their highest role's position, the owner highest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Rank {
    Everyone,
    Role(i64),
    Owner,
}

impl Rank {
    pub fn of(is_owner: bool, role_positions: impl IntoIterator<Item = i64>) -> Self {
        if is_owner {
            return Rank::Owner;
        }
        role_positions.into_iter().max().map_or(Rank::Everyone, Rank::Role)
    }

    /// Whether this rank may act on something at `other` (strictly above it).
    pub fn outranks(self, other: Rank) -> bool {
        self > other
    }
}

/// A member's server-wide permissions.
pub fn server_permissions(is_owner: bool, everyone: Permissions, roles: &[Permissions]) -> Permissions {
    if is_owner {
        return Permissions::ALL;
    }
    let perms = roles.iter().fold(everyone, |acc, r| acc.union(*r)).known();
    if perms.contains(Permissions::ADMINISTRATOR) {
        Permissions::ALL
    } else {
        perms
    }
}

/// A member's permissions in one channel. `layers` are the overwrite lists
/// to apply in order: the category's (if any), then the channel's own.
pub fn channel_permissions(
    server: Permissions,
    user_id: i64,
    role_ids: &[i64],
    layers: &[&[Overwrite]],
) -> Permissions {
    if server.contains(Permissions::ADMINISTRATOR) {
        return Permissions::ALL;
    }
    let mut perms = server;
    for layer in layers {
        let apply = |perms: Permissions, allow: Permissions, deny: Permissions| {
            let scope = Permissions::CHANNEL_SCOPED;
            perms.minus(deny.intersect(scope)).union(allow.intersect(scope))
        };
        if let Some(o) = layer.iter().find(|o| o.target == Target::Everyone) {
            perms = apply(perms, o.allow, o.deny);
        }
        let (mut allow, mut deny) = (Permissions::NONE, Permissions::NONE);
        for o in layer.iter() {
            if matches!(o.target, Target::Role(id) if role_ids.contains(&id)) {
                allow = allow.union(o.allow);
                deny = deny.union(o.deny);
            }
        }
        perms = apply(perms, allow, deny);
        if let Some(o) = layer.iter().find(|o| o.target == Target::Member(user_id)) {
            perms = apply(perms, o.allow, o.deny);
        }
    }
    if !perms.contains(Permissions::VIEW_CHANNEL) {
        return Permissions::NONE;
    }
    if !perms.contains(Permissions::SEND_MESSAGES) {
        perms = perms.minus(Permissions::NEEDS_SEND);
    }
    perms
}

/// What a caller may not do to a role or member, as a user-facing refusal.
/// `None` means allowed.
pub fn refuse_outranked(caller: Rank, target: Rank, what: &str) -> Option<String> {
    if caller.outranks(target) {
        None
    } else if target == Rank::Owner {
        Some(format!("you can't {what} the server owner"))
    } else {
        Some(format!("you can only {what} someone whose highest role is below yours"))
    }
}

// ── loading ──────────────────────────────────────────────────────────────────

/// One member as the permission checks see them.
#[cfg(feature = "server")]
#[derive(Debug, Clone)]
pub struct Member {
    pub user_id: i64,
    pub is_owner: bool,
    pub role_ids: Vec<i64>,
    pub rank: Rank,
    pub server: Permissions,
}

#[cfg(feature = "server")]
struct RoleRow {
    id: i64,
    permissions: Permissions,
    position: i64,
}

/// Everything about one server that permissions depend on, read at once.
#[cfg(feature = "server")]
pub struct ServerContext {
    pub server_id: i64,
    owner_id: i64,
    everyone: Permissions,
    roles: Vec<RoleRow>,
    /// Every member's user id with the role ids they hold.
    members: Vec<(i64, Vec<i64>)>,
    /// Every channel's id and its category's id.
    channels: Vec<(i64, Option<i64>)>,
    overwrites: std::collections::HashMap<i64, Vec<Overwrite>>,
}

#[cfg(feature = "server")]
impl ServerContext {
    /// Load `server_id`, or `None` if it doesn't exist.
    pub fn load(db: &Db, server_id: i64) -> Option<Self> {
        let c = db.lock().ok()?;
        let (owner_id, everyone): (i64, i64) = c
            .query_row(
                "SELECT owner_id, everyone_permissions FROM servers WHERE id = ?1 AND deleted_at IS NULL",
                [server_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok()?;
        let roles = c
            .prepare("SELECT id, permissions, position FROM server_roles WHERE server_id = ?1")
            .and_then(|mut s| {
                s.query_map([server_id], |r| {
                    Ok(RoleRow { id: r.get(0)?, permissions: Permissions(r.get::<_, i64>(1)? as u64), position: r.get(2)? })
                })?
                .collect::<Result<Vec<_>, _>>()
            })
            .ok()?;
        let mut members: Vec<(i64, Vec<i64>)> = c
            .prepare("SELECT user_id FROM server_memberships WHERE server_id = ?1 ORDER BY user_id")
            .and_then(|mut s| s.query_map([server_id], |r| Ok((r.get(0)?, Vec::new())))?.collect())
            .ok()?;
        let held: Vec<(i64, i64)> = c
            .prepare(
                "SELECT mr.user_id, mr.role_id FROM member_roles mr
                   JOIN server_roles r ON r.id = mr.role_id WHERE r.server_id = ?1",
            )
            .and_then(|mut s| s.query_map([server_id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect())
            .ok()?;
        for (user_id, role_id) in held {
            if let Some((_, ids)) = members.iter_mut().find(|(u, _)| *u == user_id) {
                ids.push(role_id);
            }
        }
        let channels: Vec<(i64, Option<i64>)> = c
            .prepare("SELECT id, parent_channel_id FROM channels WHERE server_id = ?1")
            .and_then(|mut s| s.query_map([server_id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect())
            .ok()?;
        let mut overwrites: std::collections::HashMap<i64, Vec<Overwrite>> = Default::default();
        let rows: Vec<(i64, String, i64, i64, i64)> = c
            .prepare(
                "SELECT o.channel_id, o.target_kind, o.target_id, o.allow, o.deny
                   FROM channel_overwrites o JOIN channels ch ON ch.id = o.channel_id
                  WHERE ch.server_id = ?1",
            )
            .and_then(|mut s| {
                s.query_map([server_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
                    .collect()
            })
            .ok()?;
        for (channel_id, kind, id, allow, deny) in rows {
            if let Some(target) = Target::from_db_parts(&kind, id) {
                overwrites.entry(channel_id).or_default().push(Overwrite {
                    target,
                    allow: Permissions(allow as u64),
                    deny: Permissions(deny as u64),
                });
            }
        }
        Some(Self { server_id, owner_id, everyone: Permissions(everyone as u64), roles, members, channels, overwrites })
    }

    /// `user_id` as a member, or `None` if they aren't one.
    pub fn member(&self, user_id: i64) -> Option<Member> {
        let (_, role_ids) = self.members.iter().find(|(u, _)| *u == user_id)?;
        let held: Vec<&RoleRow> = self.roles.iter().filter(|r| role_ids.contains(&r.id)).collect();
        let is_owner = user_id == self.owner_id;
        let perms: Vec<Permissions> = held.iter().map(|r| r.permissions).collect();
        Some(Member {
            user_id,
            is_owner,
            role_ids: role_ids.clone(),
            rank: Rank::of(is_owner, held.iter().map(|r| r.position)),
            server: server_permissions(is_owner, self.everyone, &perms),
        })
    }

    /// The rank of role `role_id`, or `None` if it isn't this server's.
    pub fn role_rank(&self, role_id: i64) -> Option<Rank> {
        self.roles.iter().find(|r| r.id == role_id).map(|r| Rank::Role(r.position))
    }

    /// The bits role `role_id` grants, or `None` if it isn't this server's.
    pub fn role_permissions(&self, role_id: i64) -> Option<Permissions> {
        self.roles.iter().find(|r| r.id == role_id).map(|r| r.permissions)
    }

    pub fn everyone(&self) -> Permissions {
        self.everyone
    }

    /// The overwrites set on `channel_id` itself (not its category's).
    pub fn overwrites(&self, channel_id: i64) -> &[Overwrite] {
        self.overwrites.get(&channel_id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// `member`'s permissions in `channel_id` (none if it isn't this server's).
    pub fn in_channel(&self, member: &Member, channel_id: i64) -> Permissions {
        let Some(&(_, parent)) = self.channels.iter().find(|(id, _)| *id == channel_id) else {
            return Permissions::NONE;
        };
        let own = self.overwrites(channel_id);
        match parent {
            Some(category) => {
                channel_permissions(member.server, member.user_id, &member.role_ids, &[self.overwrites(category), own])
            }
            None => channel_permissions(member.server, member.user_id, &member.role_ids, &[own]),
        }
    }

    /// Every channel of the server with `member`'s permissions in it.
    pub fn all_channels(&self, member: &Member) -> Vec<(i64, Permissions)> {
        self.channels.iter().map(|&(id, _)| (id, self.in_channel(member, id))).collect()
    }

    /// Whether @everyone can't see `channel_id`: its own or its category's
    /// overwrites deny them View Channel.
    pub fn is_private(&self, channel_id: i64) -> bool {
        let everyone = Member {
            user_id: 0,
            is_owner: false,
            role_ids: Vec::new(),
            rank: Rank::Everyone,
            server: self.everyone.minus(Permissions::ADMINISTRATOR),
        };
        !self.in_channel(&everyone, channel_id).contains(Permissions::VIEW_CHANNEL)
    }

    /// The user ids of every member holding `perm` in `channel_id`.
    pub fn members_with(&self, channel_id: i64, perm: Permissions) -> Vec<i64> {
        self.members
            .iter()
            .filter_map(|(user_id, _)| self.member(*user_id))
            .filter(|m| self.in_channel(m, channel_id).contains(perm))
            .map(|m| m.user_id)
            .collect()
    }
}

/// `user_id`'s permissions in `channel_id`, with the channel's server id.
/// `None` when the channel doesn't exist or `user_id` isn't a member.
#[cfg(feature = "server")]
pub fn for_channel(db: &Db, user_id: i64, channel_id: i64) -> Option<(i64, Permissions)> {
    let server_id = db.channel_full(channel_id)?.1;
    let ctx = ServerContext::load(db, server_id)?;
    let member = ctx.member(user_id)?;
    Some((server_id, ctx.in_channel(&member, channel_id)))
}

/// `user_id` in `server_id`, with the loaded context. Refuses with the
/// user-facing reason when the server is missing or they aren't a member.
#[cfg(feature = "server")]
pub fn member_in(db: &Db, server_id: i64, user_id: i64) -> Result<(ServerContext, Member), String> {
    let ctx = ServerContext::load(db, server_id).ok_or("server not found")?;
    let member = ctx.member(user_id).ok_or("you're not a member of this server")?;
    Ok((ctx, member))
}

// ── service and endpoints ────────────────────────────────────────────────────

/// The input for [`my_permissions`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyPermissionsInput {
    pub server_id: i64,
}

/// What the caller may do in one server: server-wide, and per channel (only
/// the channels they can see are listed).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MyPermissions {
    pub server_id: i64,
    pub server: Permissions,
    pub channels: Vec<(i64, Permissions)>,
    pub rank: Rank,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelOverwritesInput {
    pub channel_id: i64,
}

/// Set one target's overwrite on a channel. Allowing and denying nothing
/// removes it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetOverwriteInput {
    pub channel_id: i64,
    pub target: Target,
    pub allow: Permissions,
    pub deny: Permissions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetEveryoneInput {
    pub server_id: i64,
    pub permissions: Permissions,
}

/// Permission reads and edits; `caller` is the acting user.
#[cfg(feature = "server")]
pub struct PermissionService {
    db: Db,
}

#[cfg(feature = "server")]
impl PermissionService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub fn mine(&self, caller: i64, server_id: i64) -> Result<MyPermissions, String> {
        let (ctx, member) = member_in(&self.db, server_id, caller)?;
        let channels = ctx
            .all_channels(&member)
            .into_iter()
            .filter(|(_, p)| p.contains(Permissions::VIEW_CHANNEL))
            .collect();
        Ok(MyPermissions { server_id, server: member.server, channels, rank: member.rank })
    }

    /// The caller's context and permissions in `channel_id`, which they must
    /// be able to manage. Returns the server context, the caller and their
    /// permissions in the channel.
    fn require_manage_channel(&self, caller: i64, channel_id: i64) -> Result<(ServerContext, Member, Permissions), String> {
        let server_id = self.db.channel_full(channel_id).ok_or("channel not found")?.1;
        let (ctx, member) = member_in(&self.db, server_id, caller)?;
        let perms = ctx.in_channel(&member, channel_id);
        if !perms.contains(Permissions::VIEW_CHANNEL) {
            return Err("channel not found".into());
        }
        if !perms.contains(Permissions::MANAGE_CHANNELS) {
            return Err(missing(Permissions::MANAGE_CHANNELS));
        }
        Ok((ctx, member, perms))
    }

    pub fn overwrites(&self, caller: i64, channel_id: i64) -> Result<Vec<Overwrite>, String> {
        let (ctx, _, _) = self.require_manage_channel(caller, channel_id)?;
        Ok(ctx.overwrites(channel_id).to_vec())
    }

    /// Set `target`'s overwrite on `channel_id` and return the channel's
    /// overwrites afterwards. Returns the server id too, for the broadcast.
    pub fn set_overwrite(
        &self,
        caller: i64,
        channel_id: i64,
        target: Target,
        allow: Permissions,
        deny: Permissions,
    ) -> Result<(i64, Vec<Overwrite>), String> {
        let (ctx, _, held) = self.require_manage_channel(caller, channel_id)?;
        if allow.minus(Permissions::CHANNEL_SCOPED) != Permissions::NONE
            || deny.minus(Permissions::CHANNEL_SCOPED) != Permissions::NONE
        {
            return Err("only channel permissions can be set on a channel".into());
        }
        if allow.intersect(deny) != Permissions::NONE {
            return Err("a permission can't be both allowed and denied".into());
        }
        match target {
            Target::Everyone => {}
            Target::Role(id) => {
                ctx.role_rank(id).ok_or("role not found")?;
            }
            Target::Member(id) => {
                ctx.member(id).ok_or("member not found")?;
            }
        }
        let old = ctx.overwrites(channel_id).iter().find(|o| o.target == target).copied();
        let (old_allow, old_deny) = old.map_or((Permissions::NONE, Permissions::NONE), |o| (o.allow, o.deny));
        let changed = Permissions((old_allow.0 ^ allow.0) | (old_deny.0 ^ deny.0));
        if let Some(name) = changed.minus(held).names().first() {
            return Err(format!("you can't allow or deny {name} here, because you don't have it yourself"));
        }
        let (kind, id) = target.db_parts();
        {
            let c = self.db.lock().map_err(|e| e.to_string())?;
            if allow == Permissions::NONE && deny == Permissions::NONE {
                c.execute(
                    "DELETE FROM channel_overwrites WHERE channel_id = ?1 AND target_kind = ?2 AND target_id = ?3",
                    rusqlite::params![channel_id, kind, id],
                )
            } else {
                c.execute(
                    "INSERT INTO channel_overwrites (channel_id, target_kind, target_id, allow, deny)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT (channel_id, target_kind, target_id) DO UPDATE SET allow = ?4, deny = ?5",
                    rusqlite::params![channel_id, kind, id, allow.0 as i64, deny.0 as i64],
                )
            }
            .map_err(|e| format!("failed to save the channel's permissions: {e}"))?;
        }
        let server_id = ctx.server_id;
        let ctx = ServerContext::load(&self.db, server_id).ok_or("server not found")?;
        Ok((server_id, ctx.overwrites(channel_id).to_vec()))
    }

    /// Set what @everyone may do in `server_id`. Needs Manage Roles, and only
    /// bits the caller holds can change.
    pub fn set_everyone(&self, caller: i64, server_id: i64, perms: Permissions) -> Result<Permissions, String> {
        let (ctx, member) = member_in(&self.db, server_id, caller)?;
        if !member.server.contains(Permissions::MANAGE_ROLES) {
            return Err(missing(Permissions::MANAGE_ROLES));
        }
        let perms = perms.known().minus(Permissions::ADMINISTRATOR);
        if let Some(name) = Permissions(ctx.everyone().0 ^ perms.0).minus(member.server).names().first() {
            return Err(format!("you can't grant or remove {name}, because you don't have it yourself"));
        }
        let c = self.db.lock().map_err(|e| e.to_string())?;
        c.execute(
            "UPDATE servers SET everyone_permissions = ?2 WHERE id = ?1",
            rusqlite::params![server_id, perms.0 as i64],
        )
        .map_err(|e| format!("failed to save @everyone's permissions: {e}"))?;
        Ok(perms)
    }
}

/// Make a new channel private: hidden from @everyone, visible to `creator`
/// (so someone without Administrator doesn't lose the channel they made).
#[cfg(feature = "server")]
pub fn make_private(db: &Db, channel_id: i64, creator: i64) -> Result<(), String> {
    let c = db.lock().map_err(|e| e.to_string())?;
    c.execute(
        "INSERT INTO channel_overwrites (channel_id, target_kind, target_id, allow, deny) VALUES
            (?1, 'everyone', 0, 0, ?2), (?1, 'member', ?3, ?2, 0)",
        rusqlite::params![channel_id, Permissions::VIEW_CHANNEL.0 as i64, creator],
    )
    .map_err(|e| format!("failed to make the channel private: {e}"))?;
    Ok(())
}

#[cfg(feature = "server")]
fn db_from_state() -> Result<Db, ServerError> {
    server::use_state::<Db>().ok_or_else(|| ServerError::failed("database not installed"))
}

#[cfg(feature = "server")]
fn caller() -> Result<i64, ServerError> {
    crate::session::acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))
}

/// What the caller may do in a server and in each channel they can see.
#[server]
pub async fn my_permissions(input: MyPermissionsInput) -> Result<MyPermissions, ServerError> {
    PermissionService::new(db_from_state()?)
        .mine(caller()?, input.server_id)
        .map_err(ServerError::failed)
}

/// A channel's overwrites. Needs Manage Channels in it.
#[server]
pub async fn channel_overwrites(input: ChannelOverwritesInput) -> Result<Vec<Overwrite>, ServerError> {
    PermissionService::new(db_from_state()?)
        .overwrites(caller()?, input.channel_id)
        .map_err(ServerError::failed)
}

/// Set or clear one overwrite on a channel; returns the channel's overwrites.
#[server]
pub async fn set_channel_overwrite(input: SetOverwriteInput) -> Result<Vec<Overwrite>, ServerError> {
    let db = db_from_state()?;
    let (server_id, list) = PermissionService::new(db.clone())
        .set_overwrite(caller()?, input.channel_id, input.target, input.allow, input.deny)
        .map_err(ServerError::failed)?;
    crate::server_roles::announce(&db, server_id);
    Ok(list)
}

/// Set what @everyone may do in a server.
#[server]
pub async fn set_everyone_permissions(input: SetEveryoneInput) -> Result<Permissions, ServerError> {
    let db = db_from_state()?;
    let perms = PermissionService::new(db.clone())
        .set_everyone(caller()?, input.server_id, input.permissions)
        .map_err(ServerError::failed)?;
    crate::server_roles::announce(&db, input.server_id);
    Ok(perms)
}

/// What the old "moderator" level could do (see `0022_permissions.sql`).
#[cfg(test)]
pub const MODERATOR_LEVEL: Permissions = Permissions(480);
/// What the old "admin" level could do.
#[cfg(test)]
pub const ADMIN_LEVEL: Permissions = Permissions(16352);

/// Test helper: give `user_id` a new role in `server_id` with `perms` at
/// `position`, and return the role's id.
#[cfg(all(test, feature = "server"))]
pub fn grant(db: &Db, server_id: i64, user_id: i64, perms: Permissions, position: i64) -> i64 {
    let c = db.lock().unwrap();
    c.execute(
        "INSERT INTO server_roles (server_id, name, position, created_at, permissions)
         VALUES (?1, 'test role ' || (SELECT COUNT(*) FROM server_roles), ?2, 1, ?3)",
        rusqlite::params![server_id, position, perms.0 as i64],
    )
    .unwrap();
    let role_id = c.last_insert_rowid();
    c.execute("INSERT INTO member_roles (role_id, user_id) VALUES (?1, ?2)", rusqlite::params![role_id, user_id])
        .unwrap();
    role_id
}

/// The user-facing refusal for a missing permission.
pub fn missing(perm: Permissions) -> String {
    let name = perm.names().first().copied().unwrap_or("that permission");
    format!("you need the {name} permission to do that")
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: Permissions = Permissions::EVERYONE_DEFAULT;

    fn ow(target: Target, allow: Permissions, deny: Permissions) -> Overwrite {
        Overwrite { target, allow, deny }
    }

    #[test]
    fn flags_cover_every_bit_once() {
        let mut seen = Permissions::NONE;
        for (p, _, _) in Permissions::FLAGS {
            assert_eq!(p.0.count_ones(), 1);
            assert!(!seen.contains(p), "{p:?} listed twice");
            seen = seen.union(p);
        }
        assert_eq!(seen, Permissions::ALL);
    }

    #[test]
    fn owner_and_administrator_get_everything() {
        assert_eq!(server_permissions(true, Permissions::NONE, &[]), Permissions::ALL);
        assert_eq!(server_permissions(false, P, &[Permissions::ADMINISTRATOR]), Permissions::ALL);
        let deny_all = [ow(Target::Everyone, Permissions::NONE, Permissions::ALL)];
        assert_eq!(channel_permissions(Permissions::ALL, 1, &[], &[&deny_all]), Permissions::ALL);
    }

    #[test]
    fn roles_add_to_everyone() {
        let mods = Permissions::KICK_MEMBERS.union(Permissions::MANAGE_MESSAGES);
        let got = server_permissions(false, P, &[mods, Permissions::BAN_MEMBERS]);
        assert!(got.contains(P.union(mods).union(Permissions::BAN_MEMBERS)));
        assert!(!got.contains(Permissions::MANAGE_ROLES));
        assert_eq!(server_permissions(false, Permissions(1 << 60), &[]), Permissions::NONE, "unknown bits dropped");
    }

    #[test]
    fn a_private_channel_shows_only_to_allowed_roles_and_members() {
        let private = [
            ow(Target::Everyone, Permissions::NONE, Permissions::VIEW_CHANNEL),
            ow(Target::Role(7), Permissions::VIEW_CHANNEL, Permissions::NONE),
            ow(Target::Member(42), Permissions::VIEW_CHANNEL, Permissions::NONE),
        ];
        assert_eq!(channel_permissions(P, 1, &[], &[&private]), Permissions::NONE);
        assert_eq!(channel_permissions(P, 1, &[7], &[&private]), P);
        assert_eq!(channel_permissions(P, 42, &[], &[&private]), P);
        assert_eq!(channel_permissions(P, 1, &[8], &[&private]), Permissions::NONE);
    }

    #[test]
    fn role_allow_beats_role_deny_and_member_beats_both() {
        let layer = [
            ow(Target::Role(1), Permissions::NONE, Permissions::SEND_MESSAGES),
            ow(Target::Role(2), Permissions::SEND_MESSAGES, Permissions::NONE),
            ow(Target::Member(9), Permissions::NONE, Permissions::SEND_MESSAGES),
        ];
        assert!(channel_permissions(P, 5, &[1, 2], &[&layer]).contains(Permissions::SEND_MESSAGES));
        assert!(!channel_permissions(P, 5, &[1], &[&layer]).contains(Permissions::SEND_MESSAGES));
        assert!(!channel_permissions(P, 9, &[1, 2], &[&layer]).contains(Permissions::SEND_MESSAGES));
    }

    #[test]
    fn a_channel_inherits_its_category_and_can_override_it() {
        let category = [ow(Target::Everyone, Permissions::NONE, Permissions::VIEW_CHANNEL)];
        let reopened = [ow(Target::Everyone, Permissions::VIEW_CHANNEL, Permissions::NONE)];
        assert_eq!(channel_permissions(P, 1, &[], &[&category, &[]]), Permissions::NONE);
        assert_eq!(channel_permissions(P, 1, &[], &[&category, &reopened]), P);
    }

    #[test]
    fn overwrites_cannot_grant_server_wide_powers() {
        let layer = [ow(Target::Everyone, Permissions::BAN_MEMBERS.union(Permissions::ADMINISTRATOR), Permissions::NONE)];
        let got = channel_permissions(P, 1, &[], &[&layer]);
        assert!(!got.contains(Permissions::BAN_MEMBERS));
        assert!(!got.contains(Permissions::ADMINISTRATOR));
    }

    #[test]
    fn read_only_channels_drop_send_dependent_bits() {
        let everyone = P.union(Permissions::MENTION_EVERYONE);
        let layer = [ow(Target::Everyone, Permissions::NONE, Permissions::SEND_MESSAGES)];
        let got = channel_permissions(everyone, 1, &[], &[&layer]);
        assert!(got.contains(Permissions::VIEW_CHANNEL.union(Permissions::ADD_REACTIONS)));
        assert!(!got.contains(Permissions::ATTACH_FILES));
        assert!(!got.contains(Permissions::MENTION_EVERYONE));
    }

    #[test]
    fn rank_orders_owner_over_roles_over_everyone() {
        assert_eq!(Rank::of(false, []), Rank::Everyone);
        assert_eq!(Rank::of(false, [3, 9, 1]), Rank::Role(9));
        assert_eq!(Rank::of(true, [1]), Rank::Owner);
        assert!(Rank::Role(0).outranks(Rank::Everyone));
        assert!(Rank::Role(5).outranks(Rank::Role(4)));
        assert!(!Rank::Role(4).outranks(Rank::Role(4)));
        assert!(Rank::Owner.outranks(Rank::Role(i64::MAX)));
        assert!(refuse_outranked(Rank::Role(1), Rank::Owner, "kick").unwrap().contains("owner"));
        assert!(refuse_outranked(Rank::Role(1), Rank::Role(1), "kick").is_some());
        assert!(refuse_outranked(Rank::Role(2), Rank::Role(1), "kick").is_none());
    }

    #[test]
    fn target_db_parts_round_trip() {
        for t in [Target::Everyone, Target::Role(3), Target::Member(4)] {
            let (k, id) = t.db_parts();
            assert_eq!(Target::from_db_parts(k, id), Some(t));
        }
        assert_eq!(Target::from_db_parts("admin", 1), None);
    }

    #[test]
    fn missing_names_the_permission() {
        assert_eq!(missing(Permissions::KICK_MEMBERS), "you need the Kick Members permission to do that");
    }
}

#[cfg(all(test, feature = "server"))]
mod db_tests {
    use super::*;
    use std::path::Path;

    const MIGRATIONS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/migrations");

    fn exec(db: &Db, sql: &str) {
        db.lock().unwrap().execute_batch(sql).unwrap();
    }

    /// alice owns server 1; bob is an admin, carol a moderator, dave a member.
    /// Server 2 already has a role called "Admin". Built with every migration
    /// before 0022, which is then applied on top, like an upgrade.
    fn upgraded() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("old");
        std::fs::create_dir(&old).unwrap();
        for entry in std::fs::read_dir(MIGRATIONS).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_str().unwrap().to_string();
            if name.as_str() < "0022" {
                std::fs::copy(&path, old.join(&name)).unwrap();
            }
        }
        let db_path = dir.path().join("t.db");
        let db = Db::migrate(&db_path, &old).unwrap();
        exec(
            &db,
            "INSERT INTO users (id, username, email, created_at) VALUES
                (1,'alice','a@x',1),(2,'bob','b@x',1),(3,'carol','c@x',1),(4,'dave','d@x',1);
             INSERT INTO servers (id, name, owner_id, created_at) VALUES (1,'one',1,1),(2,'two',1,1);
             INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES
                (1,1,'owner',1),(1,2,'admin',1),(1,3,'moderator',1),(1,4,'member',1),
                (2,1,'owner',1),(2,2,'admin',1);
             INSERT INTO server_roles (server_id, name, position, created_at) VALUES (1,'Fans',3,1),(2,'Admin',0,1);
             INSERT INTO channels (id, server_id, name, kind, position, created_at) VALUES
                (10,1,'general','text',0,1),(11,1,'staff','text',1,1),
                (12,1,'Secret','category',2,1);
             INSERT INTO channels (id, server_id, name, kind, position, parent_channel_id, created_at)
                VALUES (13,1,'vault','text',3,12,1);",
        );
        drop(db);
        let db = Db::migrate(&db_path, Path::new(MIGRATIONS)).unwrap();
        (dir, db)
    }

    fn perms(db: &Db, server: i64, user: i64) -> Permissions {
        ServerContext::load(db, server).unwrap().member(user).unwrap().server
    }

    #[test]
    fn private_means_everyone_cannot_view() {
        let (_dir, db) = upgraded();
        exec(
            &db,
            "INSERT INTO channel_overwrites (channel_id, target_kind, target_id, allow, deny) VALUES
                (11,'everyone',0,0,1),(12,'everyone',0,0,1),(10,'role',1,0,1);",
        );
        let ctx = ServerContext::load(&db, 1).unwrap();
        assert!(!ctx.is_private(10), "denying one role doesn't make it private");
        assert!(ctx.is_private(11));
        assert!(ctx.is_private(13), "a channel in a private category is private");
    }

    #[test]
    fn upgrade_keeps_each_levels_powers() {
        let (_d, db) = upgraded();
        let everyone = Permissions::EVERYONE_DEFAULT;
        assert_eq!(perms(&db, 1, 1), Permissions::ALL);
        assert_eq!(perms(&db, 1, 4), everyone);
        let moderator = perms(&db, 1, 3);
        assert_eq!(moderator, everyone.union(MODERATOR_LEVEL));
        for p in [Permissions::KICK_MEMBERS, Permissions::MANAGE_MESSAGES, Permissions::MENTION_EVERYONE, Permissions::CREATE_INVITE] {
            assert!(moderator.contains(p), "moderator lost {:?}", p.names());
        }
        assert!(!moderator.contains(Permissions::BAN_MEMBERS));
        let admin = perms(&db, 1, 2);
        assert_eq!(admin, everyone.union(ADMIN_LEVEL));
        for p in [Permissions::BAN_MEMBERS, Permissions::MODERATE_MEMBERS, Permissions::MANAGE_CHANNELS, Permissions::MANAGE_ROLES, Permissions::MANAGE_SERVER] {
            assert!(admin.contains(p), "admin lost {:?}", p.names());
        }
        assert!(!admin.contains(Permissions::ADMINISTRATOR));
    }

    #[test]
    fn upgrade_ranks_admin_over_moderator_over_existing_roles() {
        let (_d, db) = upgraded();
        let ctx = ServerContext::load(&db, 1).unwrap();
        let (admin, moderator, dave) = (ctx.member(2).unwrap(), ctx.member(3).unwrap(), ctx.member(4).unwrap());
        assert_eq!(admin.rank, Rank::Role(5));
        assert_eq!(moderator.rank, Rank::Role(4));
        assert_eq!(dave.rank, Rank::Everyone);
        assert_eq!(ctx.member(1).unwrap().rank, Rank::Owner);
    }

    #[test]
    fn upgrade_does_not_reuse_an_existing_role_name() {
        let (_d, db) = upgraded();
        let names: Vec<String> = db
            .lock()
            .unwrap()
            .prepare("SELECT name FROM server_roles WHERE server_id = 2 ORDER BY position")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(names, ["Admin", "Admin (level)"]);
        assert!(perms(&db, 2, 2).contains(Permissions::MANAGE_ROLES));
    }

    #[test]
    fn private_category_hides_its_channels_from_everyone_but_the_allowed() {
        let (_d, db) = upgraded();
        exec(
            &db,
            "INSERT INTO channel_overwrites (channel_id, target_kind, target_id, allow, deny) VALUES
                (12,'everyone',0,0,1),(12,'member',4,1,0);",
        );
        let ctx = ServerContext::load(&db, 1).unwrap();
        let carol = ctx.member(3).unwrap();
        assert_eq!(ctx.in_channel(&carol, 13), Permissions::NONE, "moderator can't see the vault");
        assert!(ctx.in_channel(&carol, 10).contains(Permissions::VIEW_CHANNEL));
        let mut viewers = ctx.members_with(13, Permissions::VIEW_CHANNEL);
        viewers.sort();
        assert_eq!(viewers, [1, 4], "owner and the allowed member");
        assert_eq!(for_channel(&db, 3, 13), Some((1, Permissions::NONE)));
        assert_eq!(for_channel(&db, 99, 13), None, "non-members get nothing");
    }

    #[test]
    fn leaving_or_deleting_a_role_removes_its_overwrites() {
        let (_d, db) = upgraded();
        exec(
            &db,
            "INSERT INTO channel_overwrites (channel_id, target_kind, target_id, allow, deny) VALUES
                (11,'member',4,1,0),(11,'role',1,1,0);
             DELETE FROM server_memberships WHERE server_id = 1 AND user_id = 4;
             DELETE FROM server_roles WHERE id = 1;",
        );
        let left: i64 = db.lock().unwrap().query_row("SELECT COUNT(*) FROM channel_overwrites", [], |r| r.get(0)).unwrap();
        assert_eq!(left, 0);
    }

    #[test]
    fn member_in_refuses_strangers_and_missing_servers() {
        let (_d, db) = upgraded();
        assert!(member_in(&db, 1, 4).is_ok());
        assert_eq!(member_in(&db, 2, 4).err().unwrap(), "you're not a member of this server");
        assert_eq!(member_in(&db, 9, 1).err().unwrap(), "server not found");
    }

    #[test]
    fn overwrite_edits_are_checked_and_saved() {
        let (_d, db) = upgraded();
        let svc = PermissionService::new(db.clone());
        // dave (plain member) can't touch them; bob (admin) can.
        assert_eq!(svc.overwrites(4, 10).unwrap_err(), "you need the Manage Channels permission to do that");
        let hide = Permissions::VIEW_CHANNEL;
        // bob keeps access for himself before hiding #staff from @everyone;
        // without Administrator he'd otherwise lock himself out.
        svc.set_overwrite(2, 11, Target::Member(2), hide, Permissions::NONE).unwrap();
        let (_, list) = svc.set_overwrite(2, 11, Target::Everyone, Permissions::NONE, hide).unwrap();
        assert!(list.contains(&Overwrite { target: Target::Everyone, allow: Permissions::NONE, deny: hide }));
        assert_eq!(for_channel(&db, 4, 11), Some((1, Permissions::NONE)), "dave lost #staff");
        svc.set_overwrite(2, 11, Target::Member(4), hide, Permissions::NONE).unwrap();
        assert!(for_channel(&db, 4, 11).unwrap().1.contains(hide), "dave let back in");
        // Clearing removes the row.
        let (_, list) = svc.set_overwrite(2, 11, Target::Member(4), Permissions::NONE, Permissions::NONE).unwrap();
        assert_eq!(list.len(), 2);
        // Refusals.
        assert!(svc.set_overwrite(2, 11, Target::Everyone, Permissions::BAN_MEMBERS, Permissions::NONE).is_err());
        assert!(svc.set_overwrite(2, 11, Target::Everyone, hide, hide).unwrap_err().contains("both"));
        assert_eq!(svc.set_overwrite(2, 11, Target::Role(999), hide, Permissions::NONE).unwrap_err(), "role not found");
        assert_eq!(svc.set_overwrite(2, 11, Target::Member(99), hide, Permissions::NONE).unwrap_err(), "member not found");
    }

    #[test]
    fn nobody_can_hand_out_channel_bits_they_lack() {
        let (_d, db) = upgraded();
        let svc = PermissionService::new(db.clone());
        // In #general carol (moderator) may manage the channel but not its messages.
        svc.set_overwrite(1, 10, Target::Member(3), Permissions::MANAGE_CHANNELS, Permissions::MANAGE_MESSAGES).unwrap();
        assert!(svc
            .set_overwrite(3, 10, Target::Member(4), Permissions::MANAGE_MESSAGES, Permissions::NONE)
            .unwrap_err()
            .contains("Manage Messages"));
        svc.set_overwrite(3, 10, Target::Member(4), Permissions::ADD_REACTIONS, Permissions::NONE).unwrap();
    }

    #[test]
    fn mine_lists_only_visible_channels_and_everyone_edits_are_limited() {
        let (_d, db) = upgraded();
        make_private(&db, 11, 2).unwrap();
        let svc = PermissionService::new(db.clone());
        let dave = svc.mine(4, 1).unwrap();
        let ids: Vec<i64> = dave.channels.iter().map(|c| c.0).collect();
        assert!(!ids.contains(&11), "the private channel is hidden from dave");
        assert!(svc.mine(2, 1).unwrap().channels.iter().any(|c| c.0 == 11), "its creator sees it");
        assert_eq!(dave.rank, Rank::Everyone);
        // bob (admin, no Administrator) can't give @everyone Administrator or
        // a bit he lacks; the owner can change anything but Administrator.
        assert_eq!(svc.set_everyone(4, 1, Permissions::NONE).unwrap_err(), "you need the Manage Roles permission to do that");
        let with_kick = Permissions::EVERYONE_DEFAULT.union(Permissions::KICK_MEMBERS);
        assert_eq!(svc.set_everyone(2, 1, with_kick).unwrap(), with_kick);
        // A role that can only manage roles can't hand @everyone Ban Members.
        let _ = grant(&db, 1, 4, Permissions::MANAGE_ROLES, 1);
        let with_ban = with_kick.union(Permissions::BAN_MEMBERS);
        assert!(svc.set_everyone(4, 1, with_ban).unwrap_err().contains("Ban Members"));
        assert_eq!(svc.set_everyone(1, 1, Permissions::ALL).unwrap(), Permissions::ALL.minus(Permissions::ADMINISTRATOR));
    }
}
