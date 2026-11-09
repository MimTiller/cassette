//! Today's server stickers: the `discord-server` sticker RPCs.

use super::*;

impl StickersApi for CassetteBackend {
    async fn list_stickers(server_id: i64) -> Result<Vec<Sticker>, ServerError> {
        discord_server::stickers::list_stickers(
            discord_server::stickers::StickerListInput { server_id },
        ).await
    }
    async fn upload_sticker(
        server_id: i64,
        name: String,
        description: String,
        related_emoji: String,
        data_url: String,
    ) -> Result<Sticker, ServerError> {
        discord_server::stickers::upload_sticker(
            discord_server::stickers::UploadStickerInput { server_id, name, description, related_emoji, data_url },
        ).await
    }
    async fn edit_sticker(
        sticker_id: i64,
        name: String,
        description: String,
        related_emoji: String,
    ) -> Result<Sticker, ServerError> {
        discord_server::stickers::edit_sticker(
            discord_server::stickers::EditStickerInput { sticker_id, name, description, related_emoji },
        ).await
    }
    async fn delete_sticker(sticker_id: i64) -> Result<(), ServerError> {
        discord_server::stickers::delete_sticker(
            discord_server::stickers::DeleteStickerInput { sticker_id },
        ).await
    }
}
