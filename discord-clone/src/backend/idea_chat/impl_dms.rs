//! Direct messages: a DM is a channel on this server (`Direct`/`Group` kinds
//! outside any space), so `open_direct`/`create_group` answer channels and the
//! roster rides the snapshot. Sticker sends have no counterpart.

use crate::backend::*;
use crate::backend::idea_chat::{client, live, model};
use chat_model::{Channel, ChannelKind, Id, Message, Role};
use discord_server::dms::{DmParticipant};
use serde_json::Value;

impl DmsApi for IdeaChatBackend {
    // No list RPC: DM channels appear in the snapshot with no space.
    async fn list_dms() -> Result<Vec<Dm>, ServerError> {
        Ok(dms_of(live::snapshot()))
    }

    // `open_direct(with)` answers the conversation's channel.
    async fn start_dm(user_id: i64) -> Result<Dm, ServerError> {
        let mut args = Vec::new();
        args.push(client::arg(live::wire_of(user_id)));
        match client::call_as::<Channel>("open_direct", args).await {
            Err(e) => Err(e),
            Ok(channel) => Ok(dm_of(channel)),
        }
    }

    // A DM is a channel: `send(channel, body, reply?, attachments)`; the
    // sticker half has no counterpart on this server.
    async fn send_dm_message(channel_id: i64, content: String, sticker_id: Option<i64>) -> Result<SendDmMessageResult, ServerError> {
        if sticker_id != None {
            return Err(model::not_supported())
        }
        let mut args = Vec::new();
        args.push(client::arg(live::wire_of(channel_id)));
        args.push(client::arg(&content));
        args.push(Value::Null);
        args.push(client::arg(Vec::<chat_model::Message>::new()));
        match client::call_as::<Message>("send", args).await {
            Err(e) => Err(e),
            Ok(message) => Ok(SendDmMessageResult {
                message: dm_message_of(message),
            }),
        }
    }

    // `history(channel, Window::Latest)` answers the newest page.
    async fn list_dm_messages(channel_id: i64, limit: Option<i64>) -> Result<Vec<DmMessage>, ServerError> {
        let page = match live::history(channel_id, None).await {
            Err(e) => return Err(e),
            Ok(page) => page,
        };
        let mut out: Vec<DmMessage> = Vec::new();
        for message in page.messages.iter() {
            out.push(dm_message_of(message.clone()));
        }
        match limit {
            None => Ok(out),
            Some(n) => {
                let mut kept: Vec<DmMessage> = Vec::new();
                let start = if (n as i64) < (out.len() as i64) { (out.len() as i64) - n } else { 0 };
                for (index, row) in out.iter().enumerate() {
                    if index as i64 >= start {
                        kept.push(row.clone());
                    }
                }
                Ok(kept)
            },
        }
    }

    // `create_group(with, name)` answers the group channel.
    async fn create_group_dm(user_ids: Vec<i64>, name: String) -> Result<Dm, ServerError> {
        let mut with: Vec<Id> = Vec::new();
        for user_id in user_ids.iter() {
            with.push(live::wire_of(*user_id));
        }
        let mut args = Vec::new();
        args.push(client::arg(with));
        args.push(client::arg(&name));
        match client::call_as::<Channel>("create_group", args).await {
            Err(e) => Err(e),
            Ok(channel) => Ok(dm_of(channel)),
        }
    }

    // `add_to_group(group, add)` answers the fresh channel.
    async fn add_group_dm_member(dm_id: i64, user_id: i64) -> Result<Dm, ServerError> {
        let mut args = Vec::new();
        args.push(client::arg(live::wire_of(dm_id)));
        args.push(client::arg(live::wire_of(user_id)));
        match client::call_as::<Channel>("add_to_group", args).await {
            Err(e) => Err(e),
            Ok(channel) => Ok(dm_of(channel)),
        }
    }

    // `leave_group(group)` drops the caller from it (the wire has no verb
    // that removes an arbitrary member from someone else's group).
    async fn remove_group_dm_member(dm_id: i64, _user_id: i64) -> Result<(), ServerError> {
        let mut args = Vec::new();
        args.push(client::arg(live::wire_of(dm_id)));
        match client::post("leave_group", args).await {
            Err(e) => Err(e),
            Ok(_) => Ok(()),
        }
    }

