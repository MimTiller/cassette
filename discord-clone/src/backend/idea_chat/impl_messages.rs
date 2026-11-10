//! Channel-message operations over the idea-chat RPCs. Pins have no
//! counterpart on this server, so they answer the shared refusal; unread
//! counts come from the reducer state (`ReadState`), since idea-chat has no
//! unread RPC.

use crate::backend::*;
use serde_json::Value;

impl MessagesApi for IdeaChatBackend {
    async fn unread_counts(server_id: i64) -> Result<Vec<UnreadCount>, ServerError> {
        // idea-chat has no unread RPC: the snapshot keeps how far the viewer
        // read per channel plus each channel's newest id, so the count rides
        // the state. The count is 0 or 1 — the reducer tracks unread-ness
        // plus a mention tally, not a row count.
        let world = live.snapshot();
        let space_id = live.wire_of(server_id);
        let mut out: Vec<UnreadCount> = Vec::new();
        for (id, channel) in world.channels.iter() {
            match channel.space {
                Some(space) => {
                    if space != space_id {
                        continue
                    }
                }
                None => continue
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
                channel_id: live.see(id),
                count: if unread { 1 } else { 0 },
                unread_mentions: mentions,
            });
        }
        Ok(out)
    }

    async fn mark_read(channel_id: i64, message_id: i64) -> Result<MarkReadResult, ServerError> {
        let args = Vec.new();
        args.push(client.arg(live.wire_of(channel_id)));
        args.push(client.arg(live.wire_of(message_id)));
        match client.post("mark_read", args).await {
            Err(e) => Err(e),
            Ok(_) => Ok(MarkReadResult { channel_id, message_id }),
        }
    }

    // Pins are a RWND feature: idea-chat keeps no pinned flag on a message.
    async fn pin_message(message_id: i64) -> Result<PinResult, ServerError> {
        Err(model::not_supported())
    }
    async fn unpin_message(message_id: i64) -> Result<PinResult, ServerError> {
        Err(model::not_supported())
    }
    async fn list_pinned(channel_id: i64) -> Result<Vec<PinnedMessage>, ServerError> {
        Err(model::not_supported())
    }
    async fn pinned_count(channel_id: i64) -> Result<i64, ServerError> {
        Err(model::not_supported())
    }

    // `react` answers unit; the fresh total rides the live `ReactionChanged`
    // event, so the result's count stays 0 and the frame converges it.
    async fn add_reaction(message_id: i64, emoji: String) -> Result<ReactionResult, ServerError> {
        match react(live.wire_of(message_id), emoji, true).await {
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
        match react(live.wire_of(message_id), emoji, false).await {
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

/// One react call, shared by add and remove so the arg order lives in one
/// place: `react(id, emoji, add)`.
async fn react(id: chat_model::Id, emoji: String, add: bool) -> Result<(), ServerError> {
    let args = Vec.new();
    args.push(client.arg(id));
    args.push(client.arg(emoji));
    args.push(client.arg(add));
    match client.post("react", args).await {
        Err(e) => Err(e),
        Ok(_) => Ok(()),
    }
}
