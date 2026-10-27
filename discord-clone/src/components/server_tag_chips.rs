//! `server_tag_chips` — a self-contained row of server tag pills for the
//! Discord clone. Renders exactly 3 hardcoded local tag strings ("Gaming",
//! "Casual", "18+") as small pill badges in a horizontal row, each pill sitting
//! on a slightly different background color (muted slate-blue / teal / plum) so
//! the three tags read as distinct at a glance.
//!
//! Everything is hardcoded local data — no backend call, no props, no signals,
//! no wiring into any screen yet. Declared in `lib.rs` so it compiles as part of
//! the crate, but referenced by nothing (the same shape `category_filter_chips`
//! / `idle_timer_badge` / `verified_badge` ship in).
//!
//! Idioms, each verified against this crate's `components/category_filter_chips.rs`
//! and `components/idle_timer_badge.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the 3 pills are emitted by a keyed `for (idx, tag) in …, key = idx` over the
//!   static tag slice (the standard static-row idiom from `category_filter_chips`);
//!   `idx` also indexes the parallel background-color slice, so each pill draws
//!   its own distinct fill from the static table (no signal, so the style is a
//!   plain — not reactive — `StyleRules`, built inline per iteration);
//! - each pill is a fully-rounded `view` + child `text` (the app's standard pill
//!   shape); the pill's radius is half its fixed height, giving a clean lozenge.

use crate::theme;
use crate::style_helpers::{col, px};
use runtime_core::{
    component, ui, AlignItems, Element, FlexDirection, JustifyContent, StyleRules,
};

// Muted, on-theme dark pill fills — one distinct (slightly different) hue per
// tag so the three read as separate at a glance, while staying in the Discord
// dark palette. Kept local so the component is self-contained.
const TAG_BG: [&str; 3] = [
    "#3d4252", // "Gaming" — muted slate-blue
    "#38504a", // "Casual" — muted teal
    "#4d3b4d", // "18+"    — muted plum
];

// The 3 hardcoded local tag strings — exactly these, in this order, no backend
// fetch. Kept as a static slice so the keyed `for` renders one pill per entry;
// index-aligned with `TAG_BG` above.
const TAGS: [&str; 3] = ["Gaming", "Casual", "18+"];

/// The server tag pills. Renders exactly 3 hardcoded tag strings as small pill
/// badges in a row; each pill sits on its own (slightly different) background
/// color drawn from the parallel `TAG_BG` table by loop index.
#[component]
#[allow(non_snake_case)]
pub fn ServerTagChips() -> Element {
    ui! {
        // Horizontal row of pills, vertically centered.
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(6.0)),
            padding_top: Some(px(4.0)),
            padding_bottom: Some(px(4.0)),
            padding_left: Some(px(4.0)),
            padding_right: Some(px(4.0)),
            ..Default::default()
        }) {
            // One pill per hardcoded tag. `idx` is the loop index (and the `key`);
            // `TAG_BG[idx]` draws each pill's distinct fill from the parallel static
            // table. No signal, so the style is plain (non-reactive) and built inline
            // per iteration.
            for (idx, tag) in TAGS.iter().enumerate(), key = idx {
                view(style = StyleRules {
                    height: Some(px(22.0)),
                    padding_left: Some(px(10.0)),
                    padding_right: Some(px(10.0)),
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    background: Some(col(TAG_BG[idx])),
                    border_top_left_radius: Some(px(11.0)),
                    border_top_right_radius: Some(px(11.0)),
                    border_bottom_left_radius: Some(px(11.0)),
                    border_bottom_right_radius: Some(px(11.0)),
                    ..Default::default()
                }) {
                    text(style = StyleRules {
                        font_size: Some(px(12.0)),
                        color: Some(col(theme::TEXT_NORMAL)),
                        ..Default::default()
                    }) { tag.to_string() }
                }
            }
        }
    }
}
