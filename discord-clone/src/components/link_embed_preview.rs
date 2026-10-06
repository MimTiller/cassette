//! `link_embed_preview` — a "link embed" preview card for the Discord clone.
//! Renders the link's site name, title, and description in a bordered card
//! with a colored left accent bar.
//!
//! The three strings are `String` props (hardcoded defaults `example.com` /
//! `Example Article` / `A short summary here`, so the component also
//! compiles standalone). It is wired into `app.rs` `message_row`: a message
//! whose body contains a `http(s)://…` URL renders one `LinkEmbedPreview`
//! below its body (the host + title are parsed from the URL via
//! `link_preview_props`; a URL-less body renders no card). A deliberate
//! single instance on the link-bearing seed message — not a per-row widget.
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

/// The link-embed preview card. A bordered card with a colored left accent
/// bar, containing the site name (muted meta) above the title (primary,
/// semibold) above the description (muted). The three strings arrive as
/// `String` props (with hardcoded defaults so the component still compiles
/// standalone — the `RoleBadge` idiom); no backend, no reactivity (each prop
/// is read once at build and baked to static text).
#[component]
#[allow(non_snake_case)]
pub fn LinkEmbedPreview(
    #[prop(default = "example.com")] site_name: String,
    #[prop(default = "Example Article")] title: String,
    #[prop(default = "A short summary here")] description: String,
) -> Element {
    // The three lines are static props (never `.set`), so read them once at
    // build into owned `String`s and bake them as static text bodies. Cloned
    // (borrowed) at each use so the non-`Copy` locals are never moved out of
    // the render tree — the `ChannelTopicBanner` idiom.
    let site = site_name.get();
    let head = title.get();
    let desc = description.get();
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
                }) { site.clone() }
                text(style = StyleRules {
                    font_size: Some(px(15.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { head.clone() }
                // Written INLINE in `style = …` because `StyleRules` is not
                // `Copy` (a shared `let` would be moved by the lines above).
                text(style = StyleRules {
                    font_size: Some(px(13.0)),
                    color: Some(col(TEXT_MUTED)),
                    ..Default::default()
                }) { desc.clone() }
            }
        }
    }
}
