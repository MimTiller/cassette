#![allow(dead_code)]
//! `char_counter` — a self-contained character counter for the Discord clone.
//! Renders a `text_input` bound to a local `text` signal (starts empty) above
//! a right-aligned `"{n}/2000"` line, where `n` is the current input's
//! character count. Once `n` exceeds 1900 the counter line flips to a warning
//! color (Discord yellow); while at or under 1900 it is the muted gray.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `nickname_edit_field` /
//! `ban_reason_input` / `profile_settings` ship in).
//!
//! Idioms, each verified against this crate:
//! - the text input is the `text_input(value = …, on_change = …,
//!   placeholder = …, style = …)` primitive — exactly the `profile_settings`
//!   wiring. `value` is the `Signal<String>` (cloned, since `on_change` also
//!   captures a copy); `on_change` is a plain `move |s: String| text.set(s)`
//!   closure that pushes every native edit back into the same signal. `Signal`
//!   is `Copy`, so the closure stays `Fn` across edits.
//! - the `"{n}/2000"` line is a `move || String` closure used as a `text`
//!   content — the reactive single-node idiom `profile_settings` uses for its
//!   live avatar letter (the `text` slot re-runs the closure whenever the
//!   captured signal changes). It captures a copy of the `text` signal and
//!   returns a fresh `String` each evaluation, so it is `Copy` and safe to
//!   reference from both arms of the `when` below.
//! - the warning flip is a node-level reactive `if … > WARN_AT { … } else { … }`
//!   inside `ui!`, which lowers to the `when` primitive (mounts the warning
//!   arm once the count crosses the threshold, the muted arm otherwise) — the
//!   same swap `ban_reason_input` uses for its banned/unbanned states. The
//!   counter sits in a right-aligned row `view` (`justify_content: FlexEnd`, a
//!   variant confirmed in-tree) so it reads like Discord's composer counter.

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, JustifyContent, StyleRules,
};

// The hard message limit shown in the counter, and the point at which it warns.
const LIMIT: usize = 2000;
const WARN_AT: usize = 1900;

// Discord dark palette (kept local so the component is self-contained).
const BG_CARD: &str = "#2b2d31"; // card surface (Discord sidebar gray)
const INPUT_BG: &str = "#1e1f22"; // input surface (Discord input gray)
const TEXT: &str = "#f2f3f5"; // primary text
const MUTED: &str = "#949ba4"; // counter at/under the warn threshold
const WARNING: &str = "#faa61a"; // counter over the warn threshold (Discord yellow)

/// A self-contained character counter. A `text_input` starts empty and is bound
/// to local `text` state; the `"{n}/2000"` line beneath it tracks the live
/// character count and turns warning-yellow once `n` exceeds 1900. No backend
/// call, no props, no navigation wiring yet.
#[component]
#[allow(non_snake_case)]
pub fn CharCounter() -> Element {
    // Local controlled state: the message text (empty until the user types).
    let text = signal(String::new());

    // Every native edit is pushed straight back into the same signal. `text`
    // is `Copy`, so this closure captures a copy and stays `Fn` on each edit.
    let on_change = move |s: String| text.set(s);

    // The live "{n}/2000" label. A `move || String` closure re-evaluates
    // whenever `text` changes (the `profile_settings` avatar-letter idiom). It
    // captures a copy of the signal, so it is `Copy` and can be shared by both
    // arms of the reactive `if`/`else` below. Character count, not bytes —
    // that's what the component is named for.
    let counter_label = {
        let t = text.clone();
        move || format!("{}/{}", t.get().chars().count(), LIMIT)
    };

    // Style for the input node (lives OUTSIDE the `when`, so a plain local
    // moved into the `text_input` exactly once).
    let input_style = StyleRules {
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        height: Some(px(40.0)),
        padding_left: Some(px(12.0)),
        padding_right: Some(px(12.0)),
        background: Some(col(INPUT_BG)),
        color: Some(col(TEXT)),
        font_size: Some(px(15.0)),
        border_top_left_radius: Some(px(6.0)),
        border_top_right_radius: Some(px(6.0)),
        border_bottom_left_radius: Some(px(6.0)),
        border_bottom_right_radius: Some(px(6.0)),
        ..Default::default()
    };

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Stretch),
            gap: Some(px(10.0)),
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
            text_input(
                value = text.clone(),
                on_change = on_change,
                placeholder = "Message",
                style = input_style,
            )

            // Right-aligned row (Discord's composer counter sits bottom-right).
            // Inside it, the reactive `if … > WARN_AT { … } else { … }` lowers
            // to a `when`: it mounts the warning-colored arm once the count
            // crosses the threshold and the muted arm otherwise. `counter_label`
            // is a `Copy` closure, so both arms can reference it.
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                justify_content: Some(JustifyContent::FlexEnd),
                ..Default::default()
            }) {
                if text.get().chars().count() > WARN_AT {
                    text(style = StyleRules {
                        font_size: Some(px(13.0)),
                        color: Some(col(WARNING)),
                        ..Default::default()
                    }) { counter_label }
                } else {
                    text(style = StyleRules {
                        font_size: Some(px(13.0)),
                        color: Some(col(MUTED)),
                        ..Default::default()
                    }) { counter_label }
                }
            }
        }
    }
}
