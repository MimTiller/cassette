//! idea-chat calls: the quic-call relay session this tab drives for a voice
//! channel — the provider half `call_media` reaches through a
//! `CallTicket::IdeaChat` (the same seam as `livekit.rs`, chosen by the
//! ticket). Join dials the relay through Nicho's `call-session` with the
//! signed grant the server minted (pinning the relay's certificate hash from
//! the run's environment), mute is our mic, deafen is LOCAL ONLY (playback
//! gain 0 here, nothing sent), speaking comes from the session's own
//! detection and the relay's announcements, and the video tiles read the
//! session's per-participant streams. Grants renew through the backend's
//! `renew_grant` so a caller the server voided stops getting them.
//!
//! Web-only like the media under it: a native build keeps no session, so
//! every entry point answers there like `livekit.rs` does natively.

#[cfg(target_arch = "wasm32")]
use std::future::Future;
#[cfg(target_arch = "wasm32")]
use std::pin::Pin;

#[cfg(target_arch = "wasm32")]
use call_session::{Access, Call, Fetch, Renew, Settings, log, set_muted, sync_screen_audio};
#[cfg(target_arch = "wasm32")]
use call_audio::{ModelSettings, Processing, Suppression, VoiceStages};
#[cfg(target_arch = "wasm32")]
use runtime_core::after_ms_detached;
#[cfg(target_arch = "wasm32")]
use runtime_shared::app_env::app_env;
#[cfg(target_arch = "wasm32")]
use runtime_shared::time::epoch_millis;
#[cfg(target_arch = "wasm32")]
use discord_server::voice::RenewGrantInput;

#[cfg(target_arch = "wasm32")]
use std::cell::Cell;
#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use std::collections::BTreeMap;
#[cfg(target_arch = "wasm32")]
use std::collections::HashMap;
#[cfg(target_arch = "wasm32")]
use std::rc::Rc;

#[cfg(target_arch = "wasm32")]
use crate::backend::video_feeds::VideoFeed;
#[cfg(target_arch = "wasm32")]
use crate::errors::friendly;

/// The audio track a remote's datagram rides: main is the microphone.
#[cfg(target_arch = "wasm32")]
const TRACK_MAIN: u8 = 0;
/// A screen share's tracks (its video, and its audio where the browser gave
/// one) ride here.
#[cfg(target_arch = "wasm32")]
const TRACK_SCREEN: u8 = 1;
/// How long past its last live read a remote still counts as playing: the
/// room's announcements hold a word's gaps, and our own marks refresh every
/// quarter-second, so a second past either is a stream that stopped.
#[cfg(target_arch = "wasm32")]
const LIVE_WINDOW_MS: i64 = 1_500;
/// How often the live marks are refreshed between hook reads.
#[cfg(target_arch = "wasm32")]
const WATCH_MS: i32 = 250;

#[cfg(target_arch = "wasm32")]
thread_local! {
    /// The one live call. Dropping the `Call` hangs up: it closes the link,
    /// releases the mic, and stops every playback output.
    static LIVE: Rc<RefCell<Option<Call>>> = Rc::default();
    /// The channel the live call belongs to, and 0 for none.
    static CHANNEL: RefCell<i64> = RefCell::new(0);
    /// The user our own grant speaks for (the grant's user field): our mic
    /// line's identity in the robot hook.
    static OWN_USER: RefCell<String> = RefCell::new(String::new());
    /// When the live call joined, on `epoch_millis`: the audio elements'
    /// playing position (their clock is wall-clock while they play).
    static JOINED_AT: RefCell<i64> = RefCell::new(0);
    /// Whether our playback is muted for everyone — LOCAL deafen: gain 0
    /// here, nothing is sent to the room.
    static DEAFENED: Cell<bool> = Cell::new(false);
    /// When each relay number's audio last looked live (their announcement,
    /// or a level the receive loop just updated), on `epoch_millis`.
    static HEARD: RefCell<HashMap<u32, i64>> = RefCell::new(HashMap::new());
}

