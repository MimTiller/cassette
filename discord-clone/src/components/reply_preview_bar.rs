#![allow(dead_code)]
//! `reply_preview_bar` — a self-contained "Replying to …" bar for the
//! Discord clone. While active it renders a single line —
//! "Replying to <author>: <snippet>" — with a small "x" cancel control at
//! the right. Pressing the "x" sets the local `cancelled` boolean signal to
//! `true`, which unmounts the bar entirely (it renders nothing once
//! cancelled).
//!
//! Everything is hardcoded local state — the author name and message snippet
//! are baked in (no props, no backend call), and the cancel flag is local.
//! Not wired into any screen yet; declared in `lib.rs` so it compiles as part
//! of the crate, referenced by nothing (the same shape
//! `jump_to_bottom_button` / `idle_timer_badge` ship in).
//!
//! Idioms, each verified against this crate's `components/jump_to_bottom_button.rs`
//! / `components/idle_timer_badge.rs` / `profile_settings.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the "hide when cancelled" part is a reactive
//!   `if cancelled.get() { … }` — an `if` whose condition reads a signal
//!   (`.get()`) lowers to the `when` primitive (mounts while true, unmounts
//!   on false), with no `else` branch, exactly like
//!   `jump_to_bottom_button`'s reactive branch; the `if` is the root item of
//!   the `ui!` block, so once cancelled the component's whole body renders
//!   nothing;
//! - the "x" control is a `view` + `.on_touch(|ev| …)` chain with the
//!   `TouchPhase::Ended` guard — the app's standard click pattern (the
//!   runtime's `button` helper takes a fixed `label` prop, which doesn't fit
//!   a small square "x" glyph button); `Signal` is `Copy`, so the `move`
//!   closure captures a copy of the signal and stays `Fn` across presses.

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, JustifyContent, StyleRules,
    Tokenized, TouchPhase, TouchResponse,
};

// `StyleRules::flex_grow` is `Option<Tokenized<f32>>` — the same one-line
// local helper `profile_settings.rs` / `app.rs` define, kept local so this
// file stays self-contained.
fn grow(v: f32) -> Tokenized<f32> {
    Tokenized::Literal(v)
}

// Discord palette — kept local so the component is self-contained. The bar
// sits over the chat surface, so it takes a slightly lighter fill than the
// chat background with a hairline bottom border to read as an inset bar.
const BG_BAR: &str = "#2b2d31"; // bar fill (the "hover/active" tone)
const BG_X: &str = "#404249"; // the "x" control's disc (Discord hover gray)
const BORDER: &str = "#1e1f22"; // hairline bottom border (the rail's dark tone)
const TEXT: &str = "#dbdee1"; // the "Replying to …" line (Discord primary text)
const TEXT_X: &str = "#f2f3f5"; // the "x" glyph (near-white, to stay tappable)

/// A "Replying to <author>: <snippet>" bar with a small "x" cancel control.
/// The author ("Sam") and snippet ("sounds good!") are hardcoded local values;
/// pressing the "x" flips the local `cancelled` signal, after which the whole
/// bar renders nothing.
#[component]
#[allow(non_snake_case)]
pub fn ReplyPreviewBar() -> Element {
    // Hardcoded local inputs (the spec's example values) — no props.
    let author = "Sam";
    let snippet = "sounds good!";

    // Local cancel flag: starts active (the bar is shown). Pressing the "x"
    // sets it to `true`, and the reactive `if` below then renders nothing.
    let cancelled = signal(false);

    ui! {
        // Reactive branch: `if !cancelled.get() {…}` lowers to a `when` whose
        // render is an `Fn` closure. This is the ROOT item of the `ui!` block
        // (no wrapping `view`), so once `cancelled` is set the component's
        // entire body unmounts — it renders nothing. `StyleRules` is not
        // `Copy`, so all styles are written INLINE in `style = …` (created
        // fresh per evaluation, never captured across the `when` boundary) —
        // the same shape `jump_to_bottom_button` uses for its inline pill
        // style.
        if !cancelled.get() {
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                padding_left: Some(px(12.0)),
                padding_right: Some(px(12.0)),
                padding_top: Some(px(6.0)),
                padding_bottom: Some(px(6.0)),
                gap: Some(px(8.0)),
                background: Some(col(BG_BAR)),
                // A hairline bottom border so the bar reads as an inset strip
                // above the composer. Border *width* fields are
                // `Tokenized<f32>` (not a `Length`), so the value goes through
                // `Tokenized::Literal` rather than the `px` length helper.
                border_bottom_width: Some(Tokenized::Literal(1.0)),
                border_bottom_color: Some(col(BORDER)),
                ..Default::default()
            }) {
                // The "Replying to <author>: <snippet>" line. The values are
                // hardcoded local `&str`s that never change after mount, so
                // the body is a static `format!` — no reactive slot needed.
                text(style = StyleRules {
                    font_size: Some(px(13.0)),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) {
                    format!("Replying to {author}: {snippet}")
                }

                // Spacer pushing the "x" control to the right edge.
                view(style = StyleRules {
                    flex_grow: Some(grow(1.0)),
                    ..Default::default()
                }) {}

                // The small "x" cancel control: an 18px disc with a centered
                // glyph. The `move` closure captures a `Copy` of the local
                // `cancelled` signal (the same one the `if` above reads), so
                // it stays `Fn` and is callable on every press; the
                // `TouchPhase::Ended` guard + `CONSUMED` response is the
                // crate's standard click pattern.
                view(style = StyleRules {
                    width: Some(px(18.0)),
                    height: Some(px(18.0)),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    background: Some(col(BG_X)),
                    border_top_left_radius: Some(px(9.0)),
                    border_top_right_radius: Some(px(9.0)),
                    border_bottom_left_radius: Some(px(9.0)),
                    border_bottom_right_radius: Some(px(9.0)),
                    ..Default::default()
                }) {
                    text(style = StyleRules {
                        font_size: Some(px(12.0)),
                        color: Some(col(TEXT_X)),
                        ..Default::default()
                    }) { "×".to_string() }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        cancelled.set(true);
                    }
                    TouchResponse::CONSUMED
                })
            }
        }
    }
}
