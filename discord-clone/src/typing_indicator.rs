#![allow(dead_code)]
//! `typing_indicator` -- a self-contained "<name> is typing..." line for the
//! Discord clone. It renders one hardcoded local name (default "miri") as a
//! small muted text row under the message list. Static text only (no
//! animation), no backend call, no wiring into any screen yet: declared in
//! `lib.rs` so it compiles as part of the crate, but referenced by nothing.
//!
//! Idioms follow the crate's other standalone components (for example the
//! channel topic banner): plain `StyleRules` built with the `style_helpers`
//! `px` / `col` constructors, an inline `#[prop(default = ...)]` String prop
//! read once into an owned local (`name.get().to_string()`), and a `ui!` body.

use crate::theme;
use crate::style_helpers::{col, px};
use runtime_core::{component, ui, AlignItems, Element, FlexDirection, StyleRules};


/// The typing indicator. Shows "<name> is typing..." in small muted text.
#[component]
#[allow(non_snake_case)]
pub fn TypingIndicator(#[prop(default = "miri")] name: String) -> Element {
    // Hardcoded constant (never `.set`): read once into an owned String.
    let name_text = name.get().to_string();
    let line = format!("{} is typing...", name_text);

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            padding_top: Some(px(4.0)),
            padding_bottom: Some(px(4.0)),
            padding_left: Some(px(16.0)),
            padding_right: Some(px(16.0)),
            ..Default::default()
        }) {
            text(style = StyleRules {
                font_size: Some(px(12.0)),
                color: Some(col(theme::TEXT_MUTED)),
                ..Default::default()
            }) { line.clone() }
        }
    }
}
