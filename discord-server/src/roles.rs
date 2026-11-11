//! Channel kinds and the stored membership level.
//!
//! What a member may do is decided by [`crate::permissions`]. The
//! `server_memberships.role` column ([`Role`]) only still marks the owner's
//! row; the old admin and moderator levels became roles in
//! `0022_permissions.sql` and no longer grant anything.

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

/// The stored `server_memberships.role` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Role {
    Member,
    /// An old level, kept readable; it grants nothing.
    Moderator,
    /// An old level, kept readable; it grants nothing.
    Admin,
    /// The server's owner (also `servers.owner_id`).
    Owner,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::{ServerContext, Rank, Permissions};
    use crate::db::Db;
    use std::path::Path;
    use tempfile::TempDir;

    fn setup() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let migrations = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let db = Db::migrate(&dir.path().join("t.db"), &migrations).unwrap();
        (dir, db)
    }

    // A legacy 'admin'/'moderator' string in `server_memberships.role` grants
    // nothing on its own, and ownership comes from `servers.owner_id`, not the
    // stored role string.
    #[test]
    fn a_legacy_admin_string_grants_nothing_and_owner_is_by_owner_id() {
        let (_dir, db) = setup();
        db.lock()
            .unwrap()
            .execute_batch(
                "INSERT INTO users (id, username, email, created_at) VALUES (1,'owner','o@x',1);
                 INSERT INTO users (id, username, email, created_at) VALUES (2,'legacy','l@x',1);
                 INSERT INTO servers (id, name, owner_id, icon_url, created_at) VALUES (1,'S1',1,NULL,1);
                 INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES (1,1,'owner',1);
                 INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES (1,2,'admin',1);",
            )
            .unwrap();
        let ctx = ServerContext::load(&db, 1).unwrap();
        // @everyone's power for this server (default 31 in 0022_permissions.sql).
        let everyone = ctx.everyone();

        // User 2 stores the legacy 'admin' string but is NOT the owner: they hold
        // no named roles, so they get exactly @everyone — the string adds nothing.
        let legacy = ctx.member(2).unwrap();
        assert!(!legacy.is_owner, "a stored 'admin' string does not make them the owner");
        assert_eq!(legacy.server, everyone, "the legacy role string grants nothing");
        assert_eq!(legacy.rank, Rank::Everyone, "the legacy string sets no rank");

        // User 1 is the owner because `servers.owner_id` says so (their role
        // string matches but is not what decides it).
        let owner = ctx.member(1).unwrap();
        assert!(owner.is_owner, "owner_id marks the owner");
        assert_eq!(owner.server, Permissions::ALL, "the owner holds everything");
    }

    #[test]
    fn db_string_round_trip() {
        for r in [Role::Member, Role::Moderator, Role::Admin, Role::Owner] {
            assert_eq!(Role::from_db_str(r.db_str()), Some(r), "{r:?}");
        }
        for k in [ChannelKind::Text, ChannelKind::Voice, ChannelKind::Category] {
            assert_eq!(ChannelKind::from_db_str(k.db_str()), Some(k), "{k:?}");
        }
    }

    #[test]
    fn unknown_strings_are_none() {
        assert_eq!(Role::from_db_str("god"), None);
        assert_eq!(Role::from_db_str("ADMIN"), None);
        assert_eq!(ChannelKind::from_db_str("stage"), None);
    }
}
