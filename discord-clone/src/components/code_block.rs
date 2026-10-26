//! `code_block` — a self-contained code snippet widget for the Discord clone.
//! Renders a hardcoded local code string (`"console.log('hi')"` — no props) in
//! a monospace font on a distinct dark background, with a "Copy" button that
//! sets a local `copied` boolean signal to `true` for display purposes.
//!
//! No real clipboard access — pressing the button only flips `copied`, which
//! mounts a small "Copied!" label beside it. No backend call, no wiring into
//! any screen yet. Declared in `lib.rs` so it compiles as part of the crate,
//! but referenced by nothing (the same shape `idle_timer_badge` /
//! `profile_settings` ship in).
//!
//! Idioms, each verified against this crate's `components/idle_timer_badge.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the code's monospace look is `StyleRules.font_family` set to a
//!   CSS-style stack (`From<String> for FontFamily` → `FontFamily::System`);
//! - the button is the `button(label, on_click)` primitive with a `style = …`
//!   prop lifted onto it by the `ui!` macro; `on_click` is a plain `move`
//!   closure prebuilt in a `let` and passed by variable name; `Signal` is
//!   `Copy`, so the `move` closure captures a copy of the signal and stays `Fn`
//!   (callable on every press);
//! - the "Copied!" label is a reactive `if copied.get() { … }` inside `ui!`,
//!   which lowers to a `when` primitive (mounts while true, unmounts on
//!   false). Its `StyleRules` is written INLINE because the `ui!` DSL takes no
//!   `let` inside a node body and `StyleRules` is not `Copy` (the same inline
//!   shape `idle_timer_badge` uses for its reactive badge).

use crate::style_helpers::{col, px};
use runtime_core::{component, signal, ui, AlignItems, Element, FlexDirection, FontFamily, StyleRules};

// Discord dark palette (kept local so the component is self-contained). The
// code block's background is deliberately darker than the chat surface so it
// reads as a distinct, inset snippet.
const BG_CODE: &str = "#1e1f22";
const BG_BUTTON: &str = "#404249";
const TEXT_CODE: &str = "#f2f3f5";
const TEXT_MUTED: &str = "#949ba4";
const TEXT: &str = "#f2f3f5";

/// A CSS-style monospace stack, passed verbatim to the platform's font lookup
/// (web's `font-family`). Lowered to `FontFamily::System` by `From<String>`.
const MONO: &str = "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace";

/// The code block. Always renders the hardcoded snippet in monospace on a
/// distinct background, with a "Copy" button; pressing the button sets the
/// local `copied` signal to `true`, which mounts the "Copied!" label.
#[component]
#[allow(non_snake_case)]
pub fn CodeBlock() -> Element {
    let copied = signal(false);

    // Copy: flip `copied` to true on every press. `copied` is `Copy`, so the
    // `move` closure captures a copy of the same underlying signal (the one
    // the reactive `if` below also reads) and remains `Fn`. No clipboard read
    // — display only.
    let copy = move || copied.set(true);

    let root_style = StyleRules {
        flex_direction: Some(FlexDirection::Column),
        padding_left: Some(px(12.0)),
        padding_right: Some(px(12.0)),
        padding_top: Some(px(10.0)),
        padding_bottom: Some(px(10.0)),
        gap: Some(px(8.0)),
        background: Some(col(BG_CODE)),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        ..Default::default()
    };

    let code_style = StyleRules {
        font_family: Some(MONO.to_string().into()),
        font_size: Some(px(13.0)),
        color: Some(col(TEXT_CODE)),
        ..Default::default()
    };

    let btn_style = StyleRules {
        height: Some(px(24.0)),
        padding_left: Some(px(10.0)),
        padding_right: Some(px(10.0)),
        background: Some(col(BG_BUTTON)),
        border_top_left_radius: Some(px(6.0)),
        border_top_right_radius: Some(px(6.0)),
        border_bottom_left_radius: Some(px(6.0)),
        border_bottom_right_radius: Some(px(6.0)),
        align_items: Some(AlignItems::Center),
        flex_direction: Some(FlexDirection::Row),
        ..Default::default()
    };

    ui! {
        view(style = root_style) {
            // The hardcoded snippet, monospace on the distinct dark background.
            text(style = code_style) { "console.log('hi')".to_string() }
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(8.0)),
                ..Default::default()
            }) {
                button(label = "Copy".to_string(), on_click = copy, style = btn_style)
                // Reactive branch: `if copied.get() {…}` lowers to a `when`
                // whose render is an `Fn` closure. The `ui!` DSL takes no `let`
                // inside a node body and `StyleRules` is not `Copy`, so the
                // label styles are written INLINE (created fresh per
                // evaluation, never captured across the `when` boundary) — the
                // same shape `idle_timer_badge` uses for its reactive badge.
                if copied.get() {
                    text(style = StyleRules {
                        font_family: Some(FontFamily::System(MONO.to_string())),
                        font_size: Some(px(12.0)),
                        color: Some(col(TEXT_MUTED)),
                        ..Default::default()
                    }) { "Copied!".to_string() }
                }
            }
        }
    }
}
