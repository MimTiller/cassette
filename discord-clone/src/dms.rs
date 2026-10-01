//! Client-side direct-message wrapper (live mode).
//!
//! Thin front-end for the backend's `#[server]` fns in
//! [`discord_server::dms`] — same shape as `servers.rs` / `auth.rs`: this
//! crate depends on `discord-server` with default features, so these
//! compile to the codegen'd RPC stubs (the acting user rides the signed
//! session token the credential provider attaches). Only the read/view
//! surface is wrapped here; `start_dm` / `send_dm_message` are a separate
//! follow-up task.

pub use discord_server::dms::{Dm, DmMessage};

/// List the signed-in user's existing DMs, most-recently-created first.
pub async fn list_dms() -> Result<Vec<Dm>, String> {
    discord_server::dms::list_dms()
        .await
        .map_err(|e| e.to_string())
}

/// List a DM channel's recent message history, oldest-first. `limit` caps
/// the number of returned messages (the backend defaults it to 50).
pub async fn list_dm_messages(dm_id: i64, limit: Option<i64>) -> Result<Vec<DmMessage>, String> {
    let input = discord_server::dms::ListDmMessagesInput {
        channel_id: dm_id,
        limit,
    };
    discord_server::dms::list_dm_messages(input)
        .await
        .map_err(|e| e.to_string())
}
