//! `reply_reference_strip` — the per-message "replying to …" caption for the
//! Discord clone. Rendered ABOVE a received message that has a
//! `reply_to_message_id`, showing the quoted author and a short snippet of the
//! original message.
//!
//! Deliberately distinct from [`reply_preview_bar`]: that bar lives above the
//! *composer* ("Replying to …", dismissable with an ×) and reads the live
//! `reply_ctx` signal. This strip is a passive, non-interactive caption
//! attached to a specific message row — no signal, no controls. The caller
//! (`live_edit_row` in `app.rs`) resolves the original's author + body from the
//! client's already-loaded message buffer and calls this only when the row has
//! a reply reference.
use crate::style_helpers::{col, px};
use runtime_core::{ui, AlignItems, Element, FlexDirection, StyleRules};

// Discord palette — kept local so the component is self-contained, matching the
// muted "quoted" tone of the composer bar.
const BG_BAR: &str = "#2b2d31"; // the reference strip's fill
const QUOTE_BAR: &str = "#404249"; // the small left "quote" accent
const TEXT: &str = "#949ba4"; // muted (quoted) text tone

/// A compact, left-accented reference to the message being replied to.
///
/// `author` — the original message's author display name.
/// `snippet` — a short (already-truncated) excerpt of the original body.
pub fn reply_reference_strip(author: String, snippet: String) -> Element {
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            padding_left: Some(px(12.0)),
            padding_right: Some(px(12.0)),
            padding_top: Some(px(4.0)),
            padding_bottom: Some(px(4.0)),
            gap: Some(px(6.0)),
            background: Some(col(BG_BAR)),
            ..Default::default()
        }) {
            // The small left "quote" accent bar.
            view(style = StyleRules {
                width: Some(px(3.0)),
                height: Some(px(14.0)),
                background: Some(col(QUOTE_BAR)),
                ..Default::default()
            }) {}
            // The "<author>: <snippet>" line, built with `format!` from the two
            // owned `String`s (this component is invoked fresh per row).
            text(style = StyleRules {
                font_size: Some(px(12.0)),
                color: Some(col(TEXT)),
                ..Default::default()
            }) { format!("{}: {}", author, snippet) }
        }
    }
}

/// Truncate a message body into a short single-line snippet for the strip:
/// whitespace collapsed, trimmed, capped at 64 chars with an ellipsis.
pub fn reply_snippet(body: &str) -> String {
    let flat: String = body
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect();
    let flat = flat.trim().to_string();
    if flat.chars().count() <= 64 {
        flat
    } else {
        let cut: String = flat.chars().take(64).collect();
        format!("{}…", cut)
    }
}
