//! `featured_server_banner` — a self-contained "featured server" banner for
//! the Discord clone. Renders a hardcoded local server name and tagline over a
//! solid colored background block, with a "View" button that sets a local
//! `viewed` boolean signal (hardcoded `false` at start). Once `viewed` is set,
//! a "Viewing!" text renders in place of the button.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `server_discovery_card` /
//! `welcome_banner` / `idle_timer_badge` ship in).
//!
//! Idioms, each verified against this crate's `components/server_discovery_card.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the view control is the `button(label, on_click)` primitive; `on_click`
//!   is a plain `move` closure prebuilt in a `let` and passed by variable name
//!   (exactly the `server_discovery_card` "Join" button); `Signal` is `Copy`,
//!   so the `move` closure captures a copy of the signal and stays `Fn`
//!   (callable on every press);
//! - the control is a reactive `if !viewed.get() { button … } else {
//!   text "Viewing!" }` inside `ui!`. An `if`/`else` whose condition reads a
//!   signal (`.get()`) lowers to the `when` primitive (mounts the chosen
//!   branch, swaps on toggle) — the same reactive `if`
//!   `server_discovery_card` uses for its button portion. Because a
//!   `StyleRules` literal is not `Copy`, both branch styles are written
//!   INLINE in `style = …` (created fresh per evaluation, never captured
//!   across the `when` boundary).

use crate::theme;
use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent,
    StyleRules,
};

// Discord dark palette (kept local so the component is self-contained).

/// The hardcoded server name (no props for now).
const SERVER_NAME: &str = "Indie Devs";
/// The hardcoded tagline.
const TAGLINE: &str = "Build cool things together";

/// The featured-server banner. Renders the hardcoded name / tagline over a
/// solid colored block; while the local `viewed` signal is unset a "View"
/// button shows (pressing it sets `viewed`), and once `viewed` is set the
/// "Viewing!" text renders in its place.
#[component]
pub fn FeaturedServerBanner() -> Element {
    let viewed = signal(false);

    // View: flip the `viewed` signal to `true` on press. `viewed` is `Copy`,
    // so the `move` closure captures a copy of the same underlying signal
    // (the one the reactive `if` below also reads) and remains `Fn`.
    let view = move || viewed.set(true);

    ui! {
        // The solid colored background block: a rounded blurple banner that
        // holds the name, tagline, and the view control.
        view(style = StyleRules {
            width: Some(px(320.0)),
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Stretch),
            gap: Some(px(8.0)),
            padding_top: Some(px(18.0)),
            padding_right: Some(px(16.0)),
            padding_bottom: Some(px(16.0)),
            padding_left: Some(px(16.0)),
            background: Some(col(theme::ACCENT)),
            border_top_left_radius: Some(px(10.0)),
            border_top_right_radius: Some(px(10.0)),
            border_bottom_left_radius: Some(px(10.0)),
            border_bottom_right_radius: Some(px(10.0)),
            ..Default::default()
        }) {
            // The hardcoded server name.
            text(style = StyleRules {
                font_size: Some(px(18.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(theme::TEXT_ON_ACCENT)),
                ..Default::default()
            }) { SERVER_NAME.to_string() }
            // The hardcoded tagline — a slightly muted second line.
            text(style = StyleRules {
                font_size: Some(px(13.0)),
                color: Some(col(theme::TEXT_NORMAL)),
                ..Default::default()
            }) { TAGLINE.to_string() }
            // Reactive branch: `if !viewed.get() {…} else {…}` lowers to a
            // `when` whose per-branch render is an `Fn` closure. `StyleRules`
            // is not `Copy`, so both branch styles are written INLINE in
            // `style = …` (created fresh per evaluation, never captured
            // across the `when` boundary) — the same shape
            // `server_discovery_card` uses for its button portion.
            if !viewed.get() {
                button(
                    label = "View".to_string(),
                    on_click = view,
                    style = StyleRules {
                        height: Some(px(36.0)),
                        margin_top: Some(px(6.0)),
                        background: Some(col(theme::TEXT_ON_ACCENT)),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        border_top_left_radius: Some(px(8.0)),
                        border_top_right_radius: Some(px(8.0)),
                        border_bottom_left_radius: Some(px(8.0)),
                        border_bottom_right_radius: Some(px(8.0)),
                        ..Default::default()
                    }
                )
            } else {
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    color: Some(col(theme::TEXT_ON_ACCENT)),
                    margin_top: Some(px(6.0)),
                    ..Default::default()
                }) { "Viewing!".to_string() }
            }
        }
    }
}
