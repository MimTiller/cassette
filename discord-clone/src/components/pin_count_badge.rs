#![allow(dead_code)]
//! `pin_count_badge` — a self-contained "pinned messages" count badge for the
//! Discord clone. Renders a small pin glyph (📌) beside a local pinned-message
//! count — the compact pill Discord shows to signal how many messages are
//! pinned in a channel. When the count is `0`, the whole badge renders
//! **nothing at all** (no icon, no pill, no empty node).
//!
//! Everything is hardcoded local data — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape
//! `connection_quality_indicator` / `verified_badge` / `jump_to_bottom_button`
//! ship in).
//!
//! Idioms, each verified against this crate's
//! `components/connection_quality_indicator.rs` / `components/jump_to_bottom_button.rs`
//! / `components/dm_view.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the "render nothing when the count is 0" part is a STATIC
//!   `if PIN_COUNT > 0 { … }` wrapping the WHOLE pill, with no `else`. Because
//!   the condition is call-free (`PIN_COUNT` is a local `const`), the macro
//!   lowers it through `emit_plain_if`'s `Ctx::Child` arm to a plain Rust `if`
//!   whose taken branch flattens into its flat siblings and whose missing
//!   `else` contributes *nothing* — so a zero count mounts no node at all.
//!   (The same shape `dm_view.rs` uses for its `if unread > 0 { … }` unread
//!   badge, and `jump_to_bottom_button` uses to gate its whole pill.)
//! - the pin glyph and the count label are both static values baked to
//!   `TextSource::Static` — the glyph is a string literal, the count is a
//!   `PIN_COUNT.to_string()` bound above the `ui!` block (the exact idiom
//!   `dm_view.rs` uses for `unread_label`). No signals anywhere in this badge.

use crate::style_helpers::{col, px};
use runtime_core::{
    component, ui, AlignItems, Element, FlexDirection, FontWeight, StyleRules,
};

// Discord dark palette (kept local so the component is self-contained).
const PIN: &str = "#f2f3f5"; // the pin glyph (Discord near-white)
const TEXT: &str = "#f2f3f5"; // the count label (Discord near-white)
const BG_PILL: &str = "#3a3c41"; // the pill surface (Discord elevated gray)

// The hardcoded local pinned-message count. No backend fetch — this is a
// self-contained badge, so a fixed value stands in for a live count. It is
// nonzero so the badge renders; set it to `0` to exercise the "nothing" path.
const PIN_COUNT: usize = 3;

/// The pinned-message count badge. A small pill with a pin glyph and the
/// pinned count; renders nothing at all when `PIN_COUNT` is `0`.
#[component]
#[allow(non_snake_case)]
pub fn PinCountBadge() -> Element {
    // Static: `PIN_COUNT` is a `const`, so `to_string()` is a one-shot value,
    // not a reactive read. Bound above the `ui!` block so the branch (a plain
    // Rust `if`, borrowed capture) can reference it freely.
    let count_label = PIN_COUNT.to_string();

    ui! {
        // Static branch: `PIN_COUNT > 0` is call-free (a local `const`), so the
        // macro lowers it through `emit_plain_if`'s `Ctx::Child` arm to a plain
        // Rust `if`. The pill flattens into flat siblings when true; the
        // missing `else` contributes nothing, so a zero count mounts no node at
        // all. `StyleRules` is not `Copy`, so the styles are written INLINE
        // (`style = …`), created fresh per evaluation — the same shape
        // `jump_to_bottom_button` uses.
        if PIN_COUNT > 0 {
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
                // The pin glyph — a static string literal, so it bakes to
                // `TextSource::Static` (no reactive machinery).
                text(style = StyleRules {
                    font_size: Some(px(13.0)),
                    color: Some(col(PIN)),
                    ..Default::default()
                }) { "📌".to_string() }
                // The count label — the static `count_label` bound above, so it
                // bakes to `TextSource::Static` too.
                text(style = StyleRules {
                    font_size: Some(px(12.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { count_label }
            }
        }
    }
}
