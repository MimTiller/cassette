//! Client-side server/channel discovery wrapper.
//!
//! Thin front-end for the backend's `#[server]` fns in
//! [`discord_server::servers`]. Same shape as `auth.rs`: this crate
//! depends on `discord-server` with default features, so these
//! compile to the codegen'd RPC stubs -- no manual HTTP here.

pub use discord_server::server_settings::{MarkReadResult, UnreadCount};
pub use discord_server::servers::{ChannelSummary, ServerSummary};

/// Create a new server, owned by the caller.
#[allow(dead_code)] // consumed by the UI in a follow-up task.
pub async fn create_server(name: &str) -> Result<ServerSummary, String> {
    let input = discord_server::servers::CreateServerInput {
        name: name.to_string(),
    };
    discord_server::servers::create_server(input)
        .await
        .map_err(|e| e.to_string())
}

/// List the servers the caller is a member of.
#[allow(dead_code)] // consumed by the UI in a follow-up task.
pub async fn list_servers() -> Result<Vec<ServerSummary>, String> {
    discord_server::servers::list_servers(())
        .await
        .map_err(|e| e.to_string())
}

/// Join an existing server as a plain member.
#[allow(dead_code)] // consumed by the UI in a follow-up task.
pub async fn join_server(server_id: i64) -> Result<(), String> {
    let input = discord_server::servers::JoinServerInput { server_id };
    discord_server::servers::join_server(input)
        .await
        .map_err(|e| e.to_string())
}

/// List a server's channels, in position order.
pub async fn list_channels(server_id: i64) -> Result<Vec<ChannelSummary>, String> {
    let input = discord_server::servers::ListChannelsInput { server_id };
    discord_server::servers::list_channels(input)
        .await
        .map_err(|e| e.to_string())
}

/// Create a channel in a server. `kind` is `"text"` / `"voice"` /
/// `"category"`; `parent_channel_id` is the category it nests under, or
/// `None` for top-level. The caller must be the server's owner or admin; the
/// backend's rejection (e.g. insufficient privileges, a duplicate name) comes
/// back as the `Err` string.
pub async fn create_channel(
    server_id: i64,
    name: &str,
    kind: &str,
    parent_channel_id: Option<i64>,
) -> Result<discord_server::server_settings::ChannelResult, String> {
    let input = discord_server::server_settings::CreateChannelInput {
        server_id,
        name: name.to_string(),
        kind: kind.to_string(),
        parent_channel_id,
    };
    discord_server::server_settings::create_channel(input)
        .await
        .map_err(|e| e.to_string())
}

/// Rename a channel. Owner, admin, or moderator may call; the backend's
/// rejection comes back as the `Err` string.
pub async fn rename_channel(
    server_id: i64,
    channel_id: i64,
    new_name: &str,
) -> Result<discord_server::server_settings::ChannelResult, String> {
    let input = discord_server::server_settings::RenameChannelInput {
        server_id,
        channel_id,
        new_name: new_name.to_string(),
    };
    discord_server::server_settings::rename_channel(input)
        .await
        .map_err(|e| e.to_string())
}

/// Delete a channel. Owner or admin may call; the backend's rejection comes
/// back as the `Err` string.
pub async fn delete_channel(
    server_id: i64,
    channel_id: i64,
) -> Result<discord_server::server_settings::ChannelResult, String> {
    let input = discord_server::server_settings::DeleteChannelInput {
        server_id,
        channel_id,
    };
    discord_server::server_settings::delete_channel(input)
        .await
        .map_err(|e| e.to_string())
}

/// The per-channel unread counts for a server, keyed out as
/// `Vec<UnreadCount>`. Every channel the caller is a member of appears,
/// including fully-read ones (both counts `0`). See the backend's
/// `UnreadCount` docs for the mention-vs-total semantics.
pub async fn unread_counts(server_id: i64) -> Result<Vec<UnreadCount>, String> {
    let input = discord_server::server_settings::UnreadCountsInput { server_id };
    discord_server::server_settings::unread_counts(input)
        .await
        .map_err(|e| e.to_string())
}

/// Mark a channel read up to (at least) `message_id`. The backend advances
/// the caller's stored read state monotonically, so sending an older id is a
/// no-op, never a regression.
pub async fn mark_read(channel_id: i64, message_id: i64) -> Result<MarkReadResult, String> {
    let input = discord_server::server_settings::MarkReadInput {
        channel_id,
        message_id,
    };
    discord_server::server_settings::mark_read(input)
        .await
        .map_err(|e| e.to_string())
}
