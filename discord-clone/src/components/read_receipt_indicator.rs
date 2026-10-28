#![allow(dead_code)]
//! `read_receipt_indicator` — a Discord-style read-receipt glyph for the
//! Discord clone. It renders the small checkmark(s) Discord shows beneath a
//! sent message to indicate delivery state: one muted checkmark for "sent",
//! two muted checkmarks for "delivered", and two colored (blurple) checkmarks
//! for "read".
//!
//! Everything is hardcoded local data — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `connection_quality_indicator` /
//! `verified_badge` / `server_folder_group` ship in).
//!
//! Idioms, each verified against this crate's `components/connection_quality_indicator.rs`
//! / `components/verified_badge.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the check count and color are `const`s const-folded from the hardcoded
//!   `STATUS`, so a single `for i in 0..COUNT` inside `ui!` (lowered to the
//!   `repeat` primitive) renders the right number of checks for any status;
//! - each check is a `text` glyph with an inlined `StyleRules` (a captured,
//!   non-`Copy` binding would be moved on the first iteration of `repeat` —
//!   the same rule `connection_quality_indicator.rs` documents for its bars);
//! - the checks sit in a `flex_direction: Row` so multiple checks line up
//!   horizontally, mirroring Discord's double-check layout.

use crate::theme;
use crate::style_helpers::{col, px};
use runtime_core::{component, ui, AlignItems, Element, FlexDirection, StyleRules};

// Discord dark palette (kept local so the component is self-contained).

// The hardcoded local status — "sent", "delivered", or "read". No backend
// fetch, no props: a fixed value stands in for a live receipt state. "read" is
// chosen because it is the most featureful state (two colored checks), so the
// shipped build exercises the full glyph.
const STATUS: &str = "read";

/// The read-receipt indicator. Renders one checkmark for "sent", two checkmarks
/// for "delivered", and two colored (blurple) checkmarks for "read".
#[component]
pub fn ReadReceiptIndicator() -> Element {
    // How many checkmarks this status renders: one for "sent", two for
    // "delivered" / "read". Computed at mount from the hardcoded STATUS (a
    // plain `let`, not a `const` — `&str ==` isn't const-stable in this
    // toolchain). `count` is `usize` (Copy), so the `repeat` closure can
    // capture it freely.
    let count: usize = if STATUS == "sent" { 1 } else { 2 };

    // The check color: muted gray for "sent" / "delivered", colored blurple
    // for "read". The color is the visual discriminator between a gray
    // double-check (delivered) and a colored one (read). `&str` is Copy.
    let check_color: &str = if STATUS == "read" { theme::ACCENT } else { theme::TEXT_FAINT };

    ui! {
        // Horizontal row so multiple checks line up (Discord's double-check
        // layout); `gap` gives the two checks a hair of separation.
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(1.0)),
            ..Default::default()
        }) {
            // `for _i in 0..count` lowers to `repeat`. The index is unused
            // (each check is identical), so it is prefixed `_` to stay clean.
            // The per-check `StyleRules` is inlined as a literal — never
            // captured across the `repeat` closure boundary.
            for _i in 0..count {
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    color: Some(col(check_color)),
                    ..Default::default()
                }) { "✓".to_string() }
            }
        }
    }
}
