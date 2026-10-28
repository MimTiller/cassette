//! `server_template_card` — a self-contained "server template" card for the
//! Discord clone. Shows a hardcoded local template name and short description
//! in a card, with a "Use Template" button that sets a local `selected`
//! boolean signal (hardcoded `false` at start). Once `selected` is set, the
//! "Selected!" text renders in place of the button.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `server_discovery_card` /
//! `idle_timer_badge` / `server_folder_group` / `edited_label` ship in).
//!
//! Idioms, each verified against this crate's `components/server_discovery_card.rs`
//! / `components/idle_timer_badge.rs` / `components/jump_to_bottom_button.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the "Use Template" control is the `button(label, on_click)` primitive;
//!   `on_click` is a plain `move` closure prebuilt in a `let` and passed by
//!   variable name (exactly the `Send` button in `app.rs`); `Signal` is
//!   `Copy`, so the `move` closure captures a copy of the signal and stays
//!   `Fn` (callable on every press);
//! - the control is a reactive `if !selected.get() { button … } else {
//!   text "Selected!" }` inside `ui!`. An `if`/`else` whose condition reads a
//!   signal (`.get()`) lowers to the `when` primitive (mounts the chosen
//!   branch, swaps on toggle) — the same reactive `if`/`else`
//!   `server_discovery_card` uses for its Join/Joined portion. Because a
//!   `StyleRules` literal is not `Copy`, both branch styles are written
//!   INLINE in `style = …` (created fresh per evaluation, never captured
//!   across the `when` boundary) — the same shape `app.rs` /
//!   `server_discovery_card` use for their reactive subtrees.

use crate::theme;
use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent,
    StyleRules,
};

// Discord dark palette (kept local so the component is self-contained).

/// The hardcoded template name (no props for now).
const TEMPLATE_NAME: &str = "Gaming";
/// The hardcoded short description.
const DESCRIPTION: &str = "For gaming communities";

/// The server-template card. Renders the hardcoded template name /
/// description; while the local `selected` signal is unset a "Use Template"
/// button shows (pressing it sets `selected`), and once `selected` is set the
/// "Selected!" text renders in its place.
#[component]
pub fn ServerTemplateCard() -> Element {
    let selected = signal(false);

    // "Use Template": flip the `selected` signal to `true` on press.
    // `selected` is `Copy`, so the `move` closure captures a copy of the same
    // underlying signal (the one the reactive `if` below also reads) and
    // remains `Fn`.
    let select = move || selected.set(true);

    let root_style = StyleRules {
        flex_direction: Some(FlexDirection::Column),
        align_items: Some(AlignItems::Stretch),
        gap: Some(px(8.0)),
        padding_top: Some(px(16.0)),
        padding_right: Some(px(16.0)),
        padding_bottom: Some(px(16.0)),
        padding_left: Some(px(16.0)),
        background: Some(col(theme::BG_SECONDARY)),
        border_top_left_radius: Some(px(10.0)),
        border_top_right_radius: Some(px(10.0)),
        border_bottom_left_radius: Some(px(10.0)),
        border_bottom_right_radius: Some(px(10.0)),
        ..Default::default()
    };

    let name_style = StyleRules {
        font_size: Some(px(18.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(theme::TEXT_STRONG)),
        ..Default::default()
    };

    ui! {
        view(style = root_style) {
            text(style = name_style) { TEMPLATE_NAME.to_string() }
            // The muted description meta line — written INLINE in `style = …`
            // (a `StyleRules` `let` would be moved on its first use).
            text(style = StyleRules {
                font_size: Some(px(13.0)),
                color: Some(col(theme::TEXT_MUTED)),
                ..Default::default()
            }) { DESCRIPTION.to_string() }
            // Reactive branch: `if !selected.get() {…} else {…}` lowers to a
            // `when` whose per-branch render is an `Fn` closure. `StyleRules`
            // is not `Copy`, so both branch styles are written INLINE in
            // `style = …` (created fresh per evaluation, never captured
            // across the `when` boundary) — the same shape `server_discovery_card`
            // uses for its Join/Joined subtree.
            if !selected.get() {
                button(
                    label = "Use Template".to_string(),
                    on_click = select,
                    style = StyleRules {
                        height: Some(px(36.0)),
                        margin_top: Some(px(4.0)),
                        background: Some(col(theme::ACCENT)),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        border_top_left_radius: Some(px(8.0)),
                        border_top_right_radius: Some(px(8.0)),
                        border_bottom_left_radius: Some(px(8.0)),
                        border_bottom_right_radius: Some(px(8.0)),
                        ..Default::default()
                    }
                )
            } else {
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    color: Some(col(theme::TEXT_STRONG)),
                    margin_top: Some(px(4.0)),
                    ..Default::default()
                }) { "Selected!".to_string() }
            }
        }
    }
}
