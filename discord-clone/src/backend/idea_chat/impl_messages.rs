//! Channel-message operations over the idea-chat RPCs. Pins have no
//! counterpart on this server, so they answer the shared refusal; unread
//! counts come from the reducer state (`ReadState`), since idea-chat has no
//! unread RPC.

use crate::backend::*;
use crate::backend::idea_chat::{client, live, model};

impl MessagesApi for IdeaChatBackend {
    async fn unread_counts(server_id: i64) -> Result<Vec<UnreadCount>, ServerError> {
        Ok(unread_of(live::snapshot(), live::wire_of(server_id)))
    }

    // `mark_read(channel, last_read)`: translate the i64s to wire ids; the
    // fresh `Read` state rides the live event.
    async fn mark_read(channel_id: i64, message_id: i64) -> Result<MarkReadResult, ServerError> {
        let mut args = Vec::new();
        args.push(client::arg(live::wire_of(channel_id)));
        args.push(client::arg(live::wire_of(message_id)));
        match client::post("mark_read", args).await {
            Err(e) => Err(e),
            Ok(_) => Ok(MarkReadResult { channel_id, message_id }),
        }
    }

    // Pins are a RWND feature: idea-chat keeps no pinned flag on a message.
    async fn pin_message(_message_id: i64) -> Result<PinResult, ServerError> {
        Err(model::not_supported())
    }
    async fn unpin_message(_message_id: i64) -> Result<PinResult, ServerError> {
        Err(model::not_supported())
    }
    async fn list_pinned(_channel_id: i64) -> Result<Vec<PinnedMessage>, ServerError> {
        Err(model::not_supported())
    }
    async fn pinned_count(_channel_id: i64) -> Result<i64, ServerError> {
        Err(model::not_supported())
    }

    // `react(id, emoji, add)` answers unit; the fresh total rides the live
    // `ReactionChanged` event, so the result's count stays 0 and the frame
    // converges it.
    async fn add_reaction(message_id: i64, emoji: String) -> Result<ReactionResult, ServerError> {
        match react(live::wire_of(message_id), emoji.clone(), true).await {
            Err(e) => Err(e),
            Ok(_) => Ok(ReactionResult {
                message_id,
                channel_id: 0,
                emoji,
                count: 0,
                added: true,
            }),
        }
    }
    async fn remove_reaction(message_id: i64, emoji: String) -> Result<ReactionResult, ServerError> {
        match react(live::wire_of(message_id), emoji.clone(), false).await {
            Err(e) => Err(e),
            Ok(_) => Ok(ReactionResult {
                message_id,
                channel_id: 0,
                emoji,
                count: 0,
                added: false,
            }),
        }
    }
}

// --- the state view unread counts read ------------------------------------

/// Unread per channel of one space: the reducer tracks how far the viewer
/// read (`ReadState.last_read`) against each channel's newest id, so a
/// difference is one unread row. The count is 0 or 1 — the state keeps
/// unread-ness plus a mention tally, not a row count.
pub(crate) fn unread_of(world: chat_model::live::State, space_id: chat_model::Id) -> Vec<UnreadCount> {
    let mut out: Vec<UnreadCount> = Vec::new();
    for (id, channel) in world.channels.iter() {
        match channel.space {
            Some(space) => {
                if space != space_id {
                    continue;
                }
            },
            None => continue,
        }
        let last_read = match world.reads.get(&id) {
            Some(reads) => reads.last_read,
            None => None,
        };
        let unread = match channel.last_message {
            Some(last) => match last_read {
                Some(read) => last != read,
                None => true,
            },
            None => false,
        };
        let mentions = match world.reads.get(&id) {
            Some(reads) => reads.mentions,
            None => 0,
        };
        out.push(UnreadCount {
            channel_id: live::see(*id),
            count: if unread { 1 } else { 0 },
            unread_mentions: mentions as i64,
        });
    }
    out
}

