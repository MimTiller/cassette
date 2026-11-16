//! A call's media. Everything the voice seam does in this tab — connect and
//! disconnect, mute and deafen, screen share, who is speaking, whether
//! playback is blocked — goes through here to the idea-chat calls in
//! `calls.rs`: `call-session` joined with the signed `call-auth` grant the
//! server minted (see docs/idea-chat-wire.md).

use std::cell::RefCell;
use std::collections::HashSet;

use super::*;
use super::calls;

const NOT_CONNECTED: &str = "you are not connected to this voice channel";

thread_local! {
    /// The channels this tab holds a call for.
    static CHANNELS: RefCell<HashSet<i64>> = RefCell::new(HashSet::new());
}

fn holds(channel_id: i64) -> bool {
    CHANNELS.with(|c| c.borrow().contains(&channel_id))
}

fn remember(channel_id: i64) {
    CHANNELS.with(|c| {
        c.borrow_mut().insert(channel_id);
    });
}

fn forget(channel_id: i64) {
    CHANNELS.with(|c| {
        c.borrow_mut().remove(&channel_id);
    });
}

/// Point this tab's media at `ticket` for `channel_id`. Errors are the
/// sentences the voice row shows.
pub(crate) async fn connect(channel_id: i64, ticket: &CallTicket) -> Result<(), String> {
    match ticket {
        CallTicket::IdeaChat { relay, grant } => {
            calls::connect(channel_id, relay.clone(), grant.clone()).await?;
            remember(channel_id);
            Ok(())
        }
    }
}

/// The channels this tab holds media for.
pub(crate) fn live_channels() -> Vec<i64> {
    calls::live_channels()
}

/// Drop this tab's media for `channel_id`.
pub(crate) async fn disconnect(channel_id: i64) -> bool {
    forget(channel_id);
    crate::backend::video_feeds::drop_channel(channel_id);
    calls::disconnect(channel_id).await
}

/// Apply this tab's mute (the mic) and deafen (local playback) to the call.
pub(crate) async fn apply_flags(channel_id: i64, muted: bool, deafened: bool) {
    if holds(channel_id) {
        calls::apply_flags(channel_id, muted, deafened).await;
    }
}

/// LOCAL ONLY (nothing is sent to the room): our volume on one remote's
/// audio, keyed by their RWND user id — the idea-chat relay applies it to
/// every connection that user has and to any they open later, through this
/// tab's own `Call`.
pub(crate) fn set_user_volume(channel_id: i64, user_id: i64, position: f32) -> Result<(), String> {
    if !holds(channel_id) {
        return Err(NOT_CONNECTED.to_string());
    }
    calls::set_user_volume(channel_id, user_id, position);
    Ok(())
}

/// LOCAL ONLY: mute or unmute one remote in this tab, keeping their volume
/// (unmuting restores it).
pub(crate) fn set_user_mute(channel_id: i64, user_id: i64, on: bool) -> Result<(), String> {
    if !holds(channel_id) {
        return Err(NOT_CONNECTED.to_string());
    }
    calls::set_user_mute(channel_id, user_id, on);
    Ok(())
}

/// This tab's remembered slider position for `user_id` (1.0 = as sent).
pub(crate) fn get_user_volume(channel_id: i64, user_id: i64) -> f32 {
    calls::get_user_volume(channel_id, user_id)
}

/// Whether this tab has `user_id` locally muted.
pub(crate) fn get_user_muted(channel_id: i64, user_id: i64) -> bool {
    calls::get_user_muted(channel_id, user_id)
}

/// This tab's whole local volume table, for the robot hook.
pub(crate) fn volume_state() -> String {
    calls::volume_state()
}

/// Start or stop sharing this tab's screen (and its sound where the browser can).
pub(crate) async fn set_screen_share(channel_id: i64, on: bool) -> Result<(), String> {
    if holds(channel_id) {
        return calls::set_screen_share(channel_id, on).await;
    }
    // Nothing to stop when not connected; starting needs a call.
    if on { Err(NOT_CONNECTED.to_string()) } else { Ok(()) }
}

/// Turn this tab's camera on or off.
pub(crate) async fn set_camera(channel_id: i64, on: bool) -> Result<(), String> {
    if holds(channel_id) {
        return calls::set_camera(channel_id, on).await;
    }
    if on { Err(NOT_CONNECTED.to_string()) } else { Ok(()) }
}

/// Refresh the videos this tab can show for `channel_id`; true when that changed.
/// idea-chat's call-video produces them per participant.
pub(crate) async fn refresh_videos(channel_id: i64) -> bool {
    let feeds = if holds(channel_id) { calls::videos(channel_id).await } else { Vec::new() };
    crate::backend::video_feeds::update(channel_id, feeds)
}

/// Whether this tab is still sharing its screen in `channel_id`.
pub(crate) async fn is_screen_sharing(channel_id: i64) -> bool {
    holds(channel_id) && calls::is_screen_sharing(channel_id).await
}

/// The user ids speaking in `channel_id` right now.
pub(crate) async fn speaking_ids(channel_id: i64) -> Vec<i64> {
    if holds(channel_id) { calls::speaking_ids(channel_id).await } else { Vec::new() }
}

/// Whether the browser is still holding the call's audio back.
pub(crate) async fn audio_blocked(channel_id: i64) -> bool {
    holds(channel_id) && calls::audio_blocked(channel_id).await
}

/// The call's audio elements as JSON, for the robot hook ("[]" outside a call).
pub(crate) fn audio_state(channel_id: i64) -> String {
    if holds(channel_id) { calls::audio_state(channel_id) } else { "[]".to_string() }
}
