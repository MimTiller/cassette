//! `mute_channel_toggle` — a self-contained channel mute switch for the
//! Discord clone. It renders a single icon button (a 36px rounded square
//! hosting a speaker glyph) bound to a local `muted` boolean signal (hardcoded
//! `false` at start, so the unmuted speaker shows first). A click flips
//! `muted`, swapping the button's icon AND style between the two states —
//! the unmuted state shows a gray `🔊` on the elevated surface, the muted
//! state shows a red `🔇` on a faint red-tinted surface.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `verified_badge` /\
//! `translate_toggle` / `soundboard_button` ship in).
//!
//! Idioms, each verified against this crate's `components/soundboard_button.rs`
//! / `components/translate_toggle.rs` / `components/message_rating.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the icon button is a `view` pill (glyph) with an `.on_touch(|ev| …)`
//!   handler guarded on `TouchPhase::Ended` — the app's standard click pattern
//!   (the runtime `button` helper takes a fixed `label` prop, which fits here
//!   too, but the `view` + glyph idiom is the one `message_rating` uses for
//!   its icon buttons and keeps the glyph a styled `text` node).
//!   `Position::Relative` gives the button a definite box for hit-testing;
//! - the toggle handler is a plain `move` closure `|| muted.set(!muted.get())`
//!   prebuilt in a `let` and passed by variable name. `Signal` is `Copy`, so
//!   the `move` closure captures a copy of the signal and stays `Fn`
//!   (callable on every press);
//! - the icon/style swap is a node-level reactive
//!   `if muted.get() { … } else { … }` inside `ui!`. An `if` whose condition
//!   reads a signal (`.get()`) lowers to the `when` primitive (mounts the
//!   chosen branch, unmounts the other on toggle) — the same reactive `if`
//!   `soundboard_button` uses for its normal/playing swap. Because
//!   `StyleRules` is not `Copy`, both branches write their styles INLINE in
//!   `style = …` (created fresh per evaluation, never captured across the
//!   `when` boundary); the `on_toggle` closure IS `Copy`, so both branches
//!   can capture it by copy.

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, JustifyContent, Position,
    StyleRules, TouchPhase, TouchResponse,
};

// Discord dark palette (kept local so the component is self-contained).
const BG_CARD: &str = "#2b2d31"; // the surrounding surface (Discord sidebar gray)
const BG_BTN: &str = "#313338"; // the icon-button surface (Discord elevated surface)
const BG_MUTED: &str = "#3f2a2c"; // faint red-tinted surface for the muted state
const ICON_ON: &str = "#b5bac1"; // unmuted speaker glyph (Discord secondary text)
const ICON_OFF: &str = "#f23f43"; // muted speaker glyph (Discord danger red)

/// The channel mute switch. Renders one 36px icon button. While the local
/// `muted` signal is `false` (the start state) the button shows a gray `🔊`
/// on the elevated surface; a click flips `muted`, swapping the button to its
/// muted look (a red `🔇` on a red-tinted surface), and another click flips
/// it back. No backend call — only the local signal toggle.
#[component]
#[allow(non_snake_case)]
pub fn MuteChannelToggle() -> Element {
    let muted = signal(false);

    // Toggle: flip `muted` on every press. `muted` is `Copy`, so the `move`
    // closure captures a copy of the same underlying signal (the one the
    // reactive `if` below also reads) and remains `Fn`.
    let on_toggle = move || muted.set(!muted.get());

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::Center),
            padding_top: Some(px(20.0)),
            padding_bottom: Some(px(20.0)),
            padding_left: Some(px(20.0)),
            padding_right: Some(px(20.0)),
            background: Some(col(BG_CARD)),
            ..Default::default()
        }) {
            // Reactive branch: `if muted.get() {…} else {…}` lowers to a `when`
            // (mounts the chosen branch, unmounts the other on toggle). The
            // styles are INLINE (created fresh per evaluation) because
            // `StyleRules` is not `Copy`; `on_toggle` IS `Copy`, so both
            // branches capture it by copy. Each branch is a complete icon
            // button so BOTH the glyph and its surface style swap on toggle.
            if muted.get() {
                view(style = StyleRules {
                    position: Some(Position::Relative),
                    width: Some(px(36.0)),
                    height: Some(px(36.0)),
                    background: Some(col(BG_MUTED)),
                    border_top_left_radius: Some(px(6.0)),
                    border_top_right_radius: Some(px(6.0)),
                    border_bottom_left_radius: Some(px(6.0)),
                    border_bottom_right_radius: Some(px(6.0)),
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    ..Default::default()
                }) {
                    text(style = StyleRules {
                        font_size: Some(px(16.0)),
                        color: Some(col(ICON_OFF)),
                        ..Default::default()
                    }) { "🔇".to_string() }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        on_toggle();
                    }
                    TouchResponse::CONSUMED
                })
            } else {
                view(style = StyleRules {
                    position: Some(Position::Relative),
                    width: Some(px(36.0)),
                    height: Some(px(36.0)),
                    background: Some(col(BG_BTN)),
                    border_top_left_radius: Some(px(6.0)),
                    border_top_right_radius: Some(px(6.0)),
                    border_bottom_left_radius: Some(px(6.0)),
                    border_bottom_right_radius: Some(px(6.0)),
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    ..Default::default()
                }) {
                    text(style = StyleRules {
                        font_size: Some(px(16.0)),
                        color: Some(col(ICON_ON)),
                        ..Default::default()
                    }) { "🔊".to_string() }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        on_toggle();
                    }
                    TouchResponse::CONSUMED
                })
            }
        }
    }
}
