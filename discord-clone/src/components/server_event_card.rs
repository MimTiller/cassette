//! `server_event_card` — a self-contained "server event" card for the
//! Discord clone. Shows a hardcoded local event name and time in a card, with
//! an "Interested" button that toggles a local `interested` boolean signal
//! (hardcoded `false` at start). While `interested` is unset the button reads
//! "Interested" in the Discord blurple; once set it reads "Going" in Discord
//! green — the same button node, relabeled and re-styled on every press.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `translate_toggle` /
//! `server_discovery_card` / `verified_badge` ship in).
//!
//! Idioms, each verified against this crate's `components/translate_toggle.rs`
//! / `components/server_discovery_card.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the toggle is the `button(label, on_click)` primitive; `on_click` is a
//!   plain `move` closure prebuilt in a `let` and passed by variable name
//!   (exactly the button in `translate_toggle`). `Signal` is `Copy`, so the
//!   `move` closure captures a copy of the signal and stays `Fn` (callable on
//!   every press);
//! - the relabel/restyle is a node-level reactive
//!   `if interested.get() { … } else { … }` inside `ui!`. An `if` whose
//!   condition reads a signal (`.get()`) lowers to the `when` primitive
//!   (mounts the chosen branch, swaps on toggle) — the same reactive `if`
//!   `translate_toggle` uses for its string swap. Because a `StyleRules`
//!   literal is not `Copy`, both branch styles are written INLINE in
//!   `style = …` (created fresh per evaluation, never captured across the
//!   `when` boundary) — the same shape `translate_toggle` /
//!   `server_discovery_card` use.

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent,
    StyleRules,
};

// Discord dark palette (kept local so the component is self-contained).
const BG_CARD: &str = "#2b2d31"; // card surface (Discord sidebar gray)
const TEXT: &str = "#f2f3f5"; // primary text
const TEXT_MUTED: &str = "#949ba4"; // secondary text (the time line)
const BLURPLE: &str = "#5865f2"; // "Interested" button (Discord accent)
const GREEN: &str = "#23a55a"; // "Going" button (Discord confirmed accent)

// The two hardcoded local strings — no backend call, just literals.
const EVENT_NAME: &str = "Movie Night";
const EVENT_TIME: &str = "Fri 8:00 PM";

/// The server-event card. Renders the hardcoded event name and time; while the
/// local `interested` signal is unset an "Interested" button shows in the
/// blurple (pressing it sets `interested`), and once `interested` is set the
/// button reads "Going" in green.
#[component]
pub fn ServerEventCard() -> Element {
    let interested = signal(false);

    // Toggle: flip `interested` on every press. `interested` is `Copy`, so
    // the `move` closure captures a copy of the same underlying signal (the
    // one the reactive `if` below also reads) and remains `Fn`.
    let toggle = move || interested.set(!interested.get());

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Stretch),
            gap: Some(px(8.0)),
            padding_top: Some(px(16.0)),
            padding_bottom: Some(px(16.0)),
            padding_left: Some(px(16.0)),
            padding_right: Some(px(16.0)),
            background: Some(col(BG_CARD)),
            border_top_left_radius: Some(px(8.0)),
            border_top_right_radius: Some(px(8.0)),
            border_bottom_left_radius: Some(px(8.0)),
            border_bottom_right_radius: Some(px(8.0)),
            ..Default::default()
        }) {
            text(style = StyleRules {
                font_size: Some(px(16.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(TEXT)),
                ..Default::default()
            }) { EVENT_NAME.to_string() }
            text(style = StyleRules {
                font_size: Some(px(13.0)),
                color: Some(col(TEXT_MUTED)),
                ..Default::default()
            }) { EVENT_TIME.to_string() }
            // Reactive branch: `if interested.get() {…} else {…}` lowers to a
            // `when` (mounts the chosen branch, swaps on toggle). `StyleRules`
            // is not `Copy`, so both branch styles are written INLINE in
            // `style = …` (created fresh per evaluation, never captured across
            // the `when` boundary) — the same shape `translate_toggle` /
            // `server_discovery_card` use for their reactive subtrees.
            if interested.get() {
                button(
                    label = "Going".to_string(),
                    on_click = toggle,
                    style = StyleRules {
                        height: Some(px(30.0)),
                        margin_top: Some(px(4.0)),
                        background: Some(col(GREEN)),
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        border_top_left_radius: Some(px(6.0)),
                        border_top_right_radius: Some(px(6.0)),
                        border_bottom_left_radius: Some(px(6.0)),
                        border_bottom_right_radius: Some(px(6.0)),
                        ..Default::default()
                    }
                )
            } else {
                button(
                    label = "Interested".to_string(),
                    on_click = toggle,
                    style = StyleRules {
                        height: Some(px(30.0)),
                        margin_top: Some(px(4.0)),
                        background: Some(col(BLURPLE)),
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        border_top_left_radius: Some(px(6.0)),
                        border_top_right_radius: Some(px(6.0)),
                        border_bottom_left_radius: Some(px(6.0)),
                        border_bottom_right_radius: Some(px(6.0)),
                        ..Default::default()
                    }
                )
            }
        }
    }
}
