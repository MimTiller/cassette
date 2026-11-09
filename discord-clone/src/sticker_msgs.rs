//! Stickers on loaded messages. A sticker message has no text: the frame or
//! history row carries a [`StickerCopy`] instead. Message rows here are plain
//! `(author, body, …)` tuples, so the copy is kept in a registry by message
//! id and the row's body becomes a marker naming that id; the message view's
//! `message_body` turns the marker back into the image. The marker only ever points into
//! the registry, so a typed look-alike can at most show another message's
//! sticker, never an image of its own choosing.

use std::cell::RefCell;
use std::collections::HashMap;

use discord_server::server_settings::StickerCopy;
use runtime_core::{ui, Element, ObjectFit, StyleRules, Tokenized};

use crate::style_helpers::px;

/// How big a sticker shows in a message.
pub(crate) const SIZE: f32 = 160.0;

/// Starts every marker body. U+2063 (invisible separator) keeps it from
/// looking like anything a person would type.
const MARK: &str = "\u{2063}sticker:";

thread_local! {
    static CHANNEL: RefCell<HashMap<i64, StickerCopy>> = RefCell::new(HashMap::new());
    static DM: RefCell<HashMap<i64, StickerCopy>> = RefCell::new(HashMap::new());
}

/// The body a channel message row shows: the marker for its sticker, or its
/// own text.
pub(crate) fn channel_body(message_id: i64, sticker: Option<&StickerCopy>, content: &str) -> String {
    match sticker {
        Some(s) => {
            CHANNEL.with(|m| m.borrow_mut().insert(message_id, s.clone()));
            format!("{MARK}c{message_id}")
        }
        None => content.to_string(),
    }
}

/// The body a DM row shows: the marker for its sticker, or its own text.
pub(crate) fn dm_body(message_id: i64, sticker: Option<&StickerCopy>, content: &str) -> String {
    match sticker {
        Some(s) => {
            DM.with(|m| m.borrow_mut().insert(message_id, s.clone()));
            format!("{MARK}d{message_id}")
        }
        None => content.to_string(),
    }
}

/// The sticker a row's body stands for, if it is a sticker marker.
pub(crate) fn from_body(body: &str) -> Option<StickerCopy> {
    let rest = body.strip_prefix(MARK)?;
    let (kind, id) = rest.split_at_checked(1)?;
    let id: i64 = id.parse().ok()?;
    match kind {
        "c" => CHANNEL.with(|m| m.borrow().get(&id).cloned()),
        "d" => DM.with(|m| m.borrow().get(&id).cloned()),
        _ => None,
    }
}

/// "Sticker: <name>" for reply strips, pins and notifications.
pub(crate) fn snippet(body: &str) -> Option<String> {
    from_body(body).map(|s| format!("Sticker: {}", s.name))
}

/// A sticker as a message shows it: the image at [`SIZE`], no bubble.
pub(crate) fn view(s: &StickerCopy) -> Element {
    ui! {
        image(
            src = crate::auth::media_url(s.image.clone()),
            alt = s.name.clone(),
            test_id = "sticker-image",
            style = StyleRules {
                width: Some(px(SIZE)),
                height: Some(px(SIZE)),
                flex_shrink: Some(Tokenized::Literal(0.0)),
                object_fit: Some(ObjectFit::Contain),
                margin_top: Some(px(2.0)),
                ..Default::default()
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn copy(name: &str) -> StickerCopy {
        StickerCopy { id: 7, name: name.to_string(), image: "/media/s.png".to_string() }
    }

    #[test]
    fn a_marker_finds_its_sticker_and_text_stays_text() {
        let body = channel_body(41, Some(&copy("wave")), "");
        assert_eq!(from_body(&body).map(|s| s.name), Some("wave".to_string()));
        assert_eq!(snippet(&body).as_deref(), Some("Sticker: wave"));
        assert_eq!(channel_body(42, None, "hello"), "hello");
        assert_eq!(from_body("hello"), None);
    }

    #[test]
    fn channel_and_dm_ids_do_not_collide() {
        let c = channel_body(5, Some(&copy("chan")), "");
        let d = dm_body(5, Some(&copy("dm")), "");
        assert_eq!(from_body(&c).unwrap().name, "chan");
        assert_eq!(from_body(&d).unwrap().name, "dm");
    }

    #[test]
    fn a_typed_marker_for_an_unknown_id_is_not_a_sticker() {
        assert_eq!(from_body("\u{2063}sticker:c999999"), None);
        assert_eq!(from_body("\u{2063}sticker:x1"), None);
        assert_eq!(from_body("\u{2063}sticker:c"), None);
    }
}
