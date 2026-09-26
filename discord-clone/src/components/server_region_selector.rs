//! `server_region_selector` — a Discord-style server-region selector: a
//! vertical radio group of exactly three hardcoded local options ("US East",
//! "US West", "Europe"), one selectable at a time. Each row is a `view` whose
//! `.on_touch(|ev| …)` chain (firing on `TouchPhase::Ended`) sets the local
//! `selected` signal to that option's index; the selected row renders with a
//! highlight and a filled radio dot while the others stay dimmed and hollow.
//!
//! Next to every region label, a small hardcoded ping-time text ("12ms",
//! "45ms", "80ms") is shown in the low-latency GREEN tone, right-aligned: the
//! region label carries `flex_grow: 1` so it absorbs the free width and pushes
//! the ping to the row's right edge.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `profile_settings` and the
//! other `components/` files ship in).
//!
//! Idioms, each verified against this crate's `app.rs` / `profile_settings.rs`
//! / `components/mic_selector.rs` / `components/server_folder_group.rs` /
//! `components/latency_badge.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors;
//! - the options are authored with a `for (i, (name, ping)) in
//!   REGIONS.iter().enumerate()` inside `ui!` (a `repeat` whose per-item
//!   builder is an `Fn` closure, with the loop index as the compile-time key —
//!   the same shape `server_folder_group` / `language_picker` use, tuple
//!   destructuring verified in `app.rs:1497`);
//! - selection is REACTIVE, so every selection-dependent style is a
//!   `style = move || StyleRules { … }` closure (the `IntoStyleProp for
//!   Fn() -> StyleRules` dynamic path `app.rs` uses at `1345`/`1374`), reading
//!   `selected` at re-evaluation time rather than being baked once;
//! - the ping label does NOT read the signal, so it uses a plain
//!   `style = StyleRules { … }` literal (re-created fresh per repeat iteration —
//!   never a shared `let` binding, since `StyleRules` is not `Copy`);
//! - the radio indicator is a bordered circle `view` whose background fill is
//!   the reactive selection signal (filled = selected, hollow = not), so no
//!   text *content* is ever reactive — a `ui!` text child must be `TextContent`
//!   (a bare `if` in the content slot yields an `Element`, which it is not).

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, FontWeight, StyleRules, Tokenized,
    TouchPhase, TouchResponse,
};

// Discord dark palette (kept local so the component is self-contained).
const BG_MAIN: &str = "#313338";
const BG_SELECTED: &str = "#404249";
const TEXT: &str = "#f2f3f5";
const TEXT_MUTED: &str = "#949ba4";
/// Low-latency GREEN tone for the ping readouts — matches `latency_badge`'s
/// green "good link" color, since every hardcoded ping here is in the low
/// range.
const PING_GOOD: &str = "#23a55a";

/// The three hardcoded region options — exactly three, local only. Each entry
/// is `(region name, hardcoded ping readout)`.
const REGIONS: [(&str, &str); 3] = [
    ("US East", "12ms"),
    ("US West", "45ms"),
    ("Europe", "80ms"),
];

/// A unitless `Tokenized<f32>` border width (the `border_*_width` style fields
/// take a plain `f32` token, not a `Length`) — same shape `app.rs` uses.
fn border_width(px: f32) -> Tokenized<f32> {
    Tokenized::Literal(px)
}

/// A unitless `Tokenized<f32>` flex-grow weight (the `flex_grow` style field
/// takes a plain `f32` token, not a `Length`) — same shape `app.rs` /
/// `profile_settings.rs` use.
fn grow(v: f32) -> Tokenized<f32> {
    Tokenized::Literal(v)
}

