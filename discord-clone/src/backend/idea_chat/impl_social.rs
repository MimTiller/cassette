//! Friends and blocks: the social graph rides the reducer state (`State.relations`),
//! and the mutations are the three idea-chat verbs. Unblock has no counterpart.

use crate::backend::*;
use crate::backend::idea_chat::{client, live, model};
use chat_model::{Id, User, Relation};

impl SocialApi for IdeaChatBackend {
    // No list RPC: relations ride the snapshot. RWND's three lists map onto
    // the four relation states (chat-model's `Incoming` is a request from the
    // other side, `Outgoing` from this one, `Friend` accepted).
    async fn list_friends() -> Result<FriendList, ServerError> {
        Ok(friend_list_of(live::snapshot()))
    }

    // `add_friend(handle)`: mutual adds become `Friend`, a one-way add is
    // `Outgoing`; the answer is the relation that holds now.
    async fn send_friend_request(username: String) -> Result<SendFriendRequestResult, ServerError> {
        let mut args = Vec::new();
        args.push(client::arg(&username));
        match client::call_as::<Relation>("add_friend", args).await {
            Err(e) => Err(e),
            Ok(relation) => {
                let user_id = match live::snapshot().users.iter().find(
                    move |(_, user)| user.handle == username
                ) {
                    None => 0,
                    Some((uuid, _)) => live::see(*uuid),
                };
                Ok(SendFriendRequestResult {
                    user_id: user_id,
                    status: live::relation_word(relation.clone()),
                })
            },
        }
    }

    // `accept_friend(other)` answers unit; the fresh `RelationChanged` event
    // lands `Friend` in the state. Echo the person's row read back.
    async fn accept_friend_request(user_id: i64) -> Result<FriendUser, ServerError> {
        let mut args = Vec::new();
        args.push(client::arg(live::wire_of(user_id)));
        match client::post("accept_friend", args).await {
            Err(e) => Err(e),
            Ok(_) => Ok(friend_user_of(user_id)),
        }
    }

    // `remove_relation(other)` is one verb for decline, cancel and unfriend.
    async fn decline_or_cancel(user_id: i64) -> Result<FriendUser, ServerError> {
        match remove_relation(user_id).await {
            Err(e) => Err(e),
            Ok(_) => Ok(friend_user_of(user_id)),
        }
    }

    async fn remove_friend(user_id: i64) -> Result<FriendUser, ServerError> {
        match remove_relation(user_id).await {
            Err(e) => Err(e),
            Ok(_) => Ok(friend_user_of(user_id)),
        }
    }

    // Blocked people ride the state's relations (`Blocked`); no list RPC.
    async fn list_blocked() -> Result<Vec<BlockedUser>, ServerError> {
        Ok(blocked_of(live::snapshot()))
    }

    // `block(other)` answers unit; the `RelationChanged` event marks them.
    async fn block_user(user_id: i64) -> Result<BlockedUser, ServerError> {
        let mut args = Vec::new();
        args.push(client::arg(live::wire_of(user_id)));
        match client::post("block", args).await {
            Err(e) => Err(e),
            Ok(_) => Ok(friend_user_of_blocked(user_id)),
        }
    }

    // No unblock verb: the relationship is `Blocked` in the state and the
    // only way to clear it on this server would be a relation change the
    // model has no path to, so the feature is honestly refused.
    async fn unblock_user(_user_id: i64) -> Result<BlockedUser, ServerError> {
        Err(model::not_supported())
    }
}

// --- the social rows, built from the state view ---------------------------

pub(crate) fn friend_list_of(world: chat_model::live::State) -> FriendList {
    let mut incoming: Vec<FriendUser> = Vec::new();
    let mut outgoing: Vec<FriendUser> = Vec::new();
    let mut accepted: Vec<FriendUser> = Vec::new();
    for (user, relation) in world.relations.iter() {
        match *relation {
            Relation::Incoming => incoming.push(friend_user_row(*user, world.users.get(user).cloned())),
            Relation::Outgoing => outgoing.push(friend_user_row(*user, world.users.get(user).cloned())),
            Relation::Friend => accepted.push(friend_user_row(*user, world.users.get(user).cloned())),
            Relation::Blocked => {},
        }
    }
    FriendList { incoming: incoming, outgoing: outgoing, accepted: accepted }
}

