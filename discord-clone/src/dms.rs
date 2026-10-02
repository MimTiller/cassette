//! Client-side direct-message wrapper (live mode).
//!
//! Thin front-end for the backend's `#[server]` fns in
//! [`discord_server::dms`] — same shape as `servers.rs` / `auth.rs`: this
//! crate depends on `discord-server` with default features, so these
//! compile to the codegen'd RPC stubs (the acting user rides the signed
//! session token the credential provider attaches).

pub use discord_server::dms::{Dm, DmMessage};

/// List the signed-in user's existing DMs, most-recently-created first.
pub async fn list_dms() -> Result<Vec<Dm>, String> {
    discord_server::dms::list_dms()
        .await
        .map_err(|e| e.to_string())
}

/// Start a DM with the user named by `user_id` (idempotent: if the caller
/// and that user already share a DM, the existing channel is returned).
pub async fn start_dm(user_id: i64) -> Result<Dm, String> {
    let input = discord_server::dms::StartDmInput { user_id };
    discord_server::dms::start_dm(input)
        .await
        .map_err(|e| e.to_string())
}

/// Send a message into the DM channel named by `dm_id` and return it. The
/// server pushes a `WsServerMsg::DmMessage` frame to every participant (the
/// sender included), so the caller does NOT append the reply locally — the
/// gateway's inbound `DmMessage` handling renders it.
pub async fn send_dm_message(dm_id: i64, content: String) -> Result<DmMessage, String> {
    let input = discord_server::dms::SendDmMessageInput {
        channel_id: dm_id,
        content,
    };
    let result = discord_server::dms::send_dm_message(input)
        .await
        .map_err(|e| e.to_string())?;
    Ok(result.message)
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
