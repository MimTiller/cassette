//! `message_timestamp_tooltip` — a self-contained message timestamp for the
//! Discord clone. By default it shows a short relative string
//! ("5 minutes ago"); while hovered it shows a full local timestamp
//! ("Today at 3:42 PM"). A local `hovered` boolean signal drives which text
//! is rendered, toggled by the `view` primitive's `on_hover` callback (the
//! `bool` argument is the hover state).
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `idle_timer_badge`
//! ships in).
//!
//! Idioms, each verified against this crate's `components/idle_timer_badge.rs`
//! and `components/screen_share_indicator.rs`:
//! - `StyleRules` is not `Copy` and the `ui!` expansion captures by move into
//!   an `Fn` render closure, so every style is written INLINE in `style = …`
//!   (created fresh per evaluation, never captured across a `when` boundary)
//!   — the crate's universal style idiom;
//! - the `on_hover` closure is likewise written INLINE in the `view`
//!   attribute. `Signal<bool>` is `Copy`, so the inline closure captures a
//!   copy of the same underlying signal the reactive `if` below reads, and
//!   is moved into `view` exactly once per render;
//! - the swap is a reactive `if hovered.get() { … } else { … }` inside `ui!`,
//!   each branch reading the signal so `ui!` lowers it to a `when`.

use crate::style_helpers::{col, px};
use runtime_core::{component, signal, ui, AlignItems, Element, FlexDirection, StyleRules};

// Discord dark palette (kept local so the component is self-contained).
const BG: &str = "#1e1f22";
const TEXT: &str = "#949ba4"; // muted timestamp tone

/// The timestamp tooltip. Shows the relative string at rest and the full
/// local timestamp while hovered.
#[component]
#[allow(non_snake_case)]
pub fn MessageTimestampTooltip() -> Element {
    // The "is this timestamp hovered" flag — `false` at rest.
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
            background: Some(col(BG)),
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
                }) { "Today at 3:42 PM".to_string() }
            } else {
                text(style = StyleRules {
                    font_size: Some(px(11.0)),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { "5 minutes ago".to_string() }
            }
        }
    }
}
