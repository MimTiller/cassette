#![allow(dead_code)]
//! `message_search_bar` — a self-contained "search messages" bar for the
//! Discord clone. It renders a `text_input` bound to a local `query` signal
//! (placeholder `"Search"`, starts empty) above a list of mock results.
//!
//! The results list is HARDCODED — exactly two static text rows. They only
//! render while the input is non-empty (a node-level reactive
//! `if !query.get().is_empty() { … }`); clear the input and they unmount.
//! There is no backend call and no real filtering: whatever is typed, the
//! same two rows appear. No props, no wiring into any screen yet. Declared
//! in `lib.rs` so it compiles as part of the crate, but referenced by nothing
//! (the same shape `nickname_edit_field` / `ban_reason_input` /
//! `icon_change_confirm` ship in).
//!
//! Idioms, each verified against this crate:
//! - the text input is the `text_input(value = …, on_change = …,
//!   placeholder = …, style = …)` primitive — exactly the composer wiring in
//!   `dm_view.rs` and the field wiring in `nickname_edit_field`. `value` is a
//!   `Signal<String>` (cloned, since the `on_change` closure also captures a
//!   copy); `on_change` is a plain `move |s: String| query.set(s)` closure
//!   that pushes every native edit back into the same signal. `Signal` is
//!   `Copy`, so the closure stays `Fn` across edits.
//! - the results list is a node-level reactive single-branch
//!   `if !query.get().is_empty() { … }` inside `ui!` — an `if` whose condition
//!   reads a signal (`.get()`) desugars to the `when` primitive (mounts the
//!   branch while the input is non-empty, unmounts it when it empties). Its
//!   `StyleRules` are written INLINE (created fresh per evaluation) because
//!   `StyleRules` is not `Copy` and must not be captured across the `when`
//!   boundary; the input style lives OUTSIDE the `when`, so it is a plain
//!   `let` local.
//! - the two result rows are plain static `view`/`text` children written
//!   INLINE inside the reactive `if` (rules 9.3/9.4 — children live where
//!   they're rendered, conditional rendering inside the macro); no
//!   `Vec<Element>` push loop.

use crate::style_helpers::{col, px};
use icons_lucide::SEARCH;
use runtime_core::{
    component, icon, signal, ui, AlignItems, Color, Element, FlexDirection, StyleRules,
};

// Discord dark palette (kept local so the component is self-contained).
const BG_MAIN: &str = "#313338"; // screen background
const COMPOSER: &str = "#383a40"; // input surface
const TEXT: &str = "#f2f3f5"; // primary text (near-white)
const MUTED: &str = "#949ba4"; // secondary / muted text (Discord muted gray)
const RESULT_BG: &str = "#2b2d31"; // result-row surface (Discord sidebar gray)
const ACCENT: &str = "#5865f2"; // channel-name accent (Discord blurple)

