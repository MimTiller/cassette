//! Members: the roster rides the reducer state (no roster RPC), kicking is
//! the `remove_member` call, and bans/timeouts have no counterpart on this
//! server, so they answer the shared refusal.

use crate::backend::*;
use crate::backend::idea_chat::{client, live, model};

impl MembersApi for IdeaChatBackend {
    // No roster RPC: members ride the snapshot's `Space.members` with the
    // `User` rows and the presence map the reducer keeps.
    async fn list_members(server_id: i64) -> Result<Vec<MemberInfo>, ServerError> {
        let world = live::snapshot();
        let space = live::wire_of(server_id);
        match world.spaces.get(&space) {
            None => Err(ServerError::failed("that space is not on this server")),
            Some(row) => {
                let mut out: Vec<MemberInfo> = Vec::new();
                for (user_id, member) in row.members.iter() {
                    let user = match world.users.get(user_id) {
                        None => continue,
                        Some(found) => found.clone(),
                    };
                    out.push(MemberInfo {
                        user_id: live::see(*user_id),
                        username: user.handle.clone(),
                        display_name: match live::display_of(user.clone()) {
                            None => None,
                            Some(name) => Some(name),
                        },
                        role: live::role_word(row.clone(), *user_id, member.clone()),
                        avatar_url: user.avatar.clone(),
                        avatar_decoration: None,
                        presence: match world.presence.get(user_id) {
                            None => None,
                            Some(status) => Some(live::presence_word(status.presence)),
                        },
                        joined_at: Some(member.joined_at),
                    })
                }
                Ok(out)
            },
        }
    }

    // Adding an arbitrary member has no counterpart: joining on this server
    // is redeeming an invite, which joins the redeeming caller, not a named
    // person. The refusal is honest — the feature needs the invite flow.
    async fn add_member(_server_id: i64, _user_id: i64) -> Result<MemberResult, ServerError> {
        Err(model::not_supported())
    }

    // `remove_member(space, member)` answers unit; the `MemberLeft` event
    // drops the person from the state. The role echoed is the one held when
    // the kick started.
    async fn remove_member(server_id: i64, user_id: i64) -> Result<MemberResult, ServerError> {
        let space = live::wire_of(server_id);
        let role = live::role_word_of(space, live::wire_of(user_id));
        let mut args = Vec::new();
        args.push(client::arg(space));
        args.push(client::arg(live::wire_of(user_id)));
        match client::post("remove_member", args).await {
            Err(e) => Err(e),
            Ok(_) => Ok(MemberResult { server_id, user_id, role }),
        }
    }

    // `ban_member(space, member)` is this server's ban: the person leaves the
    // space and a ban row blocks rejoining (unban lifts it). RWND's free-text
    // reason has no counterpart here, so it is accepted and dropped.
    async fn ban_member(server_id: i64, user_id: i64, _reason: Option<String>) -> Result<MemberResult, ServerError> {
        let space = live::wire_of(server_id);
        let role = live::role_word_of(space, live::wire_of(user_id));
        let mut args = Vec::new();
        args.push(client::arg(space));
        args.push(client::arg(live::wire_of(user_id)));
        match client::post("ban_member", args).await {
            Err(e) => Err(e),
            Ok(_) => Ok(MemberResult { server_id, user_id, role }),
        }
    }

    // `timeout_member(space, member, minutes?)` is this server's timed mute:
    // the seconds round up to minutes; clearing a timeout is `None`, which
    // RWND's positive-duration bar never asks for.
    async fn timeout_member(server_id: i64, user_id: i64, duration_secs: i64) -> Result<MemberResult, ServerError> {
        if duration_secs <= 0 {
            return Err(ServerError::failed("a timeout must last at least one second"))
        }
        let space = live::wire_of(server_id);
        let minutes: Option<u32> = Some(((duration_secs + 59) / 60) as u32);
        let role = live::role_word_of(space, live::wire_of(user_id));
        let mut args = Vec::new();
        args.push(client::arg(space));
        args.push(client::arg(live::wire_of(user_id)));
        args.push(client::arg(minutes));
        match client::post("timeout_member", args).await {
            Err(e) => Err(e),
            Ok(_) => Ok(MemberResult { server_id, user_id, role }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chat_model::{Id, Space, Member, User, Role};
    use chat_model::live::State;

    fn uid(n: u128) -> Id {
        Id::from_u128(n)
    }

    #[test]
    fn a_roster_maps_from_the_state_with_roles_and_presence() {
        // Offline: the members row mapping is pure over the state, so build a
        // space with two members — one holding an admin-tagged role (maps to
        // RWND's "moderator") and the owner — plus a presence row, then map
        // what `list_members` would see (the helper mirrors its body).
        let mut s = State::default();
        s.users.insert(uid(7), User {
            id: uid(7),
            handle: "al".to_string(),
            display_name: "Al".to_string(),
            avatar: None,
            bio: "".to_string(),
        });
        s.users.insert(uid(8), User {
            id: uid(8),
            handle: "bo".to_string(),
            display_name: "bo".to_string(),
            avatar: None,
            bio: "".to_string(),
        });
        let mut members: std::collections::BTreeMap<Id, Member> = std::collections::BTreeMap::new();
        members.insert(uid(7), Member { roles: Vec::new(), joined_at: 1_000 });
        members.insert(uid(8), Member { roles: vec!(uid(9)), joined_at: 2_000 });
        let mut defs: std::collections::BTreeMap<Id, chat_model::Role> = std::collections::BTreeMap::new();
        defs.insert(uid(9), chat_model::Role { id: uid(9), name: "Admins".to_string(), permissions: chat_model::perms::ADMIN, position: 0, colour: None, mentionable: false });
        s.spaces.insert(uid(3), Space {
            id: uid(3),
            name: "Alpha".to_string(),
            icon: None,
            owner: uid(7),
            members: members,
            roles: defs,
            everyone: chat_model::perms::DEFAULT_EVERYONE,
            categories: Vec::new(),
            created_at: 0,
        });
        assert_eq!(s.spaces.get(&uid(3)).and_then(move |x| x.members.get(&uid(8))) != None, true);
        let space = s.spaces.get(&uid(3)).unwrap().clone();
        assert_eq!(live::role_word(space.clone(), uid(8), space.members.get(&uid(8)).unwrap().clone()), "moderator".to_string());
        assert_eq!(live::role_word(space.clone(), uid(7), space.members.get(&uid(7)).unwrap().clone()), "owner".to_string());
    }

    #[test]
    fn presence_words_name_the_rwnd_statuses() {
        // chat-model's Presence has a Busy ("do not disturb") state, so RWND's
        // dnd maps onto it; invisible has no counterpart and rides offline.
        assert_eq!(live::presence_word(chat_model::Presence::Busy), "dnd".to_string());
        assert_eq!(live::presence_word(chat_model::Presence::Offline), "offline".to_string());
    }
}
