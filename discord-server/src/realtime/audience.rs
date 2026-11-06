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
        out.extend(server_audience(db, server_id));
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Tell the people who care that `user_id`'s presence became `status`.
pub fn announce_presence(db: &Db, user_id: i64, status: &str) {
    hub().publish(
        &presence_audience(db, user_id),
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
        &server_audience(db, server_id),
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
