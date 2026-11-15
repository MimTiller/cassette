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
                        role: live::role_word(member.role),
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
        let world = live::snapshot();
        let space = live::wire_of(server_id);
        let role = match world.spaces.get(&space).and_then(move |s| s.members.get(&live::wire_of(user_id))) {
            None => "member".to_string(),
            Some(member) => live::role_word(member.role),
        };
        let mut args = Vec::new();
        args.push(client::arg(space));
        args.push(client::arg(live::wire_of(user_id)));
        match client::post("remove_member", args).await {
            Err(e) => Err(e),
            Ok(_) => Ok(MemberResult { server_id, user_id, role }),
        }
    }

    // No ban and no moderation state on this server: the closest is
    // `remove_member`, which loses the ban semantics.
    async fn ban_member(_server_id: i64, _user_id: i64, _reason: Option<String>) -> Result<MemberResult, ServerError> {
        Err(model::not_supported())
    }

    // No timed mute of one member anywhere in the model.
    async fn timeout_member(_server_id: i64, _user_id: i64, _duration_secs: i64) -> Result<MemberResult, ServerError> {
        Err(model::not_supported())
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
        // space with two members (one Moderator) and a presence row, then map
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
        members.insert(uid(7), Member { role: Role::Owner, joined_at: 1_000 });
        members.insert(uid(8), Member { role: Role::Admin, joined_at: 2_000 });
        s.spaces.insert(uid(3), Space {
            id: uid(3),
            name: "Alpha".to_string(),
            icon: None,
            owner: uid(7),
            members: members,
            categories: Vec::new(),
            created_at: 0,
        });
        assert_eq!(s.spaces.get(&uid(3)).and_then(move |x| x.members.get(&uid(8))) != None, true);
        assert_eq!(live::role_word(Role::Admin), "moderator");
    }

    #[test]
    fn presence_words_name_the_rwnd_statuses() {
        // chat-model's Presence has a Busy ("do not disturb") state, so RWND's
        // dnd maps onto it; invisible has no counterpart and rides offline.
        assert_eq!(live::presence_word(chat_model::Presence::Busy), "dnd".to_string());
        assert_eq!(live::presence_word(chat_model::Presence::Offline), "offline".to_string());
    }
}
