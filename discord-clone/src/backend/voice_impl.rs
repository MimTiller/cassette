//! Today's voice calls: the `discord-server` voice RPCs.

use super::*;

impl CallApi for CassetteBackend {
    async fn join_voice(channel_id: i64) -> Result<CallTicket, ServerError> {
        discord_server::voice::join_voice(
            discord_server::voice::JoinVoiceInput { channel_id },
        ).await
    }

    async fn leave_voice(channel_id: i64) -> Result<LeaveVoiceResult, ServerError> {
        discord_server::voice::leave_voice(
            discord_server::voice::LeaveVoiceInput { channel_id },
        ).await
    }

    async fn voice_state(channel_id: i64) -> Result<VoiceStateResult, ServerError> {
        discord_server::voice::voice_state(
            discord_server::voice::VoiceStateInput { channel_id },
        ).await
    }

    async fn set_voice_flags(
        channel_id: i64,
        muted: bool,
        deafened: bool,
        camera: bool,
        screen_share: bool,
    ) -> Result<(), ServerError> {
        discord_server::voice::set_voice_flags(
            discord_server::voice::VoiceFlagsInput {
                channel_id,
                muted,
                deafened,
                camera,
                screen_share,
            },
        ).await
    }
}
