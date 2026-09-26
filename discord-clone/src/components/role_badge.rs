//! `role_badge` — a self-contained role pill for the Discord clone. Renders a
//! single small pill-shaped badge whose label is a hardcoded local role name
//! ("Moderator") and whose background fill is a hardcoded local role color
//! ("#5865f2", Discord blurple).
//!
//! Everything is hardcoded local data — no backend call, no props, no signals,
//! no wiring into any screen yet. Declared in `lib.rs` so it compiles as part of
//! the crate, but referenced by nothing (the same shape `member_role_list` /
//! `verified_badge` / `pin_count_badge` ship in).
//!
//! Idioms, each verified against this crate's `components/member_role_list.rs`
//! (the app's canonical role-pill shape) and `components/pin_count_badge.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - there is NO signal anywhere, so both the label and the fill are STATIC
//!   values from local `const`s — the label is a `&str` const baked to
//!   `TextSource::Static`, and the fill is `col(ROLE_COLOR)` on the pill's
//!   inline `StyleRules`. No reactive `when`, no `.get()` reads;
//! - the pill is a fully-rounded `view` + child `text` (the exact role-pill
//!   shape `member_role_list` draws): a fixed-height lozenge with a solid
//!   role-colored background and inner padding, holding one small centered
//!   white label; the radius is half the fixed height, giving a clean pill;
//! - `StyleRules` is not `Copy`, so the pill's and the text's styles are
//!   written INLINE in `style = …` (created fresh, never shared as a `let`
//!   binding) — the crate's universal style idiom.

#![allow(dead_code)]

use crate::style_helpers::{col, px};
use runtime_core::{
    component, ui, AlignItems, Element, FlexDirection, FontWeight, StyleRules,
};

// Discord role-badge convention: each role carries its own accent color (the
// blurple here is the same `#5865f2` the `verified_badge` check uses). Both
// values are hardcoded local `const`s so the badge is self-contained.
const ROLE_NAME: &str = "Moderator";
const ROLE_COLOR: &str = "#5865f2";
const PILL_TEXT: &str = "#ffffff"; // white label, readable on the role fill

/// The role badge. A small pill-shaped badge: the hardcoded role name rendered
/// in white on the hardcoded role color. Purely local — renders the hardcoded
/// values above, no backend fetch, no reactivity.
#[component]
#[allow(non_snake_case)]
pub fn RoleBadge() -> Element {
    ui! {
        // The pill: a fixed-height, fully-rounded `view` (radius = half the
        // height) with a solid role-colored fill — the exact lozenge shape
        // `member_role_list` draws per role. `StyleRules` is not `Copy`, so the
        // style is written inline (created fresh, never captured).
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            height: Some(px(22.0)),
            padding_left: Some(px(10.0)),
            padding_right: Some(px(10.0)),
            background: Some(col(ROLE_COLOR)),
            border_top_left_radius: Some(px(11.0)),
            border_top_right_radius: Some(px(11.0)),
            border_bottom_left_radius: Some(px(11.0)),
            border_bottom_right_radius: Some(px(11.0)),
            ..Default::default()
        }) {
            // The role-name label: a static `&str` const, so it bakes to
            // `TextSource::Static` (no reactive machinery). White SemiBold on
            // the role fill — the same label treatment `member_role_list` gives
            // each of its role pills.
            text(style = StyleRules {
                font_size: Some(px(12.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(PILL_TEXT)),
                ..Default::default()
            }) { ROLE_NAME.to_string() }
        }
    }
}
