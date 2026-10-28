//! `voice_join_button` — a self-contained voice join/leave trigger for the
//! Discord clone. It renders a single `button` bound to a local `connected`
//! boolean signal (hardcoded `false` at start). A click toggles the signal:
//! while it is `false` the button reads "Join Voice" on the blurple accent,
//! and while it is `true` it reads "Leave Voice" in a distinct color (Discord
//! danger red). No real voice connection is made and there is no backend call.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `soundboard_button` /
//! `verified_badge` / `idle_timer_badge` ship in).
//!
//! Idioms, each verified against this crate's `components/soundboard_button.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the button is the `button(label, on_click)` primitive; `on_click` is a
//!   plain `move` closure prebuilt in a `let` and passed by variable name.
//!   `Signal` is `Copy`, so the `move` closure captures a copy of the signal
//!   and stays `Fn` (callable on every press);
//! - the "styled differently while `connected`" part is a node-level reactive
//!   `if connected.get() { … } else { … }` inside `ui!`. An `if` whose
//!   condition reads a signal (`.get()`) lowers to the `when` primitive
//!   (mounts the chosen branch, unmounts the other on toggle). Because
//!   `StyleRules` is not `Copy`, both styles are written INLINE in `style = …`
//!   (created fresh per evaluation, never captured across the `when` boundary);
//!   the `on_click` closure is `Copy` (it captures only the `Copy` signal), so
//!   BOTH branches can capture it by copy. Unlike `soundboard_button` there is
//!   no auto-revert timer — the state persists until the next click.

use crate::theme;
use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, JustifyContent, StyleRules,
};

// Discord palette (kept local so the component is self-contained).

/// A voice join/leave trigger. Renders one `button` bound to a local
/// `connected` signal. While the signal is `false` the button shows "Join
/// Voice" on the blurple accent; a click flips it to `true`, swapping the
/// button to "Leave Voice" in a distinct (danger red) color. Clicking again
/// toggles it back. No real voice connection, no backend call.
#[component]
pub fn VoiceJoinButton() -> Element {
    let connected = signal(false);

    // Click: toggle the `connected` state. `connected` is `Copy`, so the
    // `move` closure captures a copy of the signal (the one the reactive `if`
    // below also reads) and stays `Fn`/`Copy` — both branches can capture it
    // by copy.
    let on_click = move || connected.set(!connected.get());

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::Center),
            padding_top: Some(px(20.0)),
            padding_bottom: Some(px(20.0)),
            padding_left: Some(px(20.0)),
            padding_right: Some(px(20.0)),
            ..Default::default()
        }) {
            // Reactive branch: `if connected.get() {…} else {…}` lowers to a
            // `when` (mounts the chosen branch, unmounts the other on toggle).
            // The styles are INLINE (created fresh per evaluation) because
            // `StyleRules` is not `Copy`; `on_click` IS `Copy`, so both
            // branches capture it by copy. No real voice call — only the
            // local signal flip.
            if connected.get() {
                button(
                    label = "Leave Voice".to_string(),
                    on_click = on_click,
                    style = StyleRules {
                        height: Some(px(34.0)),
                        padding_left: Some(px(18.0)),
                        padding_right: Some(px(18.0)),
                        background: Some(col(theme::DANGER)),
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
            } else {
                button(
                    label = "Join Voice".to_string(),
                    on_click = on_click,
                    style = StyleRules {
                        height: Some(px(34.0)),
                        padding_left: Some(px(18.0)),
                        padding_right: Some(px(18.0)),
                        background: Some(col(theme::ACCENT)),
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
