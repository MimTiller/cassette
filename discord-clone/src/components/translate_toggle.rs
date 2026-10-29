//! `translate_toggle` — a translate switch for the Discord clone. It renders
//! one line of text that flips between an `original` and a `translated`
//! string, driven by a local `showTranslated` boolean signal (hardcoded
//! `false` at start, so the original shows first). A `button` toggles the
//! signal on every press, swapping which string the line displays.
//!
//! Both strings are `String` props (hardcoded defaults `"Bonjour"` /
//! `"Hello"`, so the component also compiles standalone). It is wired into
//! the mock channel message list as a per-message "Translate message"
//! overflow-menu option (`app.rs` `message_row_with_actions`): the toggle
//! card only mounts for the one row whose menu item was tapped (the shared
//! `translate_key` signal names the active row) — a deliberate single
//! instance, not a per-row widget.
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

use crate::theme;
use crate::style_helpers::{col, px, rounded};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, JustifyContent, StyleRules,
};

// Discord dark palette (kept local so the component is self-contained).

/// The translate switch. Renders a single line of text that shows the
/// `original` string while the local `showTranslated` signal is `false` (the
/// start state) and the `translated` string while it is `true`; a `button`
/// toggles the signal on every press, swapping the displayed line. Both
/// strings arrive as `String` props (with hardcoded defaults — the original
/// `"Bonjour"` / `"Hello"` — so the component still compiles standalone); no
/// backend call, just the two literals the signal switches between.
#[component]
pub fn TranslateToggle(
    #[prop(default = "Bonjour")] original: String,
    #[prop(default = "Hello")] translated: String,
) -> Element {
    // Both lines are static props (never `.set`), so read them once at build
    // into owned `String`s and bake them as static text bodies. Cloned
    // (borrowed) at each use so the non-`Copy` locals are never moved out of
    // the reactive branch's `Fn` render closure — the `ChannelTopicBanner`
    // idiom.
    let orig = original.get();
    let trans = translated.get();

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
            background: Some(col(theme::BG_SECONDARY)),
            ..rounded(8.0)
        }) {
            // Reactive branch: `if showTranslated.get() {…} else {…}` lowers to
            // a `when` (mounts the chosen branch, unmounts the other on toggle).
            // The style is inline so nothing is captured across the `when`
            // boundary (`StyleRules` is not `Copy`).
            if showTranslated.get() {
                text(style = StyleRules {
                    font_size: Some(px(18.0)),
                    color: Some(col(theme::TEXT_STRONG)),
                    ..Default::default()
                }) { trans.clone() }
            } else {
                text(style = StyleRules {
                    font_size: Some(px(18.0)),
                    color: Some(col(theme::TEXT_STRONG)),
                    ..Default::default()
                }) { orig.clone() }
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
                    background: Some(col(theme::ACCENT)),
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    ..rounded(6.0)
                }
            )
        }
    }
}
