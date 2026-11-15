//! A call's media, by provider. Everything the voice seam does in this tab —
//! connect and disconnect, mute and deafen, screen share, who is speaking,
//! whether playback is blocked — goes through here, and each channel answers
//! through the provider its call ticket named when it connected.
//!
//! LiveKit is `livekit.rs` (RWND-server build only). Idea-chat calls are
//! `calls.rs`: `call-session` joined with the signed `call-auth` grant the
//! server minted (see docs/idea-chat-wire.md).

use std::cell::RefCell;
use std::collections::HashMap;

use super::*;
use super::calls;

// The LiveKit half exists in the RWND-server build; the idea-chat build has
// no LiveKit provider, so its stand-in answers as "no call here".
#[cfg(not(feature = "idea-chat"))]
use super::livekit as lk;

#[cfg(feature = "idea-chat")]
mod lk {
    pub(crate) async fn connect(_channel_id: i64, _url: String, _token: String, _can_publish: bool) -> Result<(), String> {
        Err(super::IDEA_CHAT_CALLS_PENDING.to_string())
    }
    pub(crate) fn classify(err: String) -> String {
        err
    }
    pub(crate) fn live_channels() -> Vec<i64> {
        Vec::new()
    }
    pub(crate) async fn disconnect(_channel_id: i64) -> bool {
        false
    }
    pub(crate) async fn apply_flags(_channel_id: i64, _muted: bool, _deafened: bool) {}
    pub(crate) async fn set_screen_share(_channel_id: i64, _on: bool) -> Result<(), String> {
        Err(super::IDEA_CHAT_CALLS_PENDING.to_string())
    }
    pub(crate) async fn is_screen_sharing(_channel_id: i64) -> bool {
        false
    }
    pub(crate) async fn set_camera(_channel_id: i64, _on: bool) -> Result<(), String> {
        Err(super::IDEA_CHAT_CALLS_PENDING.to_string())
    }
    pub(crate) async fn videos(_channel_id: i64) -> Vec<crate::backend::video_feeds::VideoFeed> {
        Vec::new()
    }
    pub(crate) async fn speaking_ids(_channel_id: i64) -> Vec<i64> {
        Vec::new()
    }
    pub(crate) async fn audio_blocked(_channel_id: i64) -> bool {
        false
    }
    pub(crate) fn audio_state(_channel_id: i64) -> String {
        "[]".to_string()
    }
}

/// Which provider carries a channel's media in this tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Provider {
    LiveKit,
    IdeaChat,
}

/// The user-facing line for a provider this build cannot drive yet.
#[cfg(feature = "idea-chat")]
pub(crate) const IDEA_CHAT_CALLS_PENDING: &str = "idea-chat calls are not available in this build yet";
const NOT_CONNECTED: &str = "you are not connected to this voice channel";
/// A LiveKit call keeps no per-remote local volume, so a tile menu that tries
/// to set one says so rather than painting a control that does nothing.
const LIVEKIT_NO_REMOTE_VOLUME: &str = "this call cannot set one person's volume on your own";

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
        CallTicket::LiveKit { url, token, can_publish } => {
            lk::connect(channel_id, url.clone(), token.clone(), *can_publish)
                .await
                .map_err(lk::classify)?;
            remember(channel_id, Provider::LiveKit);
            Ok(())
        }
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
    for ch in lk::live_channels().into_iter().chain(calls::live_channels()) {
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
        // A channel the map does not know may still hold a LiveKit room from
        // before this tab's map existed (a restore): tear that down too.
        Some(Provider::LiveKit) | None => lk::disconnect(channel_id).await,
    }
}

/// Apply this tab's mute (the mic) and deafen (local playback) to the call.
pub(crate) async fn apply_flags(channel_id: i64, muted: bool, deafened: bool) {
    match provider_of(channel_id) {
        Some(Provider::LiveKit) => lk::apply_flags(channel_id, muted, deafened).await,
        Some(Provider::IdeaChat) => calls::apply_flags(channel_id, muted, deafened).await,
        None => {}
    }
}

