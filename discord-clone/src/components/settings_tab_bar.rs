//! `settings_tab_bar` — a self-contained Discord-style settings tab bar.
//! Renders exactly four hardcoded tab labels ("Overview", "Roles", "Members",
//! "Bans") as a horizontal row of tab pills. The bar is bound to a single local
//! "active tab" index signal: tapping a tab sets the signal to that tab's
//! index, and the matching tab is the one rendered highlighted (blurple fill +
//! white bold label) while the others stay muted and transparent.
//!
//! No panel content per tab — just the tab bar itself. Everything is hardcoded
//! local state — no backend call, no props, no wiring into any screen yet.
//! Declared in `lib.rs` so it compiles as part of the crate, but referenced by
//! nothing (the same shape `language_picker` / `category_filter_chips` ship in).
//!
//! Idioms, each verified against this crate's `components/language_picker.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors;
//! - the four tabs are emitted by a keyed `for (i, label) in …iter().enumerate()`
//!   over the static label slice (the loop index is the compile-time repeat key);
//! - the active-tab selection is REACTIVE, so every selection-dependent style is
//!   a `style = move || StyleRules { … }` closure reading `active` at
//!   re-evaluation time (built inline each pass — `StyleRules` is not `Copy`);
//! - the tap-to-select is `view` + `.on_touch(|ev| …)` guarded on
//!   `TouchPhase::Ended`, setting `active.set(i)` — the app's standard click.

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, FontWeight, StyleRules, TouchPhase,
    TouchResponse,
};

// Discord dark palette (kept local so the component is self-contained).
const BG_BAR: &str = "#313338"; // tab bar background (Discord main surface)
const BG_TAB_ACTIVE: &str = "#5865f2"; // active tab fill (Discord blurple)
const TEXT_ACTIVE: &str = "#ffffff"; // white bold label on the active tab
const TEXT_MUTED: &str = "#949ba4"; // inactive tab label (muted)

/// The four hardcoded settings tabs — exactly these, in this order, local only.
const SETTINGS_TABS: [&str; 4] = ["Overview", "Roles", "Members", "Bans"];

/// The settings tab bar. `active` is local reactive state (an index into
/// `SETTINGS_TABS`), starting on the first tab ("Overview"). Tapping a tab sets
/// `active` to that tab's index; the matching tab then renders highlighted
/// (blurple fill + white bold label) while the others stay muted and
/// transparent. No props, no backend, no panel content.
#[component]
pub fn SettingsTabBar() -> Element {
    let active = signal(0usize);

    ui! {
        // Horizontal tab bar: a rounded Discord-surface strip holding the four
        // tab pills in a row, vertically centered.
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(4.0)),
            background: Some(col(BG_BAR)),
            padding_top: Some(px(6.0)),
            padding_bottom: Some(px(6.0)),
            padding_left: Some(px(6.0)),
            padding_right: Some(px(6.0)),
            border_top_left_radius: Some(px(8.0)),
            border_top_right_radius: Some(px(8.0)),
            border_bottom_left_radius: Some(px(8.0)),
            border_bottom_right_radius: Some(px(8.0)),
            ..Default::default()
        }) {
            // One tab per hardcoded label. `i` is the loop index (and the repeat
            // key); the per-tab `move` closures read `i` and `active` (both
            // `Copy`) so they stay `Fn`, rebuilding a fresh `StyleRules` each
            // evaluation.
            for (i, label) in SETTINGS_TABS.iter().enumerate() {
                // The tab pill. The reactive `style = move || …` closure reads
                // `active` and rebuilds the `StyleRules` on every selection
                // change: active → blurple fill; inactive → transparent.
                view(style = move || {
                    let on = i == active.get();
                    StyleRules {
                        height: Some(px(32.0)),
                        padding_left: Some(px(16.0)),
                        padding_right: Some(px(16.0)),
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        background: Some(col(if on {
                            BG_TAB_ACTIVE
                        } else {
                            "transparent"
                        })),
                        border_top_left_radius: Some(px(6.0)),
                        border_top_right_radius: Some(px(6.0)),
                        border_bottom_left_radius: Some(px(6.0)),
                        border_bottom_right_radius: Some(px(6.0)),
                        ..Default::default()
                    }
                }) {
                    // The tab label — its color and weight flip with selection.
                    text(style = move || {
                        let on = i == active.get();
                        StyleRules {
                            font_size: Some(px(14.0)),
                            font_weight: Some(if on {
                                FontWeight::SemiBold
                            } else {
                                FontWeight::Normal
                            }),
                            color: Some(col(if on { TEXT_ACTIVE } else { TEXT_MUTED })),
                            ..Default::default()
                        }
                    }) { label.to_string() }
                }
                // Tap-to-select: set the active tab to THIS tab's index on
                // `TouchPhase::Ended`. `active` and `i` are `Copy`, so this
                // `move` closure captures its own copies and stays `Fn`.
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        active.set(i);
                    }
                    TouchResponse::CONSUMED
                })
            }
        }
    }
}
