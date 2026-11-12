//! The LiveKit client behind the voice seam: the official browser SDK
//! (`livekit-client`, UMD build pinned to an exact version from the jsDelivr
//! CDN) loaded as a script tag once per page, then driven through wasm-bindgen
//! / js interop. Everything provider-side — connect, microphone publishing,
//! remote-audio playback, mute/deafen on the real tracks, device switching,
//! the ActiveSpeakersChanged speaking indicators, disconnect on leave,
//! channel switch, or tab close, the camera and screen-share publications,
//! the Stop-sharing watcher, and the live remote-video feeds the viewer
//! tiles attach to — lives in this module. The rest of the app talks only to
//! the CallApi seam; nothing outside it touches LiveKit.
//!
//! The JS objects (the SDK's global namespace, one `Room` per connected
//! channel) live on the page itself; on the Rust side we keep only the
//! adapter handle and a registry of JS closures whose lifetime JS's GC owns.
//! App state (who speaks, error lines, the connection revision) rides the
//! signals handed in by the shell — see the sidebar's voice rows and the
//! `speaking` signal this module installs.
//!
//! Native builds compile every entry point here to a no-op stub: there is no
//! browser to connect, so the seam answers through the RPCs alone, exactly
//! like the other wasm-only halves (`crate::notify`, `crate::avatar_crop`).

#[cfg(target_arch = "wasm32")]
use js_sys::{Array, Function, Object, Promise, Reflect};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, JsValue, UnwrapThrowExt};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::JsFuture;
#[cfg(target_arch = "wasm32")]
use web_sys::{window, Window, Document, Element};

#[cfg(target_arch = "wasm32")]
use crate::backend::livekit_tiles;

/// The exact version on jsDelivr whose UMD build we drive (verified present
/// when this file was written; a missing CDN copy fails the load promise and
/// the join answers with the "unreachable" line rather than hanging).
#[cfg(target_arch = "wasm32")]
const SDK_URL: &'static str =
    "https://cdn.jsdelivr.net/npm/livekit-client@2.13.3/dist/livekit-client.umd.js";

/// The UMD attaches its export object to `globalThis.LivekitClient`; the
/// adapter reaches the SDK only through that global (and the room map it owns).
#[cfg(target_arch = "wasm32")]
const ADAPTER_GLOBAL: &'static str = "__cassette_voice";

/// The SDK's own global, which the adapter needs before it can run.
#[cfg(target_arch = "wasm32")]
const SDK_GLOBAL: &'static str = "LivekitClient";

/// How long a call waits for the SDK script to load, and how often it looks.
#[cfg(target_arch = "wasm32")]
const SDK_WAIT_MS: i32 = 15_000;
#[cfg(target_arch = "wasm32")]
const SDK_POLL_MS: i32 = 100;

