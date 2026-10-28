//! `server_discovery_card` — a self-contained "server discovery" card for the
//! Discord clone. Shows a hardcoded local server name, member count, and short
//! description in a card, with a "Join" button that sets a local `joined`
//! boolean signal (hardcoded `false` at start). Once `joined` is set, the
//! "Joined!" text renders in place of the button.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `idle_timer_badge` /
//! `server_folder_group` / `edited_label` / `profile_settings` ship in).
//!
//! Idioms, each verified against this crate's `app.rs` /
//! `profile_settings.rs` / `components/idle_timer_badge.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the join control is the `button(label, on_click)` primitive; `on_click`
//!   is a plain `move` closure prebuilt in a `let` and passed by variable name
//!   (exactly the `Send` button in `app.rs`); `Signal` is `Copy`, so the
//!   `move` closure captures a copy of the signal and stays `Fn` (callable on
//!   every press);
//! - the join control is a reactive `if !joined.get() { button … } else {
//!   text "Joined!" }` inside `ui!`. An `if`/`else` whose condition reads a
//!   signal (`.get()`) lowers to the `when` primitive (mounts the chosen
//!   branch, swaps on toggle) — the same reactive `if`
//!   `idle_timer_badge` uses for its badge portion. Because a `StyleRules`
//!   literal is not `Copy`, both branch styles are written INLINE in
//!   `style = …` (created fresh per evaluation, never captured across the
//!   `when` boundary) — the same shape `app.rs` / `idle_timer_badge` use.

use crate::theme;
use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent,
    StyleRules,
};

// Discord dark palette (kept local so the component is self-contained).

/// The hardcoded server name (no props for now).
const SERVER_NAME: &str = "Gamers United";
/// The hardcoded member count.
const MEMBER_COUNT: &str = "1.2k members";
/// The hardcoded short description.
const DESCRIPTION: &str = "A place for gamers";

/// The server-discovery card. Renders the hardcoded name / member count /
/// description; while the local `joined` signal is unset a "Join" button
/// shows (pressing it sets `joined`), and once `joined` is set the "Joined!"
/// text renders in its place.
#[component]
pub fn ServerDiscoveryCard() -> Element {
    let joined = signal(false);

    // Join: flip the `joined` signal to `true` on press. `joined` is `Copy`,
    // so the `move` closure captures a copy of the same underlying signal
    // (the one the reactive `if` below also reads) and remains `Fn`.
    let join = move || joined.set(true);

    let root_style = StyleRules {
        flex_direction: Some(FlexDirection::Column),
        align_items: Some(AlignItems::Stretch),
        gap: Some(px(8.0)),
        padding_top: Some(px(16.0)),
        padding_right: Some(px(16.0)),
        padding_bottom: Some(px(16.0)),
        padding_left: Some(px(16.0)),
        background: Some(col(theme::BG_SECONDARY)),
        border_top_left_radius: Some(px(10.0)),
        border_top_right_radius: Some(px(10.0)),
        border_bottom_left_radius: Some(px(10.0)),
        border_bottom_right_radius: Some(px(10.0)),
        ..Default::default()
    };

    let name_style = StyleRules {
        font_size: Some(px(18.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(theme::TEXT_STRONG)),
        ..Default::default()
    };

    ui! {
        view(style = root_style) {
            text(style = name_style) { SERVER_NAME.to_string() }
            // Two static siblings share the same muted meta look; `StyleRules`
            // is not `Copy`, so each is written INLINE in `style = …` (a shared
            // `let` would be moved on its first use) — the same shape the
            // `for` loop in `server_folder_group` uses for per-row styles.
            text(style = StyleRules {
                font_size: Some(px(13.0)),
                color: Some(col(theme::TEXT_MUTED)),
                ..Default::default()
            }) { MEMBER_COUNT.to_string() }
            text(style = StyleRules {
                font_size: Some(px(13.0)),
                color: Some(col(theme::TEXT_MUTED)),
                ..Default::default()
            }) { DESCRIPTION.to_string() }
            // Reactive branch: `if !joined.get() {…} else {…}` lowers to a
            // `when` whose per-branch render is an `Fn` closure. `StyleRules`
            // is not `Copy`, so both branch styles are written INLINE in
            // `style = …` (created fresh per evaluation, never captured
            // across the `when` boundary) — the same shape `app.rs` /
            // `idle_timer_badge` use for their reactive subtrees.
            if !joined.get() {
                button(
                    label = "Join".to_string(),
                    on_click = join,
                    style = StyleRules {
                        height: Some(px(36.0)),
                        margin_top: Some(px(4.0)),
                        background: Some(col(theme::ACCENT)),
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
                    color: Some(col(theme::TEXT_STRONG)),
                    margin_top: Some(px(4.0)),
                    ..Default::default()
                }) { "Joined!".to_string() }
            }
        }
    }
}
