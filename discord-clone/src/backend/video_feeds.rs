//! The live videos a voice tile can show, by (channel, user id), whatever
//! carries the call. A provider reports its feeds as [`VideoFeed`]s holding
//! the framework's own `media_stream::MediaStream`: LiveKit wraps the
//! browser's MediaStream as one (`livekit::videos`); idea-chat's call stack
//! (`call-video`) already produces them per participant. The `video` SDK
//! plays a feed in the tile's `<video>`. A stream is only replaced when its
//! track changes, so a tile keeps playing between polls.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use media_stream::MediaStream;

/// One video a provider can show: whose it is, whether it is a screen share,
/// a key that changes when the underlying track does, the stream itself, and
/// anything the provider must keep alive while the stream is shown.
pub(crate) struct VideoFeed {
    pub(crate) user_id: i64,
    pub(crate) screen: bool,
    pub(crate) track: String,
    pub(crate) stream: MediaStream,
    pub(crate) keep_alive: Option<Rc<dyn Any>>,
}

struct Held {
    track: String,
    screen: bool,
    stream: MediaStream,
    _keep_alive: Option<Rc<dyn Any>>,
}

thread_local! {
    static FEEDS: RefCell<HashMap<(i64, i64), Held>> = RefCell::new(HashMap::new());
}

/// Replace `channel_id`'s feeds with what the provider reports now; true when
/// the set changed (the caller repaints the voice rows then).
pub(crate) fn update(channel_id: i64, feeds: Vec<VideoFeed>) -> bool {
    FEEDS.with(|f| {
        let mut held = f.borrow_mut();
        let keep: Vec<i64> = feeds.iter().map(|v| v.user_id).collect();
        let before = held.len();
        held.retain(|(ch, uid), _| *ch != channel_id || keep.contains(uid));
        let mut changed = held.len() != before;
        for v in feeds {
            let same = held
                .get(&(channel_id, v.user_id))
                .map(|h| h.track == v.track && h.screen == v.screen)
                .unwrap_or(false);
            if same {
                continue;
            }
            held.insert(
                (channel_id, v.user_id),
                Held { track: v.track, screen: v.screen, stream: v.stream, _keep_alive: v.keep_alive },
            );
            changed = true;
        }
        changed
    })
}

/// Forget `channel_id`'s feeds (the call ended); true when any were dropped.
pub(crate) fn drop_channel(channel_id: i64) -> bool {
    FEEDS.with(|f| {
        let mut held = f.borrow_mut();
        let before = held.len();
        held.retain(|(ch, _), _| *ch != channel_id);
        held.len() != before
    })
}

/// The live video for `user_id` in `channel_id`, if this tab has one.
pub(crate) fn feed(channel_id: i64, user_id: i64) -> Option<MediaStream> {
    FEEDS.with(|f| f.borrow().get(&(channel_id, user_id)).map(|h| h.stream.clone()))
}

#[cfg(target_arch = "wasm32")]
web_glue::import! {
    // How many <video> elements on the page are painting a frame.
    fn js_playing_videos() -> u32 =
        "() => { let n = 0; for (const v of document.getElementsByTagName('video')) if (v.videoWidth > 0) n++; return n; }";
}

/// How many <video> elements on the page are painting a frame (the robot's
/// check that a tile really plays, not just shows an avatar).
pub(crate) fn playing_videos() -> u32 {
    #[cfg(target_arch = "wasm32")]
    {
        // SAFETY: the snippet takes no handles and returns a plain count.
        unsafe { js_playing_videos() }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_of(user_id: i64, track: &str, screen: bool) -> VideoFeed {
        let (stream, writer) = MediaStream::new();
        VideoFeed { user_id, screen, track: track.to_string(), stream, keep_alive: Some(Rc::new(writer)) }
    }

    #[test]
    fn feeds_change_only_when_their_tracks_do() {
        let ch = 900_001;
        assert!(update(ch, vec![feed_of(1, "TR_a", false), feed_of(2, "TR_b", true)]), "first sweep adds two");
        assert!(feed(ch, 1).is_some() && feed(ch, 2).is_some());
        assert!(!update(ch, vec![feed_of(1, "TR_a", false), feed_of(2, "TR_b", true)]), "same tracks: no repaint");
        assert!(update(ch, vec![feed_of(1, "TR_c", false), feed_of(2, "TR_b", true)]), "a new track for user 1");
        assert!(update(ch, vec![feed_of(1, "TR_c", false)]), "user 2 stopped sharing");
        assert!(feed(ch, 2).is_none(), "user 2's tile falls back to the avatar");
        assert!(drop_channel(ch) && feed(ch, 1).is_none(), "leaving drops the channel");
    }
}
