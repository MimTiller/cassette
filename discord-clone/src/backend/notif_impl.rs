//! Today's notification preferences: the `discord-server` settings RPCs.

use super::*;

impl NotificationsApi for CassetteBackend {
    async fn notification_settings(server_id: Option<i64>) -> Result<Vec<NotificationSetting>, ServerError> {
        discord_server::notification_settings::notification_settings(
            discord_server::notification_settings::NotificationSettingsInput { server_id },
        ).await
    }
    async fn set_notification_setting(setting: NotificationSetting) -> Result<NotificationSetting, ServerError> {
        discord_server::notification_settings::set_notification_setting(setting).await
    }
}
