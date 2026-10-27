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
use crate::style_helpers::{col, px};
// The quote chip reuses the shared avatar component (a registered avatar when
// the author has one, otherwise a tinted initial — exactly Discord's chip).
use crate::avatar_chip::{avatar_chip as reply_avatar_chip};
use runtime_core::{ui, AlignItems, Element, FlexDirection, FontWeight, Overflow, Position, StyleRules, Tokenized};

// Palette — kept local so the component is self-contained.
const CONNECTOR: &str = "#565a64"; // the "┌" hairline (muted grey)
const CHIP_TINT: &str = "#4b4d54"; // fallback avatar-chip fill
const NAME: &str = "#c6c9d1";      // semibold muted-white author name
const SNIPPET: &str = "#949ba4";   // muted original-message excerpt

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
                border_left_color: Some(col(CONNECTOR)),
                border_bottom_color: Some(col(CONNECTOR)),
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
                { reply_avatar_chip(author.as_str(), CHIP_TINT, 16.0, 8.0) }
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    color: Some(col(NAME)),
                    ..Default::default()
                }) { author }
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    color: Some(col(SNIPPET)),
                    max_width: Some(px(540.0)),
                    height: Some(px(18.0)),
                    overflow: Some(Overflow::Hidden),
                    ..Default::default()
                }) { snippet }
            }
        }
    }
}

/// Truncate a message body into a short single-line snippet for the strip:
/// whitespace collapsed, trimmed, capped at 64 chars with an ellipsis.
pub fn reply_snippet(body: &str) -> String {
    // A GIF message quotes as "GIF", not its media URL.
    if discord_server::gifs::gif_body(body).is_some() {
        return "GIF".to_string();
    }
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
