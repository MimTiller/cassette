//! Today's direct messages: the `discord-server` RPC stubs.

use super::*;

impl DmsApi for CassetteBackend {
    async fn list_dms() -> Result<Vec<Dm>, ServerError> {
        discord_server::dms::list_dms().await
    }
    async fn start_dm(user_id: i64) -> Result<Dm, ServerError> {
        discord_server::dms::start_dm(discord_server::dms::StartDmInput { user_id }).await
    }
    async fn send_dm_message(channel_id: i64, content: String, sticker_id: Option<i64>) -> Result<SendDmMessageResult, ServerError> {
        discord_server::dms::send_dm_message(
            discord_server::dms::SendDmMessageInput { channel_id, content, sticker_id },
        ).await
    }
    async fn list_dm_messages(channel_id: i64, limit: Option<i64>) -> Result<Vec<DmMessage>, ServerError> {
        discord_server::dms::list_dm_messages(
            discord_server::dms::ListDmMessagesInput { channel_id, limit },
        ).await
    }
    async fn create_group_dm(user_ids: Vec<i64>, name: String) -> Result<Dm, ServerError> {
        discord_server::dms::create_group_dm(
            discord_server::dms::CreateGroupDmInput { user_ids, name },
        ).await
    }
    async fn add_group_dm_member(dm_id: i64, user_id: i64) -> Result<Dm, ServerError> {
        discord_server::dms::add_group_dm_member(
            discord_server::dms::GroupDmMemberInput { dm_id, user_id },
        ).await
    }
    async fn remove_group_dm_member(dm_id: i64, user_id: i64) -> Result<(), ServerError> {
        discord_server::dms::remove_group_dm_member(
            discord_server::dms::GroupDmMemberInput { dm_id, user_id },
        ).await
    }
    async fn rename_group_dm(dm_id: i64, name: String) -> Result<Dm, ServerError> {
        discord_server::dms::rename_group_dm(
            discord_server::dms::RenameGroupDmInput { dm_id, name },
        ).await
    }
}
