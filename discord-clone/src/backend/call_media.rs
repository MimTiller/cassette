//! A call's media, by provider. Everything the voice seam does in this tab —
//! connect and disconnect, mute and deafen, screen share, who is speaking,
//! whether playback is blocked — goes through here, and each channel answers
//! through the provider its call ticket named when it connected.
//!
//! Idea-chat calls are `calls.rs`: `call-session` joined with the signed
//! `call-auth` grant the server minted (see docs/idea-chat-wire.md).

use std::cell::RefCell;
use std::collections::HashMap;

use super::*;
use super::calls;

/// Which provider carries a channel's media in this tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Provider {
    IdeaChat,
}

const NOT_CONNECTED: &str = "you are not connected to this voice channel";

thread_local! {
    /// The provider of each channel this tab holds media for.
    static PROVIDERS: RefCell<HashMap<i64, Provider>> = RefCell::new(HashMap::new());
}

fn provider_of(channel_id: i64) -> Option<Provider> {
    PROVIDERS.with(|p| p.borrow().get(&channel_id).copied())
}

fn remember(channel_id: i64, provider: Provider) {
    PROVIDERS.with(|p| {
        p.borrow_mut().insert(channel_id, provider);
    });
}

fn forget(channel_id: i64) {
    PROVIDERS.with(|p| {
        p.borrow_mut().remove(&channel_id);
    });
}

/// Point this tab's media at `ticket` for `channel_id`. Errors are the
/// sentences the voice row shows.
pub(crate) async fn connect(channel_id: i64, ticket: &CallTicket) -> Result<(), String> {
    match ticket {
        CallTicket::IdeaChat { relay, grant } => {
            calls::connect(channel_id, relay.clone(), grant.clone()).await?;
            remember(channel_id, Provider::IdeaChat);
            Ok(())
        }
    }
}

/// The channels this tab holds media for, whatever the provider.
pub(crate) fn live_channels() -> Vec<i64> {
    let mut out: Vec<i64> = PROVIDERS.with(|p| p.borrow().keys().copied().collect());
    for ch in calls::live_channels() {
        if !out.contains(&ch) {
            out.push(ch);
        }
    }
    out
}

/// Drop this tab's media for `channel_id`.
pub(crate) async fn disconnect(channel_id: i64) -> bool {
    let provider = provider_of(channel_id);
    forget(channel_id);
    crate::backend::video_feeds::drop_channel(channel_id);
    match provider {
        Some(Provider::IdeaChat) => calls::disconnect(channel_id).await,
        None => false,
    }
}

/// Apply this tab's mute (the mic) and deafen (local playback) to the call.
pub(crate) async fn apply_flags(channel_id: i64, muted: bool, deafened: bool) {
    match provider_of(channel_id) {
        Some(Provider::IdeaChat) => calls::apply_flags(channel_id, muted, deafened).await,
        None => {}
    }
}

/// LOCAL ONLY (nothing is sent to the room): our volume on one remote's
/// audio, keyed by their RWND user id — the idea-chat relay applies it to
/// every connection that user has and to any they open later, through this
/// tab's own `Call`.
pub(crate) fn set_user_volume(channel_id: i64, user_id: i64, position: f32) -> Result<(), String> {
    match provider_of(channel_id) {
        Some(Provider::IdeaChat) => {
            calls::set_user_volume(channel_id, user_id, position);
            Ok(())
        }
        None => Err(NOT_CONNECTED.to_string()),
    }
}

/// LOCAL ONLY: mute or unmute one remote in this tab, keeping their volume
/// (unmuting restores it).
pub(crate) fn set_user_mute(channel_id: i64, user_id: i64, on: bool) -> Result<(), String> {
    match provider_of(channel_id) {
        Some(Provider::IdeaChat) => {
            calls::set_user_mute(channel_id, user_id, on);
            Ok(())
        }
        None => Err(NOT_CONNECTED.to_string()),
    }
}

/// This tab's remembered slider position for `user_id` (1.0 = as sent).
pub(crate) fn get_user_volume(channel_id: i64, user_id: i64) -> f32 {
    match provider_of(channel_id) {
        Some(Provider::IdeaChat) => calls::get_user_volume(channel_id, user_id),
        _ => 1.0,
    }
}

/// Whether this tab has `user_id` locally muted.
pub(crate) fn get_user_muted(channel_id: i64, user_id: i64) -> bool {
    match provider_of(channel_id) {
        Some(Provider::IdeaChat) => calls::get_user_muted(channel_id, user_id),
        _ => false,
    }
}

/// This tab's whole local volume table, for the robot hook.
pub(crate) fn volume_state() -> String {
    calls::volume_state()
}

/// Start or stop sharing this tab's screen (and its sound where the browser can).
pub(crate) async fn set_screen_share(channel_id: i64, on: bool) -> Result<(), String> {
    match provider_of(channel_id) {
        Some(Provider::IdeaChat) => calls::set_screen_share(channel_id, on).await,
        // Nothing to stop when not connected; starting needs a call.
        None if !on => Ok(()),
        None => Err(NOT_CONNECTED.to_string()),
    }
}

/// Turn this tab's camera on or off.
pub(crate) async fn set_camera(channel_id: i64, on: bool) -> Result<(), String> {
    match provider_of(channel_id) {
        Some(Provider::IdeaChat) => calls::set_camera(channel_id, on).await,
        None if !on => Ok(()),
        None => Err(NOT_CONNECTED.to_string()),
    }
}

/// Refresh the videos this tab can show for `channel_id`; true when that
/// changed. The idea-chat call-video produces framework `MediaStream`s per
/// participant.
pub(crate) async fn refresh_videos(channel_id: i64) -> bool {
    let feeds = match provider_of(channel_id) {
        Some(Provider::IdeaChat) => calls::videos(channel_id).await,
        None => Vec::new(),
    };
    crate::backend::video_feeds::update(channel_id, feeds)
}

/// Whether this tab is still sharing its screen in `channel_id`.
pub(crate) async fn is_screen_sharing(channel_id: i64) -> bool {
    match provider_of(channel_id) {
        Some(Provider::IdeaChat) => calls::is_screen_sharing(channel_id).await,
        None => false,
    }
}

/// The user ids speaking in `channel_id` right now.
pub(crate) async fn speaking_ids(channel_id: i64) -> Vec<i64> {
    match provider_of(channel_id) {
        Some(Provider::IdeaChat) => calls::speaking_ids(channel_id).await,
        None => Vec::new(),
    }
}

/// Whether the browser is still holding the call's audio back.
pub(crate) async fn audio_blocked(channel_id: i64) -> bool {
    match provider_of(channel_id) {
        Some(Provider::IdeaChat) => calls::audio_blocked(channel_id).await,
        None => false,
    }
}

/// The call's audio elements as JSON, for the robot hook ("[]" outside a call).
pub(crate) fn audio_state(channel_id: i64) -> String {
    match provider_of(channel_id) {
        Some(Provider::IdeaChat) => calls::audio_state(channel_id),
        None => "[]".to_string(),
    }
}
