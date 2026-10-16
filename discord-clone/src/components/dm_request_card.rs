//! `dm_request_card` — a self-contained "direct-message request" card for the
//! Discord clone. It renders one hardcoded local sender name and one short
//! hardcoded message-preview line, plus two buttons: `"Accept"` and `"Block"`.
//! Pressing either button sets a local `result` signal (`"accepted"` /
//! `"blocked"`); once set, the two buttons are replaced by a text line showing
//! the result.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape
//! `transfer_ownership_confirm` / `server_discovery_card` / `activity_card`
//! ship in).
//!
//! Idioms, verified against `components/transfer_ownership_confirm.rs` and
//! `components/server_discovery_card.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the buttons are the `button(label, on_click)` primitive with prebuilt
//!   `move` closures passed by variable name — `Signal` is `Copy`, so each
//!   closure captures a copy of the same signal and stays `Fn`;
//! - the buttons/result swap is a node-level reactive
//!   `if let Some(r) = result.get() { … } else { … }` inside `ui!`, which
//!   lowers to the `when` primitive (mounts the chosen branch, unmounts the
//!   other when a button sets the signal). `StyleRules` is not `Copy`, so all
//!   styles are written INLINE (created fresh per evaluation, never captured
//!   across the `when` boundary).

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent,
    StyleRules,
};

// Discord dark palette (kept local so the component is self-contained).
const BG_CARD: &str = "#2b2d31"; // card surface (Discord sidebar gray)
const TEXT: &str = "#f2f3f5"; // primary text
const TEXT_MUTED: &str = "#949ba4"; // muted preview text
const ACCEPT: &str = "#248046"; // accept button (Discord green)
const BLOCK: &str = "#da373c"; // block button (Discord danger red)

/// The hardcoded sender name (no props for now, no member lookup).
const SENDER_NAME: &str = "Skye";
/// The short hardcoded message-preview line (no props for now).
const PREVIEW: &str = "hey, are you still on tonight?";

/// The direct-message request card. Renders the hardcoded sender name and
/// message preview with an `Accept` and a `Block` button. Each button sets the
/// local `result` signal; once set, the buttons are replaced by a text line
/// showing the result (`"accepted"` / `"blocked"`).
#[component]
pub fn DmRequestCard() -> Element {
    // Local result state: `None` until a button is pressed. Both button
    // closures and the reactive `if let` below read/write the same signal.
    let result = signal(Option::<String>::None);

    // Each button sets the same underlying signal (`Signal` is `Copy`, so the
    // `move` closures capture a copy and remain `Fn`). No backend call.
    let accept = move || result.set(Some("accepted".to_string()));
    let block = move || result.set(Some("blocked".to_string()));

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            gap: Some(px(10.0)),
            padding_top: Some(px(16.0)),
            padding_bottom: Some(px(16.0)),
            padding_left: Some(px(16.0)),
            padding_right: Some(px(16.0)),
            background: Some(col(BG_CARD)),
            border_top_left_radius: Some(px(10.0)),
            border_top_right_radius: Some(px(10.0)),
            border_bottom_left_radius: Some(px(10.0)),
            border_bottom_right_radius: Some(px(10.0)),
            ..Default::default()
        }) {
            // The hardcoded sender name — primary text, semibold. Written
            // INLINE because `StyleRules` is not `Copy` (a shared `let` would
            // be moved on its first use).
            text(style = StyleRules {
                font_size: Some(px(18.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(TEXT)),
                ..Default::default()
            }) { SENDER_NAME.to_string() }

            // The short hardcoded message preview — muted text.
            text(style = StyleRules {
                font_size: Some(px(13.0)),
                color: Some(col(TEXT_MUTED)),
                ..Default::default()
            }) { PREVIEW.to_string() }

            // Reactive swap: `if let Some(r) = result.get() {…} else {…}`
            // lowers to a `when` (mounts the chosen branch, unmounts the
            // other when a button sets the signal). The styles are inline so
            // nothing is captured across the `when` boundary (`StyleRules`
            // is not `Copy`).
            if let Some(r) = result.get() {
                text(style = StyleRules {
                    font_size: Some(px(15.0)),
                    color: Some(col(TEXT)),
                    margin_top: Some(px(4.0)),
                    ..Default::default()
                }) { r.clone() }
            } else {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Row),
                    gap: Some(px(12.0)),
                    margin_top: Some(px(6.0)),
                    ..Default::default()
                }) {
                    button(
                        label = "Accept".to_string(),
                        on_click = accept,
                        style = StyleRules {
                            height: Some(px(30.0)),
                            padding_left: Some(px(16.0)),
                            padding_right: Some(px(16.0)),
                            background: Some(col(ACCEPT)),
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
                        label = "Block".to_string(),
                        on_click = block,
                        style = StyleRules {
                            height: Some(px(30.0)),
                            padding_left: Some(px(16.0)),
                            padding_right: Some(px(16.0)),
                            background: Some(col(BLOCK)),
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
