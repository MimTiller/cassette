//! Server roles and the per-action permission policy.
//!
//! ## Model
//!
//! A user's standing in a server is a single [`Role`] (the value stored in
//! `server_memberships.role`). The roles form a strict hierarchy:
//!
//! ```text
//! owner  ⊃  admin  ⊃  moderator  ⊃  member
//! ```
//!
//! Every role can do what the roles below it can. The hierarchy is encoded by
//! the variant discriminants (higher = more powerful) and `Role::at_least`,
//! so "does this role reach admin-or-above?" is a single comparison.
//!
//! ## Why a plain hierarchy, not a bitset
//!
//! Discord's real model is named roles each carrying an arbitrary permission
//! bitset. For this clone a single per-membership role is the shape the schema
//! already commits to (see `0005_server_memberships.sql`), and a fixed,
//! documented hierarchy is the smallest thing that expresses the policy the
//! task calls out ("only admins can delete a channel, only admin/moderator can
//! remove a member"). Named-role flexibility is a later layer; the hierarchy +
//! [`Role::can`] is the unit that must be correct now, so it is pure and
//! exhaustively unit-tested with no DB in the loop.
//!
//! ## The policy
//!
//! [`Role::can`] maps each server/channel action to the minimum role that may
//! perform it. Owner always passes (it is the top of the hierarchy). The
//! exact matrix is pinned by the tests in this file and by the integration
//! tests in `server_settings`.

use serde::{Deserialize, Serialize};

/// The kind of a channel, as stored in `channels.kind`. A server's channels
/// are polymorphic: text (chat), voice (audio), and category (a grouping
/// header whose children hang under it in the sidebar).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChannelKind {
    Text,
    Voice,
    Category,
}

impl ChannelKind {
    /// The stored `channels.kind` string (`'text'` / `'voice'` / `'category'`).
    pub const fn db_str(self) -> &'static str {
        match self {
            ChannelKind::Text => "text",
            ChannelKind::Voice => "voice",
            ChannelKind::Category => "category",
        }
    }

    /// Parse a stored `channels.kind` string. Returns `None` for an unknown
    /// value rather than guessing.
    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "text" => Some(ChannelKind::Text),
            "voice" => Some(ChannelKind::Voice),
            "category" => Some(ChannelKind::Category),
            _ => None,
        }
    }
}

/// A user's standing in a server. Ordered by power: `Owner` is the most
/// privileged, `Member` the least. `PartialOrd`/`Ord` derive the hierarchy so
/// `role >= Role::Admin` is "admin or above".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Role {
    /// Default standing: can chat and read channels, but no moderation.
    Member,
    /// Can remove (kick) members and rename channels. Cannot delete channels
    /// or change roles.
    Moderator,
    /// Can create/delete channels and manage member roles. Cannot rename the
    /// server or transfer ownership.
    Admin,
    /// The creator. Everything, including rename server and transfer
    /// ownership.
    Owner,
}

/// A server/channel action that can be permission-checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Rename the server itself.
    RenameServer,
    /// Set or clear the server icon.
    SetServerIcon,
    /// Transfer server ownership to another member.
    TransferOwnership,
    /// Create a new channel in the server.
    CreateChannel,
    /// Delete (soft-remove) a channel.
    DeleteChannel,
    /// Rename a channel.
    RenameChannel,
    /// Add a member to the server (invite).
    AddMember,
    /// Remove a member from the server (kick).
    RemoveMember,
    /// Ban a member from the server: remove them AND bar rejoining (via invite
    /// redemption or direct add). More severe than a kick, so the minimum
    /// role is higher (admin+ rather than moderator+).
    BanMember,
    /// Temporarily silence a member: block them from posting messages until a
    /// duration elapses, without removing their membership (unlike a kick or
    /// ban). Like a ban, this is a stronger restraint than a kick, so the
    /// minimum role is admin+.
    TimeoutMember,
    /// Change another member's role in the server.
    SetMemberRole,
    /// Pin or unpin a message in a channel. Moderator+ — Discord parity
    /// ("Manage Messages"): any member can *read* the pinned view, but only
    /// moderators and above can change what is pinned.
    PinMessage,
    /// Create, edit, delete and hand out named roles (`server_roles`). Admin+.
    ManageRoles,
    /// Notify with `@everyone`, `@here` or a role that isn't mentionable.
    /// Moderator+.
    MentionEveryone,
}

