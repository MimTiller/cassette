//! `invite_link_display` — a self-contained invite-link card for the Discord
//! clone. Shows the shareable link for a real invite code in a read-only text
//! field beside a `Copy` button. Pressing `Copy` sets a local `copied` boolean
//! signal to `true`, which reactively reveals a `"Copied!"` confirmation line —
//! the boolean exists purely for that display, with no clipboard access and no
//! backend call.
//!
//! The code is a `String` prop (the `#[component]` inline-props idiom — the
//! macro rewrites it to `Reactive<String>`); the displayed link is derived as
//! `discord.gg/{code}`. Everything else is local: no backend call, no `Copy`
//! persistence (the `Copied!` line is display-only).
//!
//! Idioms, each verified against this crate:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the field is the `text_input(value, style)` canonical `ui!` tag
//!   (lowercase primitive). `value` is a controlled signal (`Signal<String>`
//!   coerces to `IntoValue<String>`); the framework's `text_input` has no
//!   `readonly` prop (only `secure`), so "read-only" here means the component
//!   never writes the `link` signal after construction;
//! - the `Copy` button is the `button(label, on_click, style)` primitive with
//!   a prebuilt `move` closure (`ban_reason_input.rs:60`). `Signal` is `Copy`,
//!   so the closure captures a copy of `copied` and stays `Fn` across presses;
//! - the `"Copied!"` line is a reactive `if copied.get() { … }` inside `ui!`
//!   (lowered to the `when` primitive). `StyleRules` is not `Copy`, so its
//!   style is written inline in `style = …` (fresh per evaluation).

use crate::style_helpers::{borderless_input, col, px};
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

/// The invite-link card. `code` is the real invite code (the redeemable
/// handle) shown as `discord.gg/{code}` in a read-only field above a `Copy`
/// button; pressing `Copy` reveals a `"Copied!"` confirmation. No backend call.
#[component]
#[allow(non_snake_case)]
pub fn InviteLinkDisplay(code: String) -> Element {
    // The shareable link, derived from the real code prop. Built as a signal
    // (never written after construction) so the field reads as read-only in
    // practice — the component's only writer is this initial value.
    let link = signal(format!("discord.gg/{}", code.get()));
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
            // always the real invite link.
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
                    ..borderless_input()
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
