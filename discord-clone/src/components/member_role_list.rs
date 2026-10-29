//! `member_role_list` — a self-contained "member + roles" row for the Discord
//! clone. Renders a hardcoded local member name followed by a small pill badge
//! for each of exactly two hardcoded local role names.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `link_embed_preview` /
//! `activity_card` / `idle_timer_badge` / `server_folder_group` /
//! `profile_settings` ship in).
//!
//! Idioms, each verified against this crate's `app.rs` /
//! `components/rules_accept_checkbox.rs` / `components/activity_card.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the row is a single horizontal `view` (`flex-direction: Row`,
//!   `align-items: Center`) whose children are the name `text` followed by the
//!   role pills, so the name reads left-to-right "followed by" its roles;
//! - each role is a small pill: a rounded `view` with a solid role-colored
//!   background and inner padding, holding one small centered `text`. `StyleRules`
//!   is not `Copy`, so each pill's style is built inline inside the loop body
//!   (created fresh per iteration, never captured across the loop boundary);
//! - the pills come from a keyed `for (idx, &(role, color)) in ROLES.iter()
//!   .enumerate(), key = idx` over the static `[(&str, &str); 2]` slice — the
//!   same `for` + `key = idx` idiom `rules_accept_checkbox` uses for its
//!   static rule lines. A real iterator (`.iter()`) is required, hence `.iter()`.

use crate::theme;
use crate::style_helpers::{col, px, rounded};
use runtime_core::{component, ui, AlignItems, Element, FlexDirection, FontWeight, StyleRules};

// Discord dark palette (kept local so the component is self-contained).

/// The hardcoded member name (no props for now).
const MEMBER: &str = "Mia Chen";
/// The hardcoded local role list — EXACTLY two `(name, pill color)` pairs.
/// The color is Discord's role-badge convention (each role carries its own
/// accent), kept here as a static pair so the badge is self-contained.
const ROLES: [(&str, &str); 2] = [
    ("Admin", theme::ACCENT),    // blurple
    ("Moderator", theme::SUCCESS), // green
];

/// The member-role row. A horizontal row: the member name (primary) followed
/// by a small rounded pill per role (role-colored background, white label).
/// Purely local — renders the hardcoded strings above.
#[component]
pub fn MemberRoleList() -> Element {
    let root_style = StyleRules {
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        gap: Some(px(8.0)),
        ..Default::default()
    };

    let name_style = StyleRules {
        font_size: Some(px(15.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(theme::TEXT_STRONG)),
        ..Default::default()
    };

    ui! {
        view(style = root_style) {
            text(style = name_style) { MEMBER.to_string() }
            // One small pill per role, from a keyed `for` over the static
            // `(name, color)` slice — the standard static-row idiom
            // (`rules_accept_checkbox`). `for` needs a real iterator, hence
            // `.iter()`. Each pill's style is built inline (fresh per
            // iteration — `StyleRules` is not `Copy`, so nothing is captured
            // across the loop boundary); `color` comes from the static pair.
            for (idx, &(role, color)) in ROLES.iter().enumerate(), key = idx {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    padding_top: Some(px(2.0)),
                    padding_right: Some(px(8.0)),
                    padding_bottom: Some(px(2.0)),
                    padding_left: Some(px(8.0)),
                    background: Some(col(color)),
                    ..rounded(10.0)
                }) {
                    text(style = StyleRules {
                        font_size: Some(px(11.0)),
                        font_weight: Some(FontWeight::SemiBold),
                        color: Some(col(theme::TEXT_ON_ACCENT)),
                        ..Default::default()
                    }) { role.to_string() }
                }
            }
        }
    }
}
