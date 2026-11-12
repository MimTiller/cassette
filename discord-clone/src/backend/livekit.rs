//! The LiveKit client behind the voice seam: the official browser SDK
//! (`livekit-client`, UMD build pinned to an exact version from the jsDelivr
//! CDN) loaded as a script tag once per page, then driven through wasm-bindgen
//! / js interop. Everything provider-side — connect, microphone publishing,
//! remote-audio playback, mute/deafen on the real tracks, device switching,
//! the ActiveSpeakersChanged speaking indicators, and disconnect on leave,
//! channel switch, or tab close — lives in this module. The rest of the app
//! talks only to the CallApi seam; nothing outside it touches LiveKit.
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

/// The adapter script, injected as a `<script>` tag after the SDK loads: it
/// lives in the page's own scope, so it sees the SDK global and every `Room`
/// it creates; Rust reaches it by the `__cassette_voice` property alone.
/// Each verb answers a Promise; Rust awaits through `JsFuture`, which is
/// what ties the join/leave handlers to the real provider socket.
#[cfg(target_arch = "wasm32")]
const ADAPTER_SRC: &'static str = r"
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

  // Publish a publication's tracks (its mic/screen-share audio) to the
  // remote side, so deafen/mute can silence exactly those tracks.
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
";

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

/// Append the pinned SDK script tag and the adapter tag to the document
/// head. The adapter bails while the SDK global is missing, and Rust
/// re-appends it (once per call, below the first) until
/// `__cassette_voice` exists — a CDN miss simply leaves the adapter absent,
/// and the join answers through the seam's error line instead.
#[cfg(target_arch = "wasm32")]
fn load_sdk_and_adapter() -> bool {
    if SDK_APPENDED.with(|c| c.get()) {
        return ADAPTER_READY.with(|c| c.get());
    }
    let win: Window = window().unwrap_throw();
    let doc: Document = win.document().unwrap_throw();
    // The pinned UMD and the adapter land on `head` when it exists (a page
    // mid-parse may not have one: the Document itself takes the append).
    let head: Option<Element> = doc.query_selector("head").unwrap_or(None);
    let script_sdk: Element = doc.create_element("script").unwrap_throw();
    let _ = script_sdk.set_attribute("src", SDK_URL);
    let adapter: Element = doc.create_element("script").unwrap_throw();
    adapter.set_inner_html(ADAPTER_SRC);
    if let Some(h) = head {
        let _ = h.append_with_node_1(&script_sdk);
        let _ = h.append_with_node_1(&adapter);
    } else {
        let _ = doc.append_with_node_1(&script_sdk);
        let _ = doc.append_with_node_1(&adapter);
    }
    SDK_APPENDED.with(|c| c.set(true));
    true
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
    if !load_sdk_and_adapter() {
        return (JsValue::UNDEFINED, "the call server could not be reached".to_string());
    }
    if !adapter_ready() {
        return (JsValue::UNDEFINED, "the call server could not be reached".to_string());
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
/// voice status bar paints: mic permission denied, LiveKit unreachable, or
/// the token refused.
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
    LIVE_CHANNELS.with(|m| { let mut v = m.borrow_mut(); if !v.contains(&channel_id) { v.push(channel_id) } });
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

#[cfg(test)]
mod tests {

    #[test]
    fn the_error_vocabulary_is_the_status_bar_text() {
        // The voice status bar shows these sentences verbatim: a refused
        // token, an unreachable provider, and a denied microphone each get
        // their own plain line; anything else passes through.
        assert_eq!(classify("JWT signature is invalid".to_string()), "the call server refused the access token".to_string());
        assert_eq!(classify("Failed to fetch: websocket timeout".to_string()), "the call server could not be reached".to_string());
        assert_eq!(classify("NotAllowedError: Permission denied".to_string()), "the browser blocked the microphone".to_string());
        assert_eq!(classify("not connected: 7".to_string()), "you are not connected to this voice channel".to_string());
    }

    #[test]
    fn native_builds_connect_nothing() {
        assert!(live_channels().is_empty(), "no browser, no rooms");
    }
}
