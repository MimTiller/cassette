//! Today's custom emoji: the `discord-server` server-emoji RPCs.

use super::*;

impl EmojiApi for CassetteBackend {
    async fn list_server_emoji(server_id: i64) -> Result<Vec<ServerEmoji>, ServerError> {
        discord_server::server_emoji::server_emoji(
            discord_server::server_emoji::ServerEmojiInput { server_id },
        ).await
    }
    async fn upload_emoji(server_id: i64, name: String, data_url: String) -> Result<ServerEmoji, ServerError> {
        discord_server::server_emoji::upload_emoji(
            discord_server::server_emoji::UploadEmojiInput { server_id, name, data_url },
        ).await
    }
    async fn rename_emoji(emoji_id: i64, name: String) -> Result<ServerEmoji, ServerError> {
        discord_server::server_emoji::rename_emoji(
            discord_server::server_emoji::RenameEmojiInput { emoji_id, name },
        ).await
    }
    async fn delete_emoji(emoji_id: i64) -> Result<(), ServerError> {
        discord_server::server_emoji::delete_emoji(
            discord_server::server_emoji::DeleteEmojiInput { emoji_id },
        ).await
    }
}
