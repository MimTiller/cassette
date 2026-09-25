#![allow(dead_code)]
//! `connection_quality_indicator` — a Discord-style connection-quality glyph
//! for the Discord clone. Renders four small ascending bars (the familiar
//! signal-strength shape) where the number of SOLID bars equals a hardcoded
//! local quality level (an integer 1..=4) and the remaining bars are drawn
//! as muted outlines. This is the little widget Discord shows next to a
//! voice channel's latency to hint at connection quality.
//!
//! Everything is hardcoded local data — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape
//! `server_folder_group` / `verified_badge` / `server_icon_fallback` ship in).
//!
//! Idioms, each verified against this crate's `components/server_folder_group.rs`
//! / `components/server_icon_fallback.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the four bars are a plain `for i in 0..4` inside `ui!` lowered to the
//!   `repeat` primitive whose per-item builder is an `Fn` closure, so the
//!   per-bar `StyleRules` are inlined as literals rather than shared `let`
//!   bindings (a captured, non-`Copy` binding would be moved on the first
//!   iteration — the same rule `server_folder_group.rs` documents for its
//!   server list);
//! - the bars are bottom-aligned in a `flex_direction: Row` with
//!   `align_items: FlexEnd` (the `FlexEnd` variant exists on `AlignItems`),
//!   and each bar's height comes from a static ascending
//!   `BAR_HEIGHTS` array so the glyph reads as a signal-meter silhouette.

use crate::style_helpers::{col, px};
use runtime_core::{component, ui, AlignItems, Element, FlexDirection, StyleRules, Tokenized};

// Discord dark palette (kept local so the component is self-contained).
const FILL: &str = "#23a55a"; // solid bar fill (Discord green — a good link)
const OUTLINE: &str = "#4e5058"; // muted outline for the unfilled bars

// The hardcoded local quality level, 1..=4: the number of SOLID bars. No
// backend fetch — this is a self-contained glyph, so a fixed value stands in
// for a live measurement. 3 of 4 bars solid.
const QUALITY_LEVEL: usize = 3;

// Ascending bar heights (px) — the classic signal-meter silhouette, tallest on
// the right. One per bar, index-aligned with the `0..4` loop below.
const BAR_HEIGHTS: [f32; 4] = [5.0, 9.0, 13.0, 17.0];

/// The connection-quality indicator. Four small bars in a row, bottom-aligned;
/// the first `QUALITY_LEVEL` bars are solid green, the rest are muted outlines.
#[component]
#[allow(non_snake_case)]
pub fn ConnectionQualityIndicator() -> Element {
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::FlexEnd),
            gap: Some(px(2.0)),
            ..Default::default()
        }) {
            for i in 0..4 {
                view(style = StyleRules {
                    width: Some(px(4.0)),
                    height: Some(px(BAR_HEIGHTS[i])),
                    // Solid fill when this bar is at-or-below the level,
                    // transparent otherwise so only the outline shows.
                    background: Some(col(if i < QUALITY_LEVEL { FILL } else { "transparent" })),
                    border_top_left_radius: Some(px(1.0)),
                    border_top_right_radius: Some(px(1.0)),
                    border_bottom_left_radius: Some(px(1.0)),
                    border_bottom_right_radius: Some(px(1.0)),
                    border_top_width: Some(Tokenized::Literal(1.0)),
                    border_right_width: Some(Tokenized::Literal(1.0)),
                    border_bottom_width: Some(Tokenized::Literal(1.0)),
                    border_left_width: Some(Tokenized::Literal(1.0)),
                    // Filled bars read as a solid block; unfilled bars read as
                    // a thin muted outline (Discord's "no signal" state).
                    border_top_color: Some(col(if i < QUALITY_LEVEL { FILL } else { OUTLINE })),
                    border_right_color: Some(col(if i < QUALITY_LEVEL { FILL } else { OUTLINE })),
                    border_bottom_color: Some(col(if i < QUALITY_LEVEL { FILL } else { OUTLINE })),
                    border_left_color: Some(col(if i < QUALITY_LEVEL { FILL } else { OUTLINE })),
                    ..Default::default()
                }) {}
            }
        }
    }
}