    // `update_channel(channel, name, topic, category, view, post)` answers the
    // fresh channel; only the name is the rename's, the rest is restated
    // unchanged from the state.
    async fn rename_group_dm(dm_id: i64, name: String) -> Result<Dm, ServerError> {
        let world = live::snapshot();
        let channel = live::wire_of(dm_id);
        let (topic, view, post) = match world.channels.get(&channel) {
            None => ("".to_string(), Role::Member, Role::Member),
            Some(found) => (found.topic.clone(), found.view, found.post),
        };
        let mut args = Vec::new();
        args.push(client::arg(channel));
        args.push(client::arg(&name));
        args.push(client::arg(&topic));
        args.push(Value::Null);
        args.push(client::arg(view));
        args.push(client::arg(post));
        match client::call_as::<Channel>("update_channel", args).await {
            Err(e) => Err(e),
            Ok(fresh) => Ok(dm_of(fresh)),
        }
    }
}

// --- the DM shapes, built from the channel/message vocab -------------------

/// One DM channel as RWND's `Dm`: participants from its member ids.
pub(crate) fn dm_of(channel: Channel) -> Dm {
    let world = live::snapshot();
    let mut participants: Vec<DmParticipant> = Vec::new();
    for user_id in channel.members.iter() {
        match world.users.get(&user_id) {
            None => {},
            Some(user) => participants.push(DmParticipant {
                user_id: live::see(*user_id),
                username: user.handle.clone(),
                display_name: live::display_of(user.clone()),
                avatar_url: user.avatar.clone(),
                avatar_decoration: None,
            }),
        }
    }
    Dm {
        channel_id: live::see(channel.id),
        created_at: live::millis_at(channel.created_at),
        participants: participants,
        is_group: channel.kind == ChannelKind::Group,
        name: match channel.name.is_empty() {
            true => None,
            false => Some(channel.name),
        },
        owner_id: Some(live::see(channel.created_by)),
    }
}

/// One wire message as RWND's `DmMessage`.
pub(crate) fn dm_message_of(message: Message) -> DmMessage {
    let world = live::snapshot();
    let author_name = match world.users.get(&message.author) {
        None => live::see(message.author).to_string(),
        Some(found) => found.handle.clone(),
    };
    let author_avatar = match world.users.get(&message.author) {
        None => None,
        Some(found) => found.avatar.clone(),
    };
    DmMessage {
        message_id: live::see(message.id),
        channel_id: live::see(message.channel),
        author_id: live::see(message.author),
        author_username: author_name,
        author_avatar: author_avatar,
        author_decoration: None,
        content: message.body,
        created_at: live::millis_at(message.at),
        edited_at: match message.edited_at {
            Some(at) => Some(live::millis_at(at)),
            None => None,
        },
        sticker: None,
    }
}

/// The DM channels in the snapshot: `Direct`/`Group` kinds with no space.
pub(crate) fn dms_of(world: chat_model::live::State) -> Vec<Dm> {
    let mut out: Vec<Dm> = Vec::new();
    for (id, channel) in world.channels.iter() {
        let _ = id;
        match channel.space {
            Some(space) => {
                let _ = space;
            },
            None => {
                if channel.kind == ChannelKind::Direct || channel.kind == ChannelKind::Group {
                    out.push(dm_of(channel.clone()));
                }
            },
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chat_model::{Channel, ChannelKind, Id, Role};
    use chat_model::live::State;

    fn uid(n: u128) -> Id {
        Id::from_u128(n)
    }

    fn blank(kind: ChannelKind, space: Option<Id>, name: String, members: Vec<Id>) -> Channel {
        Channel {
            id: uid(11),
            kind: kind,
            space: space,
            name: name,
            topic: "".to_string(),
            category: None,
            position: 0,
            parent: None,
            starter: None,
            view: Role::Member,
            post: Role::Member,
            members: members,
            archived: false,
            last_message: None,
            message_count: 0,
            created_by: uid(2),
            created_at: 0,
        }
    }

    #[test]
    fn only_direct_and_group_channels_outside_a_space_are_dms() {
        // Feed a snapshot-shaped state with one space channel, one Direct and
        // one Group channel; only the last two are DMs.
        let mut s = State::default();
        let direct = blank(ChannelKind::Direct, None, "".to_string(), vec![uid(7)]);
        let group = blank(ChannelKind::Group, None, "crew".to_string(), vec![uid(7), uid(8)]);
        let spacey = blank(ChannelKind::Text, Some(uid(1)), "general".to_string(), Vec::new());
        s.channels.insert(uid(20), direct.clone());
        s.channels.insert(uid(21), group.clone());
        s.channels.insert(uid(22), spacey.clone());
        assert_eq!(dms_of(s).len(), 2);
        assert_eq!(dm_of(direct).is_group, false);
        assert_eq!(dm_of(group).is_group, true);
        assert_eq!(dm_of(spacey).name, Some("general".to_string()));
    }

    #[test]
    fn an_empty_name_stays_unnamed_on_the_wire() {
        assert_eq!(dm_of(blank(ChannelKind::Direct, None, "".to_string(), vec![uid(7)])).name, None);
    }
}
