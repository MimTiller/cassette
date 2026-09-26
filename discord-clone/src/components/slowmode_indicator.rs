#![allow(dead_code)]
//! `slowmode_indicator` — a self-contained "slowmode" timer indicator for the
//! Discord clone. Renders a small clock glyph (🕓) beside a static
//! "Slowmode: {n}s" label — the compact notice Discord shows while a channel's
//! slowmode cooldown is active. When the seconds value is `0`, the whole
//! indicator renders **nothing at all** (no icon, no pill, no empty node).
//!
//! Everything is hardcoded local data — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape
//! `pin_count_badge` / `verified_badge` / `jump_to_bottom_button` ship in).
//!
//! Idioms, each verified against this crate's
//! `components/pin_count_badge.rs` / `components/jump_to_bottom_button.rs`
//! / `components/dm_view.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the "render nothing when the value is 0" part is a STATIC
//!   `if SLOWMODE_SECONDS > 0 { … }` wrapping the WHOLE pill, with no `else`.
//!   Because the condition is call-free (`SLOWMODE_SECONDS` is a local
//!   `const`), the macro lowers it through `emit_plain_if`'s `Ctx::Child` arm
//!   to a plain Rust `if` whose taken branch flattens into its flat siblings
//!   and whose missing `else` contributes *nothing* — so a zero value mounts
//!   no node at all. (The same shape `pin_count_badge` uses for its
//!   `if PIN_COUNT > 0 { … }` count badge.)
//! - the clock glyph and the label are both static values baked to
//!   `TextSource::Static` — the glyph is a string literal, the label is a
//!   `format!(…)` bound above the `ui!` block (the exact idiom
//!   `pin_count_badge.rs` uses for `count_label`). No signals anywhere here.

use crate::style_helpers::{col, px};
use runtime_core::{
    component, ui, AlignItems, Element, FlexDirection, FontWeight, StyleRules,
};

// Discord dark palette (kept local so the component is self-contained).
const CLOCK: &str = "#949ba4"; // the clock glyph (Discord muted text)
const TEXT: &str = "#949ba4"; // the "Slowmode: {n}s" label (Discord muted text)
const BG_PILL: &str = "#2b2d31"; // the pill surface (Discord sidebar gray)

// The hardcoded local slowmode cooldown, in seconds. No backend fetch — this
// is a self-contained indicator, so a fixed value stands in for a live
// cooldown. It is nonzero so the indicator renders; set it to `0` to exercise
// the "nothing" path.
const SLOWMODE_SECONDS: usize = 30;

/// The slowmode timer indicator. A small pill with a clock glyph and the
/// "Slowmode: {n}s" label; renders nothing at all when `SLOWMODE_SECONDS` is
/// `0`.
#[component]
#[allow(non_snake_case)]
pub fn SlowmodeIndicator() -> Element {
    // Static: `SLOWMODE_SECONDS` is a `const`, so `format!` is a one-shot
    // value, not a reactive read. Bound above the `ui!` block so the branch
    // (a plain Rust `if`, borrowed capture) can reference it freely.
    let label = format!("Slowmode: {}s", SLOWMODE_SECONDS);

    ui! {
        // Static branch: `SLOWMODE_SECONDS > 0` is call-free (a local `const`),
        // so the macro lowers it through `emit_plain_if`'s `Ctx::Child` arm to
        // a plain Rust `if`. The pill flattens into flat siblings when true;
        // the missing `else` contributes nothing, so a zero value mounts no
        // node at all. `StyleRules` is not `Copy`, so the styles are written
        // INLINE (`style = …`), created fresh per evaluation — the same shape
        // `pin_count_badge` uses.
        if SLOWMODE_SECONDS > 0 {
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(4.0)),
                padding_left: Some(px(7.0)),
                padding_right: Some(px(7.0)),
                padding_top: Some(px(3.0)),
                padding_bottom: Some(px(3.0)),
                background: Some(col(BG_PILL)),
                border_top_left_radius: Some(px(8.0)),
                border_top_right_radius: Some(px(8.0)),
                border_bottom_left_radius: Some(px(8.0)),
                border_bottom_right_radius: Some(px(8.0)),
                ..Default::default()
            }) {
                // The clock glyph — a static string literal, so it bakes to
                // `TextSource::Static` (no reactive machinery).
                text(style = StyleRules {
                    font_size: Some(px(13.0)),
                    color: Some(col(CLOCK)),
                    ..Default::default()
                }) { "🕓".to_string() }
                // The label — the static `label` bound above, so it bakes to
                // `TextSource::Static` too.
                text(style = StyleRules {
                    font_size: Some(px(12.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { label }
            }
        }
    }
}
