//! `screen_share_indicator` — a self-contained "Presenting" tag for the
//! Discord clone. Bound to a local `isSharing` boolean signal (hardcoded
//! `true` at start), it renders a small, compact tag (a tiny "Presenting"
//! label in Discord green on a subtle green-tinted, bordered pill) while the
//! signal is set, and nothing at all while it is unset.
//!
//! Discord shows a *small* "Presenting" tag next to a sharing participant's
//! name — not a large filled green button. This component was the larger
//! filled-green "Screen Sharing" button; it is now the small tag-like
//! treatment, matching the compact badge scale of the sidebar's other
//! indicators (`verified_badge`, `idle_timer_badge`, `send_failed_indicator`).
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate.
//!
//! Idioms, each verified against this crate's `components/verified_badge.rs`
//! / `components/idle_timer_badge.rs` / `components/send_failed_indicator.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the tag is a reactive `if isSharing.get() { … }` as the `ui!` root. An
//!   `if` whose condition reads a signal (`.get()`) lowers to the `when`
//!   primitive (mounts while true, unmounts on false) — the same reactive
//!   `if` `verified_badge` uses for its checkmark. A top-level `if` with no
//!   wrapping node is a legal `ui!` tree (the grammar is `ui! := node*`, and a
//!   node may be an `if`); it lowers to a single `when` node, so the fragment
//!   root holds. There is no `else` branch: while unset, the tag renders
//!   nothing.
//! - the "Presenting" pill is a static `view` inside that `when`, holding a
//!   small label as a plain child (a static string literal, so it bakes to
//!   `TextSource::Static` — the `verified_badge` glyph idiom);
//! - `StyleRules` is not `Copy`, so every per-node style is written INLINE in
//!   `style = …` (created fresh per evaluation, never captured across the
//!   `when` boundary) — the crate's universal style idiom.

use crate::theme;
use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, FontWeight, StyleRules, Tokenized,
};

// `border_width` is a local helper (not in `style_helpers`) that wraps a px
// value in the `Tokenized<f32>` the `border_*_width` fields expect — the same
// one-file helper `dm_view.rs` / `mic_selector.rs` etc. each define.
fn border_width(px: f32) -> Tokenized<f32> {
    Tokenized::Literal(px)
}

// Discord dark palette (kept local so the component is self-contained).
const TAG_BG: &str = "#2a4034"; // a subtle green-tinted surface — tag, not a filled button

/// The presenting tag. A small, compact "Presenting" tag (tiny label on a
/// subtle green-tinted, bordered pill) while the local `isSharing` signal is
/// set (hardcoded `true` for now); otherwise it renders nothing. Discord
/// signals an active screen share with this small tag, not a large filled
/// button.
#[component]
pub fn ScreenShareIndicator() -> Element {
    let isSharing = signal(true);

    ui! {
        // Reactive branch: `if isSharing.get() {…}` lowers to a `when` whose
        // render is an `Fn` closure (mounts while true, unmounts on false).
        // A top-level `if` with no wrapping node is a legal `ui!` tree (the
        // grammar is `ui! := node*`, and a node may be an `if`); it lowers to
        // a single `when` node, so the fragment root holds. `StyleRules` is
        // not `Copy`, so the tag and label styles are written INLINE in
        // `style = …` (created fresh per evaluation, never captured across the
        // `when` boundary) — the same shape `verified_badge` uses for its
        // reactive text.
        if isSharing.get() {
            // The "Presenting" tag: a small, fully-rounded pill with a subtle
            // green-tinted fill, a thin green outline, and a tiny green label.
            // Deliberately compact (≈24px tall) — a tag, not a button.
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                padding_left: Some(px(8.0)),
                padding_right: Some(px(8.0)),
                padding_top: Some(px(4.0)),
                padding_bottom: Some(px(4.0)),
                background: Some(col(TAG_BG)),
                border_top_width: Some(border_width(1.0)),
                border_right_width: Some(border_width(1.0)),
                border_bottom_width: Some(border_width(1.0)),
                border_left_width: Some(border_width(1.0)),
                border_top_color: Some(col(theme::SUCCESS)),
                border_right_color: Some(col(theme::SUCCESS)),
                border_bottom_color: Some(col(theme::SUCCESS)),
                border_left_color: Some(col(theme::SUCCESS)),
                border_top_left_radius: Some(px(8.0)),
                border_top_right_radius: Some(px(8.0)),
                border_bottom_left_radius: Some(px(8.0)),
                border_bottom_right_radius: Some(px(8.0)),
                ..Default::default()
            }) {
                // The static label — a plain child of the `when`'s tag, so it
                // is always mounted while the tag is. A static string literal
                // (the `verified_badge` glyph idiom).
                text(style = StyleRules {
                    font_size: Some(px(11.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    color: Some(col(theme::SUCCESS)),
                    ..Default::default()
                }) { "Presenting".to_string() }
            }
        }
    }
}
