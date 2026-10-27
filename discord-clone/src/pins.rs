//! Client-side wrapper for the backend's pinned-messages `#[server]` fns in
//! [`discord_server::server_settings`]. Same thin-RPC-stub shape as
//! `servers.rs` / `invite.rs`: this crate depends on `discord-server` with
//! default features, so these compile to the codegen'd RPC stubs — no manual
//! HTTP here. Every call authenticates as the signed-in user via the session
//! token the `server` SDK attaches; the backend's rejection message comes back
//! as the `Err` string.
//!
//! Permission model (enforced by the backend, not here): pinning/unpinning a
//! message is moderator+ (Discord "Manage Messages" parity), while reading the
//! pinned view / count is any member. A plain member who clicks "Pin message"
//! therefore gets `Err("... permission denied ...")` rather than a silent
//! success — the UI surfaces that.

pub use discord_server::server_settings::{PinResult, PinnedMessage};

/// Pin a message in its channel. The channel is resolved from the message on
/// the server. Returns the message's post-operation state plus the channel's
/// new pin count (the header badge updates from `pinned_count` with no second
/// round trip). Moderator+; the backend's rejection comes back as `Err`.
pub async fn pin_message(message_id: i64) -> Result<PinResult, String> {
    let input = discord_server::server_settings::PinInput { message_id };
    discord_server::server_settings::pin_message(input)
        .await
        .map_err(|e| crate::errors::friendly(&e))
}

/// Unpin a message in its channel. Idempotent (unpinning an already-unpinned
/// message is a no-op, not an error). Returns the post-operation state plus
/// the channel's new pin count. Moderator+.
pub async fn unpin_message(message_id: i64) -> Result<PinResult, String> {
    let input = discord_server::server_settings::PinInput { message_id };
    discord_server::server_settings::unpin_message(input)
        .await
        .map_err(|e| crate::errors::friendly(&e))
}

/// List a channel's pinned messages in pin order (oldest pin first), each
/// carrying the resolved author username and who pinned it. Any member of the
/// channel may read the view.
pub async fn list_pinned(channel_id: i64) -> Result<Vec<PinnedMessage>, String> {
    let input = discord_server::server_settings::PinnedChannelInput { channel_id };
    discord_server::server_settings::list_pinned(input)
        .await
        .map_err(|e| crate::errors::friendly(&e))
}

/// The count of a channel's pinned messages — the value the channel header's
/// pin badge displays. Any member of the channel may read it.
pub async fn pinned_count(channel_id: i64) -> Result<i64, String> {
    let input = discord_server::server_settings::PinnedChannelInput { channel_id };
    discord_server::server_settings::pinned_count(input)
        .await
        .map_err(|e| crate::errors::friendly(&e))
}
