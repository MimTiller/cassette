//! `edited_label` — a self-contained "(edited)" marker for the Discord
//! clone. Bound to a local `was_edited` boolean signal (hardcoded `true` at
//! start), it renders a small gray `(edited)` text while the signal is set,
//! and nothing at all while it is unset.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `idle_timer_badge` /
//! `server_folder_group` / `profile_settings` ship in).
//!
//! Idioms, each verified against this crate's `app.rs` /
//! `components/idle_timer_badge.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the "(edited)" text is a reactive `if was_edited.get() { … }` inside
//!   `ui!`. An `if` whose condition reads a signal (`.get()`) lowers to the
//!   `when` primitive (mounts while true, unmounts on false) — the same
//!   reactive `if` `idle_timer_badge` uses for its badge portion. There is no
//!   `else` branch: while unset, the marker simply renders nothing.

use crate::style_helpers::{col, px};
use runtime_core::{component, signal, ui, Element, StyleRules};

// Discord muted-text gray (kept local so the component is self-contained).
const TEXT_MUTED: &str = "#949ba4";

/// The "(edited)" marker. Renders small gray "(edited)" text while the local
/// `was_edited` signal is set (hardcoded `true` for now); otherwise it
/// renders nothing.
#[component]
pub fn edited_label() -> Element {
    let was_edited = signal(true);

    ui! {
        // Reactive branch: `if was_edited.get() {…}` lowers to a `when` whose
        // render is an `Fn` closure. `StyleRules` is not `Copy`, so the text
        // style is written INLINE in `style = …` (created fresh per
        // evaluation, never captured across the `when` boundary) — the same
        // shape `app.rs` / `idle_timer_badge` use for their reactive
        // subtrees. A top-level `if` with no wrapping node is a legal `ui!`
        // tree (the grammar is `ui! := node*`, and a node may be an `if`); it
        // lowers to a single `when` node, so the fragment root holds.
        if was_edited.get() {
            text(style = StyleRules {
                font_size: Some(px(12.0)),
                color: Some(col(TEXT_MUTED)),
                ..Default::default()
            }) { "(edited)".to_string() }
        }
    }
}
