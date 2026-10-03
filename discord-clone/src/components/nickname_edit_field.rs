#![allow(dead_code)]
//! `nickname_edit_field` — a self-contained "edit your nickname" field for the
//! Discord clone. It renders a `text_input` bound to a local `nickname` signal
//! (default `"Alex"`) above a row of two buttons:
//!
//! - **Save** — sets a local `saved` boolean signal to `true` for a brief
//!   moment (a one-shot timer flips it back to `false`), and while it is true
//!   a `"Saved!"` confirmation line renders beneath the buttons. No value is
//!   actually persisted and there is no backend call — the flash is purely
//!   local feedback.
//! - **Reset** — restores the input to its default value (`"Alex"`).
//!
//! Everything is hardcoded local state — no backend call, no props. Wired
//! into the `member_profile_panel` overlay in `app.rs`, opened by tapping a
//! member row. Declared in `lib.rs` so it compiles as part of the
//! crate, referenced by `member_profile_panel` (the same shape `soundboard_button` /
//! `ban_reason_input` / `icon_change_confirm` ship in).
//!
//! Idioms, each verified against this crate:
//! - the text input is the `text_input(value = …, on_change = …,
//!   placeholder = …, style = …)` primitive — exactly the composer wiring in
//!   `dm_view.rs`. `value` is a `Signal<String>` (cloned, since the `on_change`
//!   closure also captures a copy); `on_change` is a plain `move |s: String|
//!   nickname.set(s)` closure that pushes every native edit back into the same
//!   signal. `Signal` is `Copy`, so the closure stays `Fn` across edits.
//! - the brief "Saved!" flash is `after_ms_detached` — the framework's
//!   documented choice for event handlers that run outside any component body
//!   ("imperative, off-scope call sites — event handlers"). It owns the timer
//!   in a runtime registry (no `ScheduledTask` to keep alive, nothing to
//!   cancel), so the `Save` closure stays a plain `Fn` with no handle plumbing
//!   (exactly the `playing` flash + revert in `soundboard_button`).
//! - the "Saved!" line is a node-level reactive single-branch
//!   `if saved.get() { … }` inside `ui!` — an `if` whose condition reads a
//!   signal (`.get()`) desugars to the `when` primitive (mounts the branch when
//!   true, unmounts it when the signal flips back). Its `StyleRules` are written
//!   INLINE (created fresh per evaluation) because `StyleRules` is not `Copy`
//!   and must not be captured across the `when` boundary; the button/input
//!   styles live OUTSIDE the `when`, so they are plain `let` locals.
//! - the input carries no `border_*_width` (those fields take
//!   `Tokenized<f32>`, not `px()`) — it uses a flat background + corner radius
//!   instead, the same look as the input in `profile_settings`.

use crate::style_helpers::{col, px};
use runtime_core::{
    after_ms_detached, component, signal, ui, AlignItems, Element, FlexDirection, JustifyContent,
    StyleRules,
};

// How long the "Saved!" confirmation lingers before it disappears on its own.
const SAVED_MS: i32 = 1500;

// Discord palette (kept local so the component is self-contained).
const BG_MAIN: &str = "#313338"; // screen background
const COMPOSER: &str = "#383a40"; // input surface
const TEXT: &str = "#f2f3f5"; // primary text (near-white)
const BLURPLE: &str = "#5865f2"; // Save button (Discord accent)
const MUTED: &str = "#4e5058"; // Reset button (subtle gray)
const SAVED: &str = "#57f287"; // "Saved!" confirmation (Discord online green)

