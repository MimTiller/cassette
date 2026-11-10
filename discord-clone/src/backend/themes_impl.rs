//! Today's server custom themes: the `discord-server` theme RPCs.

use super::*;

impl ThemesApi for CassetteBackend {
    async fn list_custom_themes() -> Result<Vec<CustomTheme>, ServerError> {
        discord_server::custom_themes::list_custom_themes().await
    }
    async fn save_custom_theme(
        name: String,
        base: String,
        colors: Vec<String>,
    ) -> Result<CustomTheme, ServerError> {
        discord_server::custom_themes::save_custom_theme(
            discord_server::custom_themes::SaveCustomThemeInput { name, base, colors },
        ).await
    }
    async fn rename_custom_theme(
        from: String,
        to: String,
    ) -> Result<CustomTheme, ServerError> {
        discord_server::custom_themes::rename_custom_theme(
            discord_server::custom_themes::RenameCustomThemeInput { from, to },
        ).await
    }
    async fn delete_custom_theme(name: String) -> Result<(), ServerError> {
        discord_server::custom_themes::delete_custom_theme(
            discord_server::custom_themes::DeleteCustomThemeInput { name },
        ).await
    }
}
