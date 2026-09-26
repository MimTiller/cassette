//! `delete_account_warning` — a self-contained delete-account confirmation
//! warning for the Discord clone. It renders one hardcoded warning line —
//! `"This cannot be undone."` (a local literal, no lookup, no backend call)
//! — above two buttons: `"Cancel"` and `"Delete My Account"`. Pressing either
//! button sets a local `result` signal (`"cancelled"` / `"confirmed"`); once
//! set, the two buttons are replaced by a text line showing the result.
//!
//! Everything is hardcoded local state — no backend call, no props, no
//! wiring into any screen yet. Declared in `lib.rs` so it compiles as part
//! of the crate, but referenced by nothing (the same shape
//! `delete_message_confirm` / `kick_confirm_dialog` / `icon_change_confirm`
//! ship in).
//!
//! Idioms, verified against `components/delete_message_confirm.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors, written INLINE (created fresh per evaluation, never
//!   captured across the `when` boundary);
//! - the buttons are the `button(label, on_click)` primitive with prebuilt
//!   `move` closures passed by variable name — `Signal` is `Copy`, so each
//!   closure captures a copy of the same signal and stays `Fn`;
//! - the buttons/result swap is a node-level reactive
//!   `if let Some(r) = result.get() { … } else { … }` inside `ui!`, which
//!   lowers to the `when` primitive (mounts the chosen branch, unmounts the
//!   other when a button sets the signal).

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, JustifyContent, StyleRules,
};

// Discord dark palette (kept local so the component is self-contained).
const BG_CARD: &str = "#2b2d31"; // card surface (Discord sidebar gray)
const TEXT: &str = "#f2f3f5"; // primary text
const WARNING: &str = "#f0b232"; // warning line (Discord warning yellow)
const CANCEL: &str = "#4e5058"; // cancel button (Discord muted gray)
const DELETE: &str = "#da373c"; // delete button (Discord danger red)

/// The hardcoded warning line shown above the buttons. Local literal only —
/// no lookup, no backend call.
const WARNING_TEXT: &str = "This cannot be undone.";

/// The delete-account warning. Renders the hardcoded `"This cannot be
/// undone."` warning with a `Cancel` and a `Delete My Account` button. Each
/// button sets the local `result` signal; once set, the buttons are replaced
/// by a text line showing the result (`"cancelled"` / `"confirmed"`).
#[component]
pub fn DeleteAccountWarning() -> Element {
    // Local result state: `None` until a button is pressed. Both button
    // closures and the reactive `if let` below read/write the same signal.
    let result = signal(Option::<String>::None);

    // Each button sets the same underlying signal (`Signal` is `Copy`, so the
    // `move` closures capture a copy and remain `Fn`). No backend call.
    let cancel = move || result.set(Some("cancelled".to_string()));
    let delete = move || result.set(Some("confirmed".to_string()));

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
            // The hardcoded warning line — a local literal, no lookup, no
            // backend call.
            text(style = StyleRules {
                font_size: Some(px(16.0)),
                color: Some(col(WARNING)),
                ..Default::default()
            }) { WARNING_TEXT.to_string() }

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
                }) { r }
            } else {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Row),
                    gap: Some(px(12.0)),
                    ..Default::default()
                }) {
                    button(
                        label = "Cancel".to_string(),
                        on_click = cancel,
                        style = StyleRules {
                            height: Some(px(30.0)),
                            padding_left: Some(px(16.0)),
                            padding_right: Some(px(16.0)),
                            background: Some(col(CANCEL)),
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
                        label = "Delete My Account".to_string(),
                        on_click = delete,
                        style = StyleRules {
                            height: Some(px(30.0)),
                            padding_left: Some(px(16.0)),
                            padding_right: Some(px(16.0)),
                            background: Some(col(DELETE)),
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
