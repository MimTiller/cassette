//! `custom_status_input` — a self-contained "custom status" prompt for the
//! Discord clone. Renders a single-line `text_input` bound to a local `status`
//! string signal (placeholder `"What's on your mind?"`, starts empty) above a
//! `Clear` button. Pressing `Clear` resets the same signal to an empty string,
//! which reactively empties the input — the input is the sole consumer of the
//! signal and `Clear` its sole writer (besides native typing).
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `ban_reason_input` /
//! `profile_settings` ship in).
//!
//! Idioms, each verified against this crate:
//! - the input is the `text_input(value, placeholder, style)` canonical `ui!`
//!   tag (lowercase primitive), exactly as `profile_settings.rs:94` /
//!   `app.rs:1081` use it. `value` is the controlled signal (`Signal<String>`
//!   coerces to `IntoValue<String>`), so native typing writes straight back
//!   into the signal with no separate `on_change` needed;
//! - the `Clear` button is the `button(label, on_click, style)` primitive with
//!   a prebuilt `move` closure (`ban_reason_input.rs:60`). `Signal` is `Copy`,
//!   so the `move` closure captures a copy of the same `status` signal the
//!   input is bound to and stays `Fn` across every press;
//! - `Clear` just does `status.set(String::new())` — because the input's
//!   `value` reads that very signal, the reset is reactive and needs no
//!   imperative "clear the field" call;
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors, hoisted into `let` locals used exactly once each (no
//!   reactive branch here, so no `move ||` style closures required).

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent,
    StyleRules,
};

// Discord dark palette (kept local so the component is self-contained).
const BG_CARD: &str = "#2b2d31"; // surrounding surface (Discord sidebar gray)
const BG_INPUT: &str = "#383a40"; // input surface (Discord composer gray)
const BTN_BG: &str = "#4e5058"; // Clear button surface (Discord muted gray)
const TEXT: &str = "#f2f3f5"; // primary text
const TEXT_MUTED: &str = "#949ba4"; // header label (Discord secondary text)

/// The custom-status prompt. Renders a single-line `text_input` (placeholder
/// `"What's on your mind?"`, empty at start) above a `Clear` button that
/// resets the input's local state back to empty. No backend call, no props —
/// local state only.
#[component]
#[allow(non_snake_case)]
pub fn CustomStatusInput() -> Element {
    // Local controlled state: the status text. Empty until the user types; the
    // input's `value` reads it (typing writes back) and `Clear` resets it.
    let status = signal(String::new());

    // `Clear`: reset the status signal to empty. `status` is `Copy`, so the
    // `move` closure captures a copy of the same underlying signal the input
    // is bound to and remains `Fn` on every press. No backend call.
    let clear = move || status.set(String::new());

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Stretch),
            gap: Some(px(12.0)),
            padding_top: Some(px(20.0)),
            padding_bottom: Some(px(20.0)),
            padding_left: Some(px(20.0)),
            padding_right: Some(px(20.0)),
            background: Some(col(BG_CARD)),
            border_top_left_radius: Some(px(8.0)),
            border_top_right_radius: Some(px(8.0)),
            border_bottom_left_radius: Some(px(8.0)),
            border_bottom_right_radius: Some(px(8.0)),
            ..Default::default()
        }) {
            text(style = StyleRules {
                font_size: Some(px(13.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(TEXT_MUTED)),
                ..Default::default()
            }) { "CUSTOM STATUS".to_string() }
            text_input(
                value = status,
                placeholder = "What's on your mind?",
                style = StyleRules {
                    height: Some(px(40.0)),
                    padding_left: Some(px(12.0)),
                    padding_right: Some(px(12.0)),
                    background: Some(col(BG_INPUT)),
                    color: Some(col(TEXT)),
                    font_size: Some(px(15.0)),
                    border_top_left_radius: Some(px(8.0)),
                    border_top_right_radius: Some(px(8.0)),
                    border_bottom_left_radius: Some(px(8.0)),
                    border_bottom_right_radius: Some(px(8.0)),
                    ..Default::default()
                },
            )
            button(
                label = "Clear".to_string(),
                on_click = clear,
                style = StyleRules {
                    height: Some(px(32.0)),
                    padding_left: Some(px(16.0)),
                    padding_right: Some(px(16.0)),
                    background: Some(col(BTN_BG)),
                    color: Some(col(TEXT)),
                    font_size: Some(px(14.0)),
                    border_top_left_radius: Some(px(6.0)),
                    border_top_right_radius: Some(px(6.0)),
                    border_bottom_left_radius: Some(px(6.0)),
                    border_bottom_right_radius: Some(px(6.0)),
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    ..Default::default()
                },
            )
        }
    }
}
