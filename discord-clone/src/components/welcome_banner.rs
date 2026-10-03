//! `welcome_banner` — a self-contained "welcome" banner for the Discord
//! clone. It greets a hardcoded local username ("Sam") with "Welcome, Sam!
//! Say hello!" plus a small wave emoji, and carries a dismiss "x" button that
//! flips a local `dismissed` boolean signal. While `dismissed` is `true` the
//! component renders nothing.
//!
//! Everything is hardcoded local state — no backend call, no props. Wired
//! into the `member_profile_panel` overlay in `app.rs`, opened by tapping a
//! member row. Declared in `lib.rs` so it compiles as part of the
//! crate, referenced by `member_profile_panel` (the same shape `rules_accept_checkbox` /
//! `jump_to_bottom_button` ship in).
//!
//! Idioms, each verified against this crate's `components/rules_accept_checkbox.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the banner-vs-nothing swap is a node-level reactive
//!   `if !dismissed.get() { … }` inside `ui!` — the `if` condition reads a
//!   signal (`.get()`), so it lowers to the `when` primitive (mounts the
//!   banner while not dismissed, unmounts it on dismiss). `StyleRules` is not
//!   `Copy`, so every style is written INLINE (created fresh per evaluation,
//!   never captured across the `when` boundary);
//! - the dismiss "x" is a `view` + `.on_touch(|ev| …)` box with the
//!   `TouchPhase::Ended` guard — the app's standard click pattern (the
//!   runtime's `button` helper takes a fixed `label` prop, which doesn't fit
//!   a small glyph tap target).

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, JustifyContent, Position,
    StyleRules, TouchPhase, TouchResponse,
};

// Discord dark palette — kept local so the component is self-contained.
const BANNER_BG: &str = "#313338"; // banner surface (Discord sidebar gray)
const TEXT: &str = "#f2f3f5"; // primary text
const MUTED_BORDER: &str = "#4e5058"; // dismiss-button ring
const TEXT_MUTED: &str = "#949ba4"; // dismiss "x" glyph

/// The welcome banner. Greets the member's name (`name` prop) with a wave emoji
/// and a dismiss "x"; renders nothing once the local `dismissed` signal is set
/// (starts `false`).
#[component]
#[allow(non_snake_case)]
pub fn WelcomeBanner(#[prop(default = "Sam")] name: String) -> Element {
    let dismissed = signal(false);
    // The name is a prop (never `.set`), so read it once into an owned String
    // and bake it into the greeting text — the same shape `channel_topic_banner`
    // uses for its `topic` prop.
    let greeting = format!("Welcome, {}! Say hello! 👋", name.get());

    ui! {
        // Reactive branch: `if !dismissed.get() {…}` lowers to a `when` that
        // mounts the banner while not dismissed and unmounts it the moment
        // the "x" is tapped. A top-level `if` with no wrapping node is a
        // legal `ui!` tree (the grammar is `ui! := node*`), so the fragment
        // root holds — same shape `edited_label` uses for its marker.
        if !dismissed.get() {
            view(style = StyleRules {
                width: Some(px(320.0)),
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(10.0)),
                padding_top: Some(px(14.0)),
                padding_bottom: Some(px(14.0)),
                padding_left: Some(px(16.0)),
                padding_right: Some(px(16.0)),
                background: Some(col(BANNER_BG)),
                border_top_left_radius: Some(px(8.0)),
                border_top_right_radius: Some(px(8.0)),
                border_bottom_left_radius: Some(px(8.0)),
                border_bottom_right_radius: Some(px(8.0)),
                ..Default::default()
            }) {
                // The greeting: member name + wave emoji (from the `name` prop).
                text(style = StyleRules {
                    flex_grow: Some(runtime_core::Tokenized::Literal(1.0)),
                    font_size: Some(px(15.0)),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { greeting.clone() }

                // The dismiss "x": a 24x24 tap target. `.on_touch` sets
                // `dismissed` on `TouchPhase::Ended` — the app's standard
                // click pattern. Style built inline (nothing captured across
                // the `when` boundary).
                view(style = StyleRules {
                    width: Some(px(24.0)),
                    height: Some(px(24.0)),
                    position: Some(Position::Relative),
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    border_top_left_radius: Some(px(6.0)),
                    border_top_right_radius: Some(px(6.0)),
                    border_bottom_left_radius: Some(px(6.0)),
                    border_bottom_right_radius: Some(px(6.0)),
                    border_top_width: Some(runtime_core::Tokenized::Literal(1.0)),
                    border_right_width: Some(runtime_core::Tokenized::Literal(1.0)),
                    border_bottom_width: Some(runtime_core::Tokenized::Literal(1.0)),
                    border_left_width: Some(runtime_core::Tokenized::Literal(1.0)),
                    border_top_color: Some(col(MUTED_BORDER)),
                    border_right_color: Some(col(MUTED_BORDER)),
                    border_bottom_color: Some(col(MUTED_BORDER)),
                    border_left_color: Some(col(MUTED_BORDER)),
                    ..Default::default()
                }) {
                    text(style = StyleRules {
                        font_size: Some(px(14.0)),
                        color: Some(col(TEXT_MUTED)),
                        ..Default::default()
                    }) { "x".to_string() }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        dismissed.set(true);
                    }
                    TouchResponse::CONSUMED
                })
            }
        }
    }
}
