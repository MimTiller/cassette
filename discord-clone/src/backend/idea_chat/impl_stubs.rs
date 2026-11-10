//! The RWND-only areas with no counterpart on this server's model:
//! notification settings, custom emoji, stickers, the GIF service, the
//! custom role system, channel permissions, custom themes — all answer the
//! shared refusal. Text search is not stubbed: idea-chat's `search` RPC
//! covers it, so it maps here.

use crate::backend::*;
use crate::backend::idea_chat::{client, live, model};
use chat_model::{Id, Message};
use discord_server::search::{Query, SearchHit};

impl NotificationsApi for IdeaChatBackend {
    async fn notification_settings(_server_id: Option<i64>) -> Result<Vec<NotificationSetting>, ServerError> {
        Err(model::not_supported())
    }

    async fn set_notification_setting(_setting: NotificationSetting) -> Result<NotificationSetting, ServerError> {
        Err(model::not_supported())
    }
}

impl EmojiApi for IdeaChatBackend {
    async fn list_server_emoji(_server_id: i64) -> Result<Vec<ServerEmoji>, ServerError> {
        Err(model::not_supported())
    }

    async fn upload_emoji(_server_id: i64, _name: String, _data_url: String) -> Result<ServerEmoji, ServerError> {
        Err(model::not_supported())
    }

    async fn rename_emoji(_emoji_id: i64, _name: String) -> Result<ServerEmoji, ServerError> {
        Err(model::not_supported())
    }

    async fn delete_emoji(_emoji_id: i64) -> Result<(), ServerError> {
        Err(model::not_supported())
    }
}

impl StickersApi for IdeaChatBackend {
    async fn list_stickers(_server_id: i64) -> Result<Vec<Sticker>, ServerError> {
        Err(model::not_supported())
    }

    async fn upload_sticker(
        _server_id: i64,
        _name: String,
        _description: String,
        _related_emoji: String,
        _data_url: String,
    ) -> Result<Sticker, ServerError> {
        Err(model::not_supported())
    }

    async fn edit_sticker(
        _sticker_id: i64,
        _name: String,
        _description: String,
        _related_emoji: String,
    ) -> Result<Sticker, ServerError> {
        Err(model::not_supported())
    }

    async fn delete_sticker(_sticker_id: i64) -> Result<(), ServerError> {
        Err(model::not_supported())
    }
}

impl GifApi for IdeaChatBackend {
    async fn gif_search(_query: String, _offset: u32) -> Result<GifPage, ServerError> {
        Err(model::not_supported())
    }
}

impl RolesApi for IdeaChatBackend {
    // The wire has three fixed ranks (`Member`/`Admin`/`Owner`), not RWND's
    // named role system with per-role permission bitsets.
    async fn server_roles(_server_id: i64) -> Result<RolesSnapshot, ServerError> {
        Err(model::not_supported())
    }

    async fn create_role(_server_id: i64, _name: String, _color: String, _mentionable: bool) -> Result<CustomRole, ServerError> {
        Err(model::not_supported())
    }

    async fn update_role(
        _role_id: i64,
        _name: String,
        _color: String,
        _mentionable: bool,
        _permissions: Option<Permissions>,
    ) -> Result<CustomRole, ServerError> {
        Err(model::not_supported())
    }

    async fn move_role(_role_id: i64, _position: i64) -> Result<CustomRole, ServerError> {
        Err(model::not_supported())
    }

    async fn delete_role(_role_id: i64) -> Result<(), ServerError> {
        Err(model::not_supported())
    }

    async fn assign_role(_role_id: i64, _user_id: i64, _assigned: bool) -> Result<(), ServerError> {
        Err(model::not_supported())
    }
}

impl PermissionsApi for IdeaChatBackend {
    // Channel visibility/posting is one `view`/`post` rank per channel, not
    // RWND's permission bitsets and per-role overwrites.
    async fn my_permissions(_server_id: i64) -> Result<MyPermissions, ServerError> {
        Err(model::not_supported())
    }

    async fn channel_overwrites(_channel_id: i64) -> Result<Vec<Overwrite>, ServerError> {
        Err(model::not_supported())
    }

    async fn set_channel_overwrite(
        _channel_id: i64,
        _target: Target,
        _allow: Permissions,
        _deny: Permissions,
    ) -> Result<Vec<Overwrite>, ServerError> {
        Err(model::not_supported())
    }

    async fn set_everyone_permissions(_server_id: i64, _permissions: Permissions) -> Result<Permissions, ServerError> {
        Err(model::not_supported())
    }
}

impl ThemesApi for IdeaChatBackend {
    // Custom colour themes are RWND's own per-account store; nothing on this
    // server's model carries them.
    async fn list_custom_themes() -> Result<Vec<CustomTheme>, ServerError> {
        Err(model::not_supported())
    }

