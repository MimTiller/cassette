//! `spoiler_image_placeholder` — a fixed-size colored rectangle that stands in
//! for a spoiler image. It shows centered text "Click to view spoiler";
//! clicking it toggles a local `revealed` boolean signal and swaps the text to
//! "Image revealed" (clicking again toggles it back).
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape
//! `permission_override_row` / `translate_toggle` / `welcome_banner` ship in).
//!
//! Idioms, each verified against this crate's
//! `components/permission_override_row.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors;
//! - the rectangle is the `button(label, on_click, style)` primitive — a
//!   tappable container with a centered label, which is exactly a "colored
//!   rectangle with centered text that is clickable";
//! - `on_click` is a plain `move || revealed.set(!revealed.get())` closure.
//!   `revealed` is a `Signal<bool>` (`Copy`), so the closure captures a copy
//!   of the same underlying signal and stays `Fn` (callable on every press);
//! - the label is REACTIVE —
//!   `move || if revealed.get() { LABEL_REVEALED } else { LABEL_HIDDEN }` — a
//!   `Fn() -> &'static str` that the `TextContent for Fn() -> D: ToString`
//!   impl turns into a dynamic text value, so the label re-reads `revealed` on
//!   re-evaluation and swaps the moment the signal flips.

use crate::theme;
use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, JustifyContent, StyleRules,
};

// Discord dark palette + labels — kept local so the component is self-contained.
const LABEL_HIDDEN: &str = "Click to view spoiler";
const LABEL_REVEALED: &str = "Image revealed";

/// A fixed-size spoiler-image placeholder. Renders a colored rectangle with
/// centered text; clicking toggles a local `revealed` signal and swaps the
/// label between "Click to view spoiler" and "Image revealed". No props, no
/// backend.
#[component]
#[allow(dead_code)] // declared but not wired into any screen yet
pub fn SpoilerImagePlaceholder() -> Element {
    // Whether the spoiler has been revealed. Local only.
    let revealed = signal(false);

    ui! {
        button(
            label = move || if revealed.get() { LABEL_REVEALED } else { LABEL_HIDDEN },
            on_click = move || revealed.set(!revealed.get()),
            style = move || {
                let shown = revealed.get();
                StyleRules {
                    width: Some(px(280.0)),
                    height: Some(px(160.0)),
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    background: Some(col(if shown { theme::ACCENT } else { theme::BG_SELECTED })),
                    border_top_left_radius: Some(px(10.0)),
                    border_top_right_radius: Some(px(10.0)),
                    border_bottom_left_radius: Some(px(10.0)),
                    border_bottom_right_radius: Some(px(10.0)),
                    ..Default::default()
                }
            }
        )
    }
}
