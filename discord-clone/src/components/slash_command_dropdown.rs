//! `slash_command_dropdown` — a Discord-style slash-command autocomplete
//! dropdown: a hardcoded local list of exactly three fake slash commands
//! ("/shrug", "/tableflip", "/giphy"), each rendered as a clickable row.
//!
//! Tapping a row `.on_touch(|ev| …)` (firing on `TouchPhase::Ended`) sets the
//! local `selected` signal to that command's text — nothing more. There is no
//! insertion into any input, no backend call, and no wiring into any screen
//! yet. The selected row simply renders with a highlight while the others stay
//! dimmed.
//!
//! Everything is hardcoded local state — no props, no backend. Declared in
//! `lib.rs` so it compiles as part of the crate, but referenced by nothing
//! (the same shape `timeout_duration_picker` and the other `components/` files
//! ship in).
//!
//! Idioms, each verified against this crate's `components/timeout_duration_picker.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors;
//! - the rows are authored with a `for (i, cmd) in …iter().enumerate()`
//!   inside `ui!` (a `repeat` whose per-item builder is an `Fn` closure, with
//!   the loop index as the compile-time key);
//! - selection is REACTIVE, so every selection-dependent style is a
//!   `style = move || StyleRules { … }` closure, reading `selected` at
//!   re-evaluation time rather than being baked once;
//! - the per-row builder is a closure, so styles are built fresh per
//!   evaluation (never a shared `let` binding — `StyleRules` is not `Copy`);
//! - a row's *content* text is always static (`cmd.to_string()`); only its
//!   styling reads the `selected` signal, so a `ui!` text child never becomes
//!   reactive.

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, FontWeight, StyleRules, TouchPhase,
    TouchResponse,
};

// Discord dark palette (kept local so the component is self-contained).
const BG_MAIN: &str = "#2b2d31";
const BG_SELECTED: &str = "#404249";
const TEXT: &str = "#f2f3f5";
const TEXT_MUTED: &str = "#949ba4";

/// The three hardcoded slash commands — exactly three, local only.
const SLASH_COMMANDS: [&str; 3] = ["/shrug", "/tableflip", "/giphy"];

/// A slash-command autocomplete dropdown. `selected` is local reactive state
/// holding the text of the last tapped command (empty until one is tapped).
/// Tapping a row sets `selected` to that command's text; the matching row then
/// renders highlighted and the others dimmed. No props, no backend.
#[component]
pub fn SlashCommandDropdown() -> Element {
    let selected = signal(String::new());

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Stretch),
            background: Some(col(BG_MAIN)),
            padding_top: Some(px(8.0)),
            padding_bottom: Some(px(8.0)),
            ..Default::default()
        }) {
            for (i, cmd) in SLASH_COMMANDS.iter().enumerate(), key = i {
                // `i` is the loop index (compile-time repeat key); the closure
                // only *reads* `selected` (a `Copy` `Signal`), so it stays `Fn`
                // and the built `StyleRules` is a fresh value each time.
                view(style = move || {
                    let sel = selected.get() == *cmd;
                    StyleRules {
                        height: Some(px(40.0)),
                        margin_left: Some(px(8.0)),
                        margin_right: Some(px(8.0)),
                        padding_left: Some(px(12.0)),
                        padding_right: Some(px(12.0)),
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        background: Some(col(if sel { BG_SELECTED } else { "transparent" })),
                        border_top_left_radius: Some(px(8.0)),
                        border_top_right_radius: Some(px(8.0)),
                        border_bottom_left_radius: Some(px(8.0)),
                        border_bottom_right_radius: Some(px(8.0)),
                        ..Default::default()
                    }
                }) {
                    text(style = move || StyleRules {
                        font_size: Some(px(15.0)),
                        font_weight: Some(if selected.get() == *cmd {
                            FontWeight::SemiBold
                        } else {
                            FontWeight::Normal
                        }),
                        color: Some(col(if selected.get() == *cmd {
                            TEXT
                        } else {
                            TEXT_MUTED
                        })),
                        ..Default::default()
                    }) { cmd.to_string() }
                }.on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        selected.set(cmd.to_string());
                    }
                    TouchResponse::CONSUMED
                })
            }
        }
    }
}
