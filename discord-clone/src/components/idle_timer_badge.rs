//! `idle_timer_badge` — a self-contained "idle" indicator for the Discord
//! clone. Bound to a local `idle` boolean signal (hardcoded `false` at start),
//! it renders a small pill with a `button` that toggles the signal. While
//! `idle` is set, a small moon glyph + the text "Idle for a while" appear
//! beside the button; while it is unset, that badge portion renders nothing.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `profile_settings` /
//! `server_folder_group` ship in).
//!
//! Idioms, each verified against this crate's `app.rs` / `profile_settings.rs`
//! / `components/server_folder_group.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the toggle is the `button(label, on_click)` primitive; `on_click` is a
//!   plain `move` closure prebuilt in a `let` and passed by variable name
//!   (exactly the `Send` button in `app.rs`); `Signal` is `Copy`, so the
//!   `move` closure captures a copy of the signal and stays `Fn` (callable on
//!   every press);
//! - the badge is a reactive `if idle.get() { … }` inside `ui!`. An `if`
//!   whose condition reads a signal (`.get()`) lowers to the `when`
//!   primitive (mounts while true, unmounts on false) — the same reactive
//!   `if` `server_folder_group` uses for its expanded server list.

use crate::style_helpers::{col, px};
use runtime_core::{component, signal, ui, AlignItems, Element, FlexDirection, StyleRules};

// Discord dark palette (kept local so the component is self-contained).
const BG_RAIL: &str = "#1e1f22";
const BG_PILL: &str = "#2b2d31";
const TEXT_MUTED: &str = "#949ba4";
const TEXT: &str = "#f2f3f5";

/// The idle badge. The pill always shows a `button` that toggles the local
/// `idle` signal on press; while `idle` is set, a small moon glyph + the
/// "Idle for a while" label render beside it, otherwise that portion is
/// unmounted.
#[component]
pub fn idle_timer_badge() -> Element {
    let idle = signal(false);

    // Toggle: flip the `idle` signal on every press. `idle` is `Copy`, so the
    // `move` closure captures a copy of the same underlying signal (the one
    // the reactive `if` below also reads) and remains `Fn`.
    let toggle = move || idle.set(!idle.get());

    let root_style = StyleRules {
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        padding_left: Some(px(10.0)),
        padding_right: Some(px(10.0)),
        padding_top: Some(px(6.0)),
        padding_bottom: Some(px(6.0)),
        gap: Some(px(8.0)),
        background: Some(col(BG_RAIL)),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        ..Default::default()
    };

    let btn_style = StyleRules {
        height: Some(px(24.0)),
        padding_left: Some(px(10.0)),
        padding_right: Some(px(10.0)),
        background: Some(col(BG_PILL)),
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
            button(label = "Toggle idle".to_string(), on_click = toggle, style = btn_style)
            // Reactive branch: `if idle.get() {…}` lowers to a `when` whose
            // render is an `Fn` closure. The `ui!` DSL takes no `let` inside a
            // node body, and `StyleRules` is not `Copy`, so the badge styles
            // are written INLINE in `style = …` (each is created fresh per
            // evaluation, never captured across the `when` boundary) — the
            // same shape `app.rs` uses for its inline server-pill style.
            if idle.get() {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    gap: Some(px(4.0)),
                    ..Default::default()
                }) {
                    // The small moon glyph — "small" per spec (14px, muted tone).
                    text(style = StyleRules {
                        font_size: Some(px(14.0)),
                        color: Some(col(TEXT_MUTED)),
                        ..Default::default()
                    }) { "🌙".to_string() }
                    text(style = StyleRules {
                        font_size: Some(px(13.0)),
                        color: Some(col(TEXT)),
                        ..Default::default()
                    }) { "Idle for a while".to_string() }
                }
            }
        }
    }
}
