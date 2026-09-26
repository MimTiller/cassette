#![allow(dead_code)]
//! `dm_notification_badge` — a self-contained DM unread-notification badge for
//! the Discord clone. Renders a small red circular badge containing the local
//! unread count. When the count is `0`, the whole badge renders **nothing at
//! all** (no circle, no node).
//!
//! Everything is hardcoded local data — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape
//! `pin_count_badge` / `slowmode_indicator` / `verified_badge` ship in).
//!
//! Idioms, each verified against this crate's
//! `components/pin_count_badge.rs` / `components/slowmode_indicator.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the "render nothing when the count is 0" part is a STATIC
//!   `if UNREAD_COUNT > 0 { … }` wrapping the WHOLE badge, with no `else`.
//!   Because the condition is call-free (`UNREAD_COUNT` is a local `const`),
//!   the macro lowers it through `emit_plain_if`'s `Ctx::Child` arm to a plain
//!   Rust `if` whose taken branch flattens into its flat siblings and whose
//!   missing `else` contributes *nothing* — so a zero count mounts no node at
//!   all.
//! - the count label is a static `UNREAD_COUNT.to_string()` bound above the
//!   `ui!` block (the exact idiom `pin_count_badge.rs` uses for
//!   `count_label`), so it bakes to `TextSource::Static`. No signals anywhere.

use crate::style_helpers::{col, px};
use runtime_core::{component, ui, Element, FontWeight, StyleRules};

// Discord palette (kept local so the component is self-contained).
const TEXT: &str = "#ffffff"; // the count label (near-white on the red badge)
const BG_BADGE: &str = "#f23f43"; // the badge fill (Discord unread red)

// The hardcoded local DM unread count. No backend fetch — a self-contained
// badge, so a fixed value stands in for a live count. It is nonzero so the
// badge renders; set it to `0` to exercise the "nothing" path.
const UNREAD_COUNT: usize = 3;

/// The DM unread-notification badge. A small red circular badge with the
/// unread count; renders nothing at all when `UNREAD_COUNT` is `0`.
#[component]
#[allow(non_snake_case)]
pub fn DmNotificationBadge() -> Element {
    // Static: `UNREAD_COUNT` is a `const`, so `to_string()` is a one-shot
    // value, not a reactive read. Bound above the `ui!` block so the branch
    // (a plain Rust `if`, borrowed capture) can reference it freely.
    let count_label = UNREAD_COUNT.to_string();

    ui! {
        // Static branch: `UNREAD_COUNT > 0` is call-free (a local `const`), so
        // the macro lowers it through `emit_plain_if`'s `Ctx::Child` arm to a
        // plain Rust `if`. The badge flattens into flat siblings when true; the
        // missing `else` contributes nothing, so a zero count mounts no node at
        // all. `StyleRules` is not `Copy`, so the styles are written INLINE
        // (`style = …`), created fresh per evaluation — the same shape
        // `pin_count_badge` uses.
        if UNREAD_COUNT > 0 {
            view(style = StyleRules {
                padding_left: Some(px(6.0)),
                padding_right: Some(px(6.0)),
                padding_top: Some(px(2.0)),
                padding_bottom: Some(px(2.0)),
                background: Some(col(BG_BADGE)),
                border_top_left_radius: Some(px(9.0)),
                border_top_right_radius: Some(px(9.0)),
                border_bottom_left_radius: Some(px(9.0)),
                border_bottom_right_radius: Some(px(9.0)),
                ..Default::default()
            }) {
                // The count label — the static `count_label` bound above, so it
                // bakes to `TextSource::Static` too.
                text(style = StyleRules {
                    font_size: Some(px(12.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { count_label }
            }
        }
    }
}
