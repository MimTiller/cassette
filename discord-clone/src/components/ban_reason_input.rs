#![allow(dead_code)]
//! `ban_reason_input` — a self-contained "ban user" reason prompt for the
//! Discord clone. Renders a `text_area` bound to a local `reason` string
//! signal (placeholder `"Reason for ban (optional)"`, starts empty) above a
//! `Ban` button. Pressing `Ban` sets a local `banned` boolean signal to
//! `true`; once true, the text area and button are replaced by a single
//! `"User banned"` text line.
//!
//! Everything is hardcoded local state — no backend call, no props, no
//! wiring into any screen yet. Declared in `lib.rs` so it compiles as part
//! of the crate, but referenced by nothing (the same shape
//! `user_bio_block` / `icon_change_confirm` ship in).
//!
//! Idioms, each verified against this crate:
//! - the text area is the `text_area(value, on_change)` positional
//!   constructor (a free function re-exported at the `runtime_core` root via
//!   the `glue::*` wildcard), exactly as `user_bio_block` uses it: `value`
//!   is the controlled signal (`Signal<String>` coerces to
//!   `IntoValue<String>`), `on_change` is the native-edit upward reporter,
//!   so we push every keystroke back into the same signal with
//!   `move |s| reason.set(s)`. The result is a `GlueTextArea` (an
//!   `IntoElement`/`ChildList`), prebuilt into a `field` local and dropped
//!   into the reactive `else` branch below;
//! - the button is the `button(label, on_click)` primitive with a prebuilt
//!   `move` closure (`icon_change_confirm`). `Signal` is `Copy`, so the
//!   closure captures a copy of `banned` and stays `Fn`;
//! - the text-area-and-button / result-text swap is a node-level reactive
//!   `if banned.get() { … } else { … }` inside `ui!`, which lowers to the
//!   `when` primitive (mounts the chosen branch, unmounts the other when the
//!   signal flips). Styles are inline so nothing non-`Copy` is captured
//!   across the `when` boundary.

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, text_area, ui, AlignItems, Element, FlexDirection, JustifyContent,
    StyleRules, Tokenized,
};

// Discord dark palette (kept local so the component is self-contained).
const BG_CARD: &str = "#2b2d31"; // card surface (Discord sidebar gray)
const TEXT: &str = "#f2f3f5"; // primary text
const INPUT_BG: &str = "#1e1f22"; // text area surface (Discord input gray)
const INPUT_BORDER: &str = "#4e5058"; // text area border (Discord muted gray)
const BAN_BG: &str = "#da373c"; // ban button (Discord danger red)

/// The ban reason prompt. Renders a reason `text_area` (placeholder
/// `"Reason for ban (optional)"`) above a `Ban` button; pressing `Ban`
/// swaps the two for a `"User banned"` line. No backend call, no props —
/// local state only.
#[component]
pub fn BanReasonInput() -> Element {
    // Local controlled state: the ban reason (empty until the user types)
    // and whether the ban has been applied.
    let reason = signal(String::new());
    let banned = signal(false);

    // The `Ban` button flips `banned` to `true` (one-way). `Signal` is
    // `Copy`, so the `move` closure captures a copy of the same signal and
    // stays `Fn` across every press. No backend call.
    let ban = move || banned.set(true);

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Center),
            gap: Some(px(14.0)),
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
            // Reactive swap: `if banned.get() { … } else { … }` lowers to a
            // `when` (mounts the chosen branch, unmounts the other when `Ban`
            // sets the signal). Styles are inline so nothing non-`Copy` is
            // captured across the `when` boundary. The `field` text area is
            // moved (owned) into the `else` closure exactly once.
            if banned.get() {
                text(style = StyleRules {
                    font_size: Some(px(16.0)),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { "User banned".to_string() }
            } else {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Column),
                    align_items: Some(AlignItems::Stretch),
                    gap: Some(px(12.0)),
                    ..Default::default()
                }) {
                    // The controlled text area. `text_area` is not a canonical
                    // `ui!` tag (only `text_input` is), so it must be a Rust
                    // expression here, not a lowercase tag. The whole chain is
                    // evaluated *inside* the reactive `else` closure, which
                    // must be `Fn` (re-runnable on every re-eval); `text_area`
                    // yields a `GlueTextArea` (non-`Copy`), so it cannot be a
                    // hoisted local moved into the closure — building it fresh
                    // each call sidesteps the move. `reason` and `banned` are
                    // `Copy` signals, so the inline `on_change`/`ban` closures
                    // each capture a copy of the same underlying signal and
                    // stay `Fn`.
                    text_area(reason, move |s: String| reason.set(s))
                        .placeholder("Reason for ban (optional)".to_string())
                        .min_rows(3)
                        .max_rows(6)
                        .with_style(StyleRules {
                            width: Some(px(320.0)),
                            background: Some(col(INPUT_BG)),
                            border_top_left_radius: Some(px(6.0)),
                            border_top_right_radius: Some(px(6.0)),
                            border_bottom_left_radius: Some(px(6.0)),
                            border_bottom_right_radius: Some(px(6.0)),
                            border_top_width: Some(Tokenized::Literal(1.0)),
                            border_right_width: Some(Tokenized::Literal(1.0)),
                            border_bottom_width: Some(Tokenized::Literal(1.0)),
                            border_left_width: Some(Tokenized::Literal(1.0)),
                            border_top_color: Some(col(INPUT_BORDER)),
                            border_right_color: Some(col(INPUT_BORDER)),
                            border_bottom_color: Some(col(INPUT_BORDER)),
                            border_left_color: Some(col(INPUT_BORDER)),
                            ..Default::default()
                        })

                    button(
                        label = "Ban".to_string(),
                        on_click = ban,
                        style = StyleRules {
                            height: Some(px(32.0)),
                            padding_left: Some(px(16.0)),
                            padding_right: Some(px(16.0)),
                            background: Some(col(BAN_BG)),
                            border_top_left_radius: Some(px(6.0)),
                            border_top_right_radius: Some(px(6.0)),
                            border_bottom_left_radius: Some(px(6.0)),
                            border_bottom_right_radius: Some(px(6.0)),
                            flex_direction: Some(FlexDirection::Row),
                            align_items: Some(AlignItems::Center),
                            justify_content: Some(JustifyContent::Center),
                            ..Default::default()
                        }
                    )
                }
            }
        }
    }
}
