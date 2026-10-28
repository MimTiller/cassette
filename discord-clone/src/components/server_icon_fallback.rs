//! `server_icon_fallback` — a self-contained server-icon fallback for the
//! Discord clone. Renders a colored circle showing the initials of a
//! hardcoded local server name ("Game Devs United" -> "GD"): the first letter
//! of up to the first two words. This is the glyph shown in place of a
//! server's uploaded icon.
//!
//! Everything is hardcoded local data — no backend call, no props. Wired
//! into the `member_profile_panel` overlay in `app.rs`, opened by tapping a
//! member row. Declared in `lib.rs` so it compiles as part of the
//! crate, referenced by `member_profile_panel` (the same shape `server_discovery_card`
//! / `welcome_banner` / `verified_badge` ship in).
//!
//! Idioms, each verified against this crate's `server_discovery_card.rs` /
//! `welcome_banner.rs` / `dm_view.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the circle is a `view` with a `background` and all four
//!   `border_*_radius` corners set to half the size (the `dm_view.rs`
//!   avatar-chip idiom), with the initials centered via `flex_direction` +
//!   `align_items` / `justify_content`;
//! - the initials are a computed `String` local dropped straight into the
//!   `ui!` `text` slot (a `String` satisfies the text-slot `Display` bound —
//!   the same shape `welcome_banner.rs` uses for its `format!` greeting).

use crate::theme;
use crate::style_helpers::{col, px};
use runtime_core::{
    component, ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent, StyleRules,
};

// Discord dark palette (kept local so the component is self-contained).

// The hardcoded local server name — no backend fetch.
const SERVER_NAME: &str = "Game Devs United";

/// First letter of up to the first two whitespace-separated words.
/// "Game Devs United" -> "GD"; "Blitz" -> "B"; "" -> "".
fn initials_of(name: &str) -> String {
    name.split_whitespace()
        .take(2)
        .filter_map(|word| word.chars().next())
        .collect()
}

/// The server-icon fallback. A colored circle showing `SERVER_NAME`'s
/// initials (the first letter of up to the first two words).
#[component]
pub fn ServerIconFallback() -> Element {
    let initials = initials_of(SERVER_NAME);

    ui! {
        view(style = StyleRules {
            width: Some(px(48.0)),
            height: Some(px(48.0)),
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::Center),
            background: Some(col(theme::ACCENT)),
            border_top_left_radius: Some(px(24.0)),
            border_top_right_radius: Some(px(24.0)),
            border_bottom_left_radius: Some(px(24.0)),
            border_bottom_right_radius: Some(px(24.0)),
            ..Default::default()
        }) {
            text(style = StyleRules {
                font_size: Some(px(18.0)),
                font_weight: Some(FontWeight::Bold),
                color: Some(col(theme::TEXT_STRONG)),
                ..Default::default()
            }) { initials }
        }
    }
}
