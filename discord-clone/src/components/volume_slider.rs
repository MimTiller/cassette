//! `volume_slider` — a self-contained volume control for the Discord clone.
//! It renders a "Volume" label above a row holding an increment/decrement
//! button pair (`-10` / `+10`) with the current level shown as a live numeric
//! text node between the two buttons. The level is a local `Signal<i32>`
//! (hardcoded `50` at start) clamped to the `0..=100` range on every press.
//! A real drag-slider widget is not used — the framework's `button` primitive
//! pair is enough for this control, per the task.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `poll_widget` /
//! `message_rating` / `soundboard_button` ship in).
//!
//! Idioms, each verified against this crate's `components/poll_widget.rs` /
//! `components/message_rating.rs` / `components/soundboard_button.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the level display is a reactive f-string `text { "{ident}" }` node whose
//!   interpolated identifier is a `Signal`. Such a slot becomes a LIVE text
//!   binding automatically (it subscribes to the signal and re-renders on
//!   every `.set`) — the framework's documented counter idiom
//!   (`poll_widget` / `message_rating`);
//! - each control is the `button(label, on_click, style)` primitive with a
//!   plain `move` closure prebuilt in a `let` and passed by variable name
//!   (exactly the `button` in `poll_widget`). `Signal` is `Copy`, so the
//!   `move` closure captures a copy of the same underlying signal (the one the
//!   reactive text slot also reads) and stays `Fn` (callable on every press);
//! - the clamp is done in the handler: `-10` floors at `0`, `+10` caps at
//!   `100`, so the stored value can never leave the `0..=100` range.

// Standalone: declared so it compiles, referenced by nothing yet — the same
// "crate-level allow" the other unwired components carry.
#![allow(dead_code)]

use crate::theme;
use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent,
    StyleRules,
};

// Discord dark palette (kept local so the component is self-contained).

/// The one hardcoded starting level — no props, no backend.
const VOLUME_START: i32 = 50;

/// The fixed step applied by each button press.
const STEP: i32 = 10;

/// Inclusive bounds of the volume range.
const VOLUME_MIN: i32 = 0;
const VOLUME_MAX: i32 = 100;

/// A volume control. Renders a "Volume" label above a row of `-10` / level /
/// `+10`; the level is a local `Signal<i32>` (starting at `50`) shown as a
/// live numeric node, and each button clamps the value to `0..=100`. No
/// backend call — only the local signal.
#[component]
#[allow(non_snake_case)]
pub fn VolumeSlider() -> Element {
    // The one local level signal, `Copy`. The `move` handlers below capture a
    // copy of the same underlying signal (the one the reactive text slot also
    // reads), so they remain `Fn` (callable on every press).
    let volume = signal(VOLUME_START);

    // Decrement by the fixed step, clamped to the `0..=100` range.
    let on_dec = move || {
        let next = (volume.get() - STEP).clamp(VOLUME_MIN, VOLUME_MAX);
        volume.set(next);
    };
    // Increment by the fixed step, clamped to the `0..=100` range.
    let on_inc = move || {
        let next = (volume.get() + STEP).clamp(VOLUME_MIN, VOLUME_MAX);
        volume.set(next);
    };

    ui! {
        // The volume card: a dark surface holding the label and the control
        // row (decrement / live level / increment).
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Center),
            gap: Some(px(10.0)),
            padding_left: Some(px(12.0)),
            padding_right: Some(px(12.0)),
            padding_top: Some(px(12.0)),
            padding_bottom: Some(px(12.0)),
            background: Some(col(theme::BG_SECONDARY)),
            border_top_left_radius: Some(px(10.0)),
            border_top_right_radius: Some(px(10.0)),
            border_bottom_left_radius: Some(px(10.0)),
            border_bottom_right_radius: Some(px(10.0)),
            ..Default::default()
        }) {
            // The "Volume" header — a static string body, so it bakes to a
            // static `TextSource` (no reactive machinery).
            text(style = StyleRules {
                font_size: Some(px(15.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(theme::TEXT_NORMAL)),
                ..Default::default()
            }) { "Volume".to_string() }

            // The control row: -10 / live level / +10.
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(8.0)),
                ..Default::default()
            }) {
                // The decrement button: the `button` primitive with a prebuilt
                // `move` closure that lowers `volume` by `STEP`, clamped to
                // `0..=100`.
                button(
                    label = "-10".to_string(),
                    on_click = on_dec,
                    style = StyleRules {
                        height: Some(px(26.0)),
                        padding_left: Some(px(12.0)),
                        padding_right: Some(px(12.0)),
                        background: Some(col(theme::ACCENT)),
                        border_top_left_radius: Some(px(5.0)),
                        border_top_right_radius: Some(px(5.0)),
                        border_bottom_left_radius: Some(px(5.0)),
                        border_bottom_right_radius: Some(px(5.0)),
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        ..Default::default()
                    }
                )
                // The LIVE level: a reactive f-string slot. `volume` is a
                // `Signal`, so this slot is LIVE — it subscribes to the signal
                // and re-renders on every `.set` (the framework's documented
                // counter idiom), with a `compute_fallback` closure keeping it
                // correct on every backend.
                text(style = StyleRules {
                    font_size: Some(px(13.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    color: Some(col(theme::TEXT_NORMAL)),
                    ..Default::default()
                }) { "{volume}" }
                // The increment button: its own closure that raises `volume`
                // by `STEP`, clamped to `0..=100`.
                button(
                    label = "+10".to_string(),
                    on_click = on_inc,
                    style = StyleRules {
                        height: Some(px(26.0)),
                        padding_left: Some(px(12.0)),
                        padding_right: Some(px(12.0)),
                        background: Some(col(theme::ACCENT)),
                        border_top_left_radius: Some(px(5.0)),
                        border_top_right_radius: Some(px(5.0)),
                        border_bottom_left_radius: Some(px(5.0)),
                        border_bottom_right_radius: Some(px(5.0)),
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