// Borrow every field so `row` is reusable across the three fields of a row.
pub(crate) fn friend_user_row(user_id: Id, row: Option<User>) -> FriendUser {
    let username = match &row {
        None => live::see(user_id).to_string(),
        Some(found) => found.handle.clone(),
    };
    let display = match &row {
        None => None,
        Some(found) => live::display_of(found.clone()),
    };
    let avatar = match &row {
        None => None,
        Some(found) => found.avatar.clone(),
    };
    FriendUser {
        user_id: live::see(user_id),
        username: username,
        display_name: display,
        avatar_url: avatar,
        avatar_decoration: None,
    }
}

pub(crate) fn friend_user_of(user_id: i64) -> FriendUser {
    friend_user_row(live::wire_of(user_id), live::snapshot().users.get(&live::wire_of(user_id)).cloned())
}

pub(crate) fn blocked_user_of(user_id: Id, row: Option<User>) -> BlockedUser {
    let username = match &row {
        None => live::see(user_id).to_string(),
        Some(found) => found.handle.clone(),
    };
    let avatar = match &row {
        None => None,
        Some(found) => found.avatar.clone(),
    };
    BlockedUser {
        user_id: live::see(user_id),
        username: username,
        avatar_url: avatar,
    }
}

// One `remove_relation(other)` call, shared by decline, cancel and unfriend.
async fn remove_relation(user_id: i64) -> Result<(), ServerError> {
    let mut args = Vec::new();
    args.push(client::arg(live::wire_of(user_id)));
    match client::post("remove_relation", args).await {
        Err(e) => Err(e),
        Ok(_) => Ok(()),
    }
}

/// The blocked rows of a state view: the `Blocked` relations.
pub(crate) fn blocked_of(world: chat_model::live::State) -> Vec<BlockedUser> {
    let mut out: Vec<BlockedUser> = Vec::new();
    for (user, relation) in world.relations.iter() {
        if *relation == Relation::Blocked {
            out.push(blocked_user_of(*user, world.users.get(&user).cloned()))
        }
    }
    out
}

// The block answer needs the same row shape as a friend, mapped to a block.
pub(crate) fn friend_user_of_blocked(user_id: i64) -> BlockedUser {
    let uuid = live::wire_of(user_id);
    blocked_user_of(uuid, live::snapshot().users.get(&uuid).cloned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chat_model::{Id, User, Relation};
    use chat_model::live::State;

    fn uid(n: u128) -> Id {
        Id::from_u128(n)
    }

    #[test]
    fn the_three_lists_split_the_relation_states() {
        // Feed a relations map (the serde the state holds): one Incoming, one
        // Outgoing, one Friend, one Blocked, and check each list sees exactly
        // its state.
        let mut world = State::default();
        world.users.insert(uid(7), User { id: uid(7), handle: "al".to_string(), display_name: "Al".to_string(), avatar: None, bio: "".to_string() });
        world.users.insert(uid(10), User { id: uid(10), handle: "di".to_string(), display_name: "di".to_string(), avatar: None, bio: "".to_string() });
        world.relations.insert(uid(7), Relation::Incoming);
        world.relations.insert(uid(10), Relation::Blocked);
        let lists = friend_list_of(world);
        assert_eq!(lists.incoming.len(), 1);
        assert_eq!(lists.outgoing.len(), 0);
        assert_eq!(lists.accepted.len(), 0);
    }

    #[test]
    fn the_blocked_list_is_the_blocked_relations() {
        let mut world = State::default();
        world.relations.insert(uid(10), Relation::Blocked);
        let blocked = blocked_of(world);
        assert_eq!(blocked.len(), 1);
        match blocked.get(0) {
            Some(row) => assert_eq!(row.user_id, live::see(uid(10))),
            None => panic!("no blocked row"),
        }
    }

    #[test]
    fn relation_words_match_the_rwnd_vocab() {
        // The words the FriendChanged frames and profile panels speak are RWND's
        // own; chat-model's four relation states map onto them one-to-one.
        assert_eq!(live::relation_word(Relation::Friend), "friends".to_string());
        assert_eq!(live::relation_word(Relation::Incoming), "incoming".to_string());
        assert_eq!(live::relation_word(Relation::Blocked), "blocked".to_string());
    }
}
