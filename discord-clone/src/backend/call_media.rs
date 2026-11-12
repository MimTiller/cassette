//! A call's media, by provider. Everything the voice seam does in this tab —
//! connect and disconnect, mute and deafen, screen share, who is speaking,
//! whether playback is blocked — goes through here, and each channel answers
//! through the provider its call ticket named when it connected.
//!
//! LiveKit is `livekit.rs` (RWND-server build only). Idea-chat calls (`call-session` joined with a
//! signed `call-auth` grant; see docs/idea-chat-wire.md) are reserved: their
//! crates do not build against this framework version yet, so an idea-chat
//! ticket answers with a plain sentence instead of pretending to connect.

use std::cell::RefCell;
use std::collections::HashMap;

use super::*;

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
pub(crate) const IDEA_CHAT_CALLS_PENDING: &str = "idea-chat calls are not available in this build yet";
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
        CallTicket::LiveKit { url, token, can_publish } => {
            lk::connect(channel_id, url.clone(), token.clone(), *can_publish)
                .await
                .map_err(lk::classify)?;
            remember(channel_id, Provider::LiveKit);
            Ok(())
        }
        CallTicket::IdeaChat { .. } => Err(IDEA_CHAT_CALLS_PENDING.to_string()),
    }
}

/// The channels this tab holds media for, whatever the provider.
pub(crate) fn live_channels() -> Vec<i64> {
    let mut out: Vec<i64> = PROVIDERS.with(|p| p.borrow().keys().copied().collect());
    for ch in lk::live_channels() {
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
    match provider {
        Some(Provider::IdeaChat) => false,
        // A channel the map does not know may still hold a LiveKit room from
        // before this tab's map existed (a restore): tear that down too.
        Some(Provider::LiveKit) | None => lk::disconnect(channel_id).await,
    }
}

/// Apply this tab's mute (the mic) and deafen (local playback) to the call.
pub(crate) async fn apply_flags(channel_id: i64, muted: bool, deafened: bool) {
    if provider_of(channel_id) == Some(Provider::LiveKit) {
        lk::apply_flags(channel_id, muted, deafened).await;
    }
}

/// Start or stop sharing this tab's screen (and its sound where the browser can).
pub(crate) async fn set_screen_share(channel_id: i64, on: bool) -> Result<(), String> {
    match provider_of(channel_id) {
        Some(Provider::LiveKit) => lk::set_screen_share(channel_id, on).await,
        Some(Provider::IdeaChat) => Err(IDEA_CHAT_CALLS_PENDING.to_string()),
        // Nothing to stop when not connected; starting needs a call.
        None if !on => Ok(()),
        None => Err(NOT_CONNECTED.to_string()),
    }
}

/// Whether this tab is still sharing its screen in `channel_id`.
pub(crate) async fn is_screen_sharing(channel_id: i64) -> bool {
    match provider_of(channel_id) {
        Some(Provider::LiveKit) => lk::is_screen_sharing(channel_id).await,
        _ => false,
    }
}

/// The user ids speaking in `channel_id` right now.
pub(crate) async fn speaking_ids(channel_id: i64) -> Vec<i64> {
    match provider_of(channel_id) {
        Some(Provider::LiveKit) => lk::speaking_ids(channel_id).await,
        _ => Vec::new(),
    }
}

/// Whether the browser is still holding the call's audio back.
pub(crate) async fn audio_blocked(channel_id: i64) -> bool {
    match provider_of(channel_id) {
        Some(Provider::LiveKit) => lk::audio_blocked(channel_id).await,
        _ => false,
    }
}

/// The call's audio elements as JSON, for the robot hook ("[]" outside a call).
pub(crate) fn audio_state(channel_id: i64) -> String {
    match provider_of(channel_id) {
        Some(Provider::LiveKit) => lk::audio_state(channel_id),
        _ => "[]".to_string(),
    }
}
