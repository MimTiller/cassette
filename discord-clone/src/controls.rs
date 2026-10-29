//! Small shared controls: a text button, a text field and an on/off switch.

use runtime_core::{
    ui, AlignItems, Element, FlexDirection, JustifyContent, Signal, StyleRules, Tokenized, TouchPhase,
    TouchResponse,
};

use crate::style_helpers::{col, px, rounded};
use crate::theme;

pub use crate::styles::ButtonBoxKind as ButtonKind;

/// A text button in one of the theme's button colours. `on_press` runs on
/// release.
pub fn button_view(label: &str, kind: ButtonKind, on_press: impl Fn() + 'static) -> Element {
    let label = label.to_string();
    ui! {
        view(style = crate::styles::ButtonBox().kind(kind)) {
            text(style = crate::styles::ButtonLabel()) { label }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                on_press();
            }
            TouchResponse::CONSUMED
        })
    }
}

/// A text input bound to `value` both ways: it shows `value`, and every
/// keystroke writes back into it (`value =` alone only fills the box).
pub fn text_field(value: Signal<String>, placeholder: &str, style: StyleRules) -> Element {
    let placeholder = placeholder.to_string();
    ui! {
        text_input(
            value = value,
            on_change = move |v: String| value.set(v),
            placeholder = placeholder,
            style = style,
        )
    }
}

/// An on/off switch showing `on`. When `enabled` is false it is greyed out
/// and ignores taps; otherwise a tap runs `on_tap`.
pub fn toggle(on: bool, enabled: bool, on_tap: impl Fn() + 'static) -> Element {
    ui! {
        view(style = StyleRules {
            width: Some(px(40.0)),
            height: Some(px(24.0)),
            padding_left: Some(px(3.0)),
            padding_right: Some(px(3.0)),
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            justify_content: Some(if on { JustifyContent::FlexEnd } else { JustifyContent::FlexStart }),
            flex_shrink: Some(Tokenized::Literal(0.0)),
            background: Some(col(if on { theme::SUCCESS } else { theme::BUTTON_SECONDARY })),
            opacity: Some(Tokenized::Literal(if enabled { 1.0 } else { 0.5 })),
            ..rounded(12.0)
        }) {
            view(style = StyleRules {
                width: Some(px(18.0)),
                height: Some(px(18.0)),
                background: Some(col(theme::TEXT_ON_ACCENT)),
                ..rounded(9.0)
            }) {}
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended && enabled {
                on_tap();
            }
            TouchResponse::CONSUMED
        })
    }
}