/// A handle to the live call for the `call-session` functions that take one
/// by reference: a fresh `Rc` of the same cell, so a borrow never crosses a
/// thread_local boundary.
#[cfg(target_arch = "wasm32")]
fn live() -> Rc<RefCell<Option<Call>>> {
    LIVE.with(|l| l.clone())
}

/// Run `f` on the live call, if there is one. The borrow ends inside, so
/// nothing held across an `.await` can keep the call's cell borrowed.
#[cfg(target_arch = "wasm32")]
fn with_call<T>(f: impl FnOnce(&Call) -> T) -> Option<T> {
    LIVE.with(|l| l.borrow().as_ref().map(f))
}

/// The room to dial the relay with, read straight off the grant
/// (`g1.<room>.<user>.<send>.…`): the relay refuses a grant whose room is
/// not the room it is admitted to, so the grant — not a re-derivation —
/// names the room.
pub(crate) fn room_of(grant: &str) -> String {
    grant.split('.').nth(1).unwrap_or("").to_string()
}

/// The user the grant speaks for (a RWND user id as text): our own mic
/// line's identity, and the other end of the roster's relay-number mapping.
pub(crate) fn user_of(grant: &str) -> String {
    grant.split('.').nth(2).unwrap_or("").to_string()
}

/// Whether the grant lets us send audio: its send code carries `a`. A caller
/// without Speak gets a grant that sends nothing, and starts with the mic
/// off — the `can_publish` the LiveKit ticket carries.
pub(crate) fn can_publish(grant: &str) -> bool {
    grant.split('.').nth(3).unwrap_or("").contains('a')
}

/// The relay's pinned certificate hash for this run (empty: a CA-issued
/// certificate, which the browser checks the ordinary way). The run's
/// environment is the browser's own: the server never sees it.
#[cfg(target_arch = "wasm32")]
pub(crate) fn relay_cert_hash() -> String {
    app_env("IDEA_CHAT_RELAY_CERT_HASH").unwrap_or_default().trim().to_string()
}

/// The session's voice settings: the browser's own suppressor on the track,
/// its defaults elsewhere, and our deafen as the starting gain so the first
/// words anyone says are already at the level this device keeps for them.
#[cfg(target_arch = "wasm32")]
pub(crate) fn session_settings() -> Settings {
    Settings {
        processing: Processing {
            echo_cancellation: true,
            suppression: Suppression::Browser,
            auto_gain_control: false,
        },
        voice: VoiceStages::default(),
        model: ModelSettings::default(),
        model_path: "",
        gains: HashMap::new(),
        default_gain: if DEAFENED.with(|d| d.get()) { 0.0 } else { 1.0 },
    }
}

/// Ask the backend for a fresh grant for `channel_id`: the `call-session`
/// renews halfway through the current one's life through this callback
/// rather than a URL, so the server re-checks permissions on every renewal
/// and a caller it voided simply stops getting them (the session then says
/// through the control channel, and `on_control` hangs up).
#[cfg(target_arch = "wasm32")]
pub(crate) fn renewal_fetch(channel_id: i64) -> Fetch {
    Rc::new(move || -> Pin<Box<dyn Future<Output = Result<String, String>>>> {
        Box::pin(async move {
            match discord_server::voice::renew_grant(RenewGrantInput { channel_id }).await {
                Ok(grant) => Ok(grant),
                Err(e) => Err(friendly(&e)),
            }
        })
    })
}

