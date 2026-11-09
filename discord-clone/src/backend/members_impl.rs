//! Today's members and moderation: the `discord-server` roster and member
//! management RPCs.

use super::*;

impl MembersApi for CassetteBackend {
    async fn list_members(server_id: i64) -> Result<Vec<MemberInfo>, ServerError> {
        discord_server::server_settings::list_members(
            discord_server::server_settings::ListMembersInput { server_id },
        ).await
    }

    async fn add_member(server_id: i64, user_id: i64) -> Result<MemberResult, ServerError> {
        discord_server::server_settings::add_member(
            discord_server::server_settings::AddMemberInput { server_id, user_id },
        ).await
    }

    async fn remove_member(server_id: i64, user_id: i64) -> Result<MemberResult, ServerError> {
        discord_server::server_settings::remove_member(
            discord_server::server_settings::RemoveMemberInput { server_id, user_id },
        ).await
    }

    async fn ban_member(server_id: i64, user_id: i64, reason: Option<String>) -> Result<MemberResult, ServerError> {
        discord_server::server_settings::ban_member(
            discord_server::server_settings::BanMemberInput { server_id, user_id, reason },
        ).await
    }

    async fn timeout_member(server_id: i64, user_id: i64, duration_secs: i64) -> Result<MemberResult, ServerError> {
        discord_server::server_settings::timeout_member(
            discord_server::server_settings::TimeoutMemberInput { server_id, user_id, duration_secs },
        ).await
    }
}
