//! Today's voice calls: the `discord-server` voice RPCs, plus the browser's
//! own LiveKit connection driven through the seam's provider entry points in
//! `crate::backend::livekit`. The RPC owns the permission truth and the roster
//! frame; the provider half is this client talking to the room, and it lives
//! entirely in `livekit.rs` — nothing here re-implements it, and nothing outside
//! the seam touches LiveKit.

use super::*;

impl CallApi for CassetteBackend {
    // Ask the server first (it owns Connect/Speak and mints the ticket), then
    // point the browser's own room at it through the provider seam. A refusal
    // short-circuits — no provider socket is opened for a caller the server
    // barred.
    async fn join_voice(channel_id: i64) -> Result<CallTicket, ServerError> {
        let ticket = discord_server::voice::join_voice(
            discord_server::voice::JoinVoiceInput { channel_id },
        ).await?;
        match &ticket {
            CallTicket::LiveKit { url, token, can_publish } => {
                if let Err(msg) = crate::backend::livekit::connect(channel_id, url.clone(), token.clone(), *can_publish).await {
                    return Err(server::ServerError::Failed(crate::backend::livekit::classify(msg).into()));
                }
            }
            CallTicket::IdeaChat { .. } => {}
        }
        Ok(ticket)
    }

    async fn leave_voice(channel_id: i64) -> Result<LeaveVoiceResult, ServerError> {
        let result = discord_server::voice::leave_voice(
            discord_server::voice::LeaveVoiceInput { channel_id },
        ).await?;
        // Drop the browser's room for this channel; any other room this tab
        // still holds (a stale entry after a channel switch) gets torn down
        // too, so no orphan socket lingers behind the leave.
        for stale in crate::backend::livekit::live_channels() {
            if stale != channel_id {
                let _ = crate::backend::livekit::disconnect(stale).await;
            }
        }
        let _ = crate::backend::livekit::disconnect(channel_id).await;
        Ok(result)
    }

    async fn voice_state(channel_id: i64) -> Result<VoiceStateResult, ServerError> {
        let state = discord_server::voice::voice_state(
            discord_server::voice::VoiceStateInput { channel_id },
        ).await?;
        // A fresh browser that the server reports as connected re-arms its own
        // provider state without a new click: the server re-issued the ticket,
        // so point this tab's room at the same one.
        match &state.ticket {
            Some(CallTicket::LiveKit { url, token, can_publish }) => {
                if let Err(msg) = crate::backend::livekit::connect(channel_id, url.clone(), token.clone(), *can_publish).await {
                    return Err(server::ServerError::Failed(crate::backend::livekit::classify(msg).into()));
                }
            }
            Some(CallTicket::IdeaChat { .. }) => {}
            None => {}
        }
        Ok(state)
    }

    // Mute/deafen: the server owns the flags first (peers read them off the
    // roster frame), then the browser silences exactly the tracks the provider
    // published for this connection. A provider miss only affects this tab's
    // audio, never the icons.
    async fn set_voice_flags(
        channel_id: i64,
        muted: bool,
        deafened: bool,
        camera: bool,
        screen_share: bool,
    ) -> Result<(), ServerError> {
        // Screen share publishes before its flag goes up, so a cancelled
        // picker never leaves a share icon with nothing behind it; the track
        // comes down after the flag does.
        if screen_share {
            crate::backend::livekit::set_screen_share(channel_id, true)
                .await
                .map_err(|msg| server::ServerError::Failed(msg.into()))?;
        }
        discord_server::voice::set_voice_flags(
            discord_server::voice::VoiceFlagsInput {
                channel_id,
                muted,
                deafened,
                camera,
                screen_share,
            },
        ).await?;
        crate::backend::livekit::apply_flags(channel_id, muted, deafened).await;
        if !screen_share {
            let _ = crate::backend::livekit::set_screen_share(channel_id, false).await;
        }
        Ok(())
    }
}
