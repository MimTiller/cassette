//! `pin_confirm_toast` — a "message pinned" toast for the Discord clone. A
//! small banner that renders only while the caller-supplied `visible` signal
//! is `true`, showing the text `"Message pinned!"` with a close "x" button
//! that clears the same `visible` signal, dismissing the banner.
//!
//! The caller (`app.rs`) shows it by setting `visible` true (a "Pin message"
//! tap from the message action bar's overflow menu) and auto-dismisses it a
//! short while later (`after_ms_detached`); the "x" is the manual dismiss.
//! The toast is `Position::Absolute` at `top: 16 / right: 16` so it floats in
//! the top-right corner of the main view while visible, and collapses to
//! nothing (the reactive branch unmounts) the moment `visible` is cleared.
//!
//! Idioms, each verified against this crate's `components/welcome_banner.rs` /
//! `app.rs` (`emoji_popover` / `create_server_modal`):
//! - the show/hide is a reactive `if visible.get() {…}` inside `ui!` — the
//!   `if` condition reads a signal (`.get()`), so it lowers to the `when`
//!   primitive (mounts the banner while visible, unmounts it the moment
//!   `visible` is cleared). `StyleRules` is not `Copy`, so every style is
//!   written INLINE (created fresh per evaluation, never captured across the
//!   `when` boundary);
//! - the close "x" is a `view` + `.on_touch(|ev| …)` box with the
//!   `TouchPhase::Ended` guard — the app's standard click pattern. `visible`
//!   is `Copy` (`Signal<bool>`), so the `move` handler captures a copy of the
//!   same signal the branch reads and clears it — no callback prop needed.
use crate::theme;
use crate::style_helpers::{col, px};
use runtime_core::{
    ui, AlignItems, Element, FlexDirection, JustifyContent, Position, Signal, StyleRules,
    TouchPhase, TouchResponse,
};

// Discord dark palette — kept local so the component is self-contained.

// The hardcoded toast copy. No backend fetch.
const TOAST_TEXT: &str = "Message pinned!";

/// The "message pinned" toast. Renders the `"Message pinned!"` banner while
/// `visible` is `true`; tapping the close "x" clears `visible` (manual
/// dismiss). Absolutely positioned at the top-right by the caller's
/// positioning ancestor.
pub fn pin_confirm_toast(visible: Signal<bool>) -> Element {
    // `visible` is `Copy`; both the branch condition and the "x" handler read
    // copies of the same underlying signal.
    let visible_chk = visible;
    ui! {
        // Reactive branch: `if visible_chk.get() {…}` lowers to a `when` that
        // mounts the banner while visible and unmounts it the moment `visible`
        // is cleared (either the "x" or the caller's auto-dismiss timer).
        if visible_chk.get() {
            view(style = StyleRules {
                position: Some(Position::Absolute),
                top: Some(px(16.0)),
                right: Some(px(16.0)),
                width: Some(px(320.0)),
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(10.0)),
                padding_top: Some(px(14.0)),
                padding_bottom: Some(px(14.0)),
                padding_left: Some(px(16.0)),
                padding_right: Some(px(16.0)),
                background: Some(col(theme::BG_PRIMARY)),
                border_top_left_radius: Some(px(8.0)),
                border_top_right_radius: Some(px(8.0)),
                border_bottom_left_radius: Some(px(8.0)),
                border_bottom_right_radius: Some(px(8.0)),
                ..Default::default()
            }) {
                // The toast copy — a hardcoded local literal, so it bakes to
                // a static text source (no reactive machinery).
                text(style = StyleRules {
                    flex_grow: Some(runtime_core::Tokenized::Literal(1.0)),
                    font_size: Some(px(15.0)),
                    color: Some(col(theme::TEXT_STRONG)),
                    ..Default::default()
                }) { TOAST_TEXT.to_string() }

                // The close "x": a 24x24 tap target. `.on_touch` clears
                // `visible` on `TouchPhase::Ended` — the app's standard click
                // pattern. `visible` is `Copy`, so the `move` handler captures
                // a copy of the same signal the branch reads.
                view(style = StyleRules {
                    width: Some(px(24.0)),
                    height: Some(px(24.0)),
                    position: Some(Position::Relative),
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    border_top_left_radius: Some(px(6.0)),
                    border_top_right_radius: Some(px(6.0)),
                    border_bottom_left_radius: Some(px(6.0)),
                    border_bottom_right_radius: Some(px(6.0)),
                    border_top_width: Some(runtime_core::Tokenized::Literal(1.0)),
                    border_right_width: Some(runtime_core::Tokenized::Literal(1.0)),
                    border_bottom_width: Some(runtime_core::Tokenized::Literal(1.0)),
                    border_left_width: Some(runtime_core::Tokenized::Literal(1.0)),
                    border_top_color: Some(col(theme::BUTTON_SECONDARY)),
                    border_right_color: Some(col(theme::BUTTON_SECONDARY)),
                    border_bottom_color: Some(col(theme::BUTTON_SECONDARY)),
                    border_left_color: Some(col(theme::BUTTON_SECONDARY)),
                    ..Default::default()
                }) {
                    text(style = StyleRules {
                        font_size: Some(px(14.0)),
                        color: Some(col(theme::TEXT_MUTED)),
                        ..Default::default()
                    }) { "x".to_string() }
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
}
