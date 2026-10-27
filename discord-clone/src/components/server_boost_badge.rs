//! `server_boost_badge` — a self-contained server boost-level badge for the
//! Discord clone. Renders a single small pill-shaped badge whose label is
//! "Level {n}" for a boost level `{n}` and whose background fill is a distinct
//! color per level (0-3) — the compact pill Discord shows next to a server to
//! signal its boost tier.
//!
//! The boost level arrives as an inline prop with `#[prop(default = 2)]`, so
//! the component compiles standalone and can be dropped into any screen
//! without a call site supplying a value. No backend call, no wiring into any
//! screen yet — declared in `lib.rs` so it compiles as part of the crate, but
//! referenced by nothing (the same shape `role_badge` / `pin_count_badge` /
//! `server_stats_card` ship in).
//!
//! Idioms, each verified against this crate's `components/role_badge.rs` (the
//! app's canonical single role-pill shape) and `components/server_stats_card.rs`
//! (the inline-prop idiom):
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the boost level is an inline `i32` data prop, so `#[component]` wraps it
//!   `Reactive<i32>`; it reads `.get()` in the fn body (not inside `ui!`),
//!   clamps it into 0-3, and selects the per-level fill from a local `const`
//!   array;
//! - nothing reactive is *rendered* — the fill and the label are STATIC values
//!   derived from the prop at build time, baked to `TextSource::Static` / an
//!   inline `col(…)`;
//! - the pill is a fully-rounded `view` + child `text` (the exact role-pill
//!   shape `role_badge` draws): a fixed-height lozenge with a solid
//!   level-colored background and inner padding, holding one small centered
//!   white label; the radius is half the fixed height, giving a clean pill;
//! - `StyleRules` is not `Copy`, so the pill's and the text's styles are
//!   written INLINE in `style = …` (created fresh, never shared as a `let`
//!   binding) — the crate's universal style idiom.

#![allow(dead_code)]

use crate::theme;
use crate::style_helpers::{col, px};
use runtime_core::{
    component, ui, AlignItems, Element, FlexDirection, FontWeight, StyleRules,
};

// Per-level fill colors, levels 0-3. A muted-to-vibrant "increasing prestige"
// ramp: unboosted gray up through blurple, green, and a gold for the top tier.
// Every fill is dark enough for the white label to stay readable. Kept local
// so the component is self-contained.
const LEVEL_COLORS: [&str; 4] = [theme::TEXT_FAINT, theme::ACCENT, "#3ba55c", theme::WARNING];

/// The server boost-level badge. A small pill: "Level {n}" rendered in white on
/// a distinct per-level background (0-3). The boost level is an inline prop
/// with a hardcoded default so the component compiles standalone — no backend
/// fetch, no reactivity beyond the prop itself.
#[component]
#[allow(non_snake_case)]
pub fn ServerBoostBadge(#[prop(default = 2)] boost_level: i32) -> Element {
    // Clamp the level into 0-3 (saturating, so an out-of-range level takes the
    // nearest tier's color rather than panicking) and select its fill. The
    // label is built from the same resolved integer. Both reads happen in the
    // fn body, not inside `ui!`, so they are plain Rust — the same shape
    // `server_stats_card` uses for its `.get()` reads.
    let level = boost_level.get().clamp(0, 3) as usize;
    let fill = LEVEL_COLORS[level];
    let label = format!("Level {level}");

    ui! {
        // The pill: a fixed-height, fully-rounded `view` (radius = half the
        // height) with a solid per-level fill — the exact lozenge shape
        // `role_badge` draws. `StyleRules` is not `Copy`, so the style is
        // written inline (created fresh, never captured).
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            height: Some(px(22.0)),
            padding_left: Some(px(10.0)),
            padding_right: Some(px(10.0)),
            background: Some(col(fill)),
            border_top_left_radius: Some(px(11.0)),
            border_top_right_radius: Some(px(11.0)),
            border_bottom_left_radius: Some(px(11.0)),
            border_bottom_right_radius: Some(px(11.0)),
            ..Default::default()
        }) {
            // The "Level {n}" label: a static `String` built above from the
            // resolved level, so it bakes to `TextSource::Static`. White
            // SemiBold on the level fill — the same label treatment
            // `role_badge` gives its role pill.
            text(style = StyleRules {
                font_size: Some(px(12.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(theme::TEXT_ON_ACCENT)),
                ..Default::default()
            }) { label }
        }
    }
}
