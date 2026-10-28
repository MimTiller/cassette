//! `server_stats_card` — a self-contained "server stats" card for the Discord
//! clone. Renders three hardcoded local numbers (total members, online now,
//! messages today) as a row of three small stat blocks, each with its own
//! label.
//!
//! The three numbers arrive as inline props with `#[prop(default = …)]`
//! starting values (512 / 48 / 1204), so the component compiles standalone
//! and can be dropped into any screen without a call site supplying values.
//! No backend call, no wiring into any screen yet — declared in `lib.rs` so
//! it compiles as part of the crate, but referenced by nothing (the same
//! shape `server_discovery_card` / `server_folder_group` ship in).
//!
//! Idioms, each verified against this crate's `components/server_discovery_card.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors;
//! - the three stat blocks are a plain `for` loop inside `ui!` over a local
//!   `[(label, value); 3]`, lowered to a `repeat` whose per-item builder is an
//!   `Fn` closure — so the per-block styles are inlined as `StyleRules`
//!   literals rather than shared `let` bindings (a captured, non-`Copy`
//!   binding would be moved on the first iteration);
//! - the prop values are `i64` data params, so `#[component]` wraps them
//!   `Reactive<i64>`; they read `.get()` and `.to_string()` into `text`
//!   children.

use crate::theme;
use crate::style_helpers::{col, px};
use runtime_core::{
    component, ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent, StyleRules,
    Tokenized,
};

// Discord dark palette (kept local so the component is self-contained).

/// The server-stats card. Renders a row of three small stat blocks (total
/// members, online now, messages today), each with its own label. The three
/// numbers are inline props with hardcoded defaults so the component
/// compiles standalone.
#[component]
pub fn ServerStatsCard(
    #[prop(default = 512)] total_members: i64,
    #[prop(default = 48)] online_now: i64,
    #[prop(default = 1204)] messages_today: i64,
) -> Element {
    let total = total_members.get();
    let online = online_now.get();
    let messages = messages_today.get();

    // The three stat blocks, in order. Each tuple carries the label plus the
    // (already-resolved) value so the `for` loop below only has to render.
    let stats: [(&str, i64); 3] = [
        ("Total members", total),
        ("Online now", online),
        ("Messages today", messages),
    ];

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Stretch),
            gap: Some(px(8.0)),
            padding_top: Some(px(12.0)),
            padding_right: Some(px(12.0)),
            padding_bottom: Some(px(12.0)),
            padding_left: Some(px(12.0)),
            background: Some(col(theme::BG_SECONDARY)),
            border_top_left_radius: Some(px(10.0)),
            border_top_right_radius: Some(px(10.0)),
            border_bottom_left_radius: Some(px(10.0)),
            border_bottom_right_radius: Some(px(10.0)),
            ..Default::default()
        }) {
            for (i, (label, value)) in stats.iter().enumerate(), key = i {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Column),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    flex_grow: Some(Tokenized::Literal(1.0)),
                    gap: Some(px(4.0)),
                    padding_top: Some(px(10.0)),
                    padding_bottom: Some(px(10.0)),
                    background: Some(col(theme::BG_PRIMARY)),
                    border_top_left_radius: Some(px(8.0)),
                    border_top_right_radius: Some(px(8.0)),
                    border_bottom_left_radius: Some(px(8.0)),
                    border_bottom_right_radius: Some(px(8.0)),
                    ..Default::default()
                }) {
                    text(style = StyleRules {
                        font_size: Some(px(20.0)),
                        font_weight: Some(FontWeight::SemiBold),
                        color: Some(col(theme::TEXT_STRONG)),
                        ..Default::default()
                    }) { value.to_string() }
                    text(style = StyleRules {
                        font_size: Some(px(11.0)),
                        color: Some(col(theme::TEXT_MUTED)),
                        ..Default::default()
                    }) { (*label).to_string() }
                }
            }
        }
    }
}
