//! `reply_preview_bar` — the "Replying to …" strip for the Discord clone.
//! Rendered directly above the composer while the user is composing a reply:
//! it names the message the user is replying to ("Replying to <author>:
//! <snippet>") with a small "×" cancel control at the right.
//!
//! Data-driven: the bar reads a `Signal<Option<(String, String)>>` — `None`
//! while no reply is active (the strip renders nothing), `Some((author,
//! snippet))` while one is (the strip shows). Tapping the "×" calls the
//! `on_cancel` callback, which the caller (the composer in `app.rs`) backs by
//! clearing the same signal.
//!
//! Idioms, each verified against this crate's `app.rs` (`emoji_popover` /
//! `reaction_picker_overlay` / `create_server_modal`):
//! - the show/hide is the `switch` primitive (`switch(|| reply.get(), …)`):
//!   its branch closure is *plain Rust*, so `if let Some((author, snippet))`
//!   binds the two local `String`s (the line text is built with `format!` from
//!   them, and re-runs whenever the caller repoints the reply context — the
//!   `reaction_area` rebuild idiom, no reactive f-string slot needed), and the
//!   `on_cancel` `Rc` is cloned there (branch-local, owned) so its `move`
//!   handler can own it;
//! - the `view` + `.on_touch(|ev| …)` "×" box is the app's standard click
//!   pattern (the runtime's `button` helper takes a fixed `label` prop, which
//!   doesn't fit a small square "×" glyph button);
//! - every style is written INLINE in `style = …` (created fresh per
//!   evaluation) — the crate's universal style idiom.
use crate::theme;
use std::rc::Rc;

use crate::style_helpers::{col, px};
use runtime_core::{
    switch, ui, AlignItems, Element, FlexDirection, JustifyContent, Signal, StyleRules, Tokenized,
    TouchPhase, TouchResponse,
};

// `StyleRules::flex_grow` is `Option<Tokenized<f32>>` — the one-line local
// helper `app.rs` defines, kept local so this file stays self-contained.
fn grow(v: f32) -> Tokenized<f32> {
    Tokenized::Literal(v)
}

// Discord palette — kept local so the component is self-contained. The bar
// sits over the chat surface, so it takes a slightly lighter fill than the
// chat background with a hairline bottom border to read as an inset strip.

/// The "Replying to <author>: <snippet>" strip with a "×" cancel control.
/// Shown while `reply` is `Some((author, snippet))`; the "×" calls `on_cancel`
/// (the caller clears `reply`). Renders nothing while `reply` is `None`.
pub fn reply_preview_bar(
    reply: Signal<Option<(String, String)>>,
    on_cancel: Rc<dyn Fn()>,
) -> Element {
    switch(
        move || reply.get(),
        move |ctx: &Option<(String, String)>| {
            // No active reply: a zero-size view so the strip unmounts without
            // affecting the composer's layout (the `emoji_popover` idiom).
            if ctx.is_none() {
                return ui! {
                    view(style = StyleRules {
                        width: Some(px(0.0)),
                        height: Some(px(0.0)),
                        ..Default::default()
                    }) {}
                };
            }
            // Bind the two local `String`s (this closure is plain Rust, so the
            // `if let` binding is legal here) and clone `on_cancel` here
            // (branch-local, owned) so its `move` handler can own it.
            if let Some((author, snippet)) = ctx {
                let cancel = on_cancel.clone();
                ui! {
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        padding_left: Some(px(12.0)),
                        padding_right: Some(px(12.0)),
                        padding_top: Some(px(6.0)),
                        padding_bottom: Some(px(6.0)),
                        gap: Some(px(8.0)),
                        background: Some(col(theme::BG_SECONDARY)),
                        // A hairline bottom border so the bar reads as an inset
                        // strip above the composer. Border *width* fields are
                        // `Tokenized<f32>` (not a `Length`), so the value goes
                        // through `Tokenized::Literal` rather than the `px`
                        // length helper.
                        border_bottom_width: Some(Tokenized::Literal(1.0)),
                        border_bottom_color: Some(col(theme::BG_TERTIARY)),
                        ..Default::default()
                    }) {
                        // The "Replying to <author>: <snippet>" line. Built with
                        // `format!` from the two branch-local `String`s: this
                        // branch re-runs whenever `reply` changes (the
                        // `reaction_area` rebuild idiom), so the line is always
                        // fresh without any reactive f-string slot.
                        text(style = StyleRules {
                            font_size: Some(px(13.0)),
                            color: Some(col(theme::TEXT_NORMAL)),
                            ..Default::default()
                        }) { format!("Replying to {}: {}", author, snippet) }

                        // Spacer pushing the "×" control to the right edge.
                        view(style = StyleRules {
                            flex_grow: Some(grow(1.0)),
                            ..Default::default()
                        }) {}

                        // The small "×" cancel control: an 18px disc with a
                        // centered glyph. The `move` closure captures a branch-
                        // local `Clone` of `on_cancel` (so it stays `Fn` and
                        // callable on every press); the `TouchPhase::Ended`
                        // guard + `CONSUMED` response is the crate's standard
                        // click pattern.
                        view(style = StyleRules {
                            width: Some(px(18.0)),
                            height: Some(px(18.0)),
                            align_items: Some(AlignItems::Center),
                            justify_content: Some(JustifyContent::Center),
                            background: Some(col(theme::BG_SELECTED)),
                            border_top_left_radius: Some(px(9.0)),
                            border_top_right_radius: Some(px(9.0)),
                            border_bottom_left_radius: Some(px(9.0)),
                            border_bottom_right_radius: Some(px(9.0)),
                            ..Default::default()
                        }) {
                            text(style = StyleRules {
                                font_size: Some(px(12.0)),
                                color: Some(col(theme::TEXT_STRONG)),
                                ..Default::default()
                            }) { "×".to_string() }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                cancel();
                            }
                            TouchResponse::CONSUMED
                        })
                    }
                }
            } else {
                // Unreachable (guarded above), but keeps the branch total.
                ui! {
                    view(style = StyleRules {
                        width: Some(px(0.0)),
                        height: Some(px(0.0)),
                        ..Default::default()
                    }) {}
                }
            }
        },
    )
}
