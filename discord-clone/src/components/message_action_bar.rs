//! `message_action_bar` — a self-contained hover action bar for the Discord
//! clone. Bound to a local `visible` boolean signal (hardcoded `true` at
//! start), it renders a single horizontal row of three small icon buttons —
//! react, reply, more — while the signal is set, and nothing at all while it
//! is unset. No backend call, no props, no wiring into any screen yet.
//! Declared in `lib.rs` so it compiles as part of the crate, but referenced
//! by nothing (the same shape `verified_badge` / `jump_to_bottom_button` /
//! `reaction_summary` ship in).
//!
//! Idioms, each verified against this crate's `components/message_rating.rs`
//! / `components/jump_to_bottom_button.rs` / `components/verified_badge.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the "only render when `visible`" part is a reactive
//!   `if visible.get() { … }` inside `ui!` — an `if` whose condition reads a
//!   signal (`.get()`) lowers to the `when` primitive (mounts while true,
//!   unmounts on false), with no `else` branch, exactly like the
//!   `jump_to_bottom_button` reactive branch. `StyleRules` is not `Copy`, so
//!   every style is written INLINE in `style = …` (created fresh per
//!   evaluation, never captured across the `when` boundary) — the same shape
//!   `verified_badge` / `jump_to_bottom_button` use;
//! - each icon button is a `view` + `.on_touch(|ev| …)` chain with the
//!   `TouchPhase::Ended` guard — the app's standard click pattern (the
//!   runtime's `button` helper takes a fixed `label` prop, which doesn't fit
//!   a glyph-only square). `Position::Relative` gives each button a definite
//!   box for hit-testing. The handlers are no-op `move` closures that only
//!   consume the event: there is no backend to call yet, so clicking does
//!   nothing beyond swallowing the press — the buttons are real, pressable
//!   targets, not inert glyphs;
//! - the react / reply / more glyphs are static string bodies
//!   (`.to_string()`), so they bake to static `TextSource` with no reactive
//!   machinery.

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, JustifyContent, Position,
    StyleRules, TouchPhase, TouchResponse,
};

// Discord dark palette (kept local so the component is self-contained).
const BAR_BG: &str = "#2b2d31"; // the action-bar surface (Discord sidebar gray)
const ICON: &str = "#b5bac1"; // the react/reply/more glyphs (Discord secondary text)

/// The message action bar. Renders a row of three small icon buttons (react,
/// reply, more) while the local `visible` signal is set (hardcoded `true` for
/// now); otherwise it renders nothing. Clicking a button consumes the press
/// but does nothing else — there is no backend call and no state change yet.
#[component]
#[allow(non_snake_case)]
pub fn MessageActionBar() -> Element {
    // The "is this message hovered" flag. Hardcoded `true` for now (no real
    // hover/pointer state to feed it); it is a real `Signal` so a later
    // `.set` would mount/unmount the bar reactively.
    let visible = signal(true);

    ui! {
        // Reactive branch: `if visible.get() {…}` lowers to a `when` whose
        // render is an `Fn` closure (it rebuilds many times), so the bar and
        // button styles are all written INLINE in `style = …` — never captured
        // across the `when` boundary. No `else` branch: while unset, the bar
        // renders nothing.
        if visible.get() {
            // The action bar: a horizontal row of the three icon buttons,
            // centered, on the elevated bar surface with rounded corners — the
            // shape Discord draws on a message's top-right on hover.
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(2.0)),
                padding_left: Some(px(4.0)),
                padding_right: Some(px(4.0)),
                padding_top: Some(px(2.0)),
                padding_bottom: Some(px(2.0)),
                background: Some(col(BAR_BG)),
                border_top_left_radius: Some(px(8.0)),
                border_top_right_radius: Some(px(8.0)),
                border_bottom_left_radius: Some(px(8.0)),
                border_bottom_right_radius: Some(px(8.0)),
                ..Default::default()
            }) {
                // React button — a small square glyph button. `Position::Relative`
                // gives it a definite hit-test box; the `.on_touch` handler is a
                // no-op (consume the press, call nothing) because there is no
                // backend yet.
                view(style = StyleRules {
                    position: Some(Position::Relative),
                    width: Some(px(32.0)),
                    height: Some(px(32.0)),
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    border_top_left_radius: Some(px(4.0)),
                    border_top_right_radius: Some(px(4.0)),
                    border_bottom_left_radius: Some(px(4.0)),
                    border_bottom_right_radius: Some(px(4.0)),
                    ..Default::default()
                }) {
                    text(style = StyleRules {
                        font_size: Some(px(16.0)),
                        color: Some(col(ICON)),
                        ..Default::default()
                    }) { "😊".to_string() }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        // No backend yet — the press just consumes itself.
                    }
                    TouchResponse::CONSUMED
                })

                // Reply button — same square shape, the reply arrow glyph.
                view(style = StyleRules {
                    position: Some(Position::Relative),
                    width: Some(px(32.0)),
                    height: Some(px(32.0)),
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    border_top_left_radius: Some(px(4.0)),
                    border_top_right_radius: Some(px(4.0)),
                    border_bottom_left_radius: Some(px(4.0)),
                    border_bottom_right_radius: Some(px(4.0)),
                    ..Default::default()
                }) {
                    text(style = StyleRules {
                        font_size: Some(px(16.0)),
                        color: Some(col(ICON)),
                        ..Default::default()
                    }) { "↩".to_string() }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        // No backend yet — the press just consumes itself.
                    }
                    TouchResponse::CONSUMED
                })

                // More button — same square shape, the horizontal-ellipsis glyph
                // (Discord's "…" overflow control).
                view(style = StyleRules {
                    position: Some(Position::Relative),
                    width: Some(px(32.0)),
                    height: Some(px(32.0)),
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    border_top_left_radius: Some(px(4.0)),
                    border_top_right_radius: Some(px(4.0)),
                    border_bottom_left_radius: Some(px(4.0)),
                    border_bottom_right_radius: Some(px(4.0)),
                    ..Default::default()
                }) {
                    text(style = StyleRules {
                        font_size: Some(px(16.0)),
                        color: Some(col(ICON)),
                        ..Default::default()
                    }) { "⋯".to_string() }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        // No backend yet — the press just consumes itself.
                    }
                    TouchResponse::CONSUMED
                })
            }
        }
    }
}
