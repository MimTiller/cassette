//! `message_rating` — a self-contained up/down vote control for the Discord
//! clone. It renders an upvote icon button and a downvote icon button side by
//! side, each carrying its OWN local count signal (hardcoded `0` at start)
//! and a reactive count label beside its icon. Clicking either button
//! increments that button's count by one; the two buttons are fully
//! independent — there is no mutual exclusion, so a message can be both
//! upvoted and downvoted as many times as it is clicked.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `verified_badge` /
//! `jump_to_bottom_button` / `idle_timer_badge` ship in).
//!
//! Idioms, each verified against this crate's `components/jump_to_bottom_button.rs`
//! / `components/idle_timer_badge.rs` / `components/verified_badge.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the count labels are reactive f-string `text { "{ident}" }` nodes where
//!   the interpolated identifier is a `Signal`. Such a slot becomes a LIVE
//!   text binding automatically (the slot subscribes to the signal and
//!   re-renders on every `.set`) — the framework's documented counter
//!   idiom. The slot carries a `compute_fallback` closure, so the same source
//!   is correct on every backend (web rides the fast path; native backends
//!   fall back to the regular Effect path) with no author opt-in;
//! - the icon buttons are `view` + `.on_touch(|ev| …)` chains with the
//!   `TouchPhase::Ended` guard — the app's standard click pattern (the
//!   runtime's `button` helper takes a fixed `label` prop, which doesn't fit
//!   an icon-plus-count pill);
//! - each `on_touch` closure is a `move` closure capturing a `Copy` of its
//!   signal, so it stays `Fn` (callable on every press) and the two buttons'
//!   handlers touch only their own signal — independence by construction.

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, Position, StyleRules,
    TouchPhase, TouchResponse,
};

// Discord dark palette (kept local so the component is self-contained).
const BG_BAR: &str = "#2b2d31"; // the rating bar surface (Discord sidebar gray)
const BG_BTN: &str = "#313338"; // the icon-button pill (Discord elevated surface)
const ICON: &str = "#b5bac1"; // muted up/down arrow glyphs (Discord secondary text)
const TEXT: &str = "#dbdee1"; // the count numerals (Discord primary text)

/// The up/down vote control. Renders an upvote button and a downvote button
/// side by side; each button has its own local `Signal<i32>` count (starting
/// at `0`) and a reactive label showing that count. Clicking a button
/// increments only its own count — the two buttons are independent (no
/// mutual exclusion), so both counters can climb on their own.
#[component]
#[allow(non_snake_case)]
pub fn MessageRating() -> Element {
    // Two INDEPENDENT count signals. Each is `Copy`, so the `on_touch` closure
    // below captures a copy of the same underlying signal (the one the reactive
    // text slot also reads) and remains `Fn`. Because each closure touches only
    // its own signal, the two counters never interfere — that is the whole
    // "no mutual exclusion" requirement, satisfied by construction.
    let up_votes = signal(0);
    let down_votes = signal(0);

    // The upvote handler: increment ONLY the up-vote count.
    let on_upvote = move || up_votes.set(up_votes.get() + 1);
    // The downvote handler: increment ONLY the down-vote count.
    let on_downvote = move || down_votes.set(down_votes.get() + 1);

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(6.0)),
            padding_left: Some(px(8.0)),
            padding_right: Some(px(8.0)),
            padding_top: Some(px(4.0)),
            padding_bottom: Some(px(4.0)),
            background: Some(col(BG_BAR)),
            border_top_left_radius: Some(px(8.0)),
            border_top_right_radius: Some(px(8.0)),
            border_bottom_left_radius: Some(px(8.0)),
            border_bottom_right_radius: Some(px(8.0)),
            ..Default::default()
        }) {
            // The upvote icon button: a `view` pill (arrow glyph + count) with a
            // touch handler — the app's standard click pattern (the runtime
            // `button` takes a fixed `label` prop, which doesn't fit an icon).
            // `Position::Relative` gives it a definite box for hit-testing.
            view(style = StyleRules {
                position: Some(Position::Relative),
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(4.0)),
                padding_left: Some(px(10.0)),
                padding_right: Some(px(10.0)),
                padding_top: Some(px(4.0)),
                padding_bottom: Some(px(4.0)),
                background: Some(col(BG_BTN)),
                border_top_left_radius: Some(px(6.0)),
                border_top_right_radius: Some(px(6.0)),
                border_bottom_left_radius: Some(px(6.0)),
                border_bottom_right_radius: Some(px(6.0)),
                ..Default::default()
            }) {
                // The up arrow glyph — a static string literal, so it bakes to
                // `TextSource::Static` (no reactive machinery).
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    color: Some(col(ICON)),
                    ..Default::default()
                }) { "↑".to_string() }
                // The up-vote COUNT: a reactive f-string slot. `up_votes` is a
                // `Signal`, so this slot is LIVE — it subscribes to the signal
                // and re-renders on every `.set` (the framework's documented
                // counter idiom), with a `compute_fallback` closure keeping it
                // correct on every backend.
                text(style = StyleRules {
                    font_size: Some(px(12.0)),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { "{up_votes}" }
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    on_upvote();
                }
                TouchResponse::CONSUMED
            })

            // The downvote icon button — identical shape, its OWN signal and
            // handler. Clicking it never touches the up-vote count.
            view(style = StyleRules {
                position: Some(Position::Relative),
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(4.0)),
                padding_left: Some(px(10.0)),
                padding_right: Some(px(10.0)),
                padding_top: Some(px(4.0)),
                padding_bottom: Some(px(4.0)),
                background: Some(col(BG_BTN)),
                border_top_left_radius: Some(px(6.0)),
                border_top_right_radius: Some(px(6.0)),
                border_bottom_left_radius: Some(px(6.0)),
                border_bottom_right_radius: Some(px(6.0)),
                ..Default::default()
            }) {
                // The down arrow glyph — a static string literal.
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    color: Some(col(ICON)),
                    ..Default::default()
                }) { "↓".to_string() }
                // The down-vote COUNT: a reactive f-string slot on the second,
                // independent signal.
                text(style = StyleRules {
                    font_size: Some(px(12.0)),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { "{down_votes}" }
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    on_downvote();
                }
                TouchResponse::CONSUMED
            })
        }
    }
}
