//! `profile_settings` — a minimal ProfileSettings screen: a display-name text
//! input, a circular avatar placeholder (colored circle + the first letter of
//! the name), and a status-message text input. All three fields are bound to
//! local component state — no backend call, no navigation wiring, no
//! validation. (The fuller "User Settings" overlay lives in
//! `profile_settings.rs::user_settings`; this is the small screen.)
//!
//! Deliberately not wired into navigation yet — that is a separate follow-up.
//! Declared in `lib.rs` so it compiles as part of the crate, but referenced by
//! nothing (the same shape the `user_settings` overlay ships in).

use runtime_core::{
    component, ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent, StyleRules,
    Tokenized,
    signal,
};
use crate::style_helpers::{col, px};

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
    let display_name = signal("Alex".to_string());
    let status_message = signal("Available".to_string());

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
        ..Default::default()
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
