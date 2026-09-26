//! `translate_toggle` — a self-contained translate switch for the Discord
//! clone. It renders one line of text that flips between two hardcoded local
//! strings — an original (`"Bonjour"`) and a `"translated"` (`"Hello"`)
//! version — driven by a local `showTranslated` boolean signal (hardcoded
//! `false` at start, so the original shows first). A `button` toggles the
//! signal on every press, swapping which string the line displays.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `verified_badge` /\
//! `idle_timer_badge` / `emoji_upload_slot` ship in).
//!
//! Idioms, each verified against this crate's `components/idle_timer_badge.rs`
//! / `components/emoji_upload_slot.rs` / `components/verified_badge.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the toggle is the `button(label, on_click)` primitive; `on_click` is a
//!   plain `move` closure prebuilt in a `let` and passed by variable name
//!   (exactly the `button` in `idle_timer_badge`). `Signal` is `Copy`, so the
//!   `move` closure captures a copy of the signal and stays `Fn` (callable on
//!   every press);
//! - the string swap is a node-level reactive
//!   `if showTranslated.get() { … } else { … }` inside `ui!`. An `if` whose
//!   condition reads a signal (`.get()`) lowers to the `when` primitive
//!   (mounts the chosen branch, unmounts the other on toggle) — the same
//!   reactive `if` `emoji_upload_slot` uses for its placeholder/circle swap.
//!   `StyleRules` is not `Copy`, so both styles are written INLINE in
//!   `style = …` (created fresh per evaluation, never captured across the
//!   `when` boundary).

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, JustifyContent, StyleRules,
};

// Discord dark palette (kept local so the component is self-contained).
const BG_CARD: &str = "#2b2d31"; // card surface (Discord sidebar gray)
const TEXT: &str = "#f2f3f5"; // primary text
const BLURPLE: &str = "#5865f2"; // toggle button (Discord accent)

// The two hardcoded local strings. Original language line and its (fake)
// translation — no backend call, just two literals the signal switches between.
const ORIGINAL: &str = "Bonjour";
const TRANSLATED: &str = "Hello";

/// The translate switch. Renders a single line of text that shows the original
/// string while the local `showTranslated` signal is `false` (the start state)
/// and the translated string while it is `true`; a `button` toggles the signal
/// on every press, swapping the displayed line.
#[component]
pub fn TranslateToggle() -> Element {
    let showTranslated = signal(false);

    // Toggle: flip `showTranslated` on every press. `showTranslated` is `Copy`,
    // so the `move` closure captures a copy of the same underlying signal (the
    // one the reactive `if` below also reads) and remains `Fn`.
    let toggle = move || showTranslated.set(!showTranslated.get());

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
            // Reactive branch: `if showTranslated.get() {…} else {…}` lowers to
            // a `when` (mounts the chosen branch, unmounts the other on toggle).
            // The style is inline so nothing is captured across the `when`
            // boundary (`StyleRules` is not `Copy`).
            if showTranslated.get() {
                text(style = StyleRules {
                    font_size: Some(px(18.0)),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { TRANSLATED.to_string() }
            } else {
                text(style = StyleRules {
                    font_size: Some(px(18.0)),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { ORIGINAL.to_string() }
            }

            // The toggle button. `on_click = toggle` is the app's standard
            // button idiom (a prebuilt `move` closure by variable name) — no
            // backend call, just the local signal flip.
            button(
                label = "Translate".to_string(),
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
