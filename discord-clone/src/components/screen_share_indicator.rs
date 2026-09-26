//! `screen_share_indicator` — a self-contained "Screen Sharing" badge for the
//! Discord clone. Bound to a local `isSharing` boolean signal (hardcoded
//! `true` at start), it renders a small pill with a distinct background and a
//! "Screen Sharing" label while the signal is set, and nothing at all while it
//! is unset.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `verified_badge` /
//! `idle_timer_badge` / `send_failed_indicator` ship in).
//!
//! Idioms, each verified against this crate's `components/verified_badge.rs`
//! / `components/idle_timer_badge.rs` / `components/send_failed_indicator.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the badge is a reactive `if isSharing.get() { … }` as the `ui!` root.
//!   An `if` whose condition reads a signal (`.get()`) lowers to the `when`
//!   primitive (mounts while true, unmounts on false) — the same reactive
//!   `if` `verified_badge` uses for its checkmark. A top-level `if` with no
//!   wrapping node is a legal `ui!` tree (the grammar is `ui! := node*`, and a
//!   node may be an `if`); it lowers to a single `when` node, so the fragment
//!   root holds. There is no `else` branch: while unset, the badge renders
//!   nothing.
//! - the "Screen Sharing" pill is a static `view` inside that `when`, with a
//!   glyph + label as plain children (both static string literals, so they
//!   bake to `TextSource::Static` — the `verified_badge` glyph idiom);
//! - `StyleRules` is not `Copy`, so every per-node style is written INLINE in
//!   `style = …` (created fresh per evaluation, never captured across the
//!   `when` boundary) — the crate's universal style idiom.

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, StyleRules,
};

// Discord dark palette (kept local so the component is self-contained).
const BG_PILL: &str = "#3ba55d"; // the pill surface — a distinct live-share green
const TEXT: &str = "#f2f3f5"; // the "Screen Sharing" label (Discord near-white)

/// The screen-share indicator. Renders a small pill on a distinct live-share
/// green background with a "Screen Sharing" label while the local `isSharing`
/// signal is set (hardcoded `true` for now); otherwise it renders nothing.
#[component]
#[allow(non_snake_case)]
pub fn ScreenShareIndicator() -> Element {
    let isSharing = signal(true);

    ui! {
        // Reactive branch: `if isSharing.get() {…}` lowers to a `when` whose
        // render is an `Fn` closure (mounts while true, unmounts on false).
        // A top-level `if` with no wrapping node is a legal `ui!` tree (the
        // grammar is `ui! := node*`, and a node may be an `if`); it lowers to
        // a single `when` node, so the fragment root holds. `StyleRules` is
        // not `Copy`, so the pill and label styles are written INLINE in
        // `style = …` (created fresh per evaluation, never captured across the
        // `when` boundary) — the same shape `verified_badge` uses for its
        // reactive text.
        if isSharing.get() {
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(6.0)),
                padding_left: Some(px(10.0)),
                padding_right: Some(px(10.0)),
                padding_top: Some(px(5.0)),
                padding_bottom: Some(px(5.0)),
                background: Some(col(BG_PILL)),
                border_top_left_radius: Some(px(8.0)),
                border_top_right_radius: Some(px(8.0)),
                border_bottom_left_radius: Some(px(8.0)),
                border_bottom_right_radius: Some(px(8.0)),
                ..Default::default()
            }) {
                // The screen glyph — a static string literal, so it bakes to
                // `TextSource::Static` (the `verified_badge` glyph idiom).
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { "🖥".to_string() }
                // The static label. It is a plain child of the `when`'s pill,
                // so it is always mounted while the badge is.
                text(style = StyleRules {
                    font_size: Some(px(13.0)),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { "Screen Sharing".to_string() }
            }
        }
    }
}
