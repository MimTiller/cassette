//! `verified_badge` — a self-contained "verified" checkmark for the Discord
//! clone. Bound to a local `isVerified` boolean signal (hardcoded `true` at
//! start), it renders a small checkmark glyph in the distinct verified blue
//! while the signal is set, and nothing at all while it is unset.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `edited_label` /
//! `idle_timer_badge` / `server_folder_group` ship in).
//!
//! Idioms, each verified against this crate's `app.rs` /
//! `components/edited_label.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the checkmark is a reactive `if isVerified.get() { … }` inside `ui!`.
//!   An `if` whose condition reads a signal (`.get()`) lowers to the `when`
//!   primitive (mounts while true, unmounts on false) — the same reactive
//!   `if` `edited_label` uses for its "(edited)" text. There is no `else`
//!   branch: while unset, the badge simply renders nothing.

use crate::style_helpers::{col, px};
use runtime_core::{component, signal, ui, Element, StyleRules};
use discord_clone::components::VerifiedBadge;
use discord_clone::components::VerifiedBadge;

// Discord verified blue (the same blurple the verified check uses in the
// real client) — deliberately distinct from the muted gray of the surrounding
// message text. Kept local so the component is self-contained.
const VERIFIED_BLUE: &str = "#5865f2";

/// The verified checkmark. Renders a small blue "✓" while the local
/// `isVerified` signal is set (hardcoded `true` for now); otherwise it
/// renders nothing.
#[component]
#[allow(non_snake_case)]
pub fn VerifiedBadge() -> Element {
    let isVerified = signal(true);

    ui! {
        // Reactive branch: `if isVerified.get() {…}` lowers to a `when` whose
        // render is an `Fn` closure. `StyleRules` is not `Copy`, so the text
        // style is written INLINE in `style = …` (created fresh per
        // evaluation, never captured across the `when` boundary) — the same
        // shape `edited_label` uses for its reactive text. A top-level `if`
        // with no wrapping node is a legal `ui!` tree (the grammar is
        // `ui! := node*`, and a node may be an `if`); it lowers to a single
        // `when` node, so the fragment root holds.
        if isVerified.get() {
            text(style = StyleRules {
                font_size: Some(px(14.0)),
                color: Some(col(VERIFIED_BLUE)),
                ..Default::default()
            }) { "✓".to_string() }
        }
    }
}
