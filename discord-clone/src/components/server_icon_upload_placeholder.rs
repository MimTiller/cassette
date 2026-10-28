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
//!   — the 24 dashes in [`dashes`] are laid out on a 16px grid around the
//!   `SQUARE`-px host (an 8px dash + 8px gap per side, corner dots), which
//!   reads as a dashed outline at a glance. The box is 96px (was 72px): the
//!   wrapped "Click to upload" label was crowding the dashes at the smaller
//!   size, so the host and frame grew to give the two label lines room.
//! - the click is a `view` + `.on_touch(|ev| …)` chain with the
//!   `TouchPhase::Ended` guard — the app's standard click pattern (the
//!   runtime's `button` helper takes a fixed `label` prop, which doesn't fit
//!   a styled square area).

use crate::theme;
use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, JustifyContent, Position,
    StyleRules, TouchPhase, TouchResponse,
};

// Discord palette — kept local so the component is self-contained.

// The dashed border as 24 absolute-positioned dash cells `(x, y, w, h)` in px,
// laid out on a 6px grid around a 72px square: a 6×6 of 12px cells where each
// side carries a 6px dash + 6px gap and the four corners are 6px dots. Static
// (computed once) so the `ui!` `for` body stays a simple property list and
// `StyleRules` can be built inline per dash.
// The dashed frame is GENERATED rather than hardcoded: a `SQUARE`-px box with
// `DASH`-px corner dots and dashes on a `DASH + GAP` step, so widening the box
// to give the "Click to upload" label room can never desync the chrome from
// the box it frames. (A loop, because `StyleRules` has no `border-style` for a
// real dashed outline.)
const SQUARE: f32 = 96.0;
const DASH: f32 = 8.0;
const GAP: f32 = 8.0;

fn dashes() -> Vec<(f32, f32, f32, f32)> {
    let last = SQUARE - DASH;
    let step = DASH + GAP;
    let mut out = Vec::<(f32, f32, f32, f32)>::new();
    // Four corner dots, then dashes between them on each edge — same 24-cell
    // density as the original hand-laid frame.
    out.push((0.0, 0.0, DASH, DASH));
    out.push((last, 0.0, DASH, DASH));
    out.push((0.0, last, DASH, DASH));
    out.push((last, last, DASH, DASH));
    let mut x = step;
    while x < last {
        out.push((x, 0.0, DASH, DASH));
        out.push((x, last, DASH, DASH));
        x += step;
    }
    let mut y = step;
    while y < last {
        out.push((0.0, y, DASH, DASH));
        out.push((last, y, DASH, DASH));
        y += step;
    }
    out
}

/// The server-icon upload placeholder. Renders the dashed "Click to upload"
/// area while the local `hasImage` signal is `false` (the start state), and a
/// solid colored circle while it is `true`; clicking the square toggles the
/// signal (no real file picker — just the local flip).
#[component]
pub fn ServerIconUploadPlaceholder() -> Element {
    let hasImage = signal(false);
    // Dash positions come from `dashes()` (generated off SQUARE/DASH/GAP) so
    // the frame and the box can't drift apart.
    let frame = dashes();

    ui! {
        // The host square. `position: Relative` anchors the absolute
        // dash cells to it; `.on_touch` is the app's standard click pattern,
        // firing the toggle on `TouchPhase::Ended`. The whole square is the
        // click target so the empty gaps between the dashes are clickable too.
        view(style = StyleRules {
            width: Some(px(SQUARE)),
            height: Some(px(SQUARE)),
            position: Some(Position::Relative),
            ..Default::default()
        }) {
            // The dashed border: generated 8px cells. `for` over the static
            // `DASHES` with `key = idx` (the same loop+key
            // shape `emoji_upload_slot` uses). Each dash's style is built
            // inline (created fresh, never captured — `StyleRules` is not
            // `Copy`).
            for (idx, &(x, y, w, h)) in frame.iter().enumerate(), key = idx {
                view(style = StyleRules {
                    position: Some(Position::Absolute),
                    left: Some(px(x)),
                    top: Some(px(y)),
                    width: Some(px(w)),
                    height: Some(px(h)),
                    background: Some(col(theme::TEXT_FAINT)),
                    border_top_left_radius: Some(px(1.0)),
                    border_top_right_radius: Some(px(1.0)),
                    border_bottom_left_radius: Some(px(1.0)),
                    border_bottom_right_radius: Some(px(1.0)),
                    ..Default::default()
                })
            }

            // The center box (inset one dash, inside the dashed frame),
            // centering whatever state branch mounts into it.
            view(style = StyleRules {
                position: Some(Position::Absolute),
                left: Some(px(DASH)),
                top: Some(px(DASH)),
                width: Some(px(SQUARE - 2.0 * DASH)),
                height: Some(px(SQUARE - 2.0 * DASH)),
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
                        background: Some(col(theme::ACCENT)),
                        border_top_left_radius: Some(px(18.0)),
                        border_top_right_radius: Some(px(18.0)),
                        border_bottom_left_radius: Some(px(18.0)),
                        border_bottom_right_radius: Some(px(18.0)),
                        ..Default::default()
                    })
                } else {
                    // The "Click to upload" placeholder label. 9px in a
                    // SQUARE-16 box so the two wrapped lines sit clear of the
                    // dashed chrome on every side (the old 10px label in a
                    // 60px box crowded the dashes).
                    text(style = StyleRules {
                        font_size: Some(px(9.0)),
                        color: Some(col(theme::TEXT_FAINT)),
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
