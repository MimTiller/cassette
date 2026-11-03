//! The one place a change to who can read a server's channels goes through
//! (server-only).
//!
//! Joining, leaving, being removed or banned, gaining or losing a role, a role's
//! permissions changing and a channel's overwrites changing all decide which
//! channels a member can read. Each of those writes its change to the database
//! and then calls [`access_changed`], which tells the server's open clients to
//! refetch. When channel content is end-to-end encrypted, this is also where a
//! channel's encryption group gains or loses members, so a change that can
//! affect read access must come through here and nowhere else.

use crate::db::Db;

/// A change that may alter which channels of a server a member can read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessChange {
    /// A user became a member (an invite, or added by a moderator).
    MemberJoined { user_id: i64 },
    /// A member left on their own.
    MemberLeft { user_id: i64 },
    /// A moderator removed a member.
    MemberRemoved { user_id: i64 },
    /// A moderator banned a member, removing them.
    MemberBanned { user_id: i64 },
    /// A member gained or lost a role.
    RolesChanged { user_id: i64 },
    /// A role was created, renamed, recoloured, re-ranked or had its
    /// permissions changed.
    RoleEdited { role_id: i64 },
    /// A role was deleted; its holders lose what it granted.
    RoleDeleted { role_id: i64 },
    /// What @everyone may do changed.
    EveryoneChanged,
    /// A channel's permission overwrites changed.
    ChannelOverridesChanged { channel_id: i64 },
}

/// Act on `change` in `server_id`, after it has been written. Members see a
/// roster change as a `MemberChanged` frame; every other change makes clients
/// refetch the roles, permissions and visible channels (`RolesChanged`).
pub fn access_changed(db: &Db, server_id: i64, change: AccessChange) {
    match change {
        AccessChange::MemberJoined { user_id } => crate::realtime::announce_membership(db, server_id, user_id, true),
        AccessChange::MemberLeft { user_id }
        | AccessChange::MemberRemoved { user_id }
        | AccessChange::MemberBanned { user_id } => crate::realtime::announce_membership(db, server_id, user_id, false),
        AccessChange::RolesChanged { .. }
        | AccessChange::RoleEdited { .. }
        | AccessChange::RoleDeleted { .. }
        | AccessChange::EveryoneChanged
        | AccessChange::ChannelOverridesChanged { .. } => crate::server_roles::announce(db, server_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server_settings::WsServerMsg;

    fn setup() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let migrations = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let db = Db::migrate(&dir.path().join("t.db"), &migrations).unwrap();
        (dir, db)
    }

    /// Every variant reaches the server's members as the frame that makes
    /// their clients refetch: membership changes as `MemberChanged`, the rest
    /// as `RolesChanged`.
    #[test]
    fn each_change_reaches_the_members() {
        let _guard = crate::e2e_lock::guard();
        let (_d, db) = setup();
        db.lock()
            .unwrap()
            .execute_batch(
                "INSERT INTO users (id, username, email, created_at) VALUES (1,'alice','a@x',1);
                 INSERT INTO servers (id, name, owner_id, icon_url, created_at) VALUES (1,'S1',1,NULL,1);
                 INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES (1,1,'owner',1);",
            )
            .unwrap();
        let (owner, server_id) = (1, 1);
        let hub = crate::realtime::hub();
        hub.reset();
        let (_id, mut rx) = hub.register(owner);
        let next = |rx: &mut tokio::sync::mpsc::Receiver<WsServerMsg>| loop {
            match rx.try_recv() {
                Ok(m @ (WsServerMsg::MemberChanged { .. } | WsServerMsg::RolesChanged { .. })) => break Some(m),
                Ok(_) => continue,
                Err(_) => break None,
            }
        };

        for (change, joined) in [
            (AccessChange::MemberJoined { user_id: 9 }, true),
            (AccessChange::MemberLeft { user_id: 9 }, false),
            (AccessChange::MemberRemoved { user_id: 9 }, false),
            (AccessChange::MemberBanned { user_id: 9 }, false),
        ] {
            access_changed(&db, server_id, change);
            match next(&mut rx) {
                Some(WsServerMsg::MemberChanged { server_id: s, user_id: 9, joined: j, .. }) => {
                    assert_eq!((s, j), (server_id, joined), "{change:?}")
                }
                other => panic!("{change:?}: expected MemberChanged, got {other:?}"),
            }
        }
        for change in [
            AccessChange::RolesChanged { user_id: 9 },
            AccessChange::RoleEdited { role_id: 1 },
            AccessChange::RoleDeleted { role_id: 1 },
            AccessChange::EveryoneChanged,
            AccessChange::ChannelOverridesChanged { channel_id: 1 },
        ] {
            access_changed(&db, server_id, change);
            assert!(
                matches!(next(&mut rx), Some(WsServerMsg::RolesChanged { server_id: s, .. }) if s == server_id),
                "{change:?}: expected RolesChanged"
            );
        }
    }
}
