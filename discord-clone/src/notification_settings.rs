#![allow(dead_code)]
//! `notification_settings` -- a self-contained notification-level picker for the
//! Discord clone: three radio-style rows, "All messages", "Mentions only" and
//! "Nothing". Exactly one is selected at a time (selecting one deselects the
//! others), held in a single local `selected` signal (0 = All messages, the
//! start state). No backend call, no wiring into any screen yet: declared in
//! `lib.rs` so it compiles as part of the crate, but referenced by nothing.
//!
//! Idioms follow the crate's other standalone components (the mute-channel
//! toggle, the channel topic banner): plain `StyleRules` built with the
//! `style_helpers` `px` / `col` constructors, a reactive `if selected.get() == n`
//! that lowers to a `when`, and the `view` + `.on_touch(...)` click pattern with
//! the `TouchPhase::Ended` guard. `Signal` is `Copy`, so the `move` closures
//! capture a copy of the same underlying signal.

use crate::theme;
use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, Position, Signal, StyleRules,
    TouchPhase, TouchResponse,
};


/// One radio-style option row: a dot glyph (filled when `selected == idx`) and
/// the label. Pressing it sets `selected` to `idx`, which deselects the others.
fn option_row(label: &'static str, idx: u32, selected: Signal<u32>) -> Element {
    ui! {
        view(style = StyleRules {
            position: Some(Position::Relative),
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(10.0)),
            padding_top: Some(px(10.0)),
            padding_bottom: Some(px(10.0)),
            padding_left: Some(px(12.0)),
            padding_right: Some(px(12.0)),
            background: Some(col(theme::BG_PRIMARY)),
            border_top_left_radius: Some(px(6.0)),
            border_top_right_radius: Some(px(6.0)),
            border_bottom_left_radius: Some(px(6.0)),
            border_bottom_right_radius: Some(px(6.0)),
            ..Default::default()
        }) {
            if selected.get() == idx {
                text(style = StyleRules {
                    font_size: Some(px(16.0)),
                    color: Some(col(theme::ACCENT)),
                    ..Default::default()
                }) { "◉".to_string() }
            } else {
                text(style = StyleRules {
                    font_size: Some(px(16.0)),
                    color: Some(col(theme::TEXT_FAINT)),
                    ..Default::default()
                }) { "○".to_string() }
            }
            text(style = StyleRules {
                font_size: Some(px(14.0)),
                color: Some(col(theme::TEXT_NORMAL)),
                ..Default::default()
            }) { label.to_string() }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                selected.set(idx);
            }
            TouchResponse::CONSUMED
        })
    }
}

/// The notification-settings card: a heading and three mutually exclusive
/// options. Starts on "All messages".
#[component]
#[allow(non_snake_case)]
pub fn NotificationSettings() -> Element {
    let selected = signal(0u32);

    let all_messages = option_row("All messages", 0, selected);
    let mentions_only = option_row("Mentions only", 1, selected);
    let nothing = option_row("Nothing", 2, selected);

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            gap: Some(px(8.0)),
            padding_top: Some(px(16.0)),
            padding_bottom: Some(px(16.0)),
            padding_left: Some(px(16.0)),
            padding_right: Some(px(16.0)),
            background: Some(col(theme::BG_SECONDARY)),
            border_top_left_radius: Some(px(8.0)),
            border_top_right_radius: Some(px(8.0)),
            border_bottom_left_radius: Some(px(8.0)),
            border_bottom_right_radius: Some(px(8.0)),
            ..Default::default()
        }) {
            text(style = StyleRules {
                font_size: Some(px(14.0)),
                color: Some(col(theme::TEXT_STRONG)),
                ..Default::default()
            }) { "Notification Settings".to_string() }
            all_messages
            mentions_only
            nothing
        }
    }
}
