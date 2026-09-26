//! `category_header` — a Discord-style channel-category header row for the
//! channel sidebar: an uppercase category name ("TEXT CHANNELS") next to a
//! small chevron glyph, in one clickable row. A click toggles the local
//! `collapsed` signal, which flips the chevron between a "collapsed"
//! (`▸`, pointing right) and an "expanded" (`▾`, pointing down) direction.
//! While `collapsed` is `false` the two hardcoded fake channel names render
//! below the header as plain text rows; once it flips to `true` the list
//! unmounts.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `server_folder_group`
//! ships in — that is the closest structural sibling: a clickable header row
//! plus a conditional list of exactly two rows).
//!
//! Idioms, each verified against this crate's `components/server_folder_group.rs`
//! / `components/mute_channel_toggle.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the header row is a `view` with an `.on_touch(|ev| …)` chain toggling the
//!   `collapsed` signal on `TouchPhase::Ended` (returning
//!   `TouchResponse::CONSUMED`) — the app's standard click pattern;
//! - the chevron flip is a node-level reactive `if collapsed.get() {…} else
//!   {…}` inside `ui!`. An `if` whose condition reads a signal (`.get()`)
//!   lowers to the `when` primitive (mounts the chosen branch, unmounts the
//!   other on toggle) — the same reactive `if/else` `mute_channel_toggle`
//!   uses for its icon/style swap. Because `StyleRules` is not `Copy`, both
//!   branches write their styles INLINE in `style = …` (created fresh per
//!   evaluation, never captured across the `when` boundary);
//! - the channel list is a plain `if !collapsed.get() { … }` inside `ui!`,
//!   which lowers to the `when` primitive (mounts while false, unmounts on
//!   toggle) — the same reactive `if` `welcome_banner` / `reply_preview_bar`
//!   use for their show/gate. The `for` loop inside is lowered to a `repeat`
//!   whose per-item builder is an `Fn` closure, so the per-row styles are
//!   inlined as `StyleRules` literals rather than a shared `let` binding (a
//!   captured, non-`Copy` binding would be moved on the first iteration).

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, FontWeight, StyleRules, TouchPhase,
    TouchResponse,
};

// Discord dark palette (kept local so the component is self-contained).
const BG_SIDEBAR: &str = "#2b2d31"; // the surrounding surface (Discord sidebar gray)
const TEXT_MUTED: &str = "#949ba4"; // the uppercase category label + chevron
const TEXT_CHANNEL: &str = "#80848e"; // an inactive channel row

/// The category label — hardcoded for now, rendered uppercase by its style.
const CATEGORY_NAME: &str = "text channels";

/// The two fake channels under the category — hardcoded for now.
const CHANNELS: [&str; 2] = ["# general", "# memes"];

/// A channel-category header. The header row (chevron + category name)
/// toggles the local `collapsed` signal on click, flipping the chevron
/// between `▸` (collapsed) and `▾` (expanded); while `collapsed` is `false`
/// the two hardcoded channel names render below it, otherwise the list is
/// unmounted. No backend call — only the local signal toggle.
#[component]
pub fn category_header() -> Element {
    let collapsed = signal(false);

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Stretch),
            background: Some(col(BG_SIDEBAR)),
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
                        color: Some(col(TEXT_MUTED)),
                        ..Default::default()
                    }) { "▸".to_string() }
                } else {
                    text(style = StyleRules {
                        font_size: Some(px(11.0)),
                        color: Some(col(TEXT_MUTED)),
                        ..Default::default()
                    }) { "▾".to_string() }
                }
                text(style = StyleRules {
                    font_size: Some(px(11.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    color: Some(col(TEXT_MUTED)),
                    ..Default::default()
                }) { CATEGORY_NAME.to_string() }
            }.on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    collapsed.set(!collapsed.get());
                }
                TouchResponse::CONSUMED
            })
            // Reactive branch: the channel list shows only while `collapsed`
            // is `false` (the start state), unmounting on toggle. The `for`
            // loop lowers to a `repeat` with per-row inline `StyleRules`.
            if !collapsed.get() {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Column),
                    align_items: Some(AlignItems::Stretch),
                    ..Default::default()
                }) {
                    for name in CHANNELS.iter() {
                        view(style = StyleRules {
                            height: Some(px(32.0)),
                            margin_left: Some(px(8.0)),
                            margin_right: Some(px(8.0)),
                            padding_left: Some(px(12.0)),
                            padding_right: Some(px(8.0)),
                            flex_direction: Some(FlexDirection::Row),
                            align_items: Some(AlignItems::Center),
                            gap: Some(px(8.0)),
                            ..Default::default()
                        }) {
                            text(style = StyleRules {
                                font_size: Some(px(15.0)),
                                color: Some(col(TEXT_CHANNEL)),
                                ..Default::default()
                            }) { name.to_string() }
                        }
                    }
                }
            }
        }
    }
}
