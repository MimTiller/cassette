//! Client-side server/channel discovery wrapper.
//!
//! Thin front-end for the backend's `#[server]` fns in
//! [`discord_server::servers`]. Same shape as `auth.rs`: this crate
//! depends on `discord-server` with default features, so these
//! compile to the codegen'd RPC stubs -- no manual HTTP here.

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
#[allow(dead_code)] // consumed by the UI in a follow-up task.
pub async fn list_channels(server_id: i64) -> Result<Vec<ChannelSummary>, String> {
    let input = discord_server::servers::ListChannelsInput { server_id };
    discord_server::servers::list_channels(input)
        .await
        .map_err(|e| e.to_string())
}
