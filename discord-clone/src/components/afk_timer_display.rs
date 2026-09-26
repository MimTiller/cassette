//! `afk_timer_display` — a self-contained "AFK after N minutes" label for the
//! Discord clone. Takes a hardcoded local `minutes` value (`10`) and renders
//! the plain text "Move to AFK after {n} minutes".
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `edited_label` /
//! `thread_list_row` / `profile_settings` ship in).
//!
//! Idioms, each verified against this crate's `components/edited_label.rs` /
//! `components/thread_list_row.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the whole tree is a single `text` leaf — no `view` wrapper, no
//!   `Vec<Element>` built outside the macro. A top-level leaf node is a legal
//!   `ui!` tree (the grammar is `ui! := node*`);
//! - the label text is a `format!(…)` over the hardcoded local `minutes`,
//!   built inline in the `text` body — the same shape `welcome_banner` /
//!   `transfer_ownership_confirm` use for their dynamic `format!` text.

use crate::style_helpers::{col, px};
use runtime_core::{component, ui, Element, StyleRules};

// Discord muted-text gray (kept local so the component is self-contained).
const TEXT_MUTED: &str = "#949ba4";

/// The AFK timer label. Renders the plain text "Move to AFK after {n} minutes"
/// for the hardcoded local `minutes` value (`10`). No props, no backend.
#[component]
pub fn AfkTimerDisplay() -> Element {
    // The one hardcoded local value — no backend fetch.
    let minutes = 10u32;

    ui! {
        text(style = StyleRules {
            font_size: Some(px(14.0)),
            color: Some(col(TEXT_MUTED)),
            ..Default::default()
        }) { format!("Move to AFK after {} minutes", minutes) }
    }
}
