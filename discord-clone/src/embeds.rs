//! Link preview cards of loaded messages, by message id (filled from
//! `Message` and `MessageEmbeds` frames), and opening a card's link.

use discord_server::link_previews::Embed;
use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static CARDS: RefCell<HashMap<i64, Vec<Embed>>> = RefCell::new(HashMap::new());
}

/// Record `message_id`'s cards (empty = none). Returns whether they changed.
pub fn set(message_id: i64, embeds: Vec<Embed>) -> bool {
    CARDS.with(|c| {
        let mut c = c.borrow_mut();
        let same = match c.get(&message_id) {
            Some(old) => *old == embeds,
            None => embeds.is_empty(),
        };
        if same {
            return false;
        }
        if embeds.is_empty() {
            c.remove(&message_id);
        } else {
            c.insert(message_id, embeds);
        }
        true
    })
}

/// `message_id`'s cards.
pub fn get(message_id: i64) -> Vec<Embed> {
    CARDS.with(|c| c.borrow().get(&message_id).cloned().unwrap_or_default())
}

/// Open a card's link in a new tab (web).
pub fn open(url: &str) {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return;
    }
    #[cfg(target_arch = "wasm32")]
    if let Some(w) = web_sys::window() {
        let _ = w.open_with_url_and_target_and_features(url, "_blank", "noopener,noreferrer");
    }
}
