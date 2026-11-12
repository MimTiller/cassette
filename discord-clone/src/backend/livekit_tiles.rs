//! The live remote-video feeds behind the voice tiles: the `MediaStream`
//! objects the LiveKit adapter hands over (one per subscribed VIDEO track),
//! re-wrapped as a `media_stream::MediaStream` whose zero-copy
//! `native_source` is that browser stream — the same hand `camera`'s web leg
//! makes, so a tile's `<video>` (the `video` SDK's `Video` primitive) sets
//! `srcObject` straight from it.
//!
//! Keyed by (channel, user id): the participant identity a room reports is
//! the caller's user id (the join ticket's `sub`), and a viewer looks its
//! tile's feed up by the row's channel and the roster entry's user id. The
//! poller replaces one channel's entries on every sweep, so a track
//! unsubscribing (or a participant leaving) drops its feed and shows up in
//! the sweep's changed answer, which is what makes the voice rows repaint
//! on TrackSubscribed / TrackUnsubscribed.

#[cfg(target_arch = "wasm32")]
use std::any::Any;
#[cfg(target_arch = "wasm32")]
use std::collections::HashMap;
#[cfg(target_arch = "wasm32")]
use std::rc::Rc;
#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::wasm_bindgen;
#[cfg(target_arch = "wasm32")]
use media_stream::{MediaStream, FrameWriter};
#[cfg(target_arch = "wasm32")]
use web_glue::bridge::from_bindgen;
#[cfg(target_arch = "wasm32")]
use web_glue::dom::MediaStream as WebMediaStream;

#[cfg(target_arch = "wasm32")]
web_glue::import! {
    // How many `<video>` elements on the page are painting: a tile whose
    // feed reached a first frame has a non-zero intrinsic width. The robot
    // suite reads this through `cassette.video_playing` to tell a real feed
    // from an avatar stand-in.
    fn js_playing_videos() -> u32 =
        "() => { let n = 0; const vs = document.getElementsByTagName('video');
              for (const v of vs) if (v.videoWidth > 0) n++; return n; }";
}

/// One live feed: the wrapper the tile's `<video>` displays, plus its
/// `FrameWriter` half, kept alive so the channel never tears itself down
/// while the map holds it.
type Feed = (MediaStream, FrameWriter);

thread_local! {
    static STREAMS: std::cell::RefCell<HashMap<(i64, i64), Feed>> =
        const { std::cell::RefCell::new(HashMap::new()) };
    /// The feed fingerprint per swept channel: the (user id, screen-share?)
    /// set, compared per sweep to answer "did anything move".
    static FINGERPRINT: std::cell::RefCell<HashMap<i64, Vec<(i64, bool)>>> =
        const { std::cell::RefCell::new(HashMap::new()) };
}

/// Wrap one raw browser `MediaStream` (a wasm-bindgen value from the
/// adapter's `remote_videos` verb) as the `media_stream::MediaStream` the
/// `video` SDK displays. `from_bindgen` is the sanctioned hybrid-page
/// crossing: the page's `G` slab and wasm-bindgen's heap hold the SAME
/// objects, so the wrapper's `native_source` downcast matches what the
/// `video` SDK's web leg looks for.
#[cfg(target_arch = "wasm32")]
fn wrap_feed(raw: wasm_bindgen::JsValue) -> Feed {
    let web: WebMediaStream = from_bindgen(&raw);
    let (wrapper, writer) = MediaStream::new();
    wrapper.set_native_source(Rc::new(web.clone()));
    (wrapper, writer)
}

/// Replace one channel's feeds with this sweep. Entries for the channel the
/// sweep did not mention are dropped — an unsubscribed track (TrackUnsubscribed)
/// or a participant leaving shows up exactly that way. Answers whether the
/// feed set changed, which is what makes the poller bump the voice revision
/// only on movement.
#[cfg(target_arch = "wasm32")]
pub(crate) fn sweep_channel(channel_id: i64, feeds: Vec<(i64, bool, wasm_bindgen::JsValue)>) -> bool {
    let want = feeds.iter().map(|(uid, share, _)| (uid, share)).collect::<Vec<(i64, bool)>>();
    let changed = FINGERPRINT.with(|m| {
        let prev = m.borrow().get(channel_id);
        let moved = prev.is_none() || prev.unwrap() != want;
        m.borrow().insert(channel_id, want);
        moved
    });
    STREAMS.with(|m| {
        let mut map = m.borrow();
        map.retain(move |(ch, _)| ch != channel_id);
        for (uid, _, raw) in feeds.iter() {
            map.insert((channel_id, uid), wrap_feed(raw));
        }
    });
    changed
}

/// Drop one channel's feeds entirely (the room tore down). Viewers' tiles
/// for it clear on the next voice-revision bump.
#[cfg(target_arch = "wasm32")]
pub(crate) fn drop_channel(channel_id: i64) {
    STREAMS.with(|m| m.borrow().retain(move |(ch, _)| ch != channel_id));
    FINGERPRINT.with(|m| ignore(m.borrow().remove(channel_id)));
}

/// The live feed a tile can display right now — `Some` while the channel
/// holds a subscribed feed for that participant. The `video` SDK's source
/// closure reads this on every rebuild, so a tile's `<video>` carries the
/// current stream (or nothing, which clears the `srcObject`).
#[cfg(target_arch = "wasm32")]
pub(crate) fn remote_video_for(channel_id: i64, user_id: i64) -> Option<MediaStream> {
    STREAMS.with(|m| {
        match m.borrow().get((channel_id, user_id)) {
            Some((feed, _)) => Some(feed.clone()),
            None => None
        }
    })
}

/// Whether a (channel, user id) feed exists at all — the tile gate: a row
/// only builds the `<video>` child while a real track is subscribed.
#[cfg(target_arch = "wasm32")]
pub(crate) fn has_feed(channel_id: i64, user_id: i64) -> bool {
    STREAMS.with(|m| m.borrow().contains_key((channel_id, user_id)))
}

/// How many `<video>` elements on the page are painting a frame
/// (non-zero intrinsic width). 0 when the page holds no live feeds — the
/// robot-side observable of "the tile really plays", not a stand-in.
#[cfg(target_arch = "wasm32")]
pub(crate) fn playing_videos() -> u64 {
    // SAFETY: the snippet returns 0 or a small count; no handle involved.
    unsafe { js_playing_videos() as u64 }
}

// ── native halves: no page, no feeds ──

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn sweep_channel(_channel_id: i64, _feeds: Vec<(i64, bool, usize)>) -> bool {
    false
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn drop_channel(_channel_id: i64) {}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn remote_video_for(_channel_id: i64, _user_id: i64) -> Option<media_stream::MediaStream> {
    None
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn has_feed(_channel_id: i64, _user_id: i64) -> bool {
    false
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn playing_videos() -> u64 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_builds_have_no_feeds() {
        assert!(!has_feed(1, 2), "no browser, no remote feeds");
        assert_eq!(playing_videos(), 0u64, "no page, no playing elements");
    }
}