/// A server-region picker radio group. `selected` is local reactive state (an
/// index into `REGIONS`), starting on the first option. Tapping a row sets
/// `selected` to that row's index; the matching row then renders highlighted
/// with a filled dot and the others dimmed with a hollow one. Each row also
/// shows its region's hardcoded ping readout, right-aligned. No props, no
/// backend.
#[component]
pub fn ServerRegionSelector() -> Element {
    let selected = signal(0usize);

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Stretch),
            background: Some(col(BG_MAIN)),
            padding_top: Some(px(16.0)),
            padding_bottom: Some(px(16.0)),
            ..Default::default()
        }) {
            text(style = StyleRules {
                font_size: Some(px(13.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(TEXT_MUTED)),
                margin_left: Some(px(16.0)),
                margin_right: Some(px(16.0)),
                margin_bottom: Some(px(8.0)),
                ..Default::default()
            }) { "VOICE REGION".to_string() }
            for (i, (name, ping)) in REGIONS.iter().enumerate() {
                // `i` is the loop index (compile-time repeat key); the closure
                // only *reads* `i` and `selected` (both `Copy`), so it stays
                // `Fn` and the built `StyleRules` is a fresh value each time.
                view(style = move || {
                    let sel = i == selected.get();
                    StyleRules {
                        height: Some(px(44.0)),
                        margin_left: Some(px(12.0)),
                        margin_right: Some(px(12.0)),
                        padding_left: Some(px(12.0)),
                        padding_right: Some(px(12.0)),
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        gap: Some(px(10.0)),
                        background: Some(col(if sel { BG_SELECTED } else { "transparent" })),
                        border_top_left_radius: Some(px(8.0)),
                        border_top_right_radius: Some(px(8.0)),
                        border_bottom_left_radius: Some(px(8.0)),
                        border_bottom_right_radius: Some(px(8.0)),
                        ..Default::default()
                    }
                }) {
                    // Radio indicator: a bordered circle that FILLS when this
                    // row is selected (background = accent) and stays a hollow
                    // ring otherwise (transparent background).
                    view(style = move || StyleRules {
                        width: Some(px(16.0)),
                        height: Some(px(16.0)),
                        border_top_left_radius: Some(px(8.0)),
                        border_top_right_radius: Some(px(8.0)),
                        border_bottom_left_radius: Some(px(8.0)),
                        border_bottom_right_radius: Some(px(8.0)),
                        border_top_width: Some(border_width(2.0)),
                        border_bottom_width: Some(border_width(2.0)),
                        border_left_width: Some(border_width(2.0)),
                        border_right_width: Some(border_width(2.0)),
                        border_top_color: Some(col(if i == selected.get() {
                            TEXT
                        } else {
                            TEXT_MUTED
                        })),
                        border_bottom_color: Some(col(if i == selected.get() {
                            TEXT
                        } else {
                            TEXT_MUTED
                        })),
                        border_left_color: Some(col(if i == selected.get() {
                            TEXT
                        } else {
                            TEXT_MUTED
                        })),
                        border_right_color: Some(col(if i == selected.get() {
                            TEXT
                        } else {
                            TEXT_MUTED
                        })),
                        background: Some(col(if i == selected.get() {
                            TEXT
                        } else {
                            "transparent"
                        })),
                        ..Default::default()
                    }) {}
                    // Region label. Carries `flex_grow: 1` so it absorbs the
                    // row's free width, which pushes the ping readout to the
                    // right edge. Style reads `selected` (bold/color), so it is
                    // a `move ||` closure like the other selection-dependent
                    // styles.
                    text(style = move || StyleRules {
                        flex_grow: Some(grow(1.0)),
                        font_size: Some(px(15.0)),
                        font_weight: Some(if i == selected.get() {
                            FontWeight::SemiBold
                        } else {
                            FontWeight::Normal
                        }),
                        color: Some(col(if i == selected.get() {
                            TEXT
                        } else {
                            TEXT_MUTED
                        })),
                        ..Default::default()
                    }) { name.to_string() }
                    // Ping readout: a small GREEN `{n}ms` label, statically
                    // styled (no signal read), right-aligned by the label's
                    // `flex_grow`. Inline literal because `StyleRules` is not
                    // `Copy` (re-created fresh per repeat iteration).
                    text(style = StyleRules {
                        font_size: Some(px(13.0)),
                        font_weight: Some(FontWeight::SemiBold),
                        color: Some(col(PING_GOOD)),
                        ..Default::default()
                    }) { ping.to_string() }
                }.on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        selected.set(i);
                    }
                    TouchResponse::CONSUMED
                })
            }
        }
    }
}
