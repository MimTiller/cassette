//! `category_header` — a Discord-style channel-category header row for the
//! channel sidebar: an uppercase category name (e.g. "TEXT CHANNELS") next to
//! a small chevron glyph, in one clickable row. A click toggles the caller-
//! owned `collapsed` signal, which flips the chevron between a "collapsed"
//! (`▸`, pointing right) and an "expanded" (`▾`, pointing down) direction.
//!
//! The header does NOT render any channel rows of its own: the real rows are
//! built by the screen that mounts this header (see the sidebar build in
//! `app.rs`), which gates them on the same `collapsed` signal. That signal is
//! passed in as a prop so it can live in the screen's scope and survive the
//! sidebar `switch` remounts — a local signal here would be wiped on every
//! server/channel/voice change, losing the collapsed state.
//!
//! Idioms, each verified against this crate's `components/setup_checklist_item.rs`
//! / `components/mute_channel_toggle.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - `title` is a plain `String` prop and `collapsed` is a `Signal<bool>` prop
//!   — the `#[component]` macro leaves an existing reactive source (`Signal`)
//!   unwrapped, so the body sees the `Signal` handle directly and calls
//!   `.get()` / `.set()` on it;
//! - the header row is a `view` with an `.on_touch(|ev| …)` chain toggling
//!   `collapsed` on `TouchPhase::Ended` (returning `TouchResponse::CONSUMED`)
//!   — the app's standard click pattern;
//! - the chevron flip is a node-level reactive `if collapsed.get() {…} else
//!   {…}` inside `ui!`. An `if` whose condition reads a signal lowers to the
//!   `when` primitive (mounts the chosen branch, unmounts the other on
//!   toggle) — the same reactive `if/else` `mute_channel_toggle` uses for its
//!   icon/style swap. Because `StyleRules` is not `Copy`, both branches write
//!   their styles INLINE in `style = …` (created fresh per evaluation, never
//!   captured across the `when` boundary).

use crate::theme;
use crate::style_helpers::{col, px};
use runtime_core::{
    component, ui, AlignItems, Element, FlexDirection, FontWeight, Signal, StyleRules, TextTransform,
    TouchPhase, TouchResponse,
};

// Discord dark palette (kept local so the component is self-contained).

/// A channel-category header. The header row (chevron + category name)
/// toggles the caller-owned `collapsed` signal on click, flipping the chevron
/// between `▸` (collapsed) and `▾` (expanded). No backend call — only the
/// signal toggle. The screen that mounts this header reads the same signal to
/// show/hide the rows beneath it.
#[component]
pub fn CategoryHeader(title: String, collapsed: Signal<bool>) -> Element {
    let title_text = title.get().to_string();
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Stretch),
            background: Some(col(theme::BG_SECONDARY)),
            ..Default::default()
        }) {
            // The clickable header row: chevron + category name. A click on
            // the whole row toggles `collapsed` on `TouchPhase::Ended`.
            view(style = StyleRules {
                height: Some(px(24.0)),
                margin_left: Some(px(8.0)),
                margin_right: Some(px(8.0)),
                padding_left: Some(px(8.0)),
                padding_right: Some(px(8.0)),
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(4.0)),
                ..Default::default()
            }) {
                // Reactive branch: the chevron flips on `collapsed`. Inline
                // styles in both branches (created fresh per evaluation)
                // because `StyleRules` is not `Copy`.
                if collapsed.get() {
                    text(style = StyleRules {
                        font_size: Some(px(11.0)),
                        color: Some(col(theme::TEXT_MUTED)),
                        ..Default::default()
                    }) { "▸".to_string() }
                } else {
                    text(style = StyleRules {
                        font_size: Some(px(11.0)),
                        color: Some(col(theme::TEXT_MUTED)),
                        ..Default::default()
                    }) { "▾".to_string() }
                }
                text(style = StyleRules {
                    font_size: Some(px(11.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    color: Some(col(theme::TEXT_MUTED)),
                    text_transform: Some(TextTransform::Uppercase),
                    ..Default::default()
                }) { title_text.clone() }
            }.on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    collapsed.set(!collapsed.get());
                }
                TouchResponse::CONSUMED
            })
        }
    }
}
