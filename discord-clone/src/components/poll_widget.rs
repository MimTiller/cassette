#![allow(dead_code)]
//! `poll_widget` — a self-contained poll control for the Discord clone. It
//! renders a hardcoded local question header above exactly two hardcoded
//! options. Each option carries its OWN local vote-count `Signal<i32>`
//! (starting at `0`) shown as a live count label, next to a `"Vote"` button.
//! Clicking an option's `"Vote"` button increments ONLY that option's count —
//! the two options are fully independent (no mutual exclusion, no "already
//! voted" tracking), so either or both counters can climb on their own.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `message_rating` /
//! `reaction_summary` / `soundboard_button` ship in).
//!
//! Idioms, each verified against this crate's `components/message_rating.rs`
//! / `components/soundboard_button.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - each vote count is a reactive f-string `text { "{ident}" }` node whose
//!   interpolated identifier is a `Signal`. Such a slot becomes a LIVE text
//!   binding automatically (it subscribes to the signal and re-renders on
//!   every `.set`) — the framework's documented counter idiom
//!   (`message_rating`). The slot carries a `compute_fallback` closure, so the
//!   same source is correct on every backend (web rides the fast path; native
//!   backends fall back to the regular Effect path) with no author opt-in;
//! - each `"Vote"` button is the `button(label, on_click, style)` primitive
//!   with a plain `move` closure prebuilt in a `let` and passed by variable
//!   name (exactly the `button` in `soundboard_button`). `Signal` is `Copy`,
//!   so the `move` closure captures a copy of its own option's signal and
//!   stays `Fn` (callable on every press); because each closure touches only
//!   its own signal, the two counters never interfere — independence by
//!   construction.

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent,
    StyleRules,
};

// The hardcoded local poll — no props, no backend.
const QUESTION: &str = "Which theme do you prefer?";
const OPTION_A: &str = "Dark mode";
const OPTION_B: &str = "Light mode";

// Discord dark palette (kept local so the component is self-contained).
const BG_CARD: &str = "#2b2d31"; // the poll card surface (Discord sidebar gray)
const BG_ROW: &str = "#313338"; // an option row fill (Discord elevated surface)
const BUTTON: &str = "#5865f2"; // the "Vote" button (Discord blurple accent)
const TEXT: &str = "#dbdee1"; // primary labels / count numerals (Discord primary text)

/// A poll widget. Renders a hardcoded question header above exactly two
/// hardcoded options; each option has its own local `Signal<i32>` vote count
/// (starting at `0`) shown as a live label, next to a `"Vote"` button that
/// increments only that option's count on click. The two options are
/// independent — there is no mutual exclusion, so either or both counters can
/// climb on their own.
#[component]
#[allow(non_snake_case)]
pub fn PollWidget() -> Element {
    // Two INDEPENDENT vote-count signals, both starting at `0`. Each is `Copy`,
    // so the `on_click` closure captures a copy of the same underlying signal
    // (the one the reactive count slot also reads) and remains `Fn`. Because
    // each closure touches only its own signal, the two counters never
    // interfere — that is the whole "independent options" requirement.
    let vote_a = signal(0);
    let vote_b = signal(0);

    // Each "Vote" handler increments ONLY its own option's count.
    let on_vote_a = move || vote_a.set(vote_a.get() + 1);
    let on_vote_b = move || vote_b.set(vote_b.get() + 1);

    ui! {
        // The poll card: a dark surface holding the question and two rows.
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            gap: Some(px(10.0)),
            padding_left: Some(px(12.0)),
            padding_right: Some(px(12.0)),
            padding_top: Some(px(12.0)),
            padding_bottom: Some(px(12.0)),
            background: Some(col(BG_CARD)),
            border_top_left_radius: Some(px(10.0)),
            border_top_right_radius: Some(px(10.0)),
            border_bottom_left_radius: Some(px(10.0)),
            border_bottom_right_radius: Some(px(10.0)),
            ..Default::default()
        }) {
            // The hardcoded question header — a static string body, so it bakes
            // to a static `TextSource` (no reactive machinery).
            text(style = StyleRules {
                font_size: Some(px(15.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(TEXT)),
                ..Default::default()
            }) { QUESTION.to_string() }

            // Option A row: label + live count + a "Vote" button that
            // increments ONLY `vote_a`.
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(8.0)),
                padding_left: Some(px(8.0)),
                padding_right: Some(px(8.0)),
                padding_top: Some(px(8.0)),
                padding_bottom: Some(px(8.0)),
                background: Some(col(BG_ROW)),
                border_top_left_radius: Some(px(6.0)),
                border_top_right_radius: Some(px(6.0)),
                border_bottom_left_radius: Some(px(6.0)),
                border_bottom_right_radius: Some(px(6.0)),
                ..Default::default()
            }) {
                // The hardcoded option label — a static string body.
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { OPTION_A.to_string() }
                // The LIVE vote count for option A — a reactive f-string slot
                // whose interpolated identifier is the `vote_a` signal, so it
                // re-renders on every `.set` (the documented counter idiom).
                text(style = StyleRules {
                    font_size: Some(px(13.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { "{vote_a}" }
                // The "Vote" button for option A — the `button` primitive with a
                // prebuilt `move` closure that bumps only `vote_a`.
                button(
                    label = "Vote".to_string(),
                    on_click = on_vote_a,
                    style = StyleRules {
                        height: Some(px(26.0)),
                        padding_left: Some(px(12.0)),
                        padding_right: Some(px(12.0)),
                        background: Some(col(BUTTON)),
                        border_top_left_radius: Some(px(5.0)),
                        border_top_right_radius: Some(px(5.0)),
                        border_bottom_left_radius: Some(px(5.0)),
                        border_bottom_right_radius: Some(px(5.0)),
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        ..Default::default()
                    }
                )
            }

            // Option B row — identical shape, its OWN signal and handler.
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(8.0)),
                padding_left: Some(px(8.0)),
                padding_right: Some(px(8.0)),
                padding_top: Some(px(8.0)),
                padding_bottom: Some(px(8.0)),
                background: Some(col(BG_ROW)),
                border_top_left_radius: Some(px(6.0)),
                border_top_right_radius: Some(px(6.0)),
                border_bottom_left_radius: Some(px(6.0)),
                border_bottom_right_radius: Some(px(6.0)),
                ..Default::default()
            }) {
                // The hardcoded option label — a static string body.
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { OPTION_B.to_string() }
                // The LIVE vote count for option B — its own reactive slot.
                text(style = StyleRules {
                    font_size: Some(px(13.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { "{vote_b}" }
                // The "Vote" button for option B — its own closure bumping only
                // `vote_b`; clicking it never touches `vote_a`.
                button(
                    label = "Vote".to_string(),
                    on_click = on_vote_b,
                    style = StyleRules {
                        height: Some(px(26.0)),
                        padding_left: Some(px(12.0)),
                        padding_right: Some(px(12.0)),
                        background: Some(col(BUTTON)),
                        border_top_left_radius: Some(px(5.0)),
                        border_top_right_radius: Some(px(5.0)),
                        border_bottom_left_radius: Some(px(5.0)),
                        border_bottom_right_radius: Some(px(5.0)),
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        ..Default::default()
                    }
                )
            }
        }
    }
}