/// A self-contained nickname editor. A `text_input` starts at the member's
/// nickname (`default_nickname` prop) and is bound to local `nickname` state;
/// **Save** flashes a local `saved` boolean (rendering a `"Saved!"` line for a
/// moment via a one-shot timer) and **Reset** returns the input to the default
/// nickname. No backend call, no navigation wiring yet.
#[component]
#[allow(non_snake_case)]
pub fn NicknameEditField(#[prop(default = "Alex")] default_nickname: String) -> Element {
    // The member's display nickname seeds the local input signal (and is what
    // Reset restores to). `default_nickname` is a prop read once into an owned
    // String at build; the signal is the live state after that.
    let default_nickname = default_nickname.get();
    let nickname = signal(default_nickname.to_string());
    let saved = signal(false);

    // Every native edit is pushed straight back into the same signal. `nickname`
    // is `Copy`, so this closure captures a copy and stays `Fn` on each edit.
    let on_change = move |s: String| nickname.set(s);

    // Save: light the "Saved!" state and schedule its brief revert. `saved` is
    // `Copy`, so the outer closure captures a copy of the signal (the one the
    // reactive `if` below reads) and the inner timer closure captures its own.
    let on_save = move || {
        saved.set(true);
        after_ms_detached(SAVED_MS, move || saved.set(false));
    };

    // Reset: restore the input to the default nickname (the member's
    // `default_nickname` prop). `nickname` is `Copy`, so this closure
    // captures a copy and stays `Fn`.
    let on_reset = move || nickname.set(default_nickname.to_string());

    // Static styles for the nodes OUTSIDE the reactive `when` — plain locals
    // (created once). The "Saved!" style is written inline below because it
    // sits across the `when` boundary and `StyleRules` is not `Copy`.
    let input_style = StyleRules {
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        height: Some(px(40.0)),
        padding_left: Some(px(12.0)),
        padding_right: Some(px(12.0)),
        background: Some(col(COMPOSER)),
        color: Some(col(TEXT)),
        font_size: Some(px(15.0)),
        border_top_left_radius: Some(px(6.0)),
        border_top_right_radius: Some(px(6.0)),
        border_bottom_left_radius: Some(px(6.0)),
        border_bottom_right_radius: Some(px(6.0)),
        ..Default::default()
    };

    let save_style = StyleRules {
        height: Some(px(32.0)),
        padding_left: Some(px(18.0)),
        padding_right: Some(px(18.0)),
        background: Some(col(BLURPLE)),
        border_top_left_radius: Some(px(6.0)),
        border_top_right_radius: Some(px(6.0)),
        border_bottom_left_radius: Some(px(6.0)),
        border_bottom_right_radius: Some(px(6.0)),
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        ..Default::default()
    };

    let reset_style = StyleRules {
        height: Some(px(32.0)),
        padding_left: Some(px(18.0)),
        padding_right: Some(px(18.0)),
        background: Some(col(MUTED)),
        border_top_left_radius: Some(px(6.0)),
        border_top_right_radius: Some(px(6.0)),
        border_bottom_left_radius: Some(px(6.0)),
        border_bottom_right_radius: Some(px(6.0)),
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        ..Default::default()
    };

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::Center),
            background: Some(col(BG_MAIN)),
            padding_top: Some(px(24.0)),
            padding_bottom: Some(px(24.0)),
            padding_left: Some(px(20.0)),
            padding_right: Some(px(20.0)),
            gap: Some(px(14.0)),
            ..Default::default()
        }) {
            text(style = StyleRules {
                font_size: Some(px(15.0)),
                color: Some(col(TEXT)),
                ..Default::default()
            }) { "Nickname".to_string() }

            text_input(
                value = nickname.clone(),
                on_change = on_change,
                placeholder = "Nickname",
                style = input_style,
            )

            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(10.0)),
                ..Default::default()
            }) {
                button(label = "Save".to_string(), on_click = on_save, style = save_style)
                button(label = "Reset".to_string(), on_click = on_reset, style = reset_style)
            }

            // Reactive single-branch `if saved.get() { … }` desugars to a `when`
            // (mounts the line while `saved` is true, unmounts it when the timer
            // flips the signal back to false). The style is INLINE (created fresh
            // per evaluation) because `StyleRules` is not `Copy` and must not be
            // captured across the `when` boundary.
            if saved.get() {
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    color: Some(col(SAVED)),
                    ..Default::default()
                }) { "Saved!".to_string() }
            }
        }
    }
}
