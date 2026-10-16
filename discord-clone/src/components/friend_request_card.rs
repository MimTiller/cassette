//! `friend_request_card` — a self-contained "friend request" card for the
//! Discord clone. It renders one hardcoded local sender name and two buttons:
//! `"Accept"` and `"Decline"`. Pressing either button sets a local `status`
//! signal (`"accepted"` / `"declined"`); once set, the two buttons are
//! replaced by a text line showing the status.
//!
//! Everything is hardcoded local state — no backend call, no props. Wired
//! into the `member_profile_panel` overlay in `app.rs`, opened by tapping a
//! member row. Declared in `lib.rs` so it compiles as part of the
//! crate, referenced by `member_profile_panel` (the same shape
//! `dm_request_card` / `icon_change_confirm` / `transfer_ownership_confirm`
//! ship in).
//!
//! Idioms, verified against `components/dm_request_card.rs` and
//! `components/icon_change_confirm.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the buttons are the `button(label, on_click)` primitive with prebuilt
//!   `move` closures passed by variable name — `Signal` is `Copy`, so each
//!   closure captures a copy of the same signal and stays `Fn`;
//! - the buttons/status swap is a node-level reactive
//!   `if let Some(s) = status.get() { … } else { … }` inside `ui!`, which
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
const ACCEPT: &str = "#248046"; // accept button (Discord green)
const DECLINE: &str = "#4e5058"; // decline button (Discord muted gray)

/// The friend request card. Renders the member's friend-request sender name
/// (the `sender` prop, defaulting to the previous shared literal) with an
/// `Accept` and a `Decline` button. Each button sets the local `status`
/// signal; once set, the buttons are replaced by a text line showing the
/// status (`"accepted"` / `"declined"`).
#[component]
#[allow(non_snake_case)]
pub fn FriendRequestCard(#[prop(default = "Fen")] sender: String) -> Element {
    // The sender name arrives as a prop (never `.set`), so read it once into
    // an owned String and bake it as static text.
    let sender_name = sender.get().to_string();
    // Local status state: `None` until a button is pressed. Both button
    // closures and the reactive `if let` below read/write the same signal.
    let status = signal(Option::<String>::None);

    // Each button sets the same underlying signal (`Signal` is `Copy`, so the
    // `move` closures capture a copy and remain `Fn`). No backend call.
    let accept = move || status.set(Some("accepted".to_string()));
    let decline = move || status.set(Some("declined".to_string()));

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
            // The sender name — primary text, semibold. Written INLINE because
            // `StyleRules` is not `Copy` (a shared `let` would be moved on its
            // first use).
            text(style = StyleRules {
                font_size: Some(px(18.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(TEXT)),
                ..Default::default()
            }) { sender_name }

            // Reactive swap: `if let Some(s) = status.get() {…} else {…}`
            // lowers to a `when` (mounts the chosen branch, unmounts the
            // other when a button sets the signal). The styles are inline so
            // nothing is captured across the `when` boundary (`StyleRules`
            // is not `Copy`).
            if let Some(s) = status.get() {
                text(style = StyleRules {
                    font_size: Some(px(15.0)),
                    color: Some(col(TEXT)),
                    margin_top: Some(px(4.0)),
                    ..Default::default()
                }) { s.clone() }
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
                        label = "Decline".to_string(),
                        on_click = decline,
                        style = StyleRules {
                            height: Some(px(30.0)),
                            padding_left: Some(px(16.0)),
                            padding_right: Some(px(16.0)),
                            background: Some(col(DECLINE)),
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
