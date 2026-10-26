//! `template_gallery_grid` — a self-contained "template gallery" grid for the
//! Discord clone. Renders exactly 3 hardcoded local fake template names as a
//! horizontal row of small cards, each card holding its template name and a
//! "Preview" button. Pressing a card's "Preview" button sets THAT card's own
//! local `previewing` boolean signal (independent per card — any combination
//! can be previewing at once), which re-runs that card's reactive `style`
//! closure to swap its border from a thin muted outline to a thicker blurple
//! accent ring.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `permission_override_row` /
//! `category_filter_chips` / `server_tag_chips` ship in).
//!
//! Idioms, each verified against this crate's `components/permission_override_row.rs`
//! (per-item `button` in a keyed `for` with an inline `move || …` `on_click`
//! and a reactive `style = move || { let active = …; StyleRules { … } }`) and
//! `components/category_filter_chips.rs` (one independent `Signal<bool>` per
//! row slot, `[Signal<bool>; 3]`):
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (unitless border widths via a local `border_width` helper,
//!   the same `Tokenized<f32>` shape `permission_override_row` uses);
//! - the 3 cards are emitted by a keyed `for (idx, name) in TEMPLATES.iter()
//!   .enumerate(), key = idx` over the static template-name slice (the standard
//!   static-row idiom from `category_filter_chips` / `server_tag_chips`);
//! - the per-card "previewing" state is `[Signal<bool>; 3]` — one slot per
//!   card — so `previewing[idx]` gives each card its OWN independent local
//!   boolean signal. `Signal<bool>` is `Copy` (and an array of `Copy` elements
//!   is `Copy`), so the `move` closures below capture a copy of the array (and
//!   of `idx`, a `usize`) and each stays `Fn`, never interfering with the other
//!   cards' slots;
//! - the "Preview" button is the `button(label, on_click, style)` primitive with
//!   an inline `on_click = move || previewing[idx].set(true)` — the exact shape
//!   `permission_override_row` uses for its `on_click = move || state.set(i)`;
//! - the border highlight is REACTIVE: each card's `view` carries
//!   `style = move || { let on = previewing[idx].get(); StyleRules { … } }`
//!   (the `IntoStyleProp for Fn() -> StyleRules` dynamic path
//!   `permission_override_row` uses on its buttons), reading the signal at
//!   re-evaluation time so the `StyleRules` is a fresh value each pass (never a
//!   captured `let` binding — `StyleRules` is not `Copy`).

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent,
    StyleRules, Tokenized,
};

// Discord dark palette — kept local so the component is self-contained.
const BG_ROW: &str = "#313338"; // row surface (Discord sidebar gray)
const BG_CARD: &str = "#2b2d31"; // card surface (Discord sidebar panel)
const TEXT: &str = "#f2f3f5"; // primary text (template name)
const BUTTON_BG: &str = "#5865f2"; // preview button fill (Discord blurple)
const BORDER_IDLE: &str = "#4e5058"; // idle card outline (muted)
const BORDER_PREVIEW: &str = "#5865f2"; // previewing card ring (blurple accent)

/// A unitless `Tokenized<f32>` border width (the `border_*_width` style fields
/// take a plain `f32` token, not a `Length`) — same shape `permission_override_row`
/// uses.
fn border_width(px: f32) -> Tokenized<f32> {
    Tokenized::Literal(px)
}

// The 3 hardcoded local fake template names — exactly these, in this order, no
// backend fetch. Kept as a static slice so the keyed `for` renders one card per
// entry and the loop index doubles as the per-card signal slot.
const TEMPLATES: [&str; 3] = [
    "Weekly Standup",
    "Project Retrospective",
    "Content Calendar",
];

