//! `rules_accept_checkbox` — a self-contained "accept the server rules" card
//! for the Discord clone. It renders a hardcoded local rules summary, a
//! checkbox bound to a local `accepted` boolean signal (hardcoded `false` at
//! start), and a "Continue" button that stays disabled until `accepted` is
//! `true`.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `emoji_upload_slot` /
//! `idle_timer_badge` / `profile_settings` ship in).
//!
//! Idioms, each verified against this crate's `components/emoji_upload_slot.rs`
//! / `components/idle_timer_badge.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the checkbox is a `view` + `.on_touch(|ev| …)` square with the
//!   `TouchPhase::Ended` guard — this framework has no `checkbox` primitive, so
//!   the tap-toggle square is the standard click pattern (the runtime's
//!   `button` takes a fixed `label` prop, which doesn't fit a 20px box);
//! - the checkbox's fill/border and the "✓" glyph are driven off the
//!   `accepted` signal two ways: a **reactive `style = move || …` closure**
//!   (re-evaluated whenever `accepted` changes — fills the box blurple + swaps
//!   the ring color) and a reactive **`if accepted.get() { text { "✓" } }`**
//!   inside `ui!` (lowers to a `when` that mounts the glyph only while set).
//!   Both styles are built INLINE in the closure / inline in the `if` so
//!   nothing is captured across the reactive boundary (`StyleRules` is not
//!   `Copy`);
//! - the "Continue" button uses the framework's REAL disabled state, not a
//!   visual-only mute: `disabled = move || !accepted.get()` is a `Fn() -> bool`
//!   that coerces to a dynamic `IntoValue<bool>` (see `runtime_world`), so the
//!   button is inert + natively muted while `accepted` is `false` and becomes
//!   pressable the instant the checkbox is ticked. `on_click` is omitted — the
//!   macro defaults it to a no-op `|| {}` (no backend call yet).

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, JustifyContent, Position,
    StyleRules, Tokenized, TouchPhase, TouchResponse,
};

// Discord dark palette — kept local so the component is self-contained.
const CARD_BG: &str = "#2b2d31"; // card surface (Discord sidebar gray)
const TEXT: &str = "#f2f3f5"; // primary text
const TEXT_MUTED: &str = "#949ba4"; // secondary / rules summary text
const BLURPLE: &str = "#5865f2"; // checked fill + enabled button (Discord accent)
const MUTED_BORDER: &str = "#4e5058"; // unchecked checkbox ring
const CHECK: &str = "#ffffff"; // the "✓" glyph on the blurple fill

// The hardcoded local rules summary — one line per rule, no backend fetch.
const RULES_SUMMARY: [&str; 4] = [
    "Be respectful — no harassment, hate speech, or personal attacks.",
    "Keep conversation on topic; use the right channel for each discussion.",
    "No spam, self-promotion, or unsolicited links.",
    "Follow server moderation decisions; ping a mod if you disagree.",
];

/// The rules-accept card. Renders the rules summary, a checkbox bound to the
/// local `accepted` signal (starts unchecked), and a "Continue" button that is
/// disabled until the checkbox is ticked.
#[component]
#[allow(non_snake_case)]
pub fn RulesAcceptCheckbox() -> Element {
    let accepted = signal(false);

    ui! {
        view(style = StyleRules {
            width: Some(px(320.0)),
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Stretch),
            gap: Some(px(14.0)),
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
            }) { "Server rules".to_string() }

            // The hardcoded rules summary — one `text` line per rule, built by
            // a keyed `for` over the static slice (the standard static-row
            // idiom). `for` needs a real iterator, hence `.iter()`.
            for (idx, line) in RULES_SUMMARY.iter().enumerate(), key = idx {
                text(style = StyleRules {
                    font_size: Some(px(13.0)),
                    line_height: Some(Tokenized::Literal(19.0)),
                    color: Some(col(TEXT_MUTED)),
                    ..Default::default()
                }) { line.to_string() }
            }

            // The checkbox row: a tap-toggle 20px square + a label.
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(10.0)),
                ..Default::default()
            }) {
                // The 20x20 box. `position: Relative` pins the centered glyph
                // to it. `.on_touch` flips `accepted` on `TouchPhase::Ended` —
                // the app's standard click pattern. The fill and ring are
                // driven reactively: a `move || …` style closure re-evaluates
                // whenever `accepted` changes (blurple fill + blurple ring when
                // checked, transparent + muted ring when not). Built inline so
                // nothing is captured across the reactive boundary.
                view(style = move || {
                    let on = accepted.get();
                    StyleRules {
                        width: Some(px(20.0)),
                        height: Some(px(20.0)),
                        position: Some(Position::Relative),
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        background: Some(col(if on { BLURPLE } else { "transparent" })),
                        border_top_left_radius: Some(px(5.0)),
                        border_top_right_radius: Some(px(5.0)),
                        border_bottom_left_radius: Some(px(5.0)),
                        border_bottom_right_radius: Some(px(5.0)),
                        border_top_width: Some(Tokenized::Literal(2.0)),
                        border_right_width: Some(Tokenized::Literal(2.0)),
                        border_bottom_width: Some(Tokenized::Literal(2.0)),
                        border_left_width: Some(Tokenized::Literal(2.0)),
                        border_top_color: Some(col(if on { BLURPLE } else { MUTED_BORDER })),
                        border_right_color: Some(col(if on { BLURPLE } else { MUTED_BORDER })),
                        border_bottom_color: Some(col(if on { BLURPLE } else { MUTED_BORDER })),
                        border_left_color: Some(col(if on { BLURPLE } else { MUTED_BORDER })),
                        ..Default::default()
                    }
                }) {
                    // The "✓" glyph: a reactive `if accepted.get() {…}` lowers
                    // to a `when` that mounts it only while checked.
                    if accepted.get() {
                        text(style = StyleRules {
                            font_size: Some(px(14.0)),
                            color: Some(col(CHECK)),
                            ..Default::default()
                        }) { "✓".to_string() }
                    }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        accepted.set(!accepted.get());
                    }
                    TouchResponse::CONSUMED
                })

                // The checkbox label.
                text(style = StyleRules {
                    font_size: Some(px(13.0)),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { "I have read and accept the rules".to_string() }
            }

            // The "Continue" button. `disabled = move || !accepted.get()` is a
            // reactive `Fn() -> bool` (dynamic `IntoValue<bool>`), so the
            // button is inert + natively muted until the box is ticked.
            // `on_click` is omitted (macro defaults it to a no-op) — no backend
            // call yet.
            button(
                label = "Continue".to_string(),
                disabled = move || !accepted.get(),
                style = StyleRules {
                    height: Some(px(34.0)),
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    padding_left: Some(px(14.0)),
                    padding_right: Some(px(14.0)),
                    background: Some(col(BLURPLE)),
                    border_top_left_radius: Some(px(6.0)),
                    border_top_right_radius: Some(px(6.0)),
                    border_bottom_left_radius: Some(px(6.0)),
                    border_bottom_right_radius: Some(px(6.0)),
                    ..Default::default()
                }
            )
        }
    }
}
