//! Audiences: who should hear which event, and the presence/member frames
//! they fan out.

use crate::db::Db;
use crate::permissions::{self, Permissions};
use crate::server_settings::{WsServerMsg, next_frame_seq};
use crate::realtime::hub;

/// The user ids that belong to `server_id`.
pub fn server_audience(db: &Db, server_id: i64) -> Vec<i64> {
    db.server_members(server_id)
        .into_iter()
        .map(|(id, _, _)| id)
        .collect()
}

/// Everyone who can see channel `channel_id`. Empty when the channel doesn't
/// exist.
pub fn channel_audience(db: &Db, channel_id: i64) -> Vec<i64> {
    let Some((_id, server_id, _name, _kind, _pos, _parent)) = db.channel_full(channel_id) else {
        return Vec::new();
    };
    permissions::ServerContext::load(db, server_id)
        .map(|ctx| ctx.members_with(channel_id, Permissions::VIEW_CHANNEL))
        .unwrap_or_default()
}

/// Everyone who should hear about `user_id`'s presence: the user themself and
/// every member of every server they belong to.
pub fn presence_audience(db: &Db, user_id: i64) -> Vec<i64> {
    let mut out = vec![user_id];
    for (server_id, _, _) in db.servers_for_user(user_id) {
        out.extend(server_audience(&db, server_id));
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Tell the people who care that `user_id`'s presence became `status`.
pub fn announce_presence(db: &Db, user_id: i64, status: &str) {
    hub().publish(
        &presence_audience(&db, user_id),
        &WsServerMsg::Presence {
            user_id,
            status: status.to_string(),
            seq: next_frame_seq(),
        },
    );
}

/// What others see of a user: their picked status (`status_choice`) and
/// whether they are active. Invisible reads as offline; a picked Idle or Do
/// Not Disturb holds whatever the activity.
pub fn shown_presence(choice: &str, active: bool) -> &'static str {
    match choice {
        "invisible" => "offline",
        "dnd" => "dnd",
        "idle" => "away",
        _ if active => "online",
        _ => "away",
    }
}

/// Store `user_id`'s shown presence and tell their audience when it changed.
pub fn refresh_presence(db: &Db, user_id: i64, active: bool) {
    let shown = shown_presence(&db.status_choice(user_id), active);
    if db.set_presence(user_id, shown).is_ok_and(|changed| changed) {
        announce_presence(db, user_id, shown);
    }
}

/// Tell the other members of `server_id` that `user_id` joined (`joined:
/// true`) or left (`joined: false`). Called by the join (invite redemption)
/// and leave (kick/ban) paths so the roster — and the `MEMBERS — N` count —
/// updates live on the members' open clients. The audience is recomputed from
/// the (already-persisted) membership after the change, so a join reaches the
/// existing members and a leave reaches the remaining ones. No roster data
/// rides on the frame; the client re-fetches `list_members`.
pub fn announce_membership(db: &Db, server_id: i64, user_id: i64, joined: bool) {
    hub().publish(
        &server_audience(&db, server_id),
        &WsServerMsg::MemberChanged {
            server_id,
            user_id,
            joined,
            seq: next_frame_seq(),
        },
    );
}

#[cfg(test)]
mod presence_tests {
    use super::shown_presence;

    #[test]
    fn picked_status_overrides_activity() {
        assert_eq!(shown_presence("online", true), "online");
        assert_eq!(shown_presence("online", false), "away");
        assert_eq!(shown_presence("idle", true), "away");
        assert_eq!(shown_presence("dnd", false), "dnd");
        assert_eq!(shown_presence("invisible", true), "offline");
    }
}

#[cfg(test)]
mod audience_tests {
    use super::*;
    use crate::realtime::test_fixtures::{setup, with_private_channel};

    /// A non-member is never in a server's audience, so never in a presence
    /// audience (which is built from the servers the user belongs to).
    #[test]
    fn a_non_member_is_in_no_audience() {
        let (_dir, db) = setup();
        // user 4 is in no server
        assert_eq!(presence_audience(&db, 4), vec![4], "only themself");
        assert_eq!(server_audience(&db, 1), vec![1, 2], "server 1's members only");
        assert!(!server_audience(&db, 1).contains(&4i64));
        assert!(!server_audience(&db, 1).contains(&3i64), "owner of the OTHER server");
        // announce_presence for a non-member reaches only themself: nobody of
        // server 1 hears their presence frames.
        let _guard = crate::e2e_lock::guard();
        let hub = crate::realtime::hub();
        hub.reset();
        let (_, mut alice_rx) = hub.register(1);
        let (_, mut bob_rx) = hub.register(2);
        announce_presence(&db, 4, "online");
        assert!(alice_rx.try_recv().is_err(), "alice hears nothing about user 4");
        assert!(bob_rx.try_recv().is_err(), "bob hears nothing about user 4");
    }

    /// A private channel's audience holds only members allowed to see it:
    /// here @everyone is denied, so even the owner is out unless the channel
    /// allows them.
    #[test]
    fn a_private_channels_audience_holds_only_whoever_can_see_it() {
        let (_dir, db) = setup();
        with_private_channel(&db);
        // channel 4: @everyone denied; bob (2) has no allow, alice (1) is
        // owner and the owner bit is implicit in `overwrites`.
        assert_eq!(channel_audience(&db, 4), vec![1], "only the owner sees it");
        assert_eq!(channel_audience(&db, 1), vec![1, 2], "the public channel reaches both");
    }

    /// Presence audience is the union over the user's servers: someone in two
    /// servers is heard by both rosters (deduped), and a member who is
    /// removed from a server drops that server's members from their audience.
    #[test]
    fn presence_audience_follows_membership() {
        let (_dir, db) = setup();
        // bob (2) starts in server 1 only
        assert_eq!(presence_audience(&db, 2), vec![1, 2], "bob's audience is server 1's roster");
        db.lock().unwrap().execute(
            "INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES (2,2,'member',1)".into(),
            (),
        ).unwrap();
        assert_eq!(presence_audience(&db, 2), vec![1, 2, 3], "now both rosters, deduped");
        db.lock().unwrap().execute(
            "DELETE FROM server_memberships WHERE user_id = 2 AND server_id = 1".into(),
            (),
        ).unwrap();
        assert_eq!(presence_audience(&db, 2), vec![2, 3], "alice no longer hears bob");
    }

    #[test]
    fn audience_of_a_missing_channel_is_empty() {
        let (_dir, db) = setup();
        assert!(channel_audience(&db, 99).is_empty(), "no channel 99: nobody hears it");
    }

    #[test]
    fn announce_membership_follows_the_audience_rule() {
        let _guard = crate::e2e_lock::guard();
        let (_dir, db) = setup();
        let hub = crate::realtime::hub();
        hub.reset();
        let (_, mut bob_rx) = hub.register(2);
        let (_, mut ghost_rx) = hub.register(4);
        crate::membership::access_changed(
            &db,
            1,
            crate::membership::AccessChange::MemberJoined { user_id: 2 },
        );
        assert!(
            matches!(
                bob_rx.try_recv(),
                Ok(crate::server_settings::WsServerMsg::MemberChanged { server_id: 1, .. })
            ),
            "bob is in the audience"
        );
        assert!(ghost_rx.try_recv().is_err(), "the non-member got nothing");
    }
}
