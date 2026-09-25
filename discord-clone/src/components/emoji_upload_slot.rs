//! `emoji_upload_slot` — a self-contained "upload an emoji" slot for the
//! Discord clone: a small square with a dashed border and a `+` glyph, which a
//! single tap turns into a colored circle standing in for an uploaded emoji.
//!
//! Bound to a local `hasEmoji` boolean signal (hardcoded `false` at start).
//! While it is `false` the slot shows the dashed placeholder + `+`; while it is
//! `true` the placeholder is replaced by a solid colored circle (a stand-in for
//! the actual uploaded-emoji image). Tapping the square toggles the signal, so
//! the slot flips between the two states.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `verified_badge` /
//! `jump_to_bottom_button` ship in).
//!
//! Idioms, each verified against this crate's `components/jump_to_bottom_button.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the placeholder-vs-circle swap is a node-level reactive
//!   `if hasEmoji.get() { … } else { … }` inside `ui!` — the `if` condition
//!   reads a signal (`.get()`), so it lowers to the `when` primitive (mounts
//!   the chosen branch, unmounts the other on toggle). `StyleRules` is not
//!   `Copy`, so every style is written INLINE in `style = …` (created fresh
//!   per evaluation, never captured across the `when` boundary);
//! - the "dashed border" is emulated with a grid of small absolute-positioned
//!   dash cells. `StyleRules` exposes per-side `border_*_width` /
//!   `border_*_color` but NO `border-style`, so there is no built-in `dashed`
//!   — the 20 dashes in [`DASHES`] below are laid out on a 6px grid around a
//!   48px square (a 4×4 of 12px cells, 6px dash + 6px gap per side, corner
//!   dots), which reads as a dashed outline at a glance;
//! - the tap is a `view` + `.on_touch(|ev| …)` chain with the
//!   `TouchPhase::Ended` guard — the app's standard click pattern (the
//!   runtime's `button` helper takes a fixed `label` prop, which doesn't fit a
//!   styled square slot).

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent,
    Position, StyleRules, Tokenized, TouchPhase, TouchResponse,
};

// Discord palette — kept local so the component is self-contained.
const BORDER: &str = "#80848e"; // muted gray (Discord muted / placeholder)
const CIRCLE: &str = "#5865f2"; // blurple stand-in for an uploaded emoji

// The dashed border as 20 absolute-positioned dash cells `(x, y, w, h)` in px,
// laid out on a 6px grid around a 48px square: a 4×4 of 12px cells where each
// side carries a 6px dash + 6px gap and the four corners are 6px dots. Static
// (computed once) so the `ui!` `for` body stays a simple property list and
// `StyleRules` can be built inline per dash. `#[allow]` because the module is
// declared but not wired into any screen yet.
#[allow(dead_code)]
const DASHES: &[(u32, u32, u32, u32)] = &[
    // top edge (y=0): corner, dash, dash, corner
    (0, 0, 6, 6), (12, 0, 6, 6), (24, 0, 6, 6), (36, 0, 6, 6),
    // bottom edge (y=42): corner, dash, dash, corner
    (0, 42, 6, 6), (12, 42, 6, 6), (24, 42, 6, 6), (36, 42, 6, 6),
    // left edge (x=0), between the corner dots
    (0, 12, 6, 6), (0, 24, 6, 6),
    // right edge (x=42), between the corner dots
    (42, 12, 6, 6), (42, 24, 6, 6),
];

/// The emoji upload slot. Renders the dashed `+` placeholder while the local
/// `hasEmoji` signal is `false` (the start state), and a colored circle while
/// it is `true`; tapping the square toggles the signal.
#[component]
#[allow(non_snake_case)]
pub fn EmojiUploadSlot() -> Element {
    let hasEmoji = signal(false);

    ui! {
        // The 48px square host. `position: Relative` anchors the absolute
        // dash cells (and nothing else) to it; `.on_touch` is the app's
        // standard click pattern, firing the toggle on `TouchPhase::Ended`.
        // The whole square is the click target so the empty gaps between the
        // dashes are tappable too.
        view(style = StyleRules {
            width: Some(px(48.0)),
            height: Some(px(48.0)),
            position: Some(Position::Relative),
            ..Default::default()
        }) {
            // The dashed border: 20 absolute-positioned 6px cells. `for`
            // over the static `DASHES` with `key = idx` (the same loop+key
            // shape the whiteboard demo uses for static swatch rows). Each
            // dash's style is built inline (created fresh, never captured —
            // `StyleRules` is not `Copy`).
            for (idx, &(x, y, w, h)) in DASHES.iter().enumerate(), key = idx {
                view(style = StyleRules {
                    position: Some(Position::Absolute),
                    left: Some(px(x as f32)),
                    top: Some(px(y as f32)),
                    width: Some(px(w as f32)),
                    height: Some(px(h as f32)),
                    background: Some(col(BORDER)),
                    border_top_left_radius: Some(px(1.0)),
                    border_top_right_radius: Some(px(1.0)),
                    border_bottom_left_radius: Some(px(1.0)),
                    border_bottom_right_radius: Some(px(1.0)),
                    ..Default::default()
                })
            }

            // The center 36×36 box (inset 6px, inside the dashed frame),
            // centering whatever state branch mounts into it.
            view(style = StyleRules {
                position: Some(Position::Absolute),
                left: Some(px(6.0)),
                top: Some(px(6.0)),
                width: Some(px(36.0)),
                height: Some(px(36.0)),
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                ..Default::default()
            }) {
                // Reactive branch: `if hasEmoji.get() {…} else {…}` lowers to
                // a `when` (mounts the chosen branch, unmounts the other on
                // toggle). Both styles are inline so nothing is captured
                // across the `when` boundary.
                if hasEmoji.get() {
                    // A 24px colored circle standing in for an uploaded
                    // emoji image (no backend call — just a solid dot).
                    view(style = StyleRules {
                        width: Some(px(24.0)),
                        height: Some(px(24.0)),
                        background: Some(col(CIRCLE)),
                        border_top_left_radius: Some(px(12.0)),
                        border_top_right_radius: Some(px(12.0)),
                        border_bottom_left_radius: Some(px(12.0)),
                        border_bottom_right_radius: Some(px(12.0)),
                        ..Default::default()
                    })
                } else {
                    // The `+` placeholder glyph, centered in the 36px box.
                    text(style = StyleRules {
                        font_size: Some(px(24.0)),
                        color: Some(col(BORDER)),
                        font_weight: Some(FontWeight::Normal),
                        line_height: Some(Tokenized::Literal(36.0)),
                        ..Default::default()
                    }) { "+".to_string() }
                }
            }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                hasEmoji.set(!hasEmoji.get());
            }
            TouchResponse::CONSUMED
        })
    }
}
