//! `send_failed_indicator` — a self-contained "message failed to send"
//! indicator for the Discord clone. It renders a small warning-triangle glyph
//! beside a "Failed to send - Retry" label inside a compact pill. The pill
//! carries a click handler that flips a local `retrying` boolean signal to
//! `true` (hardcoded `false` at start); once `retrying` is set, the label is
//! replaced by "Retrying..." (the original text unmounts, the retry text
//! mounts).
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape
//! `verified_badge` / `idle_timer_badge` / `jump_to_bottom_button` ship in).
//!
//! Idioms, each verified against this crate's `components/idle_timer_badge.rs`
//! / `components/jump_to_bottom_button.rs` / `components/verified_badge.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the "swap the label once retrying" part is a reactive
//!   `if retrying.get() { … }` inside `ui!` with no `else`. An `if` whose
//!   condition reads a signal (`.get()`) lowers to the `when` primitive
//!   (mounts while true, unmounts on false) — the same reactive `if`
//!   `verified_badge` uses for its checkmark. The static "Failed to send -
//!   Retry" text is a plain sibling of that `when`, so the two labels are
//!   mutually exclusive by construction: exactly one is mounted at a time.
//! - the click is the pill's `.on_touch(|ev| …)` chain with the
//!   `TouchPhase::Ended` guard — the app's standard click pattern
//!   (`jump_to_bottom_button` / `message_rating`); the whole pill is the
//!   retry affordance, so one handler on the root covers it. The handler is a
//!   `move` closure capturing a `Copy` of the `retrying` signal, so it stays
//!   `Fn` (callable on every press) and merely drives the local signal — no
//!   backend call.
//! - `StyleRules` is not `Copy`, so the per-node styles are written INLINE in
//!   `style = …` (created fresh per evaluation, never captured across the
//!   `when` boundary) — the same shape `idle_timer_badge` uses for its badge.

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, Position, StyleRules, TouchPhase,
    TouchResponse,
};

// Discord dark palette (kept local so the component is self-contained).
const BG_PILL: &str = "#2b2d31"; // the pill surface (Discord sidebar gray)
const WARN: &str = "#f0b232"; // the warning-triangle glyph (Discord warning amber)
const TEXT: &str = "#f2f3f5"; // the "Failed to send - Retry" label (Discord near-white)
const TEXT_MUTED: &str = "#949ba4"; // the "Retrying..." label (Discord muted text)

/// The send-failure indicator. A small pill with a warning-triangle glyph and
/// a "Failed to send - Retry" label; clicking the pill flips the local
/// `retrying` signal to `true`, after which the label renders "Retrying..."
/// instead (the original text unmounts).
#[component]
#[allow(non_snake_case)]
pub fn SendFailedIndicator() -> Element {
    let retrying = signal(false);

    ui! {
        view(style = StyleRules {
            position: Some(Position::Relative),
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(6.0)),
            padding_left: Some(px(10.0)),
            padding_right: Some(px(10.0)),
            padding_top: Some(px(5.0)),
            padding_bottom: Some(px(5.0)),
            background: Some(col(BG_PILL)),
            border_top_left_radius: Some(px(8.0)),
            border_top_right_radius: Some(px(8.0)),
            border_bottom_left_radius: Some(px(8.0)),
            border_bottom_right_radius: Some(px(8.0)),
            ..Default::default()
        }) {
            // The warning glyph — a static string literal, so it bakes to
            // `TextSource::Static` (no reactive machinery).
            text(style = StyleRules {
                font_size: Some(px(14.0)),
                color: Some(col(WARN)),
                ..Default::default()
            }) { "⚠".to_string() }
            // The static failure label. It is a plain sibling of the reactive
            // `when` below, so it is always mounted while the pill is; the two
            // labels are mutually exclusive by construction (see the `when`).
            text(style = StyleRules {
                font_size: Some(px(13.0)),
                color: Some(col(TEXT)),
                ..Default::default()
            }) { "Failed to send - Retry".to_string() }
            // Reactive branch: `if retrying.get() {…}` lowers to a `when` whose
            // render is an `Fn` closure (mounts while true, unmounts on false).
            // No `else` — while unset, only the "Failed to send - Retry" label
            // shows. `StyleRules` is not `Copy`, so this style is written INLINE
            // (created fresh per evaluation, never captured across the `when`
            // boundary).
            if retrying.get() {
                text(style = StyleRules {
                    font_size: Some(px(13.0)),
                    color: Some(col(TEXT_MUTED)),
                    ..Default::default()
                }) { "Retrying...".to_string() }
            }
        }
        .on_touch(move |ev| {
            // The retry affordance: flip the local signal to `true` on press.
            // `retrying` is `Copy`, so the `move` closure captures a copy of the
            // same underlying signal (the one the reactive `if` above reads) and
            // remains `Fn`. No backend call — this only drives the local state.
            if ev.phase == TouchPhase::Ended {
                retrying.set(true);
            }
            TouchResponse::CONSUMED
        })
    }
}
