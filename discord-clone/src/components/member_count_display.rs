//! `member_count_display` — a self-contained "member count" line for the
//! Discord clone. Renders two hardcoded local numbers (online count, total
//! count) as a single plain-text line — "12 Online / 48 Members" — preceded
//! by a small green status dot before the online count.
//!
//! The two numbers arrive as inline props with `#[prop(default = …)]`
//! starting values (12 / 48), so the component compiles standalone and can be
//! dropped into any screen without a call site supplying values. No backend
//! call, no wiring into any screen yet — declared in `lib.rs` so it compiles
//! as part of the crate, but referenced by nothing (the same shape
//! `server_stats_card` / `server_discovery_card` ship in).
//!
//! Idioms, each verified against this crate's `components/server_stats_card.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors;
//! - the prop values are `i64` data params, so `#[component]` wraps them
//!   `Reactive<i64>`; they read `.get()` into locals and `.to_string()` into
//!   the `text` child;
//! - the green dot is a plain sized `view` (width/height + border radius).
//!   No border is used, so none of the `border_*_width` fields — which take
//!   `Tokenized<f32>`, not `px()` — come into play.

use crate::style_helpers::{col, px};
use runtime_core::{
    component, ui, AlignItems, Element, FlexDirection, FontWeight, StyleRules,
};

// Discord dark palette (kept local so the component is self-contained).
// `GREEN_ONLINE` matches the `Status::Online` dot color in `data.rs`.
#[allow(dead_code)]
const TEXT: &str = "#f2f3f5";
#[allow(dead_code)]
const GREEN_ONLINE: &str = "#23a559";

/// The member-count line. Renders a small green status dot followed by a
/// single plain-text string of the form "12 Online / 48 Members". The two
/// numbers are inline props with hardcoded defaults so the component compiles
/// standalone.
#[component]
#[allow(non_snake_case)]
pub fn MemberCountDisplay(
    #[prop(default = 12)] online_count: i64,
    #[prop(default = 48)] total_count: i64,
) -> Element {
    let online = online_count.get();
    let total = total_count.get();

    let label = format!("{} Online / {} Members", online, total);

    let dot_style = StyleRules {
        width: Some(px(8.0)),
        height: Some(px(8.0)),
        background: Some(col(GREEN_ONLINE)),
        border_top_left_radius: Some(px(4.0)),
        border_top_right_radius: Some(px(4.0)),
        border_bottom_left_radius: Some(px(4.0)),
        border_bottom_right_radius: Some(px(4.0)),
        ..Default::default()
    };

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(6.0)),
            ..Default::default()
        }) {
            view(style = dot_style) {}
            text(style = StyleRules {
                font_size: Some(px(13.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(TEXT)),
                ..Default::default()
            }) { label.to_string() }
        }
    }
}
