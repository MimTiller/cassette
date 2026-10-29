//! Small shared controls: a text button and a text field.

use runtime_core::{ui, Element, Signal, StyleRules, TouchPhase, TouchResponse};

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
