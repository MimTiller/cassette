#![allow(dead_code)]
//! `user_bio_block` — a self-contained "about you" bio editor for the Discord
//! clone. Renders a `text_area` bound to a local `bio` string signal
//! (hardcoded `"Tell us about yourself..."` at start; max length NOT
//! enforced) with a live character count directly beneath it showing
//! `"{n}/190"` — `n` is the current character count of the bio, `190` is the
//! display budget (visual only, not enforced).
//!
//! Everything is hardcoded local state — no backend call, no props. Wired
//! into the `member_profile_panel` overlay in `app.rs`, opened by tapping a
//! member row. Declared in `lib.rs` so it compiles as part of the
//! crate, referenced by `member_profile_panel` (the same shape `idle_timer_badge` /
//! `rules_accept_checkbox` / `profile_settings` ship in).
//!
//! Idioms, each verified against this crate's `dm_view.rs` / `app.rs` /
//! `components/idle_timer_badge.rs`:
//! - the text area is the `text_area(value, on_change)` positional constructor
//!   (a free function re-exported at the `runtime_core` root via the
//!   `glue::*` wildcard). `value` is the controlled signal (`Signal<String>`
//!   coerces to `IntoValue<String>` — `runtime_world`, line 2439); `on_change`
//!   is the native-edit upward reporter, so we push every keystroke back into
//!   the same signal with `move |s| bio.set(s)`. The result is a
//!   `GlueTextArea` (implements `IntoElement`/`ChildList` via
//!   `glue_wrapper_common!`), prebuilt into a local and dropped into `ui!` as
//!   a bare expression child — exactly the `{ messages }` pattern `dm_view`
//!   uses for its pre-built message list. `text_area` is NOT a canonical `ui!`
//!   primitive tag, so it is invoked as a Rust expression, never as a tag;
//! - the character count is a reactive `text(style = …) { move || … }` closure
//!   (the `TextContent` dynamic form — `nested-nav-demo`). A fresh `bio` clone
//!   is captured in the closure, so it re-renders the `"…/190"` string on
//!   every keystroke without the `ui!` DSL ever needing a `let` inside a node
//!   body;
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine).

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, text_area, ui, AlignItems, Element, FlexDirection, StyleRules, Tokenized,
};

// Discord dark palette — kept local so the component is self-contained.
const CARD_BG: &str = "#2b2d31"; // card surface (Discord sidebar gray)
const TEXT: &str = "#f2f3f5"; // primary text
const TEXT_MUTED: &str = "#949ba4"; // character count / hint text
const BIO_BG: &str = "#1e1f22"; // text area surface (Discord input gray)

// The bio's starting value. Max length is intentionally NOT enforced — the
// "190" below is a display budget only (it renders, it does not clamp).
const BIO_DEFAULT: &str = "Tell us about yourself...";
// The display budget shown in the character count.
const BIO_BUDGET: usize = 190;

/// The bio editor. Renders a `text_area` bound to the local `bio` signal
/// (starts at the default value) with a live `"{n}/190"` character count
/// beneath it. No backend call, no props — local state only.
#[component]
pub fn user_bio_block() -> Element {
    let bio = signal(BIO_DEFAULT.to_string());

    // Push every native edit back into the controlled `bio` signal. `bio` is
    // `Copy`, so the `move` closure captures a copy of the same underlying
    // signal (the one the character-count closure below also reads) and stays
    // `Fn` across every keystroke.
    let on_change = move |s: String| bio.set(s);

    // The controlled text area, prebuilt as a local. `text_area(value,
    // on_change)` yields a `GlueTextArea` (an `IntoElement`/`ChildList`),
    // which is then dropped into `ui!` as a bare expression child. `text_area`
    // is not a canonical `ui!` tag, so it must be a Rust expression here, not
    // a lowercase tag.
    let field = text_area(bio, on_change)
        .placeholder("Tell us about yourself...".to_string())
        .min_rows(4)
        .max_rows(8)
        .with_style(StyleRules {
            width: Some(px(320.0)),
            background: Some(col(BIO_BG)),
            border_top_left_radius: Some(px(6.0)),
            border_top_right_radius: Some(px(6.0)),
            border_bottom_left_radius: Some(px(6.0)),
            border_bottom_right_radius: Some(px(6.0)),
            border_top_width: Some(Tokenized::Literal(1.0)),
            border_right_width: Some(Tokenized::Literal(1.0)),
            border_bottom_width: Some(Tokenized::Literal(1.0)),
            border_left_width: Some(Tokenized::Literal(1.0)),
            border_top_color: Some(col("#4e5058")),
            border_right_color: Some(col("#4e5058")),
            border_bottom_color: Some(col("#4e5058")),
            border_left_color: Some(col("#4e5058")),
            ..Default::default()
        });

    // The live character count as a reactive closure bound to a local — the
    // `profile_settings` `avatar_letter` idiom. The `ui!` text node body takes
    // a single `TextContent` expression, so the closure is built here (outside
    // the DSL) and dropped in as a bare-identifier child below. A fresh `bio`
    // clone is captured so the closure owns its own signal handle and reads the
    // current count (chars, not bytes) on every evaluation.
    let char_count = {
        let bio = bio.clone();
        move || format!("{}/{}", bio.get().chars().count(), BIO_BUDGET)
    };

    ui! {
        view(style = StyleRules {
            width: Some(px(320.0)),
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Stretch),
            gap: Some(px(10.0)),
            padding_top: Some(px(18.0)),
            padding_bottom: Some(px(18.0)),
            padding_left: Some(px(20.0)),
            padding_right: Some(px(20.0)),
            background: Some(col(CARD_BG)),
            border_top_left_radius: Some(px(8.0)),
            border_top_right_radius: Some(px(8.0)),
            border_bottom_left_radius: Some(px(8.0)),
            border_bottom_right_radius: Some(px(8.0)),
            ..Default::default()
        }) {
            // Card heading.
            text(style = StyleRules {
                font_size: Some(px(16.0)),
                font_weight: Some(runtime_core::FontWeight::SemiBold),
                color: Some(col(TEXT)),
                ..Default::default()
            }) { "About you".to_string() }

            // The controlled bio text area (prebuilt `GlueTextArea` dropped in
            // as a bare expression child — the `{ messages }` pattern).
            field

            // The live character count: a reactive `text` child that
            // re-renders `"{n}/190"` on every keystroke.
            text(style = StyleRules {
                font_size: Some(px(12.0)),
                color: Some(col(TEXT_MUTED)),
                ..Default::default()
            }) { char_count }
        }
    }
}