/// Dial the relay and hold the call. The ticket carries the relay URL and
/// the signed grant; a refused grant or an unreachable relay answers with
/// `call-session`'s own sentence, shown under the voice row.
#[cfg(target_arch = "wasm32")]
pub(crate) async fn connect(channel_id: i64, relay: String, grant: String) -> Result<(), String> {
    hang_up();
    let access = Access {
        room: room_of(&grant),
        grant: Some(grant.clone()),
        renew: Some(Renew::With(renewal_fetch(channel_id))),
        key: None,
    };
    let call = call_session::join(&relay, &relay_cert_hash(), access, session_settings(), on_control).await?;
    let publish = can_publish(&grant);
    LIVE.with(|l| *l.borrow_mut() = Some(call));
    CHANNEL.with(|c| c.replace(channel_id));
    OWN_USER.with(|u| u.replace(user_of(&grant)));
    JOINED_AT.with(|j| j.replace(epoch_millis()));
    HEARD.with(|h| h.borrow_mut().clear());
    if !publish {
        set_muted(&live(), true);
    }
    watch_audio(channel_id);
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn connect(_channel_id: i64, _relay: String, _grant: String) -> Result<(), String> {
    Ok(())
}

/// The session's control channel, for what took the call out of ours: the
/// relay says `revoked` before closing a link the server kicked the caller
/// out of (or `refused …` after a renewal the backend denied); anything
/// else closing it is an ordinary loss. Either way we hang up: the server
/// no longer lets this caller in, and the call's own state must not keep
/// pretending they are in it.
#[cfg(target_arch = "wasm32")]
pub(crate) fn on_control(text: String) {
    if text == "revoked" || text.starts_with("refused") {
        log("call dropped: the relay took it out");
        hang_up();
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn on_control(_text: String) {}

/// Hang up: dropping the live `Call` closes the link, releases the mic and
/// stops every playback output. Answers whether a call was live.
#[cfg(target_arch = "wasm32")]
pub(crate) fn hang_up() -> bool {
    DEAFENED.with(|d| d.set(false));
    JOINED_AT.with(|j| j.replace(0));
    OWN_USER.with(|u| u.borrow_mut().clear());
    CHANNEL.with(|c| c.replace(0));
    HEARD.with(|h| h.borrow_mut().clear());
    LIVE.with(|l| l.borrow_mut().take()).is_some()
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn hang_up() -> bool {
    false
}

/// Leave the call (the seam's disconnect and channel-switch use this).
pub(crate) async fn disconnect(_channel_id: i64) -> bool {
    hang_up()
}

/// The channels this tab holds media for: the one live idea-chat call, if
/// any (the seam's leave and channel-switch use this to pick what to drop).
#[cfg(target_arch = "wasm32")]
pub(crate) fn live_channels() -> Vec<i64> {
    let ch = CHANNEL.with(|c| *c.borrow());
    if ch != 0 && LIVE.with(|l| l.borrow().is_some()) {
        vec![ch]
    } else {
        Vec::new()
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn live_channels() -> Vec<i64> {
    Vec::new()
}

/// Mute our mic (announced to the room, like the LiveKit seam's), and deafen
/// LOCAL ONLY: our playback gain goes to 0 for everyone, arriving audio
/// still comes through the link, we simply hear none of it, and nothing we
/// send changes.
#[cfg(target_arch = "wasm32")]
pub(crate) async fn apply_flags(_channel_id: i64, muted: bool, deafened: bool) {
    set_muted(&live(), muted);
    DEAFENED.with(|d| d.set(deafened));
    with_call(|c| c.set_default_volume(if deafened { 0.0 } else { 1.0 }));
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn apply_flags(_channel_id: i64, _muted: bool, _deafened: bool) {}

/// Start or stop sharing this tab's screen (and its sound where the browser
/// can): the session's own video path asks the browser and encodes the
/// share; its audio rides along once `sync_screen_audio` sees the share.
#[cfg(target_arch = "wasm32")]
pub(crate) async fn set_screen_share(_channel_id: i64, on: bool) -> Result<(), String> {
    let Some(video) = with_call(|c| c.video.clone()) else {
        return if on { Err("you are not connected to this voice channel".to_string()) } else { Ok(()) };
    };
    video.set_screen(on).await?;
    if on {
        sync_screen_audio(&live()).await;
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn set_screen_share(_channel_id: i64, on: bool) -> Result<(), String> {
    if on { Err("no call is in progress".to_string()) } else { Ok(()) }
}

/// Turn this tab's camera on or off (the same video path; the voice tile
/// shows it through `videos`).
#[cfg(target_arch = "wasm32")]
pub(crate) async fn set_camera(_channel_id: i64, on: bool) -> Result<(), String> {
    let Some(video) = with_call(|c| c.video.clone()) else {
        return if on { Err("you are not connected to this voice channel".to_string()) } else { Ok(()) };
    };
    video.set_camera(on).await
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn set_camera(_channel_id: i64, on: bool) -> Result<(), String> {
    if on { Err("no call is in progress".to_string()) } else { Ok(()) }
}

/// Whether this tab is still sharing its screen (the browser's own control
/// can end a share; then we treat it as off and clean up on the next ask).
#[cfg(target_arch = "wasm32")]
pub(crate) async fn is_screen_sharing(_channel_id: i64) -> bool {
    let Some(video) = with_call(|c| c.video.clone()) else { return false };
    if video.screen_ended() {
        let _ = video.set_screen(false).await;
        return false;
    }
    video.screen_on()
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn is_screen_sharing(_channel_id: i64) -> bool {
    false
}

/// The videos this tab can show: every remote's camera and screen share, as
/// framework `MediaStream`s (`VideoFeed`s), with each relay number's user id
/// read off the roster (the grant's user field, which the relay announces
/// back as `#<id> is <user>`). The seam hands these to the voice tiles.
#[cfg(target_arch = "wasm32")]
pub(crate) async fn videos(_channel_id: i64) -> Vec<VideoFeed> {
    let Some((video, roster)) = with_call(|c| (c.video.clone(), c.roster.clone())) else {
        return Vec::new();
    };
    let mut feeds: Vec<VideoFeed> = Vec::new();
    for (id, track) in video.remotes() {
        let Some(stream) = video.remote(id, track) else { continue };
        let user = roster.borrow().names.get(&id).cloned().unwrap_or_default();
        if user.is_empty() {
            continue;
        }
        feeds.push(VideoFeed {
            user_id: user.parse::<i64>().unwrap_or(0),
            screen: track == TRACK_SCREEN,
            track: format!("{id}t{track}"),
            stream,
            keep_alive: None,
        });
    }
    feeds
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn videos(_channel_id: i64) -> Vec<crate::backend::video_feeds::VideoFeed> {
    Vec::new()
}

/// The user ids speaking right now: the room's announcements (each client
/// says of itself, held over a word's gaps) plus our own detection — mapped
/// through the roster to RWND user ids.
#[cfg(target_arch = "wasm32")]
pub(crate) async fn speaking_ids(channel_id: i64) -> Vec<i64> {
    if CHANNEL.with(|c| *c.borrow()) != channel_id {
        return Vec::new();
    }
    touch_live();
    let Some((roster, talking)) = with_call(|c| (c.roster.clone(), c.speaking())) else {
        return Vec::new();
    };
    let announced: Vec<u32> = roster.borrow().speaking.iter().copied().collect();
    let mut out: Vec<i64> = Vec::new();
    for id in announced {
        let uid = roster.borrow().names.get(&id).and_then(|u| u.parse::<i64>().ok()).unwrap_or(0);
        if uid != 0 && !out.contains(&uid) {
            out.push(uid);
        }
    }
    if talking {
        let own = OWN_USER.with(|u| u.borrow().parse::<i64>().unwrap_or(0));
        if own != 0 && !out.contains(&own) {
            out.push(own);
        }
    }
    out.sort_unstable();
    out
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn speaking_ids(_channel_id: i64) -> Vec<i64> {
    Vec::new()
}

/// Whether the browser is still holding the call's audio back: a web build
/// that joined has already opened its worklets (a browser without them fails
/// the join outright), so there is nothing further blocked.
pub(crate) async fn audio_blocked(_channel_id: i64) -> bool {
    false
}

/// Refresh the per-person "this stream is live now" marks: the room's
/// announcements plus the levels the receive loop updates each time audio of
/// theirs arrives in this tab. Detached fire-and-forget, and it stops by
/// itself once the live call is gone (our poll checks the channel).
#[cfg(target_arch = "wasm32")]
fn touch_live() {
    let Some((roster, people)) = with_call(|c| (c.roster.clone(), c.people.clone())) else {
        return;
    };
    let now = epoch_millis();
    let speaking = roster.borrow().speaking.clone();
    let ids: Vec<u32> = people.borrow().keys().copied().collect();
    HEARD.with(|h| {
        let mut h = h.borrow_mut();
        for id in ids {
            let announced = speaking.contains(&id);
            let arriving = people.borrow().get(&id).copied().unwrap_or(0.0) > 0.0;
            if announced || arriving {
                h.insert(id, now);
            }
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn touch_live() {}

/// Keep the live marks fresh between hook reads: every quarter-second while
/// this call is the live one, re-mark what is speaking and what is arriving.
#[cfg(target_arch = "wasm32")]
pub(crate) fn watch_audio(channel_id: i64) {
    after_ms_detached(WATCH_MS, move || {
        if CHANNEL.with(|c| *c.borrow()) != channel_id {
            return;
        }
        touch_live();
        watch_audio(channel_id);
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn watch_audio(_channel_id: i64) {}

/// The call's audio as JSON, for the robot hook: one line per person OUR
/// browser hears (and one for our own mic), with what our playback does with
/// it — who, microphone, paused (their stream stopped coming), muted (our
/// LOCAL deafen), readyState (have we heard any of their audio yet), and a
/// playing position that advances while they play. "[]" when no live call.
#[cfg(target_arch = "wasm32")]
pub(crate) fn audio_state(channel_id: i64) -> String {
    if CHANNEL.with(|c| *c.borrow()) != channel_id {
        return "[]".to_string();
    }
    let Some((roster, sending)) = with_call(|c| (c.roster.clone(), !c.muted())) else {
        return "[]".to_string();
    };
    touch_live();
    let now = epoch_millis();
    let deafened = DEAFENED.with(|d| d.get());
    let joined = JOINED_AT.with(|j| *j.borrow());
    let snapshot: BTreeMap<u32, i64> = HEARD.with(|h| h.borrow().iter().map(|(id, t)| (*id, *t)).collect());
    let mut lines: Vec<String> = Vec::new();
    for (id, heard_at) in snapshot {
        let user = roster.borrow().names.get(&id).cloned().unwrap_or_default();
        if user.is_empty() {
            continue;
        }
        let paused = !(heard_at > 0 && now - heard_at < LIVE_WINDOW_MS);
        let ready = if heard_at > 0 { 2 } else { 0 };
        let position = if paused { 0.0 } else { (now - joined) as f64 / 1000.0 };
        lines.push(format!(
            "{{\"identity\":\"{user}\",\"source\":\"microphone\",\"paused\":{paused},\"muted\":{deafened},\"readyState\":{ready},\"currentTime\":{position}}}"
        ));
    }
    let own = OWN_USER.with(|u| u.borrow().clone());
    if !own.is_empty() {
        let paused = !sending;
        let ready = if sending { 2 } else { 0 };
        let position = if paused { 0.0 } else { (now - joined) as f64 / 1000.0 };
        lines.push(format!(
            "{{\"identity\":\"{own}\",\"source\":\"microphone\",\"paused\":{paused},\"muted\":false,\"readyState\":{ready},\"currentTime\":{position}}}"
        ));
    }
    format!("[{}]", lines.join(","))
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn audio_state(_channel_id: i64) -> String {
    "[]".to_string()
}

/// The seam's error sentences: `call-session` answers with its own plain
/// sentences already ("did not let us in…", "did not answer within…").
pub(crate) fn classify(err: String) -> String {
    err
}
