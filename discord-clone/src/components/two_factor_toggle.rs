//! `two_factor_toggle` — a self-contained two-factor-authentication switch for
//! the Discord clone. It renders a settings row bound to a local `enabled`
//! boolean signal (hardcoded `false` at start, so the "Disabled" state shows
//! first). A `button` (the switch) flips `enabled` on every press, and the
//! row's status label swaps between "Disabled" and "Enabled" with the signal.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `translate_toggle` /
//! `mute_channel_toggle` / `verified_badge` ship in).
//!
//! Idioms, each verified against this crate's `components/translate_toggle.rs`
//! / `components/mute_channel_toggle.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the switch is the `button(label, on_click)` primitive; `on_click` is a
//!   plain `move` closure prebuilt in a `let` and passed by variable name
//!   (exactly the `button` in `translate_toggle`). `Signal` is `Copy`, so the
//!   `move` closure captures a copy of the signal and stays `Fn` (callable on
//!   every press);
//! - the label swap is a node-level reactive
//!   `if enabled.get() { … } else { … }` inside `ui!`. An `if` whose condition
//!   reads a signal (`.get()`) lowers to the `when` primitive (mounts the
//!   chosen branch, unmounts the other on toggle) — the same reactive `if`
//!   `translate_toggle` uses for its original/translated swap. `StyleRules` is
//!   not `Copy`, so both styles are written INLINE in `style = …` (created
//!   fresh per evaluation, never captured across the `when` boundary).

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, JustifyContent, StyleRules,
};

// Discord dark palette (kept local so the component is self-contained).
const BG_CARD: &str = "#2b2d31"; // card surface (Discord sidebar gray)
const TEXT: &str = "#f2f3f5"; // primary text
const TEXT_MUTED: &str = "#949ba4"; // muted text — the "Disabled" state
const GREEN: &str = "#43b581"; // success green — the "Enabled" state
const BLURPLE: &str = "#5865f2"; // switch button (Discord accent)

/// The two-factor switch. Renders a settings row whose status label reads
/// "Disabled" while the local `enabled` signal is `false` (the start state)
/// and "Enabled" while it is `true`; a `button` flips the signal on every
/// press, swapping the label. No backend call — only the local signal toggle.
#[component]
pub fn TwoFactorToggle() -> Element {
    let enabled = signal(false);

    // Toggle: flip `enabled` on every press. `enabled` is `Copy`, so the `move`
    // closure captures a copy of the same underlying signal (the one the
    // reactive `if` below also reads) and remains `Fn`.
    let toggle = move || enabled.set(!enabled.get());

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
            // The static setting title.
            text(style = StyleRules {
                font_size: Some(px(18.0)),
                color: Some(col(TEXT)),
                ..Default::default()
            }) { "Two-Factor Authentication".to_string() }

            // Reactive branch: `if enabled.get() {…} else {…}` lowers to a
            // `when` (mounts the chosen branch, unmounts the other on toggle).
            // The style is inline so nothing is captured across the `when`
            // boundary (`StyleRules` is not `Copy`).
            if enabled.get() {
                text(style = StyleRules {
                    font_size: Some(px(16.0)),
                    color: Some(col(GREEN)),
                    ..Default::default()
                }) { "Enabled".to_string() }
            } else {
                text(style = StyleRules {
                    font_size: Some(px(16.0)),
                    color: Some(col(TEXT_MUTED)),
                    ..Default::default()
                }) { "Disabled".to_string() }
            }

            // The switch. `on_click = toggle` is the app's standard button
            // idiom (a prebuilt `move` closure by variable name) — no backend
            // call, just the local signal flip.
            button(
                label = "Toggle".to_string(),
                on_click = toggle,
                style = StyleRules {
                    height: Some(px(30.0)),
                    padding_left: Some(px(16.0)),
                    padding_right: Some(px(16.0)),
                    background: Some(col(BLURPLE)),
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
