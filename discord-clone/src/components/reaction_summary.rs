#![allow(dead_code)]
//! `reaction_summary` — a self-contained message-reaction pill for the Discord
//! clone. Renders a single hardcoded local emoji (a thumbs-up, 👍) next to its
//! hardcoded local count (`5`) inside a small lozenge pill. The pill's
//! border/background style is driven by a local `reactedByMe` boolean signal
//! (hardcoded `true` for now): while set, the pill draws a blurple border + a
//! blurred-tint fill (the "you reacted to this" state); while unset, it draws
//! a muted gray border on a neutral fill (the plain "someone reacted" state).
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape
//! `connection_quality_indicator` / `verified_badge` / `idle_timer_badge` ship
//! in).
//!
//! Idioms, each verified against this crate's `components/message_rating.rs`
//! / `components/idle_timer_badge.rs` / `components/connection_quality_indicator.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the pill's two appearances are selected by a `switch` keyed on the
//!   `reactedByMe` signal — the framework's live unit of reactivity (the `ui!`
//!   tree evaluates once, so a `.set` on a signal only re-renders what sits
//!   behind a `switch` key). Because the signal is hardcoded `true`, the
//!   "reacted" arm is the one that shows, but the wiring is real: flipping the
//!   signal would swap the pill's border/background on the next rebuild;
//! - the `switch` render is an `Fn` closure (it rebuilds many times), so the
//!   per-arm `StyleRules` are written INLINE in `style = …` and the arm's
//!   colors are chosen into local `&str` bindings (which are `Copy`) at the
//!   top of the closure — nothing non-`Copy` crosses the `switch` boundary;
//! - the emoji and count are static string bodies (`.to_string()` /
//!   `COUNT.to_string()`), so they bake to static `TextSource` with no reactive
//!   machinery (the count is a fixed constant, not a signal).
//! - the pill's 1px border uses `Tokenized::Literal(1.0)` for its
//!   `border_*_width` fields (they take `Tokenized<f32>`, not `px()` — the
//!   `connection_quality_indicator` precedent).

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, switch, ui, AlignItems, Element, FlexDirection, FontWeight, StyleRules,
    Tokenized,
};

// Discord dark palette (kept local so the component is self-contained).
const BLURPLE: &str = "#5865f2"; // the reacted pill's border (Discord accent)
const BLURPLE_BG: &str = "#363b63"; // reacted pill fill (a dark blurple tint)
const NEUTRAL_BG: &str = "#313338"; // the un-reacted pill fill (Discord elevated surface)
const NEUTRAL_BORDER: &str = "#4e5058"; // un-reacted pill border (Discord muted gray)
const TEXT: &str = "#dbdee1"; // primary label / count (Discord primary text)

// The hardcoded local emoji + count — exactly these, no backend fetch.
const EMOJI: &str = "👍";
const COUNT: u32 = 5;

/// The reaction summary pill. A small lozenge holding a hardcoded emoji next
/// to its hardcoded count; the pill's border/background reflect the local
/// `reactedByMe` signal (hardcoded `true`, so the reacted arm shows now).
#[component]
#[allow(non_snake_case)]
pub fn ReactionSummary() -> Element {
    // The "I reacted" flag. Hardcoded `true` for now (no backend to ask who
    // reacted); it is a real `Signal` so a later `.set` flips the pill's style.
    let reacted_by_me = signal(true);

    // `switch` keyed on the flag: rebuilds the pill only when the value
    // changes. The render is `Fn` (many rebuilds), so each arm builds its own
    // `StyleRules` inline and picks its colors into local `Copy` `&str`s.
    switch(
        move || reacted_by_me.get(),
        move |reacted: &bool| {
            let (bg, border, text) = if *reacted {
                (BLURPLE_BG, BLURPLE, TEXT)
            } else {
                (NEUTRAL_BG, NEUTRAL_BORDER, TEXT)
            };

            ui! {
                // The pill: a fully-rounded lozenge (radius = half its fixed
                // height) holding the emoji and the count side by side.
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    gap: Some(px(5.0)),
                    padding_left: Some(px(8.0)),
                    padding_right: Some(px(8.0)),
                    padding_top: Some(px(3.0)),
                    padding_bottom: Some(px(3.0)),
                    height: Some(px(22.0)),
                    background: Some(col(bg)),
                    border_top_left_radius: Some(px(11.0)),
                    border_top_right_radius: Some(px(11.0)),
                    border_bottom_left_radius: Some(px(11.0)),
                    border_bottom_right_radius: Some(px(11.0)),
                    // 1px border in the arm's color — width is `Tokenized<f32>`
                    // (not `px()`), per the `connection_quality_indicator` idiom.
                    border_top_width: Some(Tokenized::Literal(1.0)),
                    border_right_width: Some(Tokenized::Literal(1.0)),
                    border_bottom_width: Some(Tokenized::Literal(1.0)),
                    border_left_width: Some(Tokenized::Literal(1.0)),
                    border_top_color: Some(col(border)),
                    border_right_color: Some(col(border)),
                    border_bottom_color: Some(col(border)),
                    border_left_color: Some(col(border)),
                    ..Default::default()
                }) {
                    // The hardcoded emoji — a static string body, so it bakes to
                    // a static `TextSource` (no reactive slot).
                    text(style = StyleRules {
                        font_size: Some(px(14.0)),
                        ..Default::default()
                    }) { EMOJI.to_string() }
                    // The hardcoded count — a static string body too (a constant,
                    // not a signal), in the arm's label color.
                    text(style = StyleRules {
                        font_size: Some(px(12.0)),
                        font_weight: Some(FontWeight::SemiBold),
                        color: Some(col(text)),
                        ..Default::default()
                    }) { COUNT.to_string() }
                }
            }
        },
    )
}
