//! `profile_settings` — the Profile screen: a display-name text input seeded
//! from the signed-in account (the real backend username from
//! `crate::auth::current_username()`, falling back to the mock `USER` identity
//! in guest mode), a circular avatar (colored circle + the first letter of the
//! name), and a status-message text input. Opened as a full-screen overlay by
//! the "Profile" pill in the user panel (`app.rs::user_panel`).

use runtime_core::{
    component, ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent, StyleRules,
    Tokenized,
    signal,
};
use crate::style_helpers::{borderless_input, col, px};

// The same Discord dark palette as `app.rs`, kept local so the screen is
// self-contained.
const BG_MAIN: &str = "#313338";
const TEXT: &str = "#f2f3f5";
const COMPOSER: &str = "#383a40";
/// Blurple avatar tint (the signed-in user's chip color).
const AVATAR: &str = "#5865f2";

fn grow(v: f32) -> Tokenized<f32> {
    Tokenized::Literal(v)
}

/// The ProfileSettings screen. `display_name`, `avatar`, and `status_message`
/// are local reactive state — each field edits its own signal, and the avatar
/// letter tracks the display name reactively.
#[component]
pub fn profile_settings() -> Element {
    // Real signed-in identity: the backend username recorded at login in
    // `CURRENT_USERNAME` (see `crate::auth`), the same source the user panel
    // reads (`app.rs::user_panel`). Guest mode leaves it empty, so fall back to
    // the mock `USER` identity there. This component is rebuilt when the
    // `authed` gate flips on login, so a plain read here captures the real name.
    let real_name = crate::auth::current_username();
    let display_name = signal(if !real_name.is_empty() {
        real_name
    } else {
        crate::data::USER.name.to_string()
    });
    // Status message: no backend field yet, so seed from the real presence
    // status vocabulary (`Status::Online.name()` = "Online") rather than a
    // mock placeholder.
    let status_message = signal(crate::data::Status::Online.name().to_string());

    // The avatar's first letter, recomputed whenever the name changes.
    let avatar_letter = {
        let name = display_name.clone();
        move || {
            name.get()
                .chars()
                .next()
                .map(|c| c.to_ascii_uppercase())
                .unwrap_or('?')
                .to_string()
        }
    };

    let input_style = StyleRules {
        flex_grow: Some(grow(1.0)),
        height: Some(px(40.0)),
        padding_left: Some(px(12.0)),
        padding_right: Some(px(12.0)),
        background: Some(col(COMPOSER)),
        color: Some(col(TEXT)),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        ..borderless_input()
    };

    let avatar_style = StyleRules {
        width: Some(px(72.0)),
        height: Some(px(72.0)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        background: Some(col(AVATAR)),
        color: Some(col(TEXT)),
        font_size: Some(px(30.0)),
        font_weight: Some(FontWeight::Bold),
        border_top_left_radius: Some(px(36.0)),
        border_top_right_radius: Some(px(36.0)),
        border_bottom_left_radius: Some(px(36.0)),
        border_bottom_right_radius: Some(px(36.0)),
        ..Default::default()
    };

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Center),
            background: Some(col(BG_MAIN)),
            padding_top: Some(px(24.0)),
            padding_left: Some(px(20.0)),
            padding_right: Some(px(20.0)),
            gap: Some(px(16.0)),
            ..Default::default()
        }) {
            text(style = avatar_style) { avatar_letter }
            text_input(
                value = display_name.clone(),
                placeholder = "Display name",
                style = input_style.clone(),
            )
            text_input(
                value = status_message,
                placeholder = "Status message",
                style = input_style,
            )
        }
    }
}
