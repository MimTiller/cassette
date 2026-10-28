//! `blocked_user_row` — a single "blocked user" row for the Discord clone.
//! Renders one hardcoded local blocked user — username "Alex" — as a single
//! line: the username (primary text) with an "Unblock" button on the same
//! line.
//!
//! Bound to a local `unblocked` boolean signal (hardcoded `false` at start).
//! While the signal is unset the row renders; pressing "Unblock" sets the
//! signal to `true`, which unmounts the row so it renders nothing.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `ban_list_row` /
//! `audit_log_row` / `verified_badge` ship in).
//!
//! Idioms, each verified against this crate:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the row is a single `view` (`flex_direction: Row`, `align_items:
//!   Center`, `gap`) holding the `text` leaf and the `button` as inline `ui!`
//!   children — no `Vec<Element>` built outside the macro
//!   (`ban_list_row` / `audit_log_row`);
//! - the "only render while not unblocked" part is a reactive
//!   `if !unblocked.get() { … }` inside `ui!` — an `if` whose condition reads
//!   a signal (`.get()`) lowers to the `when` primitive (mounts while true,
//!   unmounts on false); there is no `else` branch, so once unblocked the row
//!   renders nothing (`welcome_banner` / `attachment_preview_card` use the
//!   negated form);
//! - the "Unblock" button is the `button(label, on_click)` primitive with a
//!   prebuilt `move` closure (`ban_reason_input`'s "Ban" button, mirrored).
//!   `Signal` is `Copy`, so the closure captures a copy of `unblocked` and
//!   stays `Fn` across every press; styles are inline so nothing non-`Copy`
//!   is captured across the `when` boundary.

use crate::theme;
use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, JustifyContent, StyleRules,
};

// Discord dark palette (kept local so the component is self-contained).

// The one hardcoded local blocked user — no backend fetch.
const USERNAME: &str = "Alex";

/// A single blocked-user row. Renders the hardcoded local username with an
/// "Unblock" button on the same line; pressing "Unblock" sets the local
/// `unblocked` signal to `true`, which unmounts the row (it renders nothing).
/// No backend call, no props — local state only.
#[component]
pub fn BlockedUserRow() -> Element {
    // Local state: whether this user has been unblocked. Starts `false`
    // (still blocked → the row is visible).
    let unblocked = signal(false);

    // The "Unblock" button flips `unblocked` to `true` (one-way). `Signal` is
    // `Copy`, so the `move` closure captures a copy of the same signal and
    // stays `Fn` across every press. No backend call.
    let unblock = move || unblocked.set(true);

    ui! {
        // Reactive branch: `if !unblocked.get() {…}` lowers to a `when` whose
        // render is an `Fn` closure. No `else` branch: while the signal is
        // set (unblocked) the row renders nothing. `StyleRules` is not `Copy`,
        // so every style is written INLINE in `style = …` (created fresh per
        // evaluation, never captured across the `when` boundary).
        if !unblocked.get() {
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(10.0)),
                padding_top: Some(px(6.0)),
                padding_bottom: Some(px(6.0)),
                ..Default::default()
            }) {
                // The blocked username — primary text, a static string literal.
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    color: Some(col(theme::TEXT_STRONG)),
                    ..Default::default()
                }) { USERNAME.to_string() }

                // The "Unblock" button — the `button(label, on_click)`
                // primitive (`ban_reason_input`'s "Ban" button, mirrored).
                // Pressing it sets `unblocked` to `true`, which unmounts this
                // whole row.
                button(
                    label = "Unblock".to_string(),
                    on_click = unblock,
                    style = StyleRules {
                        height: Some(px(28.0)),
                        padding_left: Some(px(14.0)),
                        padding_right: Some(px(14.0)),
                        background: Some(col(theme::ACCENT)),
                        color: Some(col(theme::TEXT_STRONG)),
                        border_top_left_radius: Some(px(6.0)),
                        border_top_right_radius: Some(px(6.0)),
                        border_bottom_left_radius: Some(px(6.0)),
                        border_bottom_right_radius: Some(px(6.0)),
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        ..Default::default()
                    }
                )
            }
        }
    }
}