/// LOCAL ONLY (nothing is sent to the room): our volume on one remote's
/// audio, keyed by their RWND user id — the idea-chat relay applies it to
/// every connection that user has and to any they open later, through this
/// tab's own `Call`. A LiveKit call has no per-remote local volume, so
/// setting one there answers a user-facing refusal; reads answer the
/// defaults so the tile still paints.
pub(crate) fn set_user_volume(channel_id: i64, user_id: i64, position: f32) -> Result<(), String> {
    match provider_of(channel_id) {
        Some(Provider::IdeaChat) => {
            calls::set_user_volume(channel_id, user_id, position);
            Ok(())
        }
        Some(Provider::LiveKit) => Err(LIVEKIT_NO_REMOTE_VOLUME.to_string()),
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
        Some(Provider::LiveKit) => Err(LIVEKIT_NO_REMOTE_VOLUME.to_string()),
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
        Some(Provider::LiveKit) => lk::set_screen_share(channel_id, on).await,
        Some(Provider::IdeaChat) => calls::set_screen_share(channel_id, on).await,
        // Nothing to stop when not connected; starting needs a call.
        None if !on => Ok(()),
        None => Err(NOT_CONNECTED.to_string()),
    }
}

/// Turn this tab's camera on or off.
pub(crate) async fn set_camera(channel_id: i64, on: bool) -> Result<(), String> {
    match provider_of(channel_id) {
        Some(Provider::LiveKit) => lk::set_camera(channel_id, on).await,
        Some(Provider::IdeaChat) => calls::set_camera(channel_id, on).await,
        None if !on => Ok(()),
        None => Err(NOT_CONNECTED.to_string()),
    }
}

/// Refresh the videos this tab can show for `channel_id`; true when that changed.
/// Each provider reports framework `MediaStream`s: LiveKit wraps the browser's;
/// idea-chat's call-video produces them per participant.
pub(crate) async fn refresh_videos(channel_id: i64) -> bool {
    let feeds = match provider_of(channel_id) {
        Some(Provider::LiveKit) => lk::videos(channel_id).await,
        Some(Provider::IdeaChat) => calls::videos(channel_id).await,
        None => Vec::new(),
    };
    crate::backend::video_feeds::update(channel_id, feeds)
}

/// Whether this tab is still sharing its screen in `channel_id`.
pub(crate) async fn is_screen_sharing(channel_id: i64) -> bool {
    match provider_of(channel_id) {
        Some(Provider::LiveKit) => lk::is_screen_sharing(channel_id).await,
        Some(Provider::IdeaChat) => calls::is_screen_sharing(channel_id).await,
        None => false,
    }
}

/// The user ids speaking in `channel_id` right now.
pub(crate) async fn speaking_ids(channel_id: i64) -> Vec<i64> {
    match provider_of(channel_id) {
        Some(Provider::LiveKit) => lk::speaking_ids(channel_id).await,
        Some(Provider::IdeaChat) => calls::speaking_ids(channel_id).await,
        None => Vec::new(),
    }
}

/// Whether the browser is still holding the call's audio back.
pub(crate) async fn audio_blocked(channel_id: i64) -> bool {
    match provider_of(channel_id) {
        Some(Provider::LiveKit) => lk::audio_blocked(channel_id).await,
        Some(Provider::IdeaChat) => calls::audio_blocked(channel_id).await,
        None => false,
    }
}

/// The call's audio elements as JSON, for the robot hook ("[]" outside a call).
pub(crate) fn audio_state(channel_id: i64) -> String {
    match provider_of(channel_id) {
        Some(Provider::LiveKit) => lk::audio_state(channel_id),
        Some(Provider::IdeaChat) => calls::audio_state(channel_id),
        None => "[]".to_string(),
    }
}
