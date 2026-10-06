//! `message_timestamp_tooltip` — the message timestamp for the Discord clone.
//! At rest it shows the message's short relative time (the `when` label the
//! message row already computes — "Today at 3:42 PM"); while hovered it shows
//! the full absolute date/time passed in as the `full` parameter (e.g.
//! "Saturday, September 12, 2026 at 3:42 PM"). A local `hovered` boolean
//! signal drives which text is rendered, toggled by the root `view`'s
//! `on_hover` callback (the `bool` argument is the hover state).
//!
//! Wired into every message row (`app.rs::message_row`) in place of the
//! row's plain "when" text node, so the timestamp is a real, per-row hover
//! target. The relative label is the same string `format_when` already hands
//! the row; the full label is derived from the row's absolute instant.
//!
//! Idioms, each verified against this crate's `components/idle_timer_badge.rs`
//! and `components/screen_share_indicator.rs`:
//! - `StyleRules` is not `Copy` and the `ui!` expansion captures by move into
//!   an `Fn` render closure, so every style is written INLINE in `style = …`
//!   (created fresh per evaluation, never captured across a `when` boundary)
//!   — the crate's universal style idiom;
//! - the `on_hover` closure is written INLINE in the `view` attribute. `Signal`
//!   is `Copy`, so the inline closure captures a copy of the same underlying
//!   signal the reactive `if` below reads, and is moved into `view` exactly
//!   once per render;
//! - the swap is a reactive `if hovered.get() { … } else { … }` inside `ui!`,
//!   each branch reading the signal so `ui!` lowers it to a `when`.
use crate::style_helpers::{col, px};
use runtime_core::{signal, ui, AlignItems, Element, FlexDirection, StyleRules};

// Discord dark palette (kept local so the component is self-contained).
const TEXT: &str = "#949ba4"; // muted timestamp tone

/// The timestamp with hover tooltip. Shows the short `when` label at rest and
/// the full `full` absolute date/time while hovered. `when` / `full` are static
/// per-row strings (the row's own `format_when` output and a derived absolute
/// string), so both bake to static `TextSource` with no reactive machinery.
pub fn message_timestamp_tooltip(when: String, full: String) -> Element {
    // The "is this timestamp hovered" flag — `false` at rest. `Signal` is
    // `Copy`, so the `on_hover` closure below captures a copy of this same
    // underlying signal as the reactive `if` does.
    let hovered = signal(false);

    ui! {
        // Root pill. `on_hover` is written inline (see file header): the
        // `bool` is the hover state, flipped straight into the local signal.
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            padding_left: Some(px(6.0)),
            padding_right: Some(px(6.0)),
            padding_top: Some(px(2.0)),
            padding_bottom: Some(px(2.0)),
            border_top_left_radius: Some(px(4.0)),
            border_top_right_radius: Some(px(4.0)),
            border_bottom_left_radius: Some(px(4.0)),
            border_bottom_right_radius: Some(px(4.0)),
            ..Default::default()
        }, on_hover = move |is_hovered: bool| hovered.set(is_hovered)) {
            // Reactive branch: hover swaps the short relative string for the
            // full local timestamp. `StyleRules` is not `Copy`, so both
            // branches carry their style inline (created fresh per
            // evaluation) — the same shape `screen_share_indicator` uses.
            if hovered.get() {
                text(style = StyleRules {
                    font_size: Some(px(11.0)),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { full.clone() }
            } else {
                text(style = StyleRules {
                    font_size: Some(px(11.0)),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { when.clone() }
            }
        }
    }
}