/// The adapter script, injected as a `<script>` tag after the SDK loads: it
/// lives in the page's own scope, so it sees the SDK global and every `Room`
/// it creates; Rust reaches it by the `__cassette_voice` property alone.
/// Each verb answers a Promise; Rust awaits through `JsFuture`, which is
/// what ties the join/leave handlers to the real provider socket.
#[cfg(target_arch = "wasm32")]
const ADAPTER_SRC: &'static str = r#"
(() => {
  if (window.__cassette_voice) return;
  const SDK = window.LivekitClient;
  if (!SDK) return;
  const RoomClass = SDK.Room;
  const ROOMS = new Map();

  const disconnect_all = () => {
    for (const entry of ROOMS.values()) {
      try { entry.room.disconnect(); } catch (e) {}
    }
  };
  window.addEventListener('pagehide', disconnect_all);
  window.addEventListener('beforeunload', disconnect_all);

  const mic_publication = (room) =>
    room.localParticipant.getTrackPublication(SDK.Source.Microphone);

  // Wrap a raw MediaStreamTrack in a fresh MediaStream: the SDK keeps only
  // the track itself (`mediaStreamTrack`), and a `<video>`'s srcObject needs
  // a MediaStream — same shape the SDK's own attach path builds for its
  // playback elements. A copy that refuses to build one answers null and
  // the viewer keeps the avatar stand-in.
  const media_stream_of = (track) => {
    if (!track) return null;
    try { return new MediaStream([track]); } catch (e) { return null; }
  };

  // The caller's own video publications (camera and screen share) right now,
  // as [isCamera, isScreenShare] booleans. The app polls this to keep its
  // buttons and the server-side flags honest: when the browser ended a
  // share on its own (its "Stop sharing" bar — the SDK unpublishes the
  // track), the publication goes away and the seam flips the flag off so
  // every viewer's tile drops.
  const local_video_state = (room) => {
    let cam = false, share = false;
    room.localParticipant.videoTrackPublications.forEach((pub) => {
      if (pub.isSubscribed && pub.source === SDK.Source.Camera) cam = true;
      if (pub.isSubscribed && pub.source === SDK.Source.ScreenShare) share = true;
    });
    return [cam, share];
  };

  // The live MediaStreams for every remote VIDEO track currently
  // subscribed, as [identity, isScreenShare, stream]. The SDK keeps a
  // MediaStream on each remote track (`track.mediaStream` — the RemoteTrack
  // builds `new MediaStream([track])` itself); a copy that carries only the
  // raw track (`mediaStreamTrack`) gets wrapped the same way. The Rust tiles
  // set it as a <video>'s srcObject. Subscribing a video track does not
  // start an audio element, so publishing camera or screen share never makes
  // a peer hear themselves. A publication with neither is skipped: the tile
  // keeps the avatar stand-in.
  const remote_videos = (room) => {
    const out = [];
    room.remoteParticipants.forEach((rp) => {
      rp.videoTrackPublications.forEach((pub) => {
        if (!pub.isSubscribed) return;
        const s = (pub.track ? pub.track.mediaStream : null)
          || media_stream_of(pub.mediaStreamTrack);
        if (s && s.getVideoTracks().length > 0) {
          out.push([rp.identity, pub.source === SDK.Source.ScreenShare, s]);
        }
      });
    });
    return out;
  };

  const remote_publications = (room) => {
    const out = [];
    room.remoteParticipants.forEach((rp) => {
      rp.audioTrackPublications.forEach((pub) => out.push(pub));
    });
    return out;
  };

  window.__cassette_voice = {
    // Connect a Room for channel `ch` and publish the microphone when
    // allowed. The SDK's default autoSubscribe attaches every remote
    // participant's audio to an audio element on the page, which is what
    // plays the call. Speaking state is tracked from LiveKit's
    // ActiveSpeakersChanged events.
    connect: async (ch, url, token, canPublish) => {
      const prev = ROOMS.get(ch);
      const room = new RoomClass({ token, url });
      const entry = { room, speaking: new Set() };
      room.on('activeSpeakersChanged', (speakers) => {
        entry.speaking.clear();
        for (const p of speakers) entry.speaking.add(p.identity);
      });
      ROOMS.set(ch, entry);
      if (prev && prev.room !== room) {
        try { prev.room.disconnect(); } catch (e) {}
      }
      await room.connect(url, token);
      if (canPublish) {
        try {
          await room.localParticipant.setMicrophoneEnabled(true);
        } catch (e) {
          throw new Error('the browser blocked the microphone: ' + e.message);
        }
      }
      return true;
    },
    // Mute the caller's own published mic track (the real track, not just
    // the server-side icon), keeping the connection alive. The pinned UMD
    // only has setMuted on the publication — unmuting passes false.
    mute: async (ch) => {
      const entry = ROOMS.get(ch);
      if (!entry) throw new Error('not connected: ' + ch);
      const pub = mic_publication(entry.room);
      if (pub) pub.setMuted(true);
      return true;
    },
    unmute: async (ch) => {
      const entry = ROOMS.get(ch);
      if (!entry) throw new Error('not connected: ' + ch);
      const pub = mic_publication(entry.room);
      if (pub) pub.setMuted(false);
      return true;
    },
    // Deafen stops the incoming audio of every remote participant (their
    // published tracks on this page) and mutes the local mic as well.
    deafen: async (ch) => {
      const entry = ROOMS.get(ch);
      if (!entry) throw new Error('not connected: ' + ch);
      const pub = mic_publication(entry.room);
      if (pub) pub.setMuted(true);
      for (const remote of remote_publications(entry.room)) remote.setMuted(true);
      return true;
    },
    undeafen: async (ch) => {
      const entry = ROOMS.get(ch);
      if (!entry) throw new Error('not connected: ' + ch);
      const pub = mic_publication(entry.room);
      if (pub) pub.setMuted(false);
      for (const remote of remote_publications(entry.room)) remote.setMuted(false);
      return true;
    },
    // The identities the SDK reports as speaking right now (from its
    // ActiveSpeakersChanged tracking, plus isSpeaking as a fallback).
    speaking: async (ch) => {
      const entry = ROOMS.get(ch);
      if (!entry) return [];
      const out = [];
      entry.room.remoteParticipants.forEach((rp) => {
        if (entry.speaking.has(rp.identity) || rp.isSpeaking) out.push(rp.identity);
      });
      if (entry.room.localParticipant.isSpeaking) out.push(entry.room.localParticipant.identity);
      return out;
    },
    // List the page's media devices (labels need the mic grant, which a
    // connected room already has).
    devices: async (kind) => {
      const list = await RoomClass.getLocalDevices(kind, true);
      return list.map((d) => [d.deviceId, d.label]);
    },
    // Point the room's active input/output at `deviceId`. The pinned UMD's
    // switchActiveDevice takes (kind, deviceId), with an optional trailing
    // bool defaulting to true (stop if unused).
    switch_device: async (ch, kind, deviceId) => {
      const entry = ROOMS.get(ch);
      if (!entry) throw new Error('not connected: ' + ch);
      return await entry.room.switchActiveDevice(kind, deviceId);
    },
    // Publish or unpublish the caller's own camera and screen-share tracks:
    // setCameraEnabled / setScreenShareEnabled, gated by Speak on the seam
    // side (the server refused the flags without it before this verb ran).
    // A camera or screen track also carries its audio where the browser
    // allows it, so a share's microphone stays off and the share's own
    // audio rides the ScreenShare track.
    set_video: async (ch, camera, share) => {
      const entry = ROOMS.get(ch);
      if (!entry) throw new Error('not connected: ' + ch);
      if (camera) {
        await entry.room.localParticipant.setCameraEnabled(true);
      } else {
        entry.room.localParticipant.setCameraEnabled(false);
      }
      if (share) {
        await entry.room.localParticipant.setScreenShareEnabled(true);
      } else {
        entry.room.localParticipant.setScreenShareEnabled(false);
      }
      return true;
    },
    // The caller's own video publications (camera and screen share) the
    // caller has right now, as [isCamera, isScreenShare] — the "Stop
    // sharing" watcher: a track the browser ended by itself answers
    // [false, …] here even while the caller's button/flags say on.
    video_state: async (ch) => {
      const entry = ROOMS.get(ch);
      if (!entry) return [false, false];
      return local_video_state(entry.room);
    },
    // The live MediaStreams for every remote VIDEO track currently
    // subscribed in this room, as [identity, isScreenShare, stream].
    remote_videos: async (ch) => {
      const entry = ROOMS.get(ch);
      if (!entry) return [];
      return remote_videos(entry.room);
    },
    // Tear the room down: stop every track and leave. Also drops the entry
    // so a later join starts clean.
    disconnect: async (ch) => {
      const entry = ROOMS.get(ch);
      if (!entry) return false;
      ROOMS.delete(ch);
      await entry.room.disconnect();
      return true;
    },
  };
})();
"#;

