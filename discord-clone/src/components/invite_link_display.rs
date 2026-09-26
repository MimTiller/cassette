//! `invite_link_display` — a self-contained invite-link card for the Discord
//! clone. Shows a hardcoded local invite link (`"discord.gg/abc123"`) in a
//! read-only text field beside a `Copy` button. Pressing `Copy` sets a local
//! `copied` boolean signal to `true`, which reactively reveals a `"Copied!"`
//! confirmation line — the boolean exists purely for that display, with no
//! clipboard access and no backend call.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `custom_status_input` /
//! `verified_badge` / `jump_to_bottom_button` ship in).
//!
//! Idioms, each verified against this crate:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the field is the `text_input(value, style)` canonical `ui!` tag
//!   (lowercase primitive), exactly as `custom_status_input.rs:81` /
//!   `app.rs` use it. `value` is the controlled signal (`Signal<String>`
//!   coerces to `IntoValue<String>`). The framework's `text_input` has no
//!   `readonly` prop (only `secure`), so "read-only" here means the
//!   component never writes the `link` signal — the field's only other
//!   writer would be native typing, and this is a display-only field;
//! - the `Copy` button is the `button(label, on_click, style)` primitive
//!   with a prebuilt `move` closure (`ban_reason_input.rs:60`). `Signal` is
//!   `Copy`, so the closure captures a copy of `copied` and stays `Fn`
//!   across every press;
//! - the `"Copied!"` line is a reactive `if copied.get() { … }` inside
//!   `ui!`. An `if` whose condition reads a signal lowers to the `when`
//!   primitive (mounts while true, unmounts on false). `StyleRules` is not
//!   `Copy`, so its style is written INLINE in `style = …` (created fresh
//!   per evaluation, never captured across the `when` boundary) — the same
//!   shape `verified_badge` / `ban_reason_input` use for their reactive
//!   branch. No `else`: while unset, the line renders nothing.

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent,
    StyleRules,
};

// Discord dark palette (kept local so the component is self-contained).
const BG_CARD: &str = "#2b2d31"; // card surface (Discord sidebar gray)
const INPUT_BG: &str = "#1e1f22"; // field surface (Discord input gray)
const BTN_BG: &str = "#5865f2"; // Copy button (Discord blurple)
const TEXT: &str = "#f2f3f5"; // primary text (link + button label)
const TEXT_MUTED: &str = "#949ba4"; // header + "Copied!" line (Discord secondary)

/// The invite-link card. Renders a read-only field pre-filled with the
/// hardcoded link `"discord.gg/abc123"` above a `Copy` button; pressing
/// `Copy` reveals a `"Copied!"` confirmation. No backend call, no props —
/// local state only.
#[component]
#[allow(non_snake_case)]
pub fn InviteLinkDisplay() -> Element {
    // The invite link. Hardcoded and never written after construction — that
    // is what makes the field read-only in practice (see the `text_input`
    // note in the module docs).
    let link = signal("discord.gg/abc123".to_string());
    // Flipped by the Copy button for display only — no clipboard access.
    let copied = signal(false);

    // `Copy`: mark the link as copied. `copied` is `Copy`, so the `move`
    // closure captures a copy of the same underlying signal and remains `Fn`
    // on every press. No backend call.
    let on_copy = move || copied.set(true);

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Stretch),
            gap: Some(px(12.0)),
            padding_top: Some(px(20.0)),
            padding_bottom: Some(px(20.0)),
            padding_left: Some(px(20.0)),
            padding_right: Some(px(20.0)),
            background: Some(col(BG_CARD)),
            border_top_left_radius: Some(px(8.0)),
            border_top_right_radius: Some(px(8.0)),
            border_bottom_left_radius: Some(px(8.0)),
            border_bottom_right_radius: Some(px(8.0)),
            ..Default::default()
        }) {
            text(style = StyleRules {
                font_size: Some(px(13.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(TEXT_MUTED)),
                ..Default::default()
            }) { "INVITE LINK".to_string() }
            // The read-only field: a controlled `text_input` bound to `link`.
            // `link` is never written by this component, so the value shown is
            // always the hardcoded invite link.
            text_input(
                value = link,
                style = StyleRules {
                    height: Some(px(40.0)),
                    padding_left: Some(px(12.0)),
                    padding_right: Some(px(12.0)),
                    background: Some(col(INPUT_BG)),
                    color: Some(col(TEXT)),
                    font_size: Some(px(15.0)),
                    border_top_left_radius: Some(px(8.0)),
                    border_top_right_radius: Some(px(8.0)),
                    border_bottom_left_radius: Some(px(8.0)),
                    border_bottom_right_radius: Some(px(8.0)),
                    ..Default::default()
                },
            )
            button(
                label = "Copy".to_string(),
                on_click = on_copy,
                style = StyleRules {
                    height: Some(px(32.0)),
                    padding_left: Some(px(16.0)),
                    padding_right: Some(px(16.0)),
                    background: Some(col(BTN_BG)),
                    color: Some(col(TEXT)),
                    font_size: Some(px(14.0)),
                    border_top_left_radius: Some(px(6.0)),
                    border_top_right_radius: Some(px(6.0)),
                    border_bottom_left_radius: Some(px(6.0)),
                    border_bottom_right_radius: Some(px(6.0)),
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    ..Default::default()
                },
            )
            // Reactive confirmation: `if copied.get() {…}` lowers to a `when`
            // whose render is an `Fn` closure, so the (non-`Copy`) style is
            // written inline, fresh on every evaluation. No `else` branch —
            // while `copied` is unset the line renders nothing.
            if copied.get() {
                text(style = StyleRules {
                    font_size: Some(px(13.0)),
                    color: Some(col(TEXT_MUTED)),
                    ..Default::default()
                }) { "Copied!".to_string() }
            }
        }
    }
}
