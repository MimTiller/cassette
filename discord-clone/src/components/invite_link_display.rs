//! `invite_link_display` — a self-contained invite-link card for the Discord
//! clone. Shows a real invite code in a read-only text
//! field beside a `Copy` button. Pressing `Copy` sets a local `copied` boolean
//! signal to `true`, which reactively reveals a `"Copied!"` confirmation line —
//! the boolean exists purely for that display, with no clipboard access and no
//! backend call.
//!
//! The code is a `String` prop (the `#[component]` inline-props idiom — the
//! macro rewrites it to `Reactive<String>`) and is shown as it is.
//! Everything else is local: no backend call, no `Copy`
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

use crate::theme;
use crate::style_helpers::{borderless_input, col, px, rounded, with_radius};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent,
    StyleRules,
};

// Discord dark palette (kept local so the component is self-contained).

/// The invite card. `code` is the real invite code (the redeemable handle),
/// shown in a read-only field above a `Copy` button with a line saying where
/// to paste it; pressing `Copy` reveals a `"Copied!"` confirmation.
#[component]
pub fn InviteLinkDisplay(code: String) -> Element {
    // What gets shared: the code itself, which Join a Server takes (it also
    // accepts a pasted link ending in the code). Built as a signal (never
    // written after construction) so the field reads as read-only in practice.
    let link = signal(code.get());
    // Flipped by the Copy button for display only — no clipboard access.
    let copied = signal(false);

    // `Copy`: write the link to the system clipboard for real, then reveal the
    // `Copied!` line. The clipboard SDK (`clipboard::set_text`) is async and
    // per-backend (navigator.clipboard on web, UIPasteboard on Apple,
    // ClipboardManager on Android, arboard on Linux), so the work is spawned
    // via `runtime_core::driver::spawn_async` (the same idiom the channel /
    // join / invite modals use) and the confirmation flips only on success —
    // a failed write (no clipboard available) shows nothing rather than a lie.
    // `copied` is `Copy`, so it's moved into the async block as a plain handle;
    // `text` is an owned `String` (the link) moved with it.
    let on_copy = move || {
        let text = link.get();
        let copied = copied;
        runtime_core::spawn_then(async move { clipboard::set_text(&text).await }, move |result| {
            if result.is_ok() {
                copied.set(true);
            }
        });
    };

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Stretch),
            gap: Some(px(12.0)),
            padding_top: Some(px(20.0)),
            padding_bottom: Some(px(20.0)),
            padding_left: Some(px(20.0)),
            padding_right: Some(px(20.0)),
            background: Some(col(theme::BG_SECONDARY)),
            ..rounded(8.0)
        }) {
            text(style = StyleRules {
                font_size: Some(px(13.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(theme::TEXT_MUTED)),
                ..Default::default()
            }) { "INVITE CODE".to_string() }
            text(style = StyleRules {
                font_size: Some(px(13.0)),
                color: Some(col(theme::TEXT_MUTED)),
                ..Default::default()
            }) { "Send this code to a friend. They paste it into Add a Server → Join a Server.".to_string() }
            // The read-only field: a controlled `text_input` bound to `link`.
            // `link` is never written by this component, so the value shown is
            // always the real invite link.
            text_input(
                value = link,
                style = StyleRules {
                    height: Some(px(40.0)),
                    padding_left: Some(px(12.0)),
                    padding_right: Some(px(12.0)),
                    background: Some(col(theme::BG_TERTIARY)),
                    color: Some(col(theme::TEXT_STRONG)),
                    font_size: Some(px(15.0)),
                    ..with_radius(borderless_input(), 8.0)
                },
            )
            button(
                label = "Copy".to_string(),
                on_click = on_copy,
                style = StyleRules {
                    height: Some(px(32.0)),
                    padding_left: Some(px(16.0)),
                    padding_right: Some(px(16.0)),
                    background: Some(col(theme::ACCENT)),
                    color: Some(col(theme::TEXT_STRONG)),
                    font_size: Some(px(14.0)),
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    ..rounded(6.0)
                },
            )
            // Reactive confirmation: `if copied.get() {…}` lowers to a `when`
            // whose render is an `Fn` closure, so the (non-`Copy`) style is
            // written inline, fresh on every evaluation. No `else` branch —
            // while `copied` is unset the line renders nothing.
            if copied.get() {
                text(style = StyleRules {
                    font_size: Some(px(13.0)),
                    color: Some(col(theme::TEXT_MUTED)),
                    ..Default::default()
                }) { "Copied!".to_string() }
            }
        }
    }
}