thread_local! {
    /// Channels this browser tab holds a live room for (drives leave/switch
    /// disconnects and lets `voice_state`'s restore re-arm provider state
    /// without a new click). The `Room` JS objects live in the page's map.
    static LIVE_CHANNELS: std::cell::RefCell<Vec<i64>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// Whether the pinned SDK script tag has been appended (once per page).
    static SDK_APPENDED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// The adapter verb names we know exist, cached after the first load.
    static ADAPTER_READY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Append the pinned SDK script tag once per page. It loads asynchronously,
/// so the adapter is not appended with it: `ensure_adapter` injects the
/// adapter once the SDK global exists (an inline adapter appended at the same
/// time ran before the SDK arrived, bailed, and nothing ran it again).
#[cfg(target_arch = "wasm32")]
fn load_sdk() {
    if SDK_APPENDED.with(|c| c.get()) {
        return;
    }
    let doc: Document = window().unwrap_throw().document().unwrap_throw();
    let script_sdk: Element = doc.create_element("script").unwrap_throw();
    let _ = script_sdk.set_attribute("src", SDK_URL);
    append_to_head(&doc, &script_sdk);
    SDK_APPENDED.with(|c| c.set(true));
}

/// Append to `head` when it exists (a page mid-parse may not have one: the
/// Document itself takes the append).
#[cfg(target_arch = "wasm32")]
fn append_to_head(doc: &Document, el: &Element) {
    match doc.query_selector("head").unwrap_or(None) {
        Some(h) => {
            let _ = h.append_with_node_1(el);
        }
        None => {
            let _ = doc.append_with_node_1(el);
        }
    }
}

/// Inject the adapter once the SDK global is there (an inline script runs as
/// it is appended). Answers whether the adapter is ready.
#[cfg(target_arch = "wasm32")]
fn ensure_adapter() -> bool {
    if adapter_ready() {
        return true;
    }
    let win: Window = window().unwrap_throw();
    let sdk_loaded = Reflect::get(&win, &JsValue::from_str(SDK_GLOBAL))
        .map(|v| v.is_truthy())
        .unwrap_or(false);
    if !sdk_loaded {
        return false;
    }
    let doc: Document = win.document().unwrap_throw();
    let adapter: Element = doc.create_element("script").unwrap_throw();
    adapter.set_inner_html(ADAPTER_SRC);
    append_to_head(&doc, &adapter);
    adapter_ready()
}

/// Resolve after `ms` milliseconds (a `setTimeout` Promise).
#[cfg(target_arch = "wasm32")]
async fn sleep_ms(ms: i32) {
    let p = Promise::new(&mut |resolve, _reject| {
        let _ = window()
            .unwrap_throw()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
    });
    let _ = JsFuture::from(p).await;
}

/// Whether the page's adapter object exists yet (the SDK script tag loads
/// async: the inline adapter may have run too early; a later join retries).
#[cfg(target_arch = "wasm32")]
fn adapter_ready() -> bool {
    if ADAPTER_READY.with(|c| c.get()) {
        return true;
    }
    let win = window().unwrap_throw();
    let ok = match Reflect::get(&win, &JsValue::from_str(ADAPTER_GLOBAL)) {
        Ok(v) => v.is_truthy(),
        Err(_) => false,
    };
    if ok {
        ADAPTER_READY.with(|c| c.set(true));
    }
    ok
}

/// Pull the `message` off a rejected JS value, the text the provider or the
/// SDK threw with.
#[cfg(target_arch = "wasm32")]
fn js_err_msg(e: JsValue) -> String {
    if let Some(msg) = Reflect::get(&e, &JsValue::from_str("message")).ok() {
        return msg.as_string().unwrap_or("the call failed".to_string());
    }
    e.as_string().unwrap_or("the call failed".to_string())
}

/// Await one adapter verb as a JS `Promise`; the result comes back as a
/// `JsValue` (arrays/strings decoded by the caller).
#[cfg(target_arch = "wasm32")]
async fn call_adapter(verb: &'static str, args: Vec<JsValue>) -> (JsValue, String) {
    // The first call on a page waits for the SDK download; a CDN miss
    // answers with the "unreachable" line after SDK_WAIT_MS.
    load_sdk();
    let mut waited = 0;
    while !ensure_adapter() {
        if waited >= SDK_WAIT_MS {
            return (JsValue::UNDEFINED, "the call server could not be reached".to_string());
        }
        sleep_ms(SDK_POLL_MS).await;
        waited += SDK_POLL_MS;
    }
    let win = window().unwrap_throw();
    let adapter = Reflect::get(&win, &JsValue::from_str(ADAPTER_GLOBAL));
    if adapter.is_err() {
        return (JsValue::UNDEFINED, "the call server could not be reached".to_string());
    }
    let obj: Object = adapter.unwrap_throw().unchecked_into();
    let fnv = Reflect::get(&obj, &JsValue::from_str(verb));
    if fnv.is_err() {
        return (JsValue::UNDEFINED, "the call server refused the adapter verb".to_string());
    }
    let fn_: Function = fnv.unwrap_throw().unchecked_into();
    let arr: Array = Array::new();
    for a in args.iter() {
        arr.push(&a);
    }
    match Reflect::apply(&fn_, &JsValue::NULL, &arr) {
        Err(err) => (JsValue::UNDEFINED, js_err_msg(err)),
        Ok(promise) => {
            let p: Promise = promise.unchecked_into();
            match JsFuture::from(p).await {
                Err(err) => (JsValue::UNDEFINED, js_err_msg(err)),
                Ok(value) => (value, String::new()),
            }
        }
    }
}

/// Map a raw provider/JS error message onto the user-facing sentence the
/// voice status bar paints: mic permission denied, LiveKit unreachable,
/// a refused camera grant, or the token refused.
pub(crate) fn classify(err: String) -> String {
    let low = err.to_lowercase();
    if low.contains("signature is invalid") || low.contains("invalid token")
        || low.contains("401") || low.contains("jwt") || low.contains("token") {
        return "the call server refused the access token".to_string();
    }
    if low.contains("not connected") {
        return "you are not connected to this voice channel".to_string();
    }
    if low.contains("could not be reached") || low.contains("adapter") {
        return "the call server could not be reached".to_string();
    }
    if low.contains("notallowederror") || low.contains("permission")
        || low.contains("microphone") || low.contains("blocked")
        || low.contains("user denied") {
        return "the browser blocked the microphone".to_string();
    }
    if low.contains("camera") || low.contains("getDisplayMedia")
        || low.contains("screen share") || low.contains("screen-share") {
        return "the browser blocked the camera or screen share".to_string();
    }
    if low.contains("websocket") || low.contains("connection")
        || low.contains("timeout") || low.contains("timed out")
        || low.contains("failed to fetch") || low.contains("webrtc") {
        return "the call server could not be reached".to_string();
    }
    err
}

/// Connect the browser to channel `channel_id`'s room with the ticket: the
/// join's RPC already authorized the caller, so a failure here is a
/// provider/browser problem, not a permission one. Publishing the
/// microphone follows `can_publish` (the Speak permission the ticket carries);
/// remote audio plays through the SDK's default autoSubscription. The page
/// stays the only owner of the `Room`; Rust remembers the channel so leave
/// and channel-switch disconnects can find it.
#[cfg(target_arch = "wasm32")]
pub(crate) async fn connect(
    channel_id: i64,
    url: String,
    token: String,
    can_publish: bool,
) -> Result<(), String> {
    let args = [
        JsValue::from(channel_id.to_string()),
        JsValue::from(url),
        JsValue::from(token),
        JsValue::from(can_publish),
    ].to_vec();
    let (value, raw_err) = call_adapter("connect", args).await;
    if !raw_err.is_empty() {
        return Err(raw_err);
    }
    if !value.is_truthy() {
        return Err("the call server could not be reached".to_string());
    }
    LIVE_CHANNELS.with(|m| {
        let mut v = m.borrow_mut();
        if !v.contains(&channel_id) {
            v.push(channel_id)
        }
    });
    Ok(())
}

/// The live channel set for this tab — the seam's `leave_voice` and channel
/// switch use it to decide which rooms to drop.
#[cfg(target_arch = "wasm32")]
pub(crate) fn live_channels() -> Vec<i64> {
    LIVE_CHANNELS.with(|m| m.borrow().iter().copied().collect::<Vec<i64>>())
}

/// Drop the room (leave, switch, or tab close): stops every track on the
/// page and releases the SDK's listener closures.
#[cfg(target_arch = "wasm32")]
pub(crate) async fn disconnect(channel_id: i64) -> bool {
    let args = [JsValue::from(channel_id.to_string())].to_vec();
    let (value, _) = call_adapter("disconnect", args).await;
    LIVE_CHANNELS.with(|m| m.borrow_mut().retain(|c| *c != channel_id));
    value.is_truthy()
}

/// Mirror the server-side mute/deafen onto the real tracks; the server has
/// already accepted the flags (the seam's order: RPC first, provider second),
/// so a provider miss only affects this browser's audio, never the icons.
#[cfg(target_arch = "wasm32")]
pub(crate) async fn apply_flags(channel_id: i64, muted: bool, deafened: bool) {
    let mut_verb: &'static str = if muted { "mute" } else { "unmute" };
    let (mval, merr) = call_adapter(mut_verb, [JsValue::from(channel_id.to_string())].to_vec()).await;
    let _ = (mval, merr);
    let def_verb: &'static str = if deafened { "deafen" } else { "undeafen" };
    let (dval, derr) = call_adapter(def_verb, [JsValue::from(channel_id.to_string())].to_vec()).await;
    let _ = (dval, derr);
}

/// Publish/unpublish the caller's camera and screen-share tracks: `setCameraEnabled`
/// and `setScreenShareEnabled` behind the Speak gate the seam's `set_voice_flags`
/// RPC already enforced.
#[cfg(target_arch = "wasm32")]
pub(crate) async fn set_video(
    channel_id: i64,
    camera: bool,
    screen_share: bool,
) -> Result<(), String> {
    let args = [
        JsValue::from(channel_id.to_string()),
        JsValue::from(camera),
        JsValue::from(screen_share),
    ].to_vec();
    let (value, raw_err) = call_adapter("set_video", args).await;
    if !raw_err.is_empty() {
        return Err(raw_err);
    }
    if !value.is_truthy() {
        return Err("the call server refused the video verb".to_string());
    }
    Ok(())
}

/// The caller's own published video sources per live room: each row is
/// (channel_id, camera_on, screen_share_on) as the provider really sees it
/// right now. The "Stop sharing" watcher in the sidebar compares this to the
/// user's flags: the browser's own bar ended a share, the SDK unpublished it,
/// and this answers `share_on = false` — the seam then flips the flag off and
/// rebroadcasts so every viewer's tile drops.
#[cfg(target_arch = "wasm32")]
pub(crate) async fn video_states() -> Vec<(i64, bool, bool)> {
    let out: Vec<(i64, bool, bool)> = Vec::new();
    for channel_id in live_channels() {
        let args = [JsValue::from(channel_id.to_string())].to_vec();
        let (value, raw_err) = call_adapter("video_state", args).await;
        if raw_err.is_empty() && value.is_object() {
            let arr: Array = value.unchecked_into();
            if arr.length() == 2 {
                let cam = arr.get(0).as_bool().unwrap_or(false);
                let share = arr.get(1).as_bool().unwrap_or(false);
                out.push((channel_id, cam, share));
            }
        }
    }
    out
}

// ── native halves: no browser, the seam answers through the RPCs alone ──

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn connect(_channel_id: i64, _url: String, _token: String, _can_publish: bool) -> Result<(), String> {
    Ok(())
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn live_channels() -> Vec<i64> {
    Vec::new()
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn disconnect(_channel_id: i64) -> bool {
    false
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn apply_flags(_channel_id: i64, _muted: bool, _deafened: bool) {}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn set_video(_channel_id: i64, _camera: bool, _screen_share: bool) -> Result<(), String> {
    Ok(())
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn video_states() -> Vec<(i64, bool, bool)> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_error_vocabulary_is_the_status_bar_text() {
        // The voice status bar shows these sentences verbatim: a refused
        // token, an unreachable provider, a denied microphone, and a denied
        // camera grant each get their own plain line; anything else passes
        // through.
        assert_eq!(classify("JWT signature is invalid".to_string()), "the call server refused the access token".to_string());
        assert_eq!(classify("Failed to fetch: websocket timeout".to_string()), "the call server could not be reached".to_string());
        assert_eq!(classify("NotAllowedError: Permission denied".to_string()), "the browser blocked the microphone".to_string());
        assert_eq!(classify("getDisplayMedia rejected: NotAllowedError".to_string()), "the browser blocked the microphone".to_string());
        assert_eq!(classify("not connected: 7".to_string()), "you are not connected to this voice channel".to_string());
    }

    #[test]
    fn native_builds_connect_nothing() {
        assert!(live_channels().is_empty(), "no browser, no rooms");
    }
}
