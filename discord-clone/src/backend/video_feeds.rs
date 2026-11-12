//! The live videos a voice tile can show, by (channel, user id), whatever the
//! call provider: each is the browser's MediaStream wrapped as the framework's
//! `media_stream::MediaStream` (its `native_source` is the browser stream, which
//! the `video` SDK's web leg sets as the `<video>`'s `srcObject`). The provider
//! hands raw streams to `update` on each poll; a stream is only re-wrapped when
//! its track changes, so a tile's `<video>` keeps playing between polls.

/// A provider's raw stream handle: the browser MediaStream on the web.
#[cfg(target_arch = "wasm32")]
pub(crate) type RawStream = wasm_bindgen::JsValue;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) type RawStream = ();

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    use media_stream::{FrameWriter, MediaStream};
    use web_glue::bridge::from_bindgen;
    use web_glue::dom::MediaStream as WebMediaStream;

    struct Feed {
        track: String,
        screen: bool,
        stream: MediaStream,
        // Held so the wrapper's channel stays open while the tile shows it.
        _writer: FrameWriter,
    }

    thread_local! {
        static FEEDS: RefCell<HashMap<(i64, i64), Feed>> = RefCell::new(HashMap::new());
    }

    web_glue::import! {
        // How many <video> elements on the page are painting a frame.
        fn js_playing_videos() -> u32 =
            "() => { let n = 0; for (const v of document.getElementsByTagName('video')) if (v.videoWidth > 0) n++; return n; }";
    }

    pub(crate) fn update(channel_id: i64, raw: Vec<(i64, bool, String, super::RawStream)>) -> bool {
        FEEDS.with(|f| {
            let mut feeds = f.borrow_mut();
            let mut changed = false;
            let keep: Vec<i64> = raw.iter().map(|(uid, _, _, _)| *uid).collect();
            let before = feeds.len();
            feeds.retain(|(ch, uid), _| *ch != channel_id || keep.contains(uid));
            changed |= feeds.len() != before;
            for (uid, screen, track, js) in raw {
                let same = feeds
                    .get(&(channel_id, uid))
                    .map(|f| f.track == track && f.screen == screen)
                    .unwrap_or(false);
                if same {
                    continue;
                }
                let web: WebMediaStream = from_bindgen(&js);
                let (stream, writer) = MediaStream::new();
                stream.set_native_source(Rc::new(web));
                feeds.insert((channel_id, uid), Feed { track, screen, stream, _writer: writer });
                changed = true;
            }
            changed
        })
    }

    pub(crate) fn drop_channel(channel_id: i64) -> bool {
        FEEDS.with(|f| {
            let mut feeds = f.borrow_mut();
            let before = feeds.len();
            feeds.retain(|(ch, _), _| *ch != channel_id);
            feeds.len() != before
        })
    }

    pub(crate) fn feed(channel_id: i64, user_id: i64) -> Option<MediaStream> {
        FEEDS.with(|f| f.borrow().get(&(channel_id, user_id)).map(|f| f.stream.clone()))
    }

    pub(crate) fn playing_videos() -> u32 {
        // SAFETY: the snippet takes no handles and returns a plain count.
        unsafe { js_playing_videos() }
    }
}

/// Replace `channel_id`'s feeds with what the provider reports now; true when
/// the set changed (the caller repaints the voice rows then).
pub(crate) fn update(channel_id: i64, raw: Vec<(i64, bool, String, RawStream)>) -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        web::update(channel_id, raw)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (channel_id, raw);
        false
    }
}

/// Forget `channel_id`'s feeds (the call ended); true when any were dropped.
pub(crate) fn drop_channel(channel_id: i64) -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        web::drop_channel(channel_id)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = channel_id;
        false
    }
}

/// The live video for `user_id` in `channel_id`, if this tab has one.
pub(crate) fn feed(channel_id: i64, user_id: i64) -> Option<media_stream::MediaStream> {
    #[cfg(target_arch = "wasm32")]
    {
        web::feed(channel_id, user_id)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (channel_id, user_id);
        None
    }
}

/// How many <video> elements on the page are painting a frame (the robot's
/// check that a tile really plays, not just shows an avatar).
pub(crate) fn playing_videos() -> u32 {
    #[cfg(target_arch = "wasm32")]
    {
        web::playing_videos()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        0
    }
}
