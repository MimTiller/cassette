//! `server_icon_upload_placeholder` — a self-contained "upload a server icon"
//! placeholder for the Discord clone: a square with a dashed border and a
//! "Click to upload" label, which a single click turns into a solid colored
//! circle standing in for an uploaded server icon.
//!
//! Bound to a local `hasImage` boolean signal (hardcoded `false` at start).
//! While it is `false` the area shows the dashed frame + "Click to upload";
//! while it is `true` the placeholder is replaced by a solid colored circle
//! (a stand-in for the actual uploaded-image thumbnail). Clicking the square
//! toggles the signal, so the area flips between the two states.
//!
//! Everything is hardcoded local state — no backend call, no props, no real
//! file picker, no wiring into any screen yet. Declared in `lib.rs` so it
//! compiles as part of the crate, but referenced by nothing (the same shape
//! `emoji_upload_slot` / `verified_badge` / `jump_to_bottom_button` ship in).
//!
//! Idioms, each verified against this crate's `components/emoji_upload_slot.rs`
//! / `components/jump_to_bottom_button.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the placeholder-vs-circle swap is a node-level reactive
//!   `if hasImage.get() { … } else { … }` inside `ui!` — the `if` condition
//!   reads a signal (`.get()`), so it lowers to the `when` primitive (mounts
//!   the chosen branch, unmounts the other on toggle). `StyleRules` is not
//!   `Copy`, so every style is written INLINE in `style = …` (created fresh
//!   per evaluation, never captured across the `when` boundary);
//! - the "dashed border" is emulated with a grid of small absolute-positioned
//!   dash cells. `StyleRules` exposes per-side `border_*_width` /
//!   `border_*_color` but NO `border-style`, so there is no built-in `dashed`
//!   — the 24 dashes in [`DASHES`] below are laid out on a 6px grid around a
//!   72px square (a 6×6 of 12px cells, 6px dash + 6px gap per side, corner
//!   dots), which reads as a dashed outline at a glance;
//! - the click is a `view` + `.on_touch(|ev| …)` chain with the
//!   `TouchPhase::Ended` guard — the app's standard click pattern (the
//!   runtime's `button` helper takes a fixed `label` prop, which doesn't fit
//!   a styled square area).

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, JustifyContent, Position,
    StyleRules, TouchPhase, TouchResponse,
};

// Discord palette — kept local so the component is self-contained.
const BORDER: &str = "#80848e"; // muted gray (Discord muted / placeholder)
const CIRCLE: &str = "#5865f2"; // blurple stand-in for an uploaded icon

// The dashed border as 24 absolute-positioned dash cells `(x, y, w, h)` in px,
// laid out on a 6px grid around a 72px square: a 6×6 of 12px cells where each
// side carries a 6px dash + 6px gap and the four corners are 6px dots. Static
// (computed once) so the `ui!` `for` body stays a simple property list and
// `StyleRules` can be built inline per dash.
#[allow(dead_code)]
const DASHES: &[(u32, u32, u32, u32)] = &[
    // top edge (y=0): corner, dash, dash, dash, dash, dash, corner
    (0, 0, 6, 6), (12, 0, 6, 6), (24, 0, 6, 6), (36, 0, 6, 6), (48, 0, 6, 6), (60, 0, 6, 6), (66, 0, 6, 6),
    // bottom edge (y=66): corner, dash, dash, dash, dash, dash, corner
    (0, 66, 6, 6), (12, 66, 6, 6), (24, 66, 6, 6), (36, 66, 6, 6), (48, 66, 6, 6), (60, 66, 6, 6), (66, 66, 6, 6),
    // left edge (x=0), between the corner dots
    (0, 12, 6, 6), (0, 24, 6, 6), (0, 36, 6, 6), (0, 48, 6, 6), (0, 60, 6, 6),
    // right edge (x=66), between the corner dots
    (66, 12, 6, 6), (66, 24, 6, 6), (66, 36, 6, 6), (66, 48, 6, 6), (66, 60, 6, 6),
];

/// The server-icon upload placeholder. Renders the dashed "Click to upload"
/// area while the local `hasImage` signal is `false` (the start state), and a
/// solid colored circle while it is `true`; clicking the square toggles the
/// signal (no real file picker — just the local flip).
#[component]
#[allow(non_snake_case)]
pub fn ServerIconUploadPlaceholder() -> Element {
    let hasImage = signal(false);

    ui! {
        // The 72px square host. `position: Relative` anchors the absolute
        // dash cells to it; `.on_touch` is the app's standard click pattern,
        // firing the toggle on `TouchPhase::Ended`. The whole square is the
        // click target so the empty gaps between the dashes are clickable too.
        view(style = StyleRules {
            width: Some(px(72.0)),
            height: Some(px(72.0)),
            position: Some(Position::Relative),
            ..Default::default()
        }) {
            // The dashed border: 24 absolute-positioned 6px cells. `for`
            // over the static `DASHES` with `key = idx` (the same loop+key
            // shape `emoji_upload_slot` uses). Each dash's style is built
            // inline (created fresh, never captured — `StyleRules` is not
            // `Copy`).
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

            // The center 60×60 box (inset 6px, inside the dashed frame),
            // centering whatever state branch mounts into it.
            view(style = StyleRules {
                position: Some(Position::Absolute),
                left: Some(px(6.0)),
                top: Some(px(6.0)),
                width: Some(px(60.0)),
                height: Some(px(60.0)),
                flex_direction: Some(FlexDirection::Column),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                ..Default::default()
            }) {
                // Reactive branch: `if hasImage.get() {…} else {…}` lowers to
                // a `when` (mounts the chosen branch, unmounts the other on
                // toggle). Both styles are inline so nothing is captured
                // across the `when` boundary.
                if hasImage.get() {
                    // A 36px colored circle standing in for an uploaded
                    // server-icon image (no backend call — just a solid dot).
                    view(style = StyleRules {
                        width: Some(px(36.0)),
                        height: Some(px(36.0)),
                        background: Some(col(CIRCLE)),
                        border_top_left_radius: Some(px(18.0)),
                        border_top_right_radius: Some(px(18.0)),
                        border_bottom_left_radius: Some(px(18.0)),
                        border_bottom_right_radius: Some(px(18.0)),
                        ..Default::default()
                    })
                } else {
                    // The "Click to upload" placeholder label, centered in
                    // the 60px box.
                    text(style = StyleRules {
                        font_size: Some(px(10.0)),
                        color: Some(col(BORDER)),
                        ..Default::default()
                    }) { "Click to upload".to_string() }
                }
            }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                hasImage.set(!hasImage.get());
            }
            TouchResponse::CONSUMED
        })
    }
}
