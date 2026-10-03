//! `activity_card` — a self-contained "rich presence" activity card for the
//! Discord clone. Renders hardcoded local activity text (a status verb, e.g.
//! "Playing") and detail text (the game/app title) stacked vertically, with a
//! small colored square icon placeholder to the left.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `server_discovery_card` /
//! `idle_timer_badge` / `server_folder_group` / `profile_settings` ship in).
//!
//! Idioms, each verified against this crate's `app.rs` /
//! `profile_settings.rs` / `components/server_discovery_card.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the icon placeholder is an empty `view` with a fixed square size and a
//!   solid background (a `view` with no children is a legal leaf — the same
//!   shape `dm_view` uses for its presence `dot`);
//! - the two text lines share a muted look; `StyleRules` is not `Copy`, so
//!   the detail style is written INLINE in `style = …` (a shared `let` would
//!   be moved on its first use) — the same shape `server_discovery_card` uses
//!   for its two static meta lines.

use crate::style_helpers::{col, px};
use runtime_core::{component, ui, AlignItems, Element, FlexDirection, FontWeight, StyleRules};

// Discord dark palette (kept local so the component is self-contained).
const BG_CARD: &str = "#2b2d31";
const ICON: &str = "#5865f2";
const TEXT: &str = "#f2f3f5";
const TEXT_MUTED: &str = "#949ba4";

/// The activity card. A small colored square icon placeholder on the left,
/// and a two-line stack on the right: the status verb (muted) above the title
/// (primary). Both arrive as props (with hardcoded defaults — the previous
/// single shared literals — so the component compiles standalone).
#[component]
#[allow(non_snake_case)]
pub fn ActivityCard(
    #[prop(default = "Playing")] activity: String,
    #[prop(default = "Some Game Title")] detail: String,
) -> Element {
    // Props are never `.set`, so read them once into owned Strings and bake
    // them as static text — the same shape `channel_topic_banner` uses for its
    // `topic` prop.
    let verb = activity.get().to_string();
    let title = detail.get().to_string();
    let root_style = StyleRules {
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        gap: Some(px(10.0)),
        padding_top: Some(px(12.0)),
        padding_right: Some(px(12.0)),
        padding_bottom: Some(px(12.0)),
        padding_left: Some(px(12.0)),
        background: Some(col(BG_CARD)),
        border_top_left_radius: Some(px(10.0)),
        border_top_right_radius: Some(px(10.0)),
        border_bottom_left_radius: Some(px(10.0)),
        border_bottom_right_radius: Some(px(10.0)),
        ..Default::default()
    };

    // The small colored square icon placeholder. A fixed 16x16 square with a
    // solid background; an empty `view` is a legal leaf (the same shape
    // `dm_view` uses for its presence dot).
    let icon_style = StyleRules {
        width: Some(px(16.0)),
        height: Some(px(16.0)),
        background: Some(col(ICON)),
        border_top_left_radius: Some(px(4.0)),
        border_top_right_radius: Some(px(4.0)),
        border_bottom_left_radius: Some(px(4.0)),
        border_bottom_right_radius: Some(px(4.0)),
        ..Default::default()
    };

    let stack_style = StyleRules {
        flex_direction: Some(FlexDirection::Column),
        gap: Some(px(2.0)),
        ..Default::default()
    };

    let activity_style = StyleRules {
        font_size: Some(px(13.0)),
        color: Some(col(TEXT_MUTED)),
        ..Default::default()
    };

    ui! {
        view(style = root_style) {
            view(style = icon_style) {}
            view(style = stack_style) {
                text(style = activity_style) { verb.clone() }
                // The title: primary color, semibold. Written INLINE in
                // `style = …` because `StyleRules` is not `Copy` (a shared
                // `let` would be moved by the activity line above).
                text(style = StyleRules {
                    font_size: Some(px(15.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { title }
            }
        }
    }
}
