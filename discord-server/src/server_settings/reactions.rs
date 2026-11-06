//! Reaction endpoints: the wire add/remove surface over the realtime service.

use serde::Deserialize;
use server::{server, ServerError};


/// The acting user for the current request, resolved from a verified session
/// token (see [`crate::session`]); `None` when unauthenticated.
#[cfg(feature = "server")]
use crate::session::acting_user;
#[cfg(feature = "server")]
use crate::server_settings::db_from_state;

// ── message reactions ─────────────────────────────────────────────────────
//
// Add/remove an emoji reaction on a channel message (adding needs Add
// Reactions), persisting a `(message, user, emoji)` row in
// `message_reactions` and broadcasting the post-state count to everyone who
// can see the channel over the gateway
// (`WsServerMsg::ReactionAdd` / `ReactionRemove`). The acting user comes from
// the verified session token, never from the request.

/// React to a message with an emoji. Needs Add Reactions in the message's
/// channel. Idempotent
/// for a repeat add; the result's `count` is the message's total for that emoji
/// after the operation.
#[server]
pub async fn add_reaction(input: ReactionInput) -> Result<ReactionResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let (channel_id, count) =
        crate::realtime::add_channel_reaction(&db, caller, input.message_id, &input.emoji)
            .map_err(ServerError::failed)?;
    Ok(
        ReactionResult {
            message_id: input.message_id,
            channel_id,
            emoji: input.emoji,
            count,
            added: true,
        }
    )
}

/// Drop the caller's own emoji reaction from a message. Same member bar as
/// `add_reaction`; idempotent (removing a reaction that is not there is a
/// no-op). `count` is the message's total for the emoji after the removal —
/// 0 means the last one was dropped.
#[server]
pub async fn remove_reaction(input: ReactionInput) -> Result<ReactionResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let (channel_id, count) =
        crate::realtime::remove_channel_reaction(&db, caller, input.message_id, &input.emoji)
            .map_err(ServerError::failed)?;
    Ok(
        ReactionResult {
            message_id: input.message_id,
            channel_id,
            emoji: input.emoji,
            count,
            added: false,
        }
    )
}

/// Which message the caller (the session-token user) is adding or dropping one
/// emoji reaction on. The message's channel is resolved server-side from the
/// stored row, like the pin endpoints.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct ReactionInput {
    pub message_id: i64,
    pub emoji: String,
}

/// The outcome of an add/remove reaction. `count` is the message's total for
/// `emoji` AFTER the operation; `added` reports which way the call went.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ReactionResult {
    pub message_id: i64,
    pub channel_id: i64,
    pub emoji: String,
    pub count: u32,
    pub added: bool,
}