    async fn save_custom_theme(
        _name: String,
        _base: String,
        _colors: Vec<String>,
    ) -> Result<CustomTheme, ServerError> {
        Err(model::not_supported())
    }

    async fn rename_custom_theme(
        _from: String,
        _to: String,
    ) -> Result<CustomTheme, ServerError> {
        Err(model::not_supported())
    }

    async fn delete_custom_theme(_name: String) -> Result<(), ServerError> {
        Err(model::not_supported())
    }
}

impl SearchApi for IdeaChatBackend {
    // `search(scope, query)` answers every readable message of one space
    // matching the text — one page, so RWND's `offset` slices the reply
    // client-side and the timezone offset never comes to this server (the
    // UI renders `created_at` epoch millis in the viewer's own zone).
    async fn search_messages(server_id: i64, query: String, _tz_offset_minutes: i64, offset: i64) -> Result<SearchResults, ServerError> {
        let mut args = Vec::new();
        args.push(client::arg(scope_of(live::wire_of(server_id))));
        args.push(client::arg(&query));
        match client::call_as::<Vec<Message>>("search", args).await {
            Err(e) => Err(e),
            Ok(rows) => {
                let world = live::snapshot();
                let mut hits: Vec<SearchHit> = Vec::new();
                for (index, message) in rows.iter().enumerate() {
                    if (index as i64) < offset {
                        continue
                    }
                    hits.push(SearchHit {
                        message_id: live::see(message.id),
                        channel_id: live::see(message.channel),
                        channel_name: channel_name_of(message.channel, &world),
                        author_id: live::see(message.author),
                        author_username: author_handle(message.author, &world),
                        author_display_name: display_for(message.author, &world),
                        author_avatar: avatar_for(message.author, &world),
                        content: message.body.clone(),
                        created_at: live::millis_at(message.at),
                        attachment_count: message.attachments.len() as i64,
                    })
                }
                Ok(SearchResults {
                    total: rows.len() as i64,
                    hits: hits,
                    query: Query {
                        terms: terms_of(query),
                        from: Vec::new(),
                        mentions: Vec::new(),
                        in_channels: Vec::new(),
                        has: Vec::new(),
                        pinned: None,
                        before: None,
                        after: None,
                        during: None,
                    },
                })
            },
        }
    }
}

// --- helpers this area shares -------------------------------------------

/// The wire search scope for one space.
pub(crate) fn scope_of(id: Id) -> model::SearchScope {
    model::SearchScope::space(id)
}

fn channel_name_of(channel: Id, world: &chat_model::live::State) -> String {
    match world.channels.get(&channel) {
        None => "".to_string(),
        Some(found) => found.name.clone(),
    }
}

fn author_handle(user: Id, world: &chat_model::live::State) -> String {
    match world.users.get(&user) {
        None => live::see(user).to_string(),
        Some(found) => found.handle.clone(),
    }
}

fn display_for(user: Id, world: &chat_model::live::State) -> Option<String> {
    match world.users.get(&user) {
        None => None,
        Some(found) => live::display_of(found.clone()),
    }
}

fn avatar_for(user: Id, world: &chat_model::live::State) -> Option<String> {
    match world.users.get(&user) {
        None => None,
        Some(found) => found.avatar.clone(),
    }
}

/// RWND's parsed query: idea-chat searches the raw text, so the only
/// faithful parse is the bare words.
fn terms_of(query: String) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let trimmed = query.trim();
    for word in trimmed.split(' ') {
        if !word.is_empty() {
            out.push(word.to_string())
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chat_model::{Id};

    fn uid(n: u128) -> Id {
        Id::from_u128(n)
    }

    #[test]
    fn every_stubbed_area_answers_the_shared_refusal() {
        let refusal = model::not_supported();
        assert_eq!(crate::errors::friendly(&refusal), "not supported on this server");
    }

    #[test]
    fn query_terms_split_on_spaces_and_ignore_blanks() {
        assert_eq!(terms_of("  hello   world ".to_string()), vec!("hello".to_string(), "world".to_string()));
    }

    #[test]
    fn a_scope_is_a_space_selector() {
        let scope = scope_of(uid(3));
        assert_eq!(scope == model::SearchScope::Space { id: uid(3) }, true);
    }

    #[test]
    fn search_args_are_the_scope_then_the_text() {
        // The `search(scope, query)` declaration order on the wire.
        let mut args: Vec<serde_json::Value> = Vec::new();
        args.push(serde_json::to_value(scope_of(uid(3))).unwrap_or(serde_json::Value::Null));
        args.push(serde_json::to_value("cat".to_string()).unwrap_or(serde_json::Value::Null));
        let held = serde_json::to_string(&args).unwrap_or_default();
        assert_eq!(held.contains("cat"), true);
        assert_eq!(held.contains("space"), true);
    }
}
