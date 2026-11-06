//! Today's permission queries: the `discord-server` permissions RPCs.

use super::*;

impl PermissionsApi for CassetteBackend {
    async fn my_permissions(server_id: i64) -> Result<MyPermissions, ServerError> {
        discord_server::permissions::my_permissions(
            discord_server::permissions::MyPermissionsInput { server_id },
        ).await
    }

    async fn channel_overwrites(channel_id: i64) -> Result<Vec<Overwrite>, ServerError> {
        discord_server::permissions::channel_overwrites(
            discord_server::permissions::ChannelOverwritesInput { channel_id },
        ).await
    }

    async fn set_channel_overwrite(channel_id: i64, target: Target, allow: Permissions, deny: Permissions) -> Result<Vec<Overwrite>, ServerError> {
        discord_server::permissions::set_channel_overwrite(
            discord_server::permissions::SetOverwriteInput { channel_id, target, allow, deny },
        ).await
    }

    async fn set_everyone_permissions(server_id: i64, permissions: Permissions) -> Result<Permissions, ServerError> {
        discord_server::permissions::set_everyone_permissions(
            discord_server::permissions::SetEveryoneInput { server_id, permissions },
        ).await
    }
}
