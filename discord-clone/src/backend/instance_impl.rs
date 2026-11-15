//! Today's instance store: the `discord-server` instance RPCs.

use super::*;

impl InstanceApi for CassetteBackend {
    async fn instance_info() -> Result<InstanceInfo, ServerError> {
        discord_server::instance::instance_info().await
    }
    async fn get_instance_settings() -> Result<InstanceSettingsResult, ServerError> {
        discord_server::instance::get_instance_settings(()).await
    }
    async fn set_instance_settings(
        call_provider: String,
        livekit_url: String,
        livekit_api_key: String,
        livekit_api_secret: String,
        chat_backend: String,
        idea_chat_url: String,
    ) -> Result<InstanceSettingsResult, ServerError> {
        discord_server::instance::set_instance_settings(
            discord_server::instance::SetInstanceSettingsInput {
                call_provider,
                livekit_url,
                livekit_api_key,
                livekit_api_secret,
                chat_backend,
                idea_chat_url,
            },
        ).await
    }
    async fn test_call_provider(
        call_provider: String,
        livekit_url: String,
        livekit_api_key: String,
        livekit_api_secret: String,
    ) -> Result<CallTestResult, ServerError> {
        discord_server::instance::test_call_provider(
            discord_server::instance::TestCallInput {
                call_provider,
                livekit_url,
                livekit_api_key,
                livekit_api_secret,
            },
        ).await
    }
    async fn claim_instance_admin() -> Result<ClaimAdminResult, ServerError> {
        discord_server::instance::claim_instance_admin(()).await
    }
}
