//! `notification_toast` — a new-message notification toast for the Discord
//! clone. A top-right banner (the same shape as `pin_confirm_toast`) that
//! renders only while the caller-supplied `visible` signal is `true`, showing
//! the `title` / `body` lines (each a `Signal<String>` owned by `app_shell`)
//! with a close "x".
//!
//! It is a plain `fn -> Element` (NOT a `#[component]`) called as a child
//! expression in `app.rs`'s root `ui!` — the same shape as
//! `pin_confirm_toast` (a bare function call, splatted as a child). Reading the
//! three `Signal`s inside its own `ui!` means the toast reacts to the gateway
//! effect setting them on each inbound message WITHOUT the (huge) root `ui!`
//! rebuilding — the `if visible.get()` lowers to a `when` that tracks
//! `visible`, and the two `text` nodes track `title` / `body` so their copy
//! updates in place.
//!
//! Idioms mirror `pin_confirm_toast`: `Position::Absolute` top-right, the
//! `view` + `.on_touch` close "x" with the `TouchPhase::Ended` guard, and the
//! `Signal` handles (which are `Copy`) read by both the branch and the close
//! handler. The auto-dismiss (the ~4.5 s clear of `visible`) is the caller's
//! job (`notify::notify_message` arms it) — this component only owns the
//! manual "x".

use crate::theme;
use crate::style_helpers::{col, px, rounded};
use runtime_core::{
    ui, AlignItems, Element, FlexDirection, Position, Signal, StyleRules, TouchPhase, TouchResponse,
};

// Discord dark palette (kept local so the component is self-contained).

/// The new-message notification toast. Renders the `title` / `body` banner
/// while `visible` is `true`; tapping the close "x" clears `visible`. All three
/// are `Copy` `Signal` handles, so the branch, the two `text` nodes, and the
/// "x" handler read copies of the same signals the gateway effect writes.
pub fn notification_toast(
    visible: Signal<bool>,
    title: Signal<String>,
    body: Signal<String>,
) -> Element {
    let visible_chk = visible;
    ui! {
        if visible_chk.get() {
            view(style = StyleRules {
                position: Some(Position::Absolute),
                top: Some(px(16.0)),
                right: Some(px(16.0)),
                width: Some(px(340.0)),
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(10.0)),
                padding_top: Some(px(14.0)),
                padding_bottom: Some(px(14.0)),
                padding_left: Some(px(16.0)),
                padding_right: Some(px(16.0)),
                background: Some(col(theme::BG_PRIMARY)),
                ..rounded(8.0)
            }) {
                // The two lines: the title (author / mention), then the message
                // preview. Each reads its `Signal` reactively, so a new inbound
                // message updates the copy in place (no root rebuild).
                view(style = StyleRules {
                    flex_grow: Some(runtime_core::Tokenized::Literal(1.0)),
                    flex_direction: Some(FlexDirection::Column),
                    gap: Some(px(3.0)),
                    ..Default::default()
                }) {
                    text(style = StyleRules {
                        font_size: Some(px(14.0)),
                        font_weight: Some(runtime_core::FontWeight::SemiBold),
                        color: Some(col(theme::TEXT_STRONG)),
                        ..Default::default()
                    }) { move || title.get().to_string() }
                    text(style = StyleRules {
                        font_size: Some(px(13.0)),
                        color: Some(col(theme::TEXT_NORMAL)),
                        ..Default::default()
                    }) { move || body.get().to_string() }
                }
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
                    justify_content: Some(runtime_core::JustifyContent::Center),
                    border_top_width: Some(runtime_core::Tokenized::Literal(1.0)),
                    border_right_width: Some(runtime_core::Tokenized::Literal(1.0)),
                    border_bottom_width: Some(runtime_core::Tokenized::Literal(1.0)),
                    border_left_width: Some(runtime_core::Tokenized::Literal(1.0)),
                    border_top_color: Some(col(theme::BUTTON_SECONDARY)),
                    border_right_color: Some(col(theme::BUTTON_SECONDARY)),
                    border_bottom_color: Some(col(theme::BUTTON_SECONDARY)),
                    border_left_color: Some(col(theme::BUTTON_SECONDARY)),
                    ..rounded(6.0)
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
