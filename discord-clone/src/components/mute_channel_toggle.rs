//! `mute_channel_toggle` — a per-channel mute switch for the Discord clone.
//! It renders a single compact icon button (a 24px rounded square hosting a
//! speaker glyph) bound to the `muted` `Signal<bool>` it is given. The
//! unmuted state shows a muted-grey `🔊` on the elevated surface; the muted
//! state shows a red `🔇` on a faint red-tinted surface. A press flips the
//! signal, swapping the glyph AND the surface style between the two states.
//!
//! It is wired into the channel sidebar as a **hover-revealed** control: the
//! mock channel rows in `app.rs` mount it behind an `if hovered.get() ||
//! muted.get()` guard, so it appears only while the row is hovered — or while
//! the channel is already muted (the persistent red glyph is the muted
//! affordance, exactly like Discord's per-channel mute). It is deliberately
//! NOT spliced into every row unconditionally (the graveyard anti-pattern):
//! the hover gate means it is a per-row OPTION, not a persistent widget.
//!
//! The `muted` state is owned by the caller (one `signal(false)` per channel
//! row in `app.rs`), so the mute choice survives a sidebar rebuild within a
//! selection — the same per-row `Signal` ownership the rows use for `hovered`.
//!
//! Idioms, each verified against this crate's `components/soundboard_button.rs`
//! / `components/translate_toggle.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the icon button is a `view` pill (glyph) with an `.on_touch(|ev| …)`
//!   handler guarded on `TouchPhase::Ended` — the app's standard click pattern.
//!   `Position::Relative` gives the button a definite box for hit-testing;
//! - the toggle handler is a plain `move` closure prebuilt in a `let` and
//!   called by the `.on_touch` handler. `Signal` is `Copy`, so the `move`
//!   closure captures a copy of the signal and stays `Fn` (callable on every
//!   press);
//! - the icon/style swap is a node-level reactive
//!   `if muted.get() { … } else { … }` inside `ui!`. An `if` whose condition
//!   reads a signal (`.get()`) lowers to the `when` primitive (mounts the
//!   chosen branch, unmounts the other on toggle) — the same reactive `if`
//!   `soundboard_button` uses for its normal/playing swap. Because
//!   `StyleRules` is not `Copy`, both branches write their styles INLINE in
//!   `style = …` (created fresh per evaluation, never captured across the
//!   `when` boundary); the handler closure IS `Copy`, so both branches
//!   capture it by copy.

use crate::style_helpers::{col, px};
use runtime_core::{
    component, ui, AlignItems, Element, FlexDirection, JustifyContent, Position, Signal,
    StyleRules, TouchPhase, TouchResponse,
};

// Discord dark palette (kept local so the component is self-contained).
const BG_BTN: &str = "#313338"; // the icon-button surface (Discord elevated surface)
const BG_MUTED: &str = "#3f2a2c"; // faint red-tinted surface for the muted state
const ICON_ON: &str = "#b5bac1"; // unmuted speaker glyph (Discord secondary text)
const ICON_OFF: &str = "#f23f43"; // muted speaker glyph (Discord danger red)

/// The per-channel mute switch. Renders one 24px icon button. While the
/// `muted` signal is `false` the button shows a muted-grey `🔊` on the
/// elevated surface; a press flips `muted`, swapping the button to its muted
/// look (a red `🔇` on a red-tinted surface), and another press flips it back.
/// The signal is owned by the caller — no backend call.
#[component]
#[allow(non_snake_case)]
pub fn MuteChannelToggle(muted: Signal<bool>) -> Element {
    // Toggle: flip `muted` on every press. `Signal` is `Copy`, so the `move`
    // handler captures a copy of the same underlying signal (the one the
    // reactive `if` below also reads) and remains `Fn` on every press.
    let on_toggle = move || muted.set(!muted.get());

    ui! {
        // Reactive branch: `if muted.get() {…} else {…}` lowers to a `when`
        // (mounts the chosen branch, unmounts the other on toggle). The
        // styles are INLINE (created fresh per evaluation) because
        // `StyleRules` is not `Copy`; `on_toggle` IS `Copy`, so both
        // branches capture it by copy. Each branch is a complete icon
        // button so BOTH the glyph and its surface style swap on toggle.
        if muted.get() {
            view(style = StyleRules {
                position: Some(Position::Relative),
                width: Some(px(24.0)),
                height: Some(px(24.0)),
                background: Some(col(BG_MUTED)),
                border_top_left_radius: Some(px(6.0)),
                border_top_right_radius: Some(px(6.0)),
                border_bottom_left_radius: Some(px(6.0)),
                border_bottom_right_radius: Some(px(6.0)),
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                flex_shrink: Some(runtime_core::Tokenized::Literal(0.0)),
                ..Default::default()
            }, test_id = "chan-mute-muted") {
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
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
                width: Some(px(24.0)),
                height: Some(px(24.0)),
                background: Some(col(BG_BTN)),
                border_top_left_radius: Some(px(6.0)),
                border_top_right_radius: Some(px(6.0)),
                border_bottom_left_radius: Some(px(6.0)),
                border_bottom_right_radius: Some(px(6.0)),
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                flex_shrink: Some(runtime_core::Tokenized::Literal(0.0)),
                ..Default::default()
            }, test_id = "chan-mute-unmuted") {
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
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
