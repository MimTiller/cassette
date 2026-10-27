//! `reply_reference_strip` — the per-message "replying to …" line for the
//! Discord clone. Rendered ABOVE a received message that has a
//! `reply_to_message_id`, the way Discord does it: one compact line, no
//! background bar, indented to the message TEXT column — a thin L-shaped
//! connector (a "┌" hairline) sitting in the avatar column above the reply's
//! own avatar, then a small round chip of the original author, the author's
//! name, and the original message on one muted line.
//!
//! Deliberately distinct from [`reply_preview_bar`]: that bar lives above the
//! *composer* ("Replying to …", dismissable with an ×) and reads the live
//! `reply_ctx` signal. This strip is a passive, non-interactive caption
//! attached to a specific message row — no signal, no controls. The caller
//! (`live_edit_row` in `app.rs`) resolves the original's author + body from the
//! client's already-loaded message buffer and calls this only when the row has
//! a reply reference.
use crate::theme;
use crate::style_helpers::{col, px};
// The quote chip reuses the shared avatar component (a registered avatar when
// the author has one, otherwise a tinted initial — exactly Discord's chip).
use crate::avatar_chip::{avatar_chip as reply_avatar_chip};
use runtime_core::{ui, AlignItems, Element, FlexDirection, FontWeight, Overflow, Position, StyleRules, Tokenized};

// Palette — kept local so the component is self-contained.

/// A compact reference to the message being replied to: connector + chip +
/// name + one-line snippet, aligned to the row's text column.
///
/// `author` — the original message's author display name.
/// `snippet` — a short (already-truncated) excerpt of the original body.
pub fn reply_reference_strip(author: String, snippet: String) -> Element {
    ui! {
        view(style = StyleRules {
            position: Some(Position::Relative),
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            ..Default::default()
        }) {
            // The connector: a 2px L (down, then right) anchored in the
            // message row's avatar column — the reply's own avatar sits at
            // x=16..56 (16px padding + 40px avatar), so the vertical stroke
            // runs at that column's center (x=36) and turns right toward the
            // quote chip. Radius rounds the elbow.
            view(style = StyleRules {
                position: Some(Position::Absolute),
                left: Some(px(35.0)),
                top: Some(px(0.0)),
                width: Some(px(30.0)),
                height: Some(px(18.0)),
                flex_shrink: Some(Tokenized::Literal(0.0)),
                border_left_width: Some(Tokenized::Literal(2.0)),
                border_bottom_width: Some(Tokenized::Literal(2.0)),
                border_bottom_right_radius: Some(px(6.0)),
                border_left_color: Some(col(theme::BUTTON_SECONDARY)),
                border_bottom_color: Some(col(theme::BUTTON_SECONDARY)),
                ..Default::default()
            }) {}
            // Spacer: the message row's 16px padding + 40px avatar + 10px
            // gap, so the quote line starts at the 66px text column.
            view(style = StyleRules { width: Some(px(66.0)), ..Default::default() }) {}
            // The quote line: 16px round chip, semibold muted-white name,
            // one muted line of the original.
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(6.0)),
                ..Default::default()
            }) {
                { reply_avatar_chip(author.as_str(), theme::BG_SELECTED, 16.0, 8.0) }
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    color: Some(col(theme::TEXT_NORMAL)),
                    ..Default::default()
                }) { author }
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    color: Some(col(theme::TEXT_MUTED)),
                    max_width: Some(px(540.0)),
                    height: Some(px(18.0)),
                    overflow: Some(Overflow::Hidden),
                    ..Default::default()
                }) { snippet }
            }
        }
    }
}

/// Display text for a message body wherever it is shown as a text EXCERPT
/// (toast, search result, pinned row, DM-list preview): a GIF message (a body
/// that is just a Giphy media URL, see `discord_server::gifs::gif_body`)
/// reads as "GIF" instead of its URL. Every excerpt site calls this so the
/// GIF rule lives in one place.
pub fn excerpt_text(body: &str) -> String {
    if discord_server::gifs::gif_body(body).is_some() {
        return "GIF".to_string();
    }
    body.to_string()
}

/// Truncate a message body into a short single-line snippet for the strip:
/// whitespace collapsed, trimmed, capped at 64 chars with an ellipsis.
pub fn reply_snippet(body: &str) -> String {
    // A GIF message quotes as "GIF", not its media URL — one short word, so
    // the excerpt passes straight through the truncation below.
    let body: String = excerpt_text(body);
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

#[cfg(test)]
mod excerpt_tests {
    use super::excerpt_text;

    #[test]
    fn a_giphy_media_body_reads_as_gif() {
        assert_eq!(excerpt_text("https://media2.giphy.com/media/abc/giphy.gif#480x360"), "GIF");
        assert_eq!(excerpt_text(" https://i.giphy.com/abc.webp "), "GIF");
    }

    #[test]
    fn a_plain_body_passes_through_unchanged() {
        assert_eq!(excerpt_text("hello there"), "hello there");
        // Not a Giphy media URL (a plain gif mention, an untrusted host).
        assert_eq!(excerpt_text("https://evil.com/x.gif"), "https://evil.com/x.gif");
        assert_eq!(excerpt_text("check out this gif"), "check out this gif");
    }
}
