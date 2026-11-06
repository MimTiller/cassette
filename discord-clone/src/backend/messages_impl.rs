//! Today's message operations: the `discord-server` RPC stubs.

use super::*;

impl MessagesApi for CassetteBackend {
    async fn unread_counts(server_id: i64) -> Result<Vec<UnreadCount>, ServerError> {
        discord_server::server_settings::unread_counts(
            discord_server::server_settings::UnreadCountsInput { server_id },
        ).await
    }
    async fn mark_read(channel_id: i64, message_id: i64) -> Result<MarkReadResult, ServerError> {
        discord_server::server_settings::mark_read(
            discord_server::server_settings::MarkReadInput { channel_id, message_id },
        ).await
    }
    async fn pin_message(message_id: i64) -> Result<PinResult, ServerError> {
        discord_server::server_settings::pin_message(
            discord_server::server_settings::PinInput { message_id },
        ).await
    }
    async fn unpin_message(message_id: i64) -> Result<PinResult, ServerError> {
        discord_server::server_settings::unpin_message(
            discord_server::server_settings::PinInput { message_id },
        ).await
    }
    async fn list_pinned(channel_id: i64) -> Result<Vec<PinnedMessage>, ServerError> {
        discord_server::server_settings::list_pinned(
            discord_server::server_settings::PinnedChannelInput { channel_id },
        ).await
    }
    async fn pinned_count(channel_id: i64) -> Result<i64, ServerError> {
        discord_server::server_settings::pinned_count(
            discord_server::server_settings::PinnedChannelInput { channel_id },
        ).await
    }
    async fn add_reaction(message_id: i64, emoji: String) -> Result<ReactionResult, ServerError> {
        discord_server::server_settings::add_reaction(
            discord_server::server_settings::ReactionInput { message_id, emoji },
        ).await
    }
    async fn remove_reaction(message_id: i64, emoji: String) -> Result<ReactionResult, ServerError> {
        discord_server::server_settings::remove_reaction(
            discord_server::server_settings::ReactionInput { message_id, emoji },
        ).await
    }
}
