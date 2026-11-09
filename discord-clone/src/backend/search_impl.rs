//! Today's message search: the `discord-server` search RPC.

use super::*;

impl SearchApi for CassetteBackend {
    async fn search_messages(server_id: i64, query: String, tz_offset_minutes: i64, offset: i64) -> Result<SearchResults, ServerError> {
        discord_server::search::search_messages(
            discord_server::search::SearchInput { server_id, query, tz_offset_minutes, offset },
        ).await
    }
}
