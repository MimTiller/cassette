//! Client-side wrapper for the backend's message-reaction `#[server]` fns in
//! [`discord_server::server_settings`]. Same thin-RPC-stub shape as `pins.rs`
//! / `servers.rs`: this crate depends on `discord-server` with default
//! features, so these compile to the codegen'd RPC stubs — no manual HTTP
//! here. Every call authenticates as the signed-in user via the session token
//! the `server` SDK attaches; the backend's rejection message comes back as
//! the `Err` string.
//!
//! Permission model (enforced by the backend, not here): unlike pinning
//! (moderator+), reacting is allowed for ANY member of the message's
//! channel's server — Discord parity. A guest (no session token) gets
//! `Err("unauthenticated")`.

pub use discord_server::server_settings::{ReactionResult};

/// React to a message with an emoji. Persists a `(message, user, emoji)` row
/// server-side and broadcasts the new per-emoji count to every connected
/// member of the message's channel's server (the caller's own echo included),
/// so the pill updates live everywhere. Idempotent for a repeat add. The
/// result's `count` is the message's total for the emoji after the operation.
pub async fn add_reaction(message_id: i64, emoji: String) -> Result<ReactionResult, String> {
    let input = discord_server::server_settings::ReactionInput {
        message_id,
        emoji,
    };
    discord_server::server_settings::add_reaction(input)
        .await
        .map_err(|e| crate::errors::friendly(&e))
}

/// Drop the caller's own emoji reaction from a message. Idempotent (removing
/// a reaction that is not there is a no-op, not an error). `count` is the
/// message's total for the emoji after the removal — 0 means the last one
/// was dropped.
pub async fn remove_reaction(message_id: i64, emoji: String) -> Result<ReactionResult, String> {
    let input = discord_server::server_settings::ReactionInput {
        message_id,
        emoji,
    };
    discord_server::server_settings::remove_reaction(input)
        .await
        .map_err(|e| crate::errors::friendly(&e))
}