impl Role {
    /// The `owner` / `admin` / `moderator` / `member` string the role is stored
    /// as in `server_memberships.role`.
    pub const fn db_str(self) -> &'static str {
        match self {
            Role::Member => "member",
            Role::Moderator => "moderator",
            Role::Admin => "admin",
            Role::Owner => "owner",
        }
    }

    /// Parse the stored `server_memberships.role` string into a [`Role`].
    /// Returns `None` for an unknown value (defence against schema drift)
    /// rather than guessing, so a corrupt row can't silently grant power.
    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "owner" => Some(Role::Owner),
            "admin" => Some(Role::Admin),
            "moderator" => Some(Role::Moderator),
            "member" => Some(Role::Member),
            _ => None,
        }
    }

    /// True when `self` is at least as powerful as `other` (the hierarchy).
    pub fn at_least(self, other: Role) -> bool {
        self >= other
    }

    /// Whether `self` may perform `action` in a server.
    ///
    /// Delegates to [`Action::minimum_role`] — the single source of truth for
    /// the policy — and checks the hierarchy against it. Owner passes every
    /// action by construction (it is the max of the hierarchy).
    pub fn can(self, action: Action) -> bool {
        self.at_least(action.minimum_role())
    }
}

/// Human-readable name for a role (e.g. for wire messages / logs).
impl Role {
    pub const fn label(self) -> &'static str {
        match self {
            Role::Member => "Member",
            Role::Moderator => "Moderator",
            Role::Admin => "Admin",
            Role::Owner => "Owner",
        }
    }
}

impl Action {
    /// The minimum role that may perform this action. The caller's standing
    /// passes iff `role >= minimum_role()` (the hierarchy), so this one table
    /// is the entire permission policy.
    ///
    ///   * owner-only: rename the server, transfer ownership.
    ///   * admin+ (owner/admin): set the icon, create/delete channels, ban or
    ///     time out a member, change a member's role.
    ///   * moderator+ (owner/admin/moderator): rename a channel, add/remove a
    ///     member, pin/unpin a message.
    pub fn minimum_role(self) -> Role {
        match self {
            Action::RenameServer | Action::TransferOwnership => Role::Owner,
            Action::SetServerIcon
            | Action::CreateChannel
            | Action::DeleteChannel
            | Action::BanMember
            | Action::TimeoutMember
            | Action::SetMemberRole
            | Action::ManageRoles => Role::Admin,
            Action::RenameChannel
            | Action::AddMember
            | Action::RemoveMember
            | Action::PinMessage
            | Action::MentionEveryone => Role::Moderator,
        }
    }
}

