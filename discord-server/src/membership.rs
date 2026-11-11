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

    /// The audience of a membership frame is the server's members recomputed
    /// after the write, so a user whose row was just removed (ban, kick,
    /// leave) hears nothing further from that server: their frames stop.
    #[test]
    fn a_removed_member_stops_receiving_the_servers_frames() {
        let _guard = crate::e2e_lock::guard();
        let (_d, db) = setup();
        db.lock()
            .unwrap()
            .execute_batch(
                "INSERT INTO users (id, username, email, created_at) VALUES (1,'alice','a@x',1);
                 INSERT INTO users (id, username, email, created_at) VALUES (2,'bob','b@x',1);
                 INSERT INTO servers (id, name, owner_id, icon_url, created_at) VALUES (1,'S1',1,NULL,1);
                 INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES (1,1,'owner',1);
                 INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES (1,2,'member',1);",
            )
            .unwrap();
        let hub = crate::realtime::hub();
        hub.reset();
        let (_id, mut alice_rx) = hub.register(1);
        let (_id2, mut bob_rx) = hub.register(2);

        // Kick bob: the membership row goes first, then the access change.
        db.lock()
            .unwrap()
            .execute("DELETE FROM server_memberships where server_id = 1 and user_id = 2", ())
            .unwrap();
        access_changed(&db, 1, AccessChange::MemberBanned { user_id: 2 });
        let bob_frames = collect(&mut bob_rx);
        assert!(
            bob_frames.iter().all(|m| !matches!(m, WsServerMsg::MemberChanged { server_id: 1, user_id: 2, .. })),
            "bob got the very frame announcing his own removal"
        );
        let alice_frames = collect(&mut alice_rx);
        assert!(
            alice_frames.iter().any(|m| matches!(m, WsServerMsg::MemberChanged { server_id: 1, user_id: 2, joined: false, .. })),
            "alice is told bob left"
        );
        // Nothing further about that server reaches bob: another member joins.
        db.lock()
            .unwrap()
            .execute_batch(
                "INSERT INTO users (id, username, email, created_at) VALUES (9,'zoe','z@x',1);
                 INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES (1,9,'member',1);",
            )
            .unwrap();
        access_changed(&db, 1, AccessChange::MemberJoined { user_id: 9 });
        assert!(collect(&mut bob_rx).is_empty(), "the ex-member receives nothing more");
        assert!(
            collect(&mut alice_rx).iter().any(|m| matches!(m, WsServerMsg::MemberChanged { server_id: 1, user_id: 9, joined: true, .. })),
            "alice sees zoe join"
        );
    }

    /// A user who never joined the server is not in its audience at all —
    /// neither roster frames nor refetch pings.
    #[test]
    fn an_outsider_receives_nothing_from_the_server() {
        let _guard = crate::e2e_lock::guard();
        let (_d, db) = setup();
        db.lock()
            .unwrap()
            .execute_batch(
                "INSERT INTO users (id, username, email, created_at) VALUES (1,'alice','a@x',1);
                 INSERT INTO users (id, username, email, created_at) VALUES (42,'ghost','g@x',1);
                 INSERT INTO servers (id, name, owner_id, icon_url, created_at) VALUES (1,'S1',1,NULL,1);
                 INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES (1,1,'owner',1);",
            )
            .unwrap();
        let hub = crate::realtime::hub();
        hub.reset();
        let (_id, mut alice_rx) = hub.register(1);
        let (_id2, mut ghost_rx) = hub.register(42);

        access_changed(&db, 1, AccessChange::EveryoneChanged);
        access_changed(&db, 1, AccessChange::ChannelOverridesChanged { channel_id: 1 });
        assert!(collect(&mut ghost_rx).is_empty(), "the outsider got nothing");
        assert_eq!(collect(&mut alice_rx).len(), 2, "members get both refetch pings");
    }

    /// A change in one server reaches only that server's members: members of
    /// another server hear nothing, so roster pings can't be used to spy across
    /// servers.
    #[test]
    fn a_servers_frames_never_reach_another_servers_members() {
        let _guard = crate::e2e_lock::guard();
        let (_d, db) = setup();
        db.lock()
            .unwrap()
            .execute_batch(
                "INSERT INTO users (id, username, email, created_at) VALUES (1,'alice','a@x',1);
                 INSERT INTO users (id, username, email, created_at) VALUES (3,'carol','c@x',1);
                 INSERT INTO servers (id, name, owner_id, icon_url, created_at) VALUES (1,'S1',1,NULL,1),(2,'S2',3,NULL,1);
                 INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES (1,1,'owner',1),(2,3,'owner',1);",
            )
            .unwrap();
        let hub = crate::realtime::hub();
        hub.reset();
        let (_id, mut alice_rx) = hub.register(1);
        access_changed(&db, 2, AccessChange::EveryoneChanged);
        access_changed(&db, 2, AccessChange::MemberJoined { user_id: 3 });
        assert!(collect(&mut alice_rx).is_empty(), "server 1's member got no server 2 pings");
    }

    /// A member who left and was invited back starts receiving the server's
    /// frames again: the audience is recomputed from the rows, not remembered.
    #[test]
    fn a_readded_member_hears_the_servers_frames_again() {
        let _guard = crate::e2e_lock::guard();
        let (_d, db) = setup();
        db.lock()
            .unwrap()
            .execute_batch(
                "INSERT INTO users (id, username, email, created_at) VALUES (1,'alice','a@x',1);
                 INSERT INTO users (id, username, email, created_at) VALUES (2,'bob','b@x',1);
                 INSERT INTO servers (id, name, owner_id, icon_url, created_at) VALUES (1,'S1',1,NULL,1);
                 INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES (1,1,'owner',1),(1,2,'member',1);",
            )
            .unwrap();
        let hub = crate::realtime::hub();
        hub.reset();
        let (_id, mut bob_rx) = hub.register(2);
        db.lock()
            .unwrap()
            .execute("DELETE FROM server_memberships WHERE server_id = 1 AND user_id = 2".into(), ())
            .unwrap();
        access_changed(&db, 1, AccessChange::MemberRemoved { user_id: 2 });
        db.lock()
            .unwrap()
            .execute("INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES (1,2,'member',2)".into(), ())
            .unwrap();
        access_changed(&db, 1, AccessChange::MemberJoined { user_id: 2 });
        let frames = collect(&mut bob_rx);
        assert!(
            frames.iter().any(|m| matches!(m, WsServerMsg::MemberChanged { server_id: 1, user_id: 2, joined: true, .. })),
            "the re-added member got the join frame"
        );
    }

    /// A ban removes the membership before the change goes through `access_changed`,
    /// so the audience is recomputed without the banned user: they get no
    /// frame at all, and nothing further from that server.
    #[test]
    fn a_banned_member_hears_nothing_from_their_old_server() {
        let _guard = crate::e2e_lock::guard();
        let (_d, db) = setup();
        db.lock()
            .unwrap()
            .execute_batch(
                "INSERT INTO users (id, username, email, created_at) VALUES (1,'alice','a@x',1),(2,'bob','b@x',1);
                 INSERT INTO servers (id, name, owner_id, created_at) VALUES (1,'S1',1,1);
                 INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES (1,1,'owner',1),(1,2,'member',1);",
            )
            .unwrap();
        let hub = crate::realtime::hub();
        hub.reset();
        let (_id, mut bob_rx) = hub.register(2);
        db.lock()
            .unwrap()
            .execute("DELETE FROM server_memberships WHERE server_id = 1 AND user_id = 2".into(), ())
            .unwrap();
        access_changed(&db, 1, AccessChange::MemberBanned { user_id: 2 });
        access_changed(&db, 1, AccessChange::EveryoneChanged);
        assert!(collect(&mut bob_rx).is_empty(), "the banned member got no frames at all");
    }

    /// Leaving on one's own is the same audience rule as a removal: after the
    /// row is deleted the leaver hears nothing more from that server.
    #[test]
    fn a_member_who_left_on_their_own_hears_nothing_further() {
        let _guard = crate::e2e_lock::guard();
        let (_d, db) = setup();
        db.lock()
            .unwrap()
            .execute_batch(
                "INSERT INTO users (id, username, email, created_at) VALUES (1,'alice','a@x',1),(2,'bob','b@x',1);
                 INSERT INTO servers (id, name, owner_id, created_at) VALUES (1,'S1',1,1);
                 INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES (1,1,'owner',1),(1,2,'member',1);",
            )
            .unwrap();
        let hub = crate::realtime::hub();
        hub.reset();
        let (_id, mut bob_rx) = hub.register(2);
        db.lock()
            .unwrap()
            .execute("DELETE FROM server_memberships WHERE server_id = 1 AND user_id = 2".into(), ())
            .unwrap();
        access_changed(&db, 1, AccessChange::MemberLeft { user_id: 2 });
        access_changed(&db, 1, AccessChange::EveryoneChanged);
        assert!(collect(&mut bob_rx).is_empty(), "the leaver got no frames at all");
    }

    fn collect(rx: &mut tokio::sync::mpsc::Receiver<WsServerMsg>) -> Vec<WsServerMsg> {
        let mut out: Vec<WsServerMsg> = vec![];
        loop {
            match rx.try_recv() {
                Ok(m) => out.push(m),
                Err(_) => break,
            }
        }
        out
    }
}
