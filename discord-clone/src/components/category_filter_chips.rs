//! `category_filter_chips` — a self-contained row of category filter chips for
//! the Discord clone. Renders exactly 3 hardcoded local category names
//! ("Gaming", "Music", "Education") as toggle chips. Each chip is bound to its
//! OWN local boolean "active" signal (independent — not mutually exclusive, so
//! any combination of chips can be active at once) and changes style when
//! active (muted outline pill → solid blurple fill with white text).
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `rules_accept_checkbox` /
//! `idle_timer_badge` / `profile_settings` ship in).
//!
//! Idioms, each verified against this crate's `components/rules_accept_checkbox.rs`
//! and `components/idle_timer_badge.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - `Signal` is `Copy` (`runtime_world`), so a `move` closure captures a copy
//!   of the same underlying signal and stays `Fn` (callable / re-evaluable on
//!   every interaction);
//! - "changes style when active" is the reactive `style = move || …` idiom: the
//!   closure re-evaluates whenever the chip's `active` signal flips, rebuilding
//!   the `StyleRules` (background, border color, text color) each pass. The
//!   style is built INLINE in the closure so nothing is captured across the
//!   reactive boundary (`StyleRules` is not `Copy`);
//! - the tap-toggle is `view` + `.on_touch(|ev| …)` guarded on
//!   `TouchPhase::Ended` — the app's standard click pattern;
//! - the 3 chips are emitted by a keyed `for (idx, name) in …, key = idx` over
//!   the static name slice (the standard static-row idiom from
//!   `rules_accept_checkbox`); each chip reads/writes `active[idx]`, so each is
//!   bound to its own independent local boolean signal.

use crate::theme;
use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, JustifyContent, StyleRules,
    Tokenized, TouchPhase, TouchResponse,
};

// Discord dark palette — kept local so the component is self-contained.

// The 3 hardcoded local category names — exactly these, in this order, no
// backend fetch. Kept as a static slice so the keyed `for` renders one chip per
// entry and the loop index doubles as the per-chip signal slot.
const CATEGORIES: [&str; 3] = ["Gaming", "Music", "Education"];

/// The category filter chips. Renders exactly 3 hardcoded category names as
/// toggle chips, each bound to its own local boolean "active" signal (starts
/// inactive, independent — not mutually exclusive). Tapping a chip flips ONLY
/// that chip's signal, which re-runs its reactive `style` closure to swap the
/// chip between the inactive (muted outline) and active (blurple fill) looks.
#[component]
pub fn CategoryFilterChips() -> Element {
    // One independent local boolean signal per chip (one slot per category).
    // `Signal<bool>` is `Copy`, so `active[idx]` hands each closure its own
    // copy pointing at the same underlying slot — reads and writes on different
    // closures / chips never interfere.
    let active: [runtime_core::Signal<bool>; 3] = [
        signal(false),
        signal(false),
        signal(false),
    ];

    ui! {
        // Horizontal row of chips, vertically centered.
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(8.0)),
            padding_top: Some(px(8.0)),
            padding_bottom: Some(px(8.0)),
            padding_left: Some(px(8.0)),
            padding_right: Some(px(8.0)),
            ..Default::default()
        }) {
            // One chip per hardcoded category. `idx` is the loop index (and the
            // `key`), and the per-chip `move` closures below capture `idx` + a
            // copy of the `active` array to read/write `active[idx]` — so chip
            // 0 ↔ active[0], chip 1 ↔ active[1], chip 2 ↔ active[2], each
            // independent.
            for (idx, name) in CATEGORIES.iter().enumerate(), key = idx {
                // The chip pill. The reactive `style = move || …` closure reads
                // `active[idx]` and rebuilds the `StyleRules` on every flip:
                // inactive → transparent fill + muted ring + muted text; active
                // → blurple fill + blurple ring + white text. Built inline so
                // nothing is captured across the reactive boundary.
                view(style = move || {
                    let on = active[idx].get();
                    StyleRules {
                        height: Some(px(30.0)),
                        padding_left: Some(px(14.0)),
                        padding_right: Some(px(14.0)),
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        background: Some(col(if on { theme::ACCENT } else { "transparent" })),
                        border_top_left_radius: Some(px(8.0)),
                        border_top_right_radius: Some(px(8.0)),
                        border_bottom_left_radius: Some(px(8.0)),
                        border_bottom_right_radius: Some(px(8.0)),
                        border_top_width: Some(Tokenized::Literal(1.0)),
                        border_right_width: Some(Tokenized::Literal(1.0)),
                        border_bottom_width: Some(Tokenized::Literal(1.0)),
                        border_left_width: Some(Tokenized::Literal(1.0)),
                        border_top_color: Some(col(if on { theme::ACCENT } else { theme::BUTTON_SECONDARY })),
                        border_right_color: Some(col(if on { theme::ACCENT } else { theme::BUTTON_SECONDARY })),
                        border_bottom_color: Some(col(if on { theme::ACCENT } else { theme::BUTTON_SECONDARY })),
                        border_left_color: Some(col(if on { theme::ACCENT } else { theme::BUTTON_SECONDARY })),
                        ..Default::default()
                    }
                }) {
                    // The category label — its color also flips with the chip.
                    text(style = move || {
                        StyleRules {
                            font_size: Some(px(13.0)),
                            color: Some(col(if active[idx].get() {
                                theme::TEXT_ON_ACCENT
                            } else {
                                theme::TEXT_MUTED
                            })),
                            ..Default::default()
                        }
                    }) { name.to_string() }
                }
                // Tap-toggle: flip ONLY this chip's signal on `TouchPhase::Ended`.
                // `active` and `idx` are `Copy`, so this `move` closure captures
                // its own copies and stays `Fn`.
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        active[idx].set(!active[idx].get());
                    }
                    TouchResponse::CONSUMED
                })
            }
        }
    }
}
