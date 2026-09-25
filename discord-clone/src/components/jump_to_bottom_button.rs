//! `jump_to_bottom_button` — a self-contained "jump to bottom" control for the
//! Discord clone. Bound to a local `visible` boolean signal (hardcoded `true`
//! at start), it renders a small floating pill with a down-arrow glyph and a
//! "Jump to bottom" label while the signal is set, and nothing at all while it
//! is unset. Clicking it sets the signal back to `false` (the message view
//! will own the real "scrolled up from bottom" state once wired in).
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `verified_badge` /
//! `idle_timer_badge` ship in).
//!
//! Idioms, each verified against this crate's `app.rs` /
//! `components/verified_badge.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the "only render when `visible`" part is a reactive
//!   `if visible.get() { … }` inside `ui!` — an `if` whose condition reads a
//!   signal (`.get()`) lowers to the `when` primitive (mounts while true,
//!   unmounts on false), with no `else` branch, exactly like the badge's
//!   reactive branch;
//! - the pill is a `view` + `.on_touch(|ev| …)` chain with the
//!   `TouchPhase::Ended` guard — the app's standard click pattern (the
//!   runtime's `button` helper takes a fixed `label` prop, which doesn't fit
//!   a styled floating pill);
//! - "floating" is `position: Absolute` with `bottom` / `right` insets
//!   (the `StyleRules` position fields), so once the screen that hosts it is
//!   wired in, it floats over the message list's bottom-right corner.

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, FontWeight, Position, StyleRules,
    TouchPhase, TouchResponse,
};

// Discord palette (same constants `app.rs` uses for the blurple button and
// near-white text) — kept local so the component is self-contained.
const BUTTON: &str = "#5865f2";
const TEXT: &str = "#f2f3f5";

/// The floating "jump to bottom" pill. Renders a down-arrow + "Jump to
/// bottom" label while the local `visible` signal is set (hardcoded `true`
/// for now); clicking sets it to `false`, which unmounts the pill.
#[component]
#[allow(non_snake_case)]
pub fn JumpToBottomButton() -> Element {
    let visible = signal(true);

    ui! {
        // Reactive branch: `if visible.get() {…}` lowers to a `when` whose
        // render is an `Fn` closure. `StyleRules` is not `Copy`, so both text
        // styles are written INLINE in `style = …` (created fresh per
        // evaluation, never captured across the `when` boundary) — the same
        // shape `verified_badge` uses. No `else` branch: while unset, the
        // button renders nothing.
        if visible.get() {
            view(style = StyleRules {
                position: Some(Position::Absolute),
                bottom: Some(px(16.0)),
                right: Some(px(16.0)),
                padding_left: Some(px(10.0)),
                padding_right: Some(px(10.0)),
                padding_top: Some(px(6.0)),
                padding_bottom: Some(px(6.0)),
                background: Some(col(BUTTON)),
                color: Some(col(TEXT)),
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(6.0)),
                font_size: Some(px(12.0)),
                font_weight: Some(FontWeight::SemiBold),
                border_top_left_radius: Some(px(16.0)),
                border_top_right_radius: Some(px(16.0)),
                border_bottom_left_radius: Some(px(16.0)),
                border_bottom_right_radius: Some(px(16.0)),
                ..Default::default()
            }) {
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { "↓".to_string() }
                text(style = StyleRules {
                    font_size: Some(px(12.0)),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { "Jump to bottom".to_string() }
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    visible.set(false);
                }
                TouchResponse::CONSUMED
            })
        }
    }
}
