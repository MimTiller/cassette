//! `timeout_duration_picker` — a Discord-style timeout-duration picker: a
//! vertical radio group of exactly four hardcoded duration options ("60
//! Seconds", "5 Minutes", "1 Hour", "1 Day"), one selectable at a time.
//!
//! The chosen duration is owned by the CALLER as a `Signal<i64>` of seconds
//! (two-way prop, the `TextInput`-style unified `Signal<T>` idiom used for
//! `value`). Each row is a `view` whose `.on_touch(|ev| …)` chain (firing on
//! `TouchPhase::Ended`) writes that option's seconds into the prop; the row
//! whose seconds match the prop renders highlighted with a filled radio dot
//! while the others stay dimmed and hollow. Because the prop is reactive, a
//! row highlighted by an external write (e.g. the caller resetting the
//! duration) re-renders automatically — no local state to sync.
//!
//! Idioms, each verified against this crate's `app.rs` / `components/`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors;
//! - the options are authored with a `for (i, name) in …iter().enumerate()`
//!   inside `ui!` (a `repeat` whose per-item builder is an `Fn` closure, with
//!   the loop index as the compile-time key — the same shape
//!   `server_folder_group` uses, tuple destructuring verified in `app.rs`);
//! - selection is REACTIVE, so every selection-dependent style is a
//!   `style = move || StyleRules { … }` closure (the `IntoStyleProp for
//!   Fn() -> StyleRules` dynamic path `app.rs` uses), reading `duration` at
//!   re-evaluation time rather than being baked once;
//! - because the per-row builder is a closure, styles are built fresh per
//!   evaluation (never a shared `let` binding — `StyleRules` is not `Copy`
//!   and a captured binding would be moved on the first iteration);
//! - the radio indicator is a bordered circle `view` whose background fill is
//!   the reactive selection (filled = selected, hollow = not), so no text
//!   *content* is ever reactive — a `ui!` text child must be `TextContent`
//!   (a bare `if` in the content slot yields an `Element`, which it is not).

use crate::theme;
use crate::style_helpers::{col, px, rounded};
use runtime_core::{
    component, ui, AlignItems, Element, FlexDirection, FontWeight, Signal, StyleRules, Tokenized,
    TouchPhase, TouchResponse,
};

// Discord dark palette (kept local so the component is self-contained).

/// The four hardcoded timeout-duration labels — exactly four, in ascending
/// order. `SECS[i]` is the seconds behind `TIMEOUT_OPTIONS[i]`.
const TIMEOUT_OPTIONS: [&str; 4] = ["60 Seconds", "5 Minutes", "1 Hour", "1 Day"];
const TIMEOUT_SECS: [i64; 4] = [60, 300, 3600, 86400];

/// A unitless `Tokenized<f32>` border width (the `border_*_width` style fields
/// take a plain `f32` token, not a `Length`) — same shape `app.rs` uses.
fn border_width(px: f32) -> Tokenized<f32> {
    Tokenized::Literal(px)
}

/// A timeout-duration picker radio group. `duration` is the caller-owned
/// chosen duration in seconds (a unified two-way `Signal<i64>`). Tapping a row
/// writes that row's seconds into `duration`; the matching row renders
/// highlighted with a filled dot and the others dimmed with a hollow one.
/// No local state — the prop IS the single source of truth.
#[component]
pub fn TimeoutDurationPicker(duration: Signal<i64>) -> Element {
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Stretch),
            background: Some(col(theme::BG_PRIMARY)),
            padding_top: Some(px(16.0)),
            padding_bottom: Some(px(16.0)),
            ..Default::default()
        }) {
            text(style = StyleRules {
                font_size: Some(px(13.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(theme::TEXT_MUTED)),
                margin_left: Some(px(16.0)),
                margin_right: Some(px(16.0)),
                margin_bottom: Some(px(8.0)),
                ..Default::default()
            }) { "TIMEOUT DURATION".to_string() }
            for (i, name) in TIMEOUT_OPTIONS.iter().enumerate(), key = i {
                // `i` is the loop index (compile-time repeat key); the closure
                // only *reads* `i` and `duration` (`Copy`), so it stays `Fn`
                // and the built `StyleRules` is a fresh value each time.
                view(style = move || {
                    let sel = TIMEOUT_SECS[i] == duration.get();
                    StyleRules {
                        height: Some(px(44.0)),
                        margin_left: Some(px(12.0)),
                        margin_right: Some(px(12.0)),
                        padding_left: Some(px(12.0)),
                        padding_right: Some(px(12.0)),
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        gap: Some(px(10.0)),
                        background: Some(col(if sel { theme::BG_SELECTED } else { "transparent" })),
                        ..rounded(8.0)
                    }
                }) {
                    // Radio indicator: a bordered circle that FILLS when this
                    // row is selected (background = accent) and stays a hollow
                    // ring otherwise (transparent background).
                    view(style = move || StyleRules {
                        width: Some(px(16.0)),
                        height: Some(px(16.0)),
                        border_top_width: Some(border_width(2.0)),
                        border_bottom_width: Some(border_width(2.0)),
                        border_left_width: Some(border_width(2.0)),
                        border_right_width: Some(border_width(2.0)),
                        border_top_color: Some(col(if TIMEOUT_SECS[i] == duration.get() {
                            theme::TEXT_STRONG
                        } else {
                            theme::TEXT_MUTED
                        })),
                        border_bottom_color: Some(col(if TIMEOUT_SECS[i] == duration.get() {
                            theme::TEXT_STRONG
                        } else {
                            theme::TEXT_MUTED
                        })),
                        border_left_color: Some(col(if TIMEOUT_SECS[i] == duration.get() {
                            theme::TEXT_STRONG
                        } else {
                            theme::TEXT_MUTED
                        })),
                        border_right_color: Some(col(if TIMEOUT_SECS[i] == duration.get() {
                            theme::TEXT_STRONG
                        } else {
                            theme::TEXT_MUTED
                        })),
                        background: Some(col(if TIMEOUT_SECS[i] == duration.get() {
                            theme::TEXT_STRONG
                        } else {
                            "transparent"
                        })),
                        ..rounded(8.0)
                    }) {}
                    text(style = move || StyleRules {
                        font_size: Some(px(15.0)),
                        font_weight: Some(if TIMEOUT_SECS[i] == duration.get() {
                            FontWeight::SemiBold
                        } else {
                            FontWeight::Normal
                        }),
                        color: Some(col(if TIMEOUT_SECS[i] == duration.get() {
                            theme::TEXT_STRONG
                        } else {
                            theme::TEXT_MUTED
                        })),
                        ..Default::default()
                    }) { name.to_string() }
                }.on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        duration.set(TIMEOUT_SECS[i]);
                    }
                    TouchResponse::CONSUMED
                })
            }
        }
    }
}
