//! Today's named roles: the `discord-server` server-roles RPCs.

use super::*;

impl RolesApi for CassetteBackend {
    async fn server_roles(server_id: i64) -> Result<RolesSnapshot, ServerError> {
        discord_server::server_roles::server_roles(
            discord_server::server_roles::ServerRolesInput { server_id },
        ).await
    }

    async fn create_role(server_id: i64, name: String, color: String, mentionable: bool) -> Result<CustomRole, ServerError> {
        discord_server::server_roles::create_role(
            discord_server::server_roles::CreateRoleInput { server_id, name, color, mentionable },
        ).await
    }

    async fn update_role(role_id: i64, name: String, color: String, mentionable: bool, permissions: Option<Permissions>) -> Result<CustomRole, ServerError> {
        discord_server::server_roles::update_role(
            discord_server::server_roles::UpdateRoleInput { role_id, name, color, mentionable, permissions },
        ).await
    }

    async fn move_role(role_id: i64, position: i64) -> Result<CustomRole, ServerError> {
        discord_server::server_roles::move_role(
            discord_server::server_roles::MoveRoleInput { role_id, position },
        ).await
    }

    async fn delete_role(role_id: i64) -> Result<(), ServerError> {
        discord_server::server_roles::delete_role(
            discord_server::server_roles::DeleteRoleInput { role_id },
        ).await
    }

    async fn assign_role(role_id: i64, user_id: i64, assigned: bool) -> Result<(), ServerError> {
        discord_server::server_roles::assign_role(
            discord_server::server_roles::AssignRoleInput { role_id, user_id, assigned },
        ).await
    }
}
