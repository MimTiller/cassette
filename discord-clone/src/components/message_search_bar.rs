#![allow(dead_code)]
//! `message_search_bar` — a self-contained search field for the Discord clone.
//! It renders a `text_input` bound to a local `query` signal (placeholder
//! `"Search"`, starts empty) with a leading magnifying-glass icon.
//!
//! It is wired into the **channel header row** in `app.rs` as a clean field on
//! the right side of the header — no separate label above it, matching
//! Discord's actual layout. Declared in `lib.rs` and rendered by `app.rs`'s
//! channel header `switch`.
//!
//! Idioms, each verified against this crate:
//! - the text input is the `text_input(value = …, on_change = …,
//!   placeholder = …, style = …)` primitive — exactly the composer wiring in
//!   `dm_view.rs`. `value` is a `Signal<String>` (cloned, since the
//!   `on_change` closure also captures a copy); `on_change` is a plain
//!   `move |s: String| query.set(s)` closure that pushes every native edit
//!   back into the same signal. `Signal` is `Copy`, so the closure stays
//!   `Fn` across edits.
//! - the input style is a plain local (created once). It carries no
//!   `border_*_width` (those fields take `Tokenized<f32>`, not `px()`) —
//!   flat background + corner radius, the same look as the input in
//!   `nickname_edit_field`.
//! - the leading `SEARCH` icon is a row sibling to the left of the input,
//!   not a nested child (the `text_input` primitive takes no children).

use crate::style_helpers::{col, px};
use icons_lucide::SEARCH;
use runtime_core::{
    component, icon, signal, ui, AlignItems, Color, Element, FlexDirection, StyleRules, Tokenized,
};

// Discord dark palette (kept local so the component is self-contained).
const COMPOSER: &str = "#383a40"; // input surface
const TEXT: &str = "#f2f3f5"; // primary text (near-white)
const MUTED: &str = "#949ba4"; // secondary / muted text (Discord muted gray)

/// A self-contained search field. A `text_input` is bound to local `query`
/// state; renders as a single row with a leading magnifying-glass icon and
/// the input growing to fill the remaining width. No label, no mock results,
/// no props, no navigation wiring.
#[component]
#[allow(non_snake_case)]
pub fn MessageSearchBar() -> Element {
    // Local controlled state: the search query (empty until the user types).
    let query = signal(String::new());

    // Every native edit is pushed straight back into the same signal. `query`
    // is `Copy`, so this closure captures a copy and stays `Fn` on each edit.
    let on_change = move |s: String| query.set(s);

    // The input style: a flat rounded field that flex-grows to fill the row
    // after the fixed-width icon + gap. No `border_*_width` (those take
    // `Tokenized<f32>`, not `px()`) — flat background + corner radius, same
    // look as the input in `nickname_edit_field`.
    let input_style = StyleRules {
        flex_grow: Some(Tokenized::Literal(1.0)),
        padding_left: Some(px(10.0)),
        padding_right: Some(px(12.0)),
        background: Some(col(COMPOSER)),
        color: Some(col(TEXT)),
        font_size: Some(px(14.0)),
        border_top_left_radius: Some(px(4.0)),
        border_top_right_radius: Some(px(4.0)),
        border_bottom_left_radius: Some(px(4.0)),
        border_bottom_right_radius: Some(px(4.0)),
        ..Default::default()
    };

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(6.0)),
            width: Some(px(160.0)),
            height: Some(px(28.0)),
            background: Some(col(COMPOSER)),
            border_top_left_radius: Some(px(4.0)),
            border_top_right_radius: Some(px(4.0)),
            border_bottom_left_radius: Some(px(4.0)),
            border_bottom_right_radius: Some(px(4.0)),
            padding_left: Some(px(10.0)),
            padding_right: Some(px(4.0)),
            ..Default::default()
        }) {
            // Leading magnifying-glass icon: fixed size + muted color via
            // the icon builder (same shape as the rail's `icon(...)` call
            // sites), left of the input field.
            icon(SEARCH).color(|| Color::from(MUTED)).size(14.0)

            text_input(
                value = query.clone(),
                on_change = on_change,
                placeholder = "Search",
                style = input_style,
            )
        }
    }
}