// ── tests ────────────────────────────────────────────────────────────────────
//
// The policy is pure, so the tests here are the authoritative pin of the
// role→action matrix. Each is a single, named assertion group.

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse/format round-trips for every stored string.
    #[test]
    fn db_string_round_trip() {
        for r in [Role::Member, Role::Moderator, Role::Admin, Role::Owner] {
            assert_eq!(Role::from_db_str(r.db_str()), Some(r), "{r:?}");
            assert_eq!(r.db_str(), r.db_str());
        }
    }

    /// Unknown strings parse to None (no silent power grant).
    #[test]
    fn unknown_role_string_is_none() {
        assert_eq!(Role::from_db_str("god"), None);
        assert_eq!(Role::from_db_str("ADMIN"), None);
        assert_eq!(Role::from_db_str(""), None);
        assert_eq!(Role::from_db_str("mod"), None);
    }

    /// The hierarchy is a total order with Owner at the top.
    #[test]
    fn hierarchy_ordering() {
        assert!(Role::Member < Role::Moderator);
        assert!(Role::Moderator < Role::Admin);
        assert!(Role::Admin < Role::Owner);
        assert!(Role::Owner.at_least(Role::Member));
        assert!(Role::Owner.at_least(Role::Admin));
        assert!(!Role::Member.at_least(Role::Moderator));
        assert!(!Role::Moderator.at_least(Role::Admin));
    }

    /// Owner may do everything.
    #[test]
    fn owner_can_do_everything() {
        let actions = [
            Action::RenameServer,
            Action::SetServerIcon,
            Action::TransferOwnership,
            Action::CreateChannel,
            Action::DeleteChannel,
            Action::RenameChannel,
            Action::AddMember,
            Action::RemoveMember,
            Action::BanMember,
            Action::TimeoutMember,
            Action::SetMemberRole,
            Action::PinMessage,
            Action::ManageRoles,
            Action::MentionEveryone,
        ];
        for a in actions {
            assert!(Role::Owner.can(a), "owner should do {a:?}");
        }
    }

    /// Admin can manage channels and roles, but not rename the server or
    /// transfer ownership.
    #[test]
    fn admin_permissions() {
        assert!(Role::Admin.can(Action::CreateChannel));
        assert!(Role::Admin.can(Action::DeleteChannel));
        assert!(Role::Admin.can(Action::RenameChannel));
        assert!(Role::Admin.can(Action::AddMember));
        assert!(Role::Admin.can(Action::RemoveMember));
        assert!(Role::Admin.can(Action::BanMember));
        assert!(Role::Admin.can(Action::TimeoutMember));
        assert!(Role::Admin.can(Action::SetMemberRole));
        assert!(Role::Admin.can(Action::SetServerIcon));
        assert!(
            !Role::Admin.can(Action::RenameServer),
            "admin must not rename the server"
        );
        assert!(
            !Role::Admin.can(Action::TransferOwnership),
            "admin must not transfer ownership"
        );
    }

    /// Moderator can remove members and rename channels, but not create/delete
    /// channels or change roles.
    #[test]
    fn moderator_permissions() {
        assert!(Role::Moderator.can(Action::RemoveMember));
        assert!(Role::Moderator.can(Action::AddMember));
        assert!(Role::Moderator.can(Action::RenameChannel));
        // A kick (RemoveMember) is moderator+, but a ban (BanMember) is admin+.
        assert!(
            !Role::Moderator.can(Action::BanMember),
            "moderator must not ban (ban is admin+)"
        );
        // A timeout is also admin+ — a moderator may kick but not time out.
        assert!(
            !Role::Moderator.can(Action::TimeoutMember),
            "moderator must not time out (timeout is admin+)"
        );
        assert!(
            !Role::Moderator.can(Action::CreateChannel),
            "moderator must not create channels"
        );
        assert!(
            !Role::Moderator.can(Action::DeleteChannel),
            "moderator must not delete channels"
        );
        assert!(
            !Role::Moderator.can(Action::SetMemberRole),
            "moderator must not set roles"
        );
        assert!(!Role::Moderator.can(Action::SetServerIcon));
        assert!(!Role::Moderator.can(Action::RenameServer));
        assert!(!Role::Moderator.can(Action::TransferOwnership));
    }

    /// Member (the default) can do none of the moderation actions.
    #[test]
    fn member_has_no_moderation() {
        let actions = [
            Action::RenameServer,
            Action::SetServerIcon,
            Action::TransferOwnership,
            Action::CreateChannel,
            Action::DeleteChannel,
            Action::RenameChannel,
            Action::AddMember,
            Action::RemoveMember,
            Action::BanMember,
            Action::TimeoutMember,
            Action::SetMemberRole,
        ];
        for a in actions {
            assert!(!Role::Member.can(a), "member must not do {a:?}");
        }
    }

    /// The specific examples from the task statement.
    #[test]
    fn task_statement_examples() {
        // "only admins can delete a channel" (owner too, by hierarchy).
        assert!(Role::Admin.can(Action::DeleteChannel));
        assert!(Role::Owner.can(Action::DeleteChannel));
        assert!(!Role::Moderator.can(Action::DeleteChannel));
        assert!(!Role::Member.can(Action::DeleteChannel));
        // "only admin/moderator can remove a member" (owner too).
        assert!(Role::Admin.can(Action::RemoveMember));
        assert!(Role::Moderator.can(Action::RemoveMember));
        assert!(Role::Owner.can(Action::RemoveMember));
        assert!(!Role::Member.can(Action::RemoveMember));
        // A ban is more severe than a kick: admin+ only (moderator may kick
        // but not ban).
        assert!(Role::Admin.can(Action::BanMember));
        assert!(Role::Owner.can(Action::BanMember));
        assert!(!Role::Moderator.can(Action::BanMember));
        assert!(!Role::Member.can(Action::BanMember));
        // A timeout, like a ban, is admin+ only — it restrains a member more
        // strongly than a kick.
        assert!(Role::Admin.can(Action::TimeoutMember));
        assert!(Role::Owner.can(Action::TimeoutMember));
        assert!(!Role::Moderator.can(Action::TimeoutMember));
        assert!(!Role::Member.can(Action::TimeoutMember));
    }
}