/// The template-gallery grid. Renders exactly 3 hardcoded fake template names
/// as a row of small cards; each card holds its name and a "Preview" button.
/// Pressing a card's "Preview" button sets ONLY that card's local `previewing`
/// signal, which flips the card's border from the idle muted outline to the
/// blurple preview ring. No props, no backend.
#[component]
pub fn TemplateGalleryGrid() -> Element {
    // One independent local boolean "previewing" signal per card (one slot per
    // template). `Signal<bool>` is `Copy` and an array of `Copy` elements is
    // `Copy`, so `previewing[idx]` hands each closure its own copy pointing at
    // the same underlying slot — a press on card 0 never touches card 1's slot.
    let previewing: [runtime_core::Signal<bool>; 3] = [signal(false), signal(false), signal(false)];

    ui! {
        // Horizontal row of small cards, vertically centered, on a Discord-gray
        // surface.
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(10.0)),
            padding_top: Some(px(16.0)),
            padding_bottom: Some(px(16.0)),
            padding_left: Some(px(16.0)),
            padding_right: Some(px(16.0)),
            background: Some(col(BG_ROW)),
            border_top_left_radius: Some(px(10.0)),
            border_top_right_radius: Some(px(10.0)),
            border_bottom_left_radius: Some(px(10.0)),
            border_bottom_right_radius: Some(px(10.0)),
            ..Default::default()
        }) {
            // One small card per hardcoded template. `idx` is the loop index
            // (and the `key`); the card's reactive `style` closure and the
            // button's `on_click` closure each capture `idx` + a copy of the
            // `previewing` array to read/write `previewing[idx]` — so card 0
            // ↔ previewing[0], card 1 ↔ previewing[1], card 2 ↔ previewing[2],
            // each independent.
            for (idx, name) in TEMPLATES.iter().enumerate(), key = idx {
                // The card. Its border is REACTIVE: the `style = move || …`
                // closure reads `previewing[idx]` and rebuilds the `StyleRules`
                // on every press — idle → thin muted outline; previewing →
                // thicker blurple ring. Built inline so nothing is captured
                // across the reactive boundary (`StyleRules` is not `Copy`).
                view(style = move || {
                    let on = previewing[idx].get();
                    StyleRules {
                        flex_direction: Some(FlexDirection::Column),
                        align_items: Some(AlignItems::Stretch),
                        gap: Some(px(10.0)),
                        padding_top: Some(px(12.0)),
                        padding_right: Some(px(12.0)),
                        padding_bottom: Some(px(12.0)),
                        padding_left: Some(px(12.0)),
                        background: Some(col(BG_CARD)),
                        border_top_left_radius: Some(px(8.0)),
                        border_top_right_radius: Some(px(8.0)),
                        border_bottom_left_radius: Some(px(8.0)),
                        border_bottom_right_radius: Some(px(8.0)),
                        border_top_width: Some(border_width(if on { 2.0 } else { 1.0 })),
                        border_right_width: Some(border_width(if on { 2.0 } else { 1.0 })),
                        border_bottom_width: Some(border_width(if on { 2.0 } else { 1.0 })),
                        border_left_width: Some(border_width(if on { 2.0 } else { 1.0 })),
                        border_top_color: Some(col(if on { BORDER_PREVIEW } else { BORDER_IDLE })),
                        border_right_color: Some(col(if on { BORDER_PREVIEW } else { BORDER_IDLE })),
                        border_bottom_color: Some(col(if on { BORDER_PREVIEW } else { BORDER_IDLE })),
                        border_left_color: Some(col(if on { BORDER_PREVIEW } else { BORDER_IDLE })),
                        ..Default::default()
                    }
                }) {
                    // The template name — primary text, semibold. Static (does
                    // not depend on the signal), written inline.
                    text(style = StyleRules {
                        font_size: Some(px(15.0)),
                        font_weight: Some(FontWeight::SemiBold),
                        color: Some(col(TEXT)),
                        ..Default::default()
                    }) { name.to_string() }

                    // The "Preview" button. Its `on_click` sets ONLY this card's
                    // local `previewing` signal to `true` (`previewing` and
                    // `idx` are both `Copy`, so this `move` closure captures its
                    // own copies and stays `Fn` — callable on every press, and it
                    // never touches another card's slot). The button's own style
                    // is static (only the card's border reacts to the signal),
                    // written inline.
                    button(
                        label = "Preview".to_string(),
                        on_click = move || previewing[idx].set(true),
                        style = StyleRules {
                            height: Some(px(30.0)),
                            margin_top: Some(px(2.0)),
                            flex_direction: Some(FlexDirection::Row),
                            align_items: Some(AlignItems::Center),
                            justify_content: Some(JustifyContent::Center),
                            background: Some(col(BUTTON_BG)),
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
    }
}
