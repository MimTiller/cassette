//! `ban_list_row` — a single "banned user" row for the Discord clone. Renders
//! one hardcoded local banned user — username "Jordan", reason "Spamming
//! links" — as a single line: the username (primary text) and the reason
//! (muted text) side by side, with an "Unban" button on the same line.
//!
//! Bound to a local `unbanned` boolean signal (hardcoded `false` at start).
//! While the signal is unset the row renders; pressing "Unban" sets the
//! signal to `true`, which unmounts the row so it renders nothing.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `audit_log_row` /
//! `ban_reason_input` / `verified_badge` ship in).
//!
//! Idioms, each verified against this crate:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the row is a single `view` (`flex_direction: Row`, `align_items:
//!   Center`, `gap`) holding the two `text` leaves and the `button` as inline
//!   `ui!` children — no `Vec<Element>` built outside the macro
//!   (`audit_log_row`);
//! - the "only render while not unbanned" part is a reactive
//!   `if !unbanned.get() { … }` inside `ui!` — an `if` whose condition reads a
//!   signal (`.get()`) lowers to the `when` primitive (mounts while true,
//!   unmounts on false); there is no `else` branch, so once unbanned the row
//!   renders nothing (`welcome_banner` / `attachment_preview_card` use the
//!   negated form);
//! - the "Unban" button is the `button(label, on_click)` primitive with a
//!   prebuilt `move` closure (`ban_reason_input`'s "Ban" button, mirrored).
//!   `Signal` is `Copy`, so the closure captures a copy of `unbanned` and
//!   stays `Fn` across every press; styles are inline so nothing non-`Copy`
//!   is captured across the `when` boundary.

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, JustifyContent, StyleRules,
};

// Discord dark palette (kept local so the component is self-contained).
const TEXT: &str = "#f2f3f5"; // primary text (the username)
const TEXT_MUTED: &str = "#949ba4"; // muted text (the reason)
const UNBAN_BG: &str = "#5865f2"; // unban button (Discord blurple)
const UNBAN_TEXT: &str = "#f2f3f5"; // unban button label (near-white)

// The one hardcoded local banned user — no backend fetch.
const USERNAME: &str = "Jordan";
const REASON: &str = "Spamming links";

/// A single banned-user row. Renders the hardcoded local username and reason
/// side by side with an "Unban" button on the same line; pressing "Unban"
/// sets the local `unbanned` signal to `true`, which unmounts the row (it
/// renders nothing). No backend call, no props — local state only.
#[component]
#[allow(non_snake_case)]
pub fn BanListRow() -> Element {
    // Local state: whether this user has been unbanned. Starts `false`
    // (still banned → the row is visible).
    let unbanned = signal(false);

    // The "Unban" button flips `unbanned` to `true` (one-way). `Signal` is
    // `Copy`, so the `move` closure captures a copy of the same signal and
    // stays `Fn` across every press. No backend call.
    let unban = move || unbanned.set(true);

    ui! {
        // Reactive branch: `if !unbanned.get() {…}` lowers to a `when` whose
        // render is an `Fn` closure. No `else` branch: while the signal is
        // set (unbanned) the row renders nothing. `StyleRules` is not `Copy`,
        // so every style is written INLINE in `style = …` (created fresh per
        // evaluation, never captured across the `when` boundary).
        if !unbanned.get() {
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(10.0)),
                padding_top: Some(px(6.0)),
                padding_bottom: Some(px(6.0)),
                ..Default::default()
            }) {
                // The banned username — primary text, a static string literal.
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { USERNAME.to_string() }

                // The ban reason — muted text, a static string literal, on the
                // same line (side by side) as the username.
                text(style = StyleRules {
                    font_size: Some(px(13.0)),
                    color: Some(col(TEXT_MUTED)),
                    ..Default::default()
                }) { REASON.to_string() }

                // The "Unban" button — the `button(label, on_click)` primitive
                // (`ban_reason_input`'s "Ban" button, mirrored). Pressing it
                // sets `unbanned` to `true`, which unmounts this whole row.
                button(
                    label = "Unban".to_string(),
                    on_click = unban,
                    style = StyleRules {
                        height: Some(px(28.0)),
                        padding_left: Some(px(14.0)),
                        padding_right: Some(px(14.0)),
                        background: Some(col(UNBAN_BG)),
                        color: Some(col(UNBAN_TEXT)),
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
