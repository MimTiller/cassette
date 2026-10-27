//! `notification_permission_prompt` — a self-contained notification
//! permission prompt for the Discord clone. It renders one hardcoded
//! prompt line — `"Enable notifications?"` — plus two buttons:
//! `"Not Now"` and `"Enable"`. Pressing either button sets a local
//! `result` signal (`"dismissed"` / `"enabled"`); once set, the two
//! buttons are replaced by a text line showing the result.
//!
//! Everything is hardcoded local state — no backend call, no props, no
//! wiring into any screen yet. Declared in `lib.rs` so it compiles as part
//! of the crate, but referenced by nothing (the same shape
//! `transfer_ownership_confirm` / `translate_toggle` ship in).
//!
//! Idioms, verified against `components/transfer_ownership_confirm.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors;
//! - the buttons are the `button(label, on_click)` primitive with prebuilt
//!   `move` closures passed by variable name — `Signal` is `Copy`, so each
//!   closure captures a copy of the same signal and stays `Fn`;
//! - the buttons/result swap is a node-level reactive
//!   `if let Some(r) = result.get() { … } else { … }` inside `ui!`, which
//!   lowers to the `when` primitive (mounts the chosen branch, unmounts the
//!   other when a button sets the signal). `StyleRules` is not `Copy`, so
//!   all styles are written INLINE (created fresh per evaluation, never
//!   captured across the `when` boundary).

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, JustifyContent, StyleRules,
};

// Discord dark palette (kept local so the component is self-contained).
const BG_CARD: &str = "#2b2d31"; // card surface (Discord sidebar gray)
const TEXT: &str = "#f2f3f5"; // primary text
const PROMPT: &str = "#f0b232"; // prompt line (Discord warning yellow)
const DISMISS: &str = "#4e5058"; // "Not Now" button (Discord muted gray)
const ENABLE: &str = "#5865f2"; // "Enable" button (Discord brand blurple)

/// The notification permission prompt. Renders the hardcoded line
/// `"Enable notifications?"` with a `Not Now` and an `Enable` button.
/// Each button sets the local `result` signal; once set, the buttons are
/// replaced by a text line showing the result (`"dismissed"` /
/// `"enabled"`).
#[component]
#[allow(dead_code)] // declared but not wired into any screen yet
pub fn NotificationPermissionPrompt() -> Element {
    // Local result state: `None` until a button is pressed. Both button
    // closures and the reactive `if let` below read/write the same signal.
    let result = signal(Option::<String>::None);

    // Each button sets the same underlying signal (`Signal` is `Copy`, so the
    // `move` closures capture a copy and remain `Fn`). No backend call.
    let not_now = move || result.set(Some("dismissed".to_string()));
    let enable = move || result.set(Some("enabled".to_string()));

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Center),
            gap: Some(px(14.0)),
            padding_top: Some(px(20.0)),
            padding_bottom: Some(px(20.0)),
            padding_left: Some(px(20.0)),
            padding_right: Some(px(20.0)),
            background: Some(col(BG_CARD)),
            border_top_left_radius: Some(px(8.0)),
            border_top_right_radius: Some(px(8.0)),
            border_bottom_left_radius: Some(px(8.0)),
            border_bottom_right_radius: Some(px(8.0)),
            ..Default::default()
        }) {
            // The hardcoded prompt line — a local literal, no backend call.
            text(style = StyleRules {
                font_size: Some(px(18.0)),
                color: Some(col(PROMPT)),
                ..Default::default()
            }) { "Enable notifications?" }

            // Reactive swap: `if let Some(r) = result.get() {…} else {…}`
            // lowers to a `when` (mounts the chosen branch, unmounts the
            // other when a button sets the signal). The styles are inline so
            // nothing is captured across the `when` boundary (`StyleRules`
            // is not `Copy`).
            if let Some(r) = result.get() {
                text(style = StyleRules {
                    font_size: Some(px(16.0)),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { r.clone() }
            } else {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Row),
                    gap: Some(px(12.0)),
                    ..Default::default()
                }) {
                    button(
                        label = "Not Now".to_string(),
                        on_click = not_now,
                        style = StyleRules {
                            height: Some(px(30.0)),
                            padding_left: Some(px(16.0)),
                            padding_right: Some(px(16.0)),
                            background: Some(col(DISMISS)),
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
                    button(
                        label = "Enable".to_string(),
                        on_click = enable,
                        style = StyleRules {
                            height: Some(px(30.0)),
                            padding_left: Some(px(16.0)),
                            padding_right: Some(px(16.0)),
                            background: Some(col(ENABLE)),
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
}