/// A self-contained message-search bar. A `text_input` is bound to local
/// `query` state; while the input is non-empty, exactly two hardcoded mock
/// result rows render beneath it. No backend call, no real filtering, no
/// props, no navigation wiring yet.
#[component]
#[allow(non_snake_case)]
pub fn MessageSearchBar() -> Element {
    // Local controlled state: the search query (empty until the user types).
    let query = signal(String::new());

    // Every native edit is pushed straight back into the same signal. `query`
    // is `Copy`, so this closure captures a copy and stays `Fn` on each edit.
    let on_change = move |s: String| query.set(s);

    // The input sits OUTSIDE the reactive `when` below, so its style is a
    // plain local (created once). It carries no `border_*_width` (those fields
    // take `Tokenized<f32>`, not `px()`) — flat background + corner radius,
    // the same look as the input in `nickname_edit_field`. The `text_input`
    // primitive takes no children, so the leading `SEARCH` icon is a row
    // sibling to its left (same "separate dot view + field" pattern as the
    // rail's status dot), not a nested child. The row keeps the original
    // 320px footprint; the input shrinks to leave room for the icon.
    let input_style = StyleRules {
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        height: Some(px(40.0)),
        width: Some(px(288.0)),
        padding_left: Some(px(10.0)),
        padding_right: Some(px(12.0)),
        background: Some(col(COMPOSER)),
        color: Some(col(TEXT)),
        font_size: Some(px(15.0)),
        border_top_left_radius: Some(px(6.0)),
        border_top_right_radius: Some(px(6.0)),
        border_bottom_left_radius: Some(px(6.0)),
        border_bottom_right_radius: Some(px(6.0)),
        ..Default::default()
    };
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Stretch),
            background: Some(col(BG_MAIN)),
            padding_top: Some(px(24.0)),
            padding_bottom: Some(px(24.0)),
            padding_left: Some(px(20.0)),
            padding_right: Some(px(20.0)),
            gap: Some(px(14.0)),
            ..Default::default()
        }) {
            text(style = StyleRules {
                font_size: Some(px(15.0)),
                color: Some(col(TEXT)),
                ..Default::default()
            }) { "Search messages".to_string() }

            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(8.0)),
                ..Default::default()
            }) {
                // Leading magnifying-glass icon: fixed size + muted color via
                // the icon builder (same shape as the rail's `icon(...)`
                // call sites), left of the input field.
                icon(SEARCH).color(|| Color::from(MUTED)).size(16.0)

                text_input(
                    value = query.clone(),
                    on_change = on_change,
                    placeholder = "Search",
                    style = input_style,
                )
            }

            // Reactive single-branch `if !query.get().is_empty() { … }`
            // desugars to a `when` (mounts the results list while the input is
            // non-empty, unmounts it the moment it empties). Every `StyleRules`
            // below is INLINE (created fresh per evaluation) because
            // `StyleRules` is not `Copy` and must not be captured across the
            // `when` boundary. The two rows are hardcoded static text — no
            // backend call, no real filtering; the same two appear regardless
            // of what is typed.
            if !query.get().is_empty() {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Column),
                    align_items: Some(AlignItems::Stretch),
                    gap: Some(px(10.0)),
                    ..Default::default()
                }) {
                    // Mock result row 1.
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Column),
                        gap: Some(px(4.0)),
                        background: Some(col(RESULT_BG)),
                        padding_top: Some(px(10.0)),
                        padding_bottom: Some(px(10.0)),
                        padding_left: Some(px(12.0)),
                        padding_right: Some(px(12.0)),
                        border_top_left_radius: Some(px(6.0)),
                        border_top_right_radius: Some(px(6.0)),
                        border_bottom_left_radius: Some(px(6.0)),
                        border_bottom_right_radius: Some(px(6.0)),
                        ..Default::default()
                    }) {
                        text(style = StyleRules {
                            font_size: Some(px(13.0)),
                            color: Some(col(ACCENT)),
                            ..Default::default()
                        }) { "#general  ·  Rust & Beers".to_string() }

                        text(style = StyleRules {
                            font_size: Some(px(14.0)),
                            color: Some(col(MUTED)),
                            ..Default::default()
                        }) {
                            "anyone up for a nightcap of compiler errors tonight?"
                                .to_string()
                        }
                    }

                    // Mock result row 2.
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Column),
                        gap: Some(px(4.0)),
                        background: Some(col(RESULT_BG)),
                        padding_top: Some(px(10.0)),
                        padding_bottom: Some(px(10.0)),
                        padding_left: Some(px(12.0)),
                        padding_right: Some(px(12.0)),
                        border_top_left_radius: Some(px(6.0)),
                        border_top_right_radius: Some(px(6.0)),
                        border_bottom_left_radius: Some(px(6.0)),
                        border_bottom_right_radius: Some(px(6.0)),
                        ..Default::default()
                    }) {
                        text(style = StyleRules {
                            font_size: Some(px(13.0)),
                            color: Some(col(ACCENT)),
                            ..Default::default()
                        }) { "#showcase  ·  Pixel Forge".to_string() }

                        text(style = StyleRules {
                            font_size: Some(px(14.0)),
                            color: Some(col(MUTED)),
                            ..Default::default()
                        }) {
                            "finally got the parallax layer working — it floats now"
                                .to_string()
                        }
                    }
                }
            }
        }
    }
}
