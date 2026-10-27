//! `permission_override_row` — a single Discord channel-permission override row.
//! It shows one hardcoded local permission name ("Send Messages") and a row of
//! exactly three override buttons — "Allow", "Neutral", "Deny" — bound to a
//! local `state` signal (an index into the three options, starting on Neutral,
//! which is Discord's default for a fresh override). Tapping any button sets
//! `state` to that button's index; the tapped button then renders as the ACTIVE
//! override (filled accent + ring) while the other two render INACTIVE (dim
//! pill + muted ring).
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `mic_selector` /
//! `translate_toggle` / `welcome_banner` ship in).
//!
//! Idioms, each verified against this crate's `components/mic_selector.rs` /
//! `components/translate_toggle.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (unitless border widths via a local `border_width` helper,
//!   the same `Tokenized<f32>` shape `mic_selector` uses);
//! - the three buttons are the `button(label, on_click, style)` primitive,
//!   authored with a `for (i, name) in OVERRIDES.iter().enumerate()` inside
//!   `ui!` (a `repeat` whose per-item builder is an `Fn` closure, the loop
//!   index as the compile-time key — the exact shape `mic_selector` uses for
//!   its two hardcoded options);
//! - `on_click` is a plain `move || state.set(i)` closure. `state` is
//!   `Signal` (`Copy`) and `i` is `usize` (`Copy`), so the closure captures a
//!   copy of the same underlying signal (the one the reactive style below also
//!   reads) and a copy of the index, and stays `Fn` (callable on every press);
//! - the active/inactive highlight is REACTIVE, so each button's style is a
//!   `style = move || StyleRules { … }` closure (the `IntoStyleProp for
//!   Fn() -> StyleRules` dynamic path `mic_selector` uses on its rows), reading
//!   `state.get()` at re-evaluation time rather than being baked once. Because
//!   the per-button builder is a closure, the `StyleRules` is a fresh value
//!   each evaluation (never a captured `let` binding — `StyleRules` is not
//!   `Copy`).

use crate::theme;
use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, JustifyContent, StyleRules,
    Tokenized,
};

// Discord dark palette — kept local so the component is self-contained.

// The one hardcoded local permission — no backend fetch.
const PERMISSION: &str = "Send Messages";

// The three override options, in display order. The local `state` signal holds
// an index into this array; index 1 (Neutral) is the default.
const OVERRIDES: [&str; 3] = ["Allow", "Neutral", "Deny"];

/// A unitless `Tokenized<f32>` border width (the `border_*_width` style fields
/// take a plain `f32` token, not a `Length`) — same shape `mic_selector` uses.
fn border_width(px: f32) -> Tokenized<f32> {
    Tokenized::Literal(px)
}

/// A permission-override row. Renders the hardcoded `PERMISSION` name above a
/// row of three override buttons; the button whose index matches the local
/// `state` signal renders active (filled + ringed) and the other two render
/// inactive (dim pill + muted ring). No props, no backend.
#[component]
#[allow(dead_code)] // declared but not wired into any screen yet
pub fn PermissionOverrideRow() -> Element {
    // Which override is active: an index into `OVERRIDES`. Starts on Neutral
    // (index 1) — Discord's default for a freshly added override. Local only.
    let state = signal(1usize);

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            gap: Some(px(10.0)),
            padding_top: Some(px(16.0)),
            padding_bottom: Some(px(16.0)),
            padding_left: Some(px(16.0)),
            padding_right: Some(px(16.0)),
            background: Some(col(theme::BG_PRIMARY)),
            border_top_left_radius: Some(px(10.0)),
            border_top_right_radius: Some(px(10.0)),
            border_bottom_left_radius: Some(px(10.0)),
            border_bottom_right_radius: Some(px(10.0)),
            ..Default::default()
        }) {
            // The hardcoded permission name.
            text(style = StyleRules {
                font_size: Some(px(16.0)),
                color: Some(col(theme::TEXT_STRONG)),
                ..Default::default()
            }) { PERMISSION.to_string() }

            for (i, name) in OVERRIDES.iter().enumerate(), key = i {
                // `i` is the loop index (compile-time repeat key). Both the
                // `on_click` and the reactive `style` closure only *read* `i`
                // and *write/read* `state` (both `Copy`), so each stays `Fn`
                // and the built `StyleRules` is a fresh value every evaluation.
                button(
                    label = name.to_string(),
                    on_click = move || state.set(i),
                    style = move || {
                        let active = i == state.get();
                        StyleRules {
                            height: Some(px(34.0)),
                            padding_left: Some(px(16.0)),
                            padding_right: Some(px(16.0)),
                            flex_direction: Some(FlexDirection::Row),
                            align_items: Some(AlignItems::Center),
                            justify_content: Some(JustifyContent::Center),
                            background: Some(col(if active {
                                theme::ACCENT
                            } else {
                                theme::BG_SELECTED
                            })),
                            border_top_left_radius: Some(px(8.0)),
                            border_top_right_radius: Some(px(8.0)),
                            border_bottom_left_radius: Some(px(8.0)),
                            border_bottom_right_radius: Some(px(8.0)),
                            border_top_width: Some(border_width(if active {
                                1.5
                            } else {
                                1.0
                            })),
                            border_right_width: Some(border_width(if active {
                                1.5
                            } else {
                                1.0
                            })),
                            border_bottom_width: Some(border_width(if active {
                                1.5
                            } else {
                                1.0
                            })),
                            border_left_width: Some(border_width(if active {
                                1.5
                            } else {
                                1.0
                            })),
                            border_top_color: Some(col(if active {
                                theme::ACCENT
                            } else {
                                theme::BUTTON_SECONDARY
                            })),
                            border_right_color: Some(col(if active {
                                theme::ACCENT
                            } else {
                                theme::BUTTON_SECONDARY
                            })),
                            border_bottom_color: Some(col(if active {
                                theme::ACCENT
                            } else {
                                theme::BUTTON_SECONDARY
                            })),
                            border_left_color: Some(col(if active {
                                theme::ACCENT
                            } else {
                                theme::BUTTON_SECONDARY
                            })),
                            ..Default::default()
                        }
                    }
                )
            }
        }
    }
}
