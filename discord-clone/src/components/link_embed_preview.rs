//! `link_embed_preview` — a self-contained "link embed" preview card for the
//! Discord clone. Renders hardcoded local link-preview strings (a title, a
//! short description, and a site name) in a bordered card with a colored left
//! accent bar.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `activity_card` /
//! `server_discovery_card` / `idle_timer_badge` / `server_folder_group` /
//! `profile_settings` ship in).
//!
//! Idioms, each verified against this crate's `app.rs` /
//! `profile_settings.rs` / `components/activity_card.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - per-side border widths are `Tokenized<f32>` (not `Length`) — border
//!   widths are never percentages in this framework (`StyleRules` documents
//!   it), so the accent bar width is written as `Tokenized::Literal(4.0)`
//!   rather than through the `px` helper;
//! - the three text lines share the muted meta look where they can;
//!   `StyleRules` is not `Copy`, so a style used twice is written INLINE in
//!   `style = …` at each use (a shared `let` would be moved on its first
//!   use) — the same shape `server_discovery_card` uses for its two static
//!   meta lines.

use crate::style_helpers::{col, px};
use runtime_core::{
    component, ui, AlignItems, Element, FlexDirection, FontWeight, StyleRules, Tokenized,
};

// Discord dark palette (kept local so the component is self-contained).
const BG_CARD: &str = "#2b2d31";
const BORDER: &str = "#1e1f22";
const ACCENT: &str = "#5865f2";
const TEXT: &str = "#f2f3f5";
const TEXT_MUTED: &str = "#949ba4";

/// The hardcoded link title (no props for now).
const TITLE: &str = "Example Article";
/// The hardcoded link description.
const DESCRIPTION: &str = "A short summary here";
/// The hardcoded site name shown as the card's top meta line.
const SITE_NAME: &str = "example.com";

/// The link-embed preview card. A bordered card with a colored left accent
/// bar, containing the hardcoded site name (muted meta) above the title
/// (primary, semibold) above the description (muted). Purely local — renders
/// the hardcoded strings above.
#[component]
pub fn LinkEmbedPreview() -> Element {
    let root_style = StyleRules {
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Stretch),
        // The outer 1px border (all four sides) gives the card its edge.
        border_top_width: Some(Tokenized::Literal(1.0)),
        border_right_width: Some(Tokenized::Literal(1.0)),
        border_bottom_width: Some(Tokenized::Literal(1.0)),
        border_left_width: Some(Tokenized::Literal(4.0)),
        border_top_color: Some(col(BORDER)),
        border_right_color: Some(col(BORDER)),
        border_bottom_color: Some(col(BORDER)),
        // The colored left accent bar: a wider, brand-colored left border.
        border_left_color: Some(col(ACCENT)),
        padding_top: Some(px(12.0)),
        padding_right: Some(px(12.0)),
        padding_bottom: Some(px(12.0)),
        padding_left: Some(px(12.0)),
        background: Some(col(BG_CARD)),
        border_top_left_radius: Some(px(10.0)),
        border_top_right_radius: Some(px(10.0)),
        border_bottom_left_radius: Some(px(10.0)),
        border_bottom_right_radius: Some(px(10.0)),
        ..Default::default()
    };

    let stack_style = StyleRules {
        flex_direction: Some(FlexDirection::Column),
        gap: Some(px(2.0)),
        ..Default::default()
    };

    ui! {
        view(style = root_style) {
            view(style = stack_style) {
                text(style = StyleRules {
                    font_size: Some(px(12.0)),
                    color: Some(col(TEXT_MUTED)),
                    ..Default::default()
                }) { SITE_NAME.to_string() }
                text(style = StyleRules {
                    font_size: Some(px(15.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { TITLE.to_string() }
                // Written INLINE in `style = …` because `StyleRules` is not
                // `Copy` (a shared `let` would be moved by the lines above).
                text(style = StyleRules {
                    font_size: Some(px(13.0)),
                    color: Some(col(TEXT_MUTED)),
                    ..Default::default()
                }) { DESCRIPTION.to_string() }
            }
        }
    }
}
