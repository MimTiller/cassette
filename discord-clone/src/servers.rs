//! Client-side server/channel discovery wrapper.
//!
//! Thin front-end for the backend's `#[server]` fns: every call goes through
//! the single backend interface (`crate::backend::<verb>`), so the RPCs never
//! appear here. Wire types still ride in from `discord_server` (a shared
//! types crate is a later decision).

pub use discord_server::server_settings::{MarkReadResult, UnreadCount};
pub use discord_server::servers::{ChannelSummary, ServerSummary};

/// Create a new server, owned by the caller.
pub async fn create_server(name: &str) -> Result<ServerSummary, String> {
    crate::backend::create_server(name.to_string()).await
}

/// List the servers the caller is a member of.
pub async fn list_servers() -> Result<Vec<ServerSummary>, String> {
    crate::backend::list_servers().await
}

/// List a server's channels, in position order.
pub async fn list_channels(server_id: i64) -> Result<Vec<ChannelSummary>, String> {
    crate::backend::list_channels(server_id).await
}

/// Create a channel in a server. `kind` is `"text"` / `"voice"` /
/// `"category"`; `parent_channel_id` is the category it nests under, or
/// `None` for top-level; `private` hides it from @everyone. The caller needs
/// Manage Channels; the backend's rejection (e.g. insufficient privileges, a
/// duplicate name) comes back as the `Err` string.
pub async fn create_channel(
    server_id: i64,
    name: &str,
    kind: &str,
    parent_channel_id: Option<i64>,
    private: bool,
) -> Result<discord_server::server_settings::ChannelResult, String> {
    crate::backend::create_channel(server_id, name.to_string(), kind.to_string(), parent_channel_id, private)
        .await
}

/// Rename a channel. Owner, admin, or moderator may call; the backend's
/// rejection comes back as the `Err` string.
pub async fn rename_channel(
    server_id: i64,
    channel_id: i64,
    new_name: &str,
) -> Result<discord_server::server_settings::ChannelResult, String> {
    crate::backend::rename_channel(server_id, channel_id, new_name.to_string()).await
}

/// Delete a channel. Owner or admin may call; the backend's rejection comes
/// back as the `Err` string.
pub async fn delete_channel(
    server_id: i64,
    channel_id: i64,
) -> Result<discord_server::server_settings::ChannelResult, String> {
    crate::backend::delete_channel(server_id, channel_id).await
}

/// The per-channel unread counts for a server, keyed out as
/// `Vec<UnreadCount>`. Every channel the caller is a member of appears,
/// including fully-read ones (both counts `0`). See the backend's
/// `UnreadCount` docs for the mention-vs-total semantics.
pub async fn unread_counts(server_id: i64) -> Result<Vec<UnreadCount>, String> {
    crate::backend::unread_counts(server_id).await
}

/// Mark a channel read up to (at least) `message_id`. The backend advances
/// the caller's stored read state monotonically, so sending an older id is a
/// no-op, never a regression.
pub async fn mark_read(channel_id: i64, message_id: i64) -> Result<MarkReadResult, String> {
    crate::backend::mark_read(channel_id, message_id).await
}

/// Rename a server. Only the current owner may call (owner-only by policy);
/// the backend's rejection (e.g. "not the server owner") comes back as the
/// `Err` string. Returns the updated name + icon + owner so the caller can
/// refresh its local model in one round trip.
pub async fn rename_server(
    server_id: i64,
    new_name: &str,
) -> Result<discord_server::server_settings::ServerSettingsResult, String> {
    crate::backend::rename_server(server_id, new_name.to_string()).await
}

/// Set (or clear) a server's icon. `icon` is a URL string, or `None` to clear
/// it. Owner or admin may call; the backend's rejection comes back as the
/// `Err` string. Returns the updated name + icon + owner.
pub async fn set_server_icon(
    server_id: i64,
    icon: Option<String>,
) -> Result<discord_server::server_settings::ServerSettingsResult, String> {
    crate::backend::set_server_icon(server_id, icon).await
}

/// Transfer server ownership to another member. Only the current owner may
/// call; the backend's rejection (e.g. target isn't a member) comes back as
/// the `Err` string. Returns the updated owner id (among name + icon).
pub async fn transfer_server_ownership(
    server_id: i64,
    new_owner_id: i64,
) -> Result<discord_server::server_settings::ServerSettingsResult, String> {
    crate::backend::transfer_server_ownership(server_id, new_owner_id).await
}