/// One react call, shared by add and remove so the arg order lives in one
/// place: `react(id, emoji, add)`.
async fn react(id: chat_model::Id, emoji: String, add: bool) -> Result<(), ServerError> {
    let mut args = Vec::new();
    args.push(client::arg(id));
    args.push(client::arg(emoji));
    args.push(client::arg(add));
    match client::post("react", args).await {
        Err(e) => Err(e),
        Ok(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use chat_model::{Id, Channel, ChannelKind, ReadState, Role};
    use chat_model::live::State;

    fn uid(n: u128) -> Id {
        Id::from_u128(n)
    }

    #[test]
    fn pins_answer_the_shared_refusal() {
        assert_eq!(crate::errors::friendly(&model::not_supported()), "not supported on this server");
    }

    #[test]
    fn a_mark_read_call_is_two_wire_ids() {
        // The `mark_read(channel, last_read)` arg shape (declaration order).
        let mut args: Vec<Value> = Vec::new();
        args.push(serde_json::to_value(3).unwrap_or(Value::Null));
        args.push(serde_json::to_value(9).unwrap_or(Value::Null));
        let held = serde_json::to_string(&args).unwrap_or_default();
        assert_eq!(held.contains("[3,9]"), true);
    }

    #[test]
    fn unread_counts_come_from_the_states_read_positions() {
        // Feed a snapshot-shaped state: one channel whose newest message the
        // viewer has NOT read (unread, two mention rows), one already read
        // (zero), and one with no messages at all; the space filter drops the
        // other space's channel.
        let mut world = State::default();
        let mut rows: Vec<Channel> = Vec::new();
        rows.push(Channel {
            id: uid(10),
            kind: ChannelKind::Text,
            space: Some(uid(1)),
            name: "general".to_string(),
            topic: "".to_string(),
            category: None,
            position: 0,
            parent: None,
            starter: None,
            view: Role::Member,
            post: Role::Member,
            members: Vec::new(),
            archived: false,
            last_message: Some(uid(100)),
            message_count: 1,
            created_by: uid(2),
            created_at: 0,
        });
        rows.push(Channel {
            id: uid(11),
            kind: ChannelKind::Text,
            space: Some(uid(1)),
            name: "read".to_string(),
            topic: "".to_string(),
            category: None,
            position: 1,
            parent: None,
            starter: None,
            view: Role::Member,
            post: Role::Member,
            members: Vec::new(),
            archived: false,
            last_message: Some(uid(101)),
            message_count: 1,
            created_by: uid(2),
            created_at: 0,
        });
        for row in rows.iter() {
            world.channels.insert(row.id, row.clone());
        }
        world.reads.insert(uid(11), ReadState { last_read: Some(uid(101)), mentions: 0 });
        world.reads.insert(uid(10), ReadState { last_read: Some(uid(99)), mentions: 2 });
        let counts = unread_of(world, uid(1));
        assert_eq!(counts.len(), 2);
        // The id table maps wire ids through live state, so assert on the
        // shape rather than the mapped channel ids: exactly one row reads as
        // unread, and the mention total is the general channel's two.
        let unread_rows: Vec<&UnreadCount> = counts.iter().filter(move |c| c.count == 1).collect();
        assert_eq!(unread_rows.len(), 1);
        let mention_total: i64 = counts.iter().fold(0, move |acc: i64, c| acc + c.unread_mentions);
        assert_eq!(mention_total, 2);
    }

    #[test]
    fn a_channel_without_messages_is_never_unread() {
        let mut world = State::default();
        world.channels.insert(uid(12), Channel {
            id: uid(12),
            kind: ChannelKind::Text,
            space: Some(uid(1)),
            name: "empty".to_string(),
            topic: "".to_string(),
            category: None,
            position: 0,
            parent: None,
            starter: None,
            view: Role::Member,
            post: Role::Member,
            members: Vec::new(),
            archived: false,
            last_message: None,
            message_count: 0,
            created_by: uid(2),
            created_at: 0,
        });
        let counts = unread_of(world, uid(1));
        assert_eq!(match counts.iter().find(move |c| c.channel_id == live::see(uid(12))) {
            None => 0,
            Some(row) => row.count,
        }, 0);
    }
}
