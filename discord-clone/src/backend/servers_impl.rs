//! Today's server and channel operations: the `discord-server` RPC stubs.

use super::*;

impl ServersApi for CassetteBackend {
    async fn list_servers() -> Result<Vec<ServerSummary>, ServerError> {
        discord_server::servers::list_servers(()).await
    }
    async fn create_server(name: String) -> Result<ServerSummary, ServerError> {
        discord_server::servers::create_server(
            discord_server::servers::CreateServerInput { name },
        ).await
    }
    async fn list_channels(server_id: i64) -> Result<Vec<ChannelSummary>, ServerError> {
        discord_server::servers::list_channels(
            discord_server::servers::ListChannelsInput { server_id },
        ).await
    }
    async fn create_channel(
        server_id: i64,
        name: String,
        kind: String,
        parent_channel_id: Option<i64>,
        private: bool,
    ) -> Result<ChannelResult, ServerError> {
        discord_server::server_settings::create_channel(
            discord_server::server_settings::CreateChannelInput {
                server_id,
                name,
                kind,
                parent_channel_id,
                private,
            },
        ).await
    }
    async fn rename_channel(
        server_id: i64,
        channel_id: i64,
        new_name: String,
    ) -> Result<ChannelResult, ServerError> {
        discord_server::server_settings::rename_channel(
            discord_server::server_settings::RenameChannelInput {
                server_id,
                channel_id,
                new_name,
            },
        ).await
    }
    async fn delete_channel(
        server_id: i64,
        channel_id: i64,
    ) -> Result<ChannelResult, ServerError> {
        discord_server::server_settings::delete_channel(
            discord_server::server_settings::DeleteChannelInput {
                server_id,
                channel_id,
            },
        ).await
    }
    async fn rename_server(
        server_id: i64,
        new_name: String,
    ) -> Result<ServerSettingsResult, ServerError> {
        discord_server::server_settings::rename_server(
            discord_server::server_settings::RenameServerInput {
                server_id,
                new_name,
            },
        ).await
    }
    async fn set_server_icon(
        server_id: i64,
        icon: Option<String>,
    ) -> Result<ServerSettingsResult, ServerError> {
        discord_server::server_settings::set_server_icon(
            discord_server::server_settings::SetServerIconInput { server_id, icon },
        ).await
    }
    async fn transfer_server_ownership(
        server_id: i64,
        new_owner_id: i64,
    ) -> Result<ServerSettingsResult, ServerError> {
        discord_server::server_settings::transfer_server_ownership(
            discord_server::server_settings::TransferOwnershipInput {
                server_id,
                new_owner_id,
            },
        ).await
    }
    async fn leave_server(server_id: i64) -> Result<(), ServerError> {
        discord_server::server_settings::leave_server(
            discord_server::server_settings::LeaveServerInput { server_id },
        ).await
    }
}
