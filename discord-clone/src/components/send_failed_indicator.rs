//! `send_failed_indicator` — a "message failed to send" inline alert for the
//! Discord clone. It renders a small warning-triangle glyph beside a "Failed
//! to send - Retry" label inside a compact banner pill.
//!
//! The whole alert is gated on a `failed: Signal<bool>` prop (NOT local
//! state): while the signal is unset it renders NOTHING (a zero-size
//! placeholder), so it only appears as chrome above the composer after a real
//! send failure. The app owns the signal (`send_failed` in `app_shell`); the
//! gateway `Error` frame the server replies on a failed send sets it (see the
//! `gateway_incoming_effect` in `app.rs`). Clicking the pill (the "Retry"
//! affordance) flips `failed` back to `false`, dismissing the banner so the
//! user can resend.
//!
//! This is UI chrome, not message-list content: it sits directly above the
//! composer row (see `app.rs`), never inline among the messages. The previous
//! self-contained version (a local `retrying` boolean, "Retrying..." label,
//! always-mounted pill) was spliced into the message row during the
//! graveyard-component rewire, which rendered a "Failed to send - Retry"
//! button in the stream as if it were a chat message. That was the UI-chrome
//! bug; this rewrite takes the app's shared `failed` signal so it only shows
//! on a genuine failure and lives above the composer.
//!
//! Idioms, each verified against this crate's `app.rs` / `components/…`:
//! - the "show only while failed" part is a reactive `if failed.get() { … }
//!   else { zero-height spacer }` inside `ui!`. An `if` whose condition reads
//!   a signal lowers to the `when` primitive (mounts while true, unmounts on
//!   false); the `else` is a 0-height `view` so it occupies no layout.
//! - the click is the pill's `.on_touch(|ev| …)` chain with the
//!   `TouchPhase::Ended` guard — the app's standard click pattern; the handler
//!   captures a `Copy` of the `failed` signal, so it stays `Fn` and merely
//!   clears the shared state (no backend call — the user resends to retry).
//! - `StyleRules` is not `Copy`, so the per-node styles are written INLINE in
//!   `style = …` (created fresh per evaluation, never captured across the
//!   `when` boundary).

use crate::theme;
use crate::style_helpers::{col, px};
use runtime_core::{
    component, ui, AlignItems, Element, FlexDirection, Position, StyleRules, TouchPhase,
    TouchResponse,
};

// Discord dark palette (kept local so the component is self-contained).

/// The send-failure banner. While `failed` is set it renders a pill with a
/// warning-triangle glyph and a "Failed to send - Retry" label; clicking the
/// pill clears `failed` (dismissing the banner). While `failed` is unset it
/// renders nothing.
#[component]
pub fn SendFailedIndicator(failed: runtime_core::Signal<bool>) -> Element {
    ui! {
        if failed.get() {
            view(style = StyleRules {
                position: Some(Position::Relative),
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(6.0)),
                padding_left: Some(px(10.0)),
                padding_right: Some(px(10.0)),
                padding_top: Some(px(5.0)),
                padding_bottom: Some(px(5.0)),
                background: Some(col(theme::BG_SECONDARY)),
                border_top_left_radius: Some(px(8.0)),
                border_top_right_radius: Some(px(8.0)),
                border_bottom_left_radius: Some(px(8.0)),
                border_bottom_right_radius: Some(px(8.0)),
                margin_top: Some(px(6.0)),
                margin_bottom: Some(px(6.0)),
                margin_left: Some(px(16.0)),
                margin_right: Some(px(16.0)),
                ..Default::default()
            }) {
                // The warning glyph — a static string literal, so it bakes to
                // `TextSource::Static` (no reactive machinery).
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    color: Some(col(theme::WARNING)),
                    ..Default::default()
                }) { "⚠".to_string() }
                // The failure label — the "Retry" affordance (a tap dismisses
                // the banner so the user can resend).
                text(style = StyleRules {
                    font_size: Some(px(13.0)),
                    color: Some(col(theme::TEXT_STRONG)),
                    ..Default::default()
                }) { "Failed to send - Retry".to_string() }
            }
            .on_touch(move |ev| {
                // Dismiss: clear the shared `failed` signal on press. `failed`
                // is `Copy`, so the `move` closure captures a copy of the same
                // signal the reactive `if` reads and remains `Fn`. No backend
                // call — resending clears/retries naturally.
                if ev.phase == TouchPhase::Ended {
                    let f = failed;
                    f.set(false);
                }
                TouchResponse::CONSUMED
            })
        } else {
            // Nothing to show: a 0-height placeholder occupies no layout, so
            // the composer's y-offset never shifts when the banner appears or
            // disappears.
            view(style = StyleRules { height: Some(px(0.0)), ..Default::default() }) {}
        }
    }
}
