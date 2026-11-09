//! Today's GIF search: the `discord-server` gif-search RPC.

use super::*;

impl GifApi for CassetteBackend {
    async fn gif_search(query: String, offset: u32) -> Result<GifPage, ServerError> {
        discord_server::gifs::gif_search(discord_server::gifs::GifSearchInput { query, offset }).await
    }
}
