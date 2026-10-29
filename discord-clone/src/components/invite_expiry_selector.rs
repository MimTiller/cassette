//! `invite_expiry_selector` — a Discord-style invite-expiry picker: a
//! vertical radio group of exactly three hardcoded expiry options ("30
//! minutes", "1 day", "Never"), one selectable at a time. Each row is a
//! `view` whose `.on_touch(|ev| …)` chain (firing on `TouchPhase::Ended`)
//! sets the local `selected` signal to that option's index; the selected row
//! renders with a highlight and a filled radio dot while the others stay
//! dimmed and hollow.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `profile_settings` and
//! the other `components/` files ship in).
//!
//! Idioms, each verified against this crate's `app.rs` / `profile_settings.rs`
//! / `components/mic_selector.rs` / `components/timeout_duration_picker.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors;
//! - the options are authored with a `for (i, name) in …iter().enumerate()`
//!   inside `ui!` (a `repeat` whose per-item builder is an `Fn` closure, with
//!   the loop index as the compile-time key — the same shape `server_folder_group`
//!   uses, tuple destructuring verified in `app.rs:1497`);
//! - selection is REACTIVE, so every selection-dependent style is a
//!   `style = move || StyleRules { … }` closure (the `IntoStyleProp for
//!   Fn() -> StyleRules` dynamic path `app.rs` uses at `1345`/`1374`), reading
//!   `selected` at re-evaluation time rather than being baked once;
//! - because the per-row builder is a closure, styles are built fresh per
//!   evaluation (never a shared `let` binding — `StyleRules` is not `Copy` and
//!   a captured binding would be moved on the first iteration);
//! - the radio indicator is a bordered circle `view` whose background fill is
//!   the reactive selection signal (filled = selected, hollow = not), so no
//!   text *content* is ever reactive — a `ui!` text child must be `TextContent`
//!   (a bare `if` in the content slot yields an `Element`, which it is not).

use crate::theme;
use crate::style_helpers::{col, px, rounded};
use runtime_core::{
    component, ui, AlignItems, Element, FlexDirection, FontWeight, Signal, StyleRules, Tokenized,
    TouchPhase, TouchResponse,
};

// Discord dark palette (kept local so the component is self-contained).

/// The three hardcoded invite-expiry options — exactly three, local only.
const INVITE_EXPIRY_OPTIONS: [&str; 3] = ["30 minutes", "1 day", "Never"];

/// A unitless `Tokenized<f32>` border width (the `border_*_width` style fields
/// take a plain `f32` token, not a `Length`) — same shape `app.rs` uses.
fn border_width(px: f32) -> Tokenized<f32> {
    Tokenized::Literal(px)
}

/// An invite-expiry picker radio group. `value` is the selected index into
/// `INVITE_EXPIRY_OPTIONS`, owned by the parent (which reads it when creating
/// the invite) — the picker writes it on tap. Starting index is whatever the
/// parent initializes the signal to (the demo inverts: 0 = first option).
/// No props beyond the controlled `value`; no backend.
#[component]
pub fn InviteExpirySelector(value: Signal<usize>) -> Element {
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Stretch),
            background: Some(col(theme::BG_PRIMARY)),
            padding_top: Some(px(16.0)),
            padding_bottom: Some(px(16.0)),
            ..Default::default()
        }) {
            text(style = StyleRules {
                font_size: Some(px(13.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(theme::TEXT_MUTED)),
                margin_left: Some(px(16.0)),
                margin_right: Some(px(16.0)),
                margin_bottom: Some(px(8.0)),
                ..Default::default()
            }) { "INVITE EXPIRY".to_string() }
            for (i, name) in INVITE_EXPIRY_OPTIONS.iter().enumerate(), key = i {
                // `i` is the loop index (compile-time repeat key); the closure
                // only *reads* `i` and `value` (both `Copy`), so it stays
                // `Fn` and the built `StyleRules` is a fresh value each time.
                view(style = move || {
                    let sel = i == value.get();
                    StyleRules {
                        height: Some(px(44.0)),
                        margin_left: Some(px(12.0)),
                        margin_right: Some(px(12.0)),
                        padding_left: Some(px(12.0)),
                        padding_right: Some(px(12.0)),
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        gap: Some(px(10.0)),
                        background: Some(col(if sel { theme::BG_SELECTED } else { "transparent" })),
                        ..rounded(8.0)
                    }
                }) {
                    // Radio indicator: a bordered circle that FILLS when this
                    // row is selected (background = accent) and stays a hollow
                    // ring otherwise (transparent background).
                    view(style = move || StyleRules {
                        width: Some(px(16.0)),
                        height: Some(px(16.0)),
                        border_top_width: Some(border_width(2.0)),
                        border_bottom_width: Some(border_width(2.0)),
                        border_left_width: Some(border_width(2.0)),
                        border_right_width: Some(border_width(2.0)),
                        border_top_color: Some(col(if i == value.get() {
                            theme::TEXT_STRONG
                        } else {
                            theme::TEXT_MUTED
                        })),
                        border_bottom_color: Some(col(if i == value.get() {
                            theme::TEXT_STRONG
                        } else {
                            theme::TEXT_MUTED
                        })),
                        border_left_color: Some(col(if i == value.get() {
                            theme::TEXT_STRONG
                        } else {
                            theme::TEXT_MUTED
                        })),
                        border_right_color: Some(col(if i == value.get() {
                            theme::TEXT_STRONG
                        } else {
                            theme::TEXT_MUTED
                        })),
                        background: Some(col(if i == value.get() {
                            theme::TEXT_STRONG
                        } else {
                            "transparent"
                        })),
                        ..rounded(8.0)
                    }) {}
                    text(style = move || StyleRules {
                        font_size: Some(px(15.0)),
                        font_weight: Some(if i == value.get() {
                            FontWeight::SemiBold
                        } else {
                            FontWeight::Normal
                        }),
                        color: Some(col(if i == value.get() {
                            theme::TEXT_STRONG
                        } else {
                            theme::TEXT_MUTED
                        })),
                        ..Default::default()
                    }) { name.to_string() }
                }.on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        value.set(i);
                    }
                    TouchResponse::CONSUMED
                })
            }
        }
    }
}
