//! A fixed row of exactly four hardcoded emoji reaction buttons —
//! thumbs up, heart, laugh, eyes — each carrying its own local
//! click-count signal that starts at zero and increments on click.
//!
//! Self-contained: no backend call, no shared refs, not yet wired into
//! any screen. Drop it in a `ui!` tree as `EmojiReactionBar()` and it
//! renders as-is; the counts live entirely inside this component.

use std::rc::Rc;

use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, Signal, StyleRules, StyleSheet,
};

use crate::style_helpers::{col, px, static_sheet};

/// Bar background — a Discord-style dark surface.
const COLOR_BAR_BG: &str = "#2f3136";
/// Per-chip surface, one shade lighter than the bar.
const COLOR_CHIP_BG: &str = "#40444b";
/// Count text, dimmed against the chip.
const COLOR_COUNT: &str = "#949ba4";

#[component]
pub fn EmojiReactionBar() -> Element {
    let bar = bar_sheet();
    let chip = chip_sheet();
    // One click-count signal per reaction. Each starts at 0 and is
    // owned here — local state, no external wiring.
    let up = signal(0i32);
    let heart = signal(0i32);
    let laugh = signal(0i32);
    let eyes = signal(0i32);
    ui! {
        view(style = bar) {
            emoji_chip("👍", up, &chip)
            emoji_chip("❤️", heart, &chip)
            emoji_chip("😂", laugh, &chip)
            emoji_chip("👀", eyes, &chip)
        }
    }
}

/// One reaction button: the emoji glyph over its live click count.
/// The label is a reactive closure (`TextContent` accepts `Fn() ->
/// ToString`), so the count repaints each time `count` changes without
/// rebuilding the chip.
fn emoji_chip(emoji: &'static str, count: Signal<i32>, chip: &Rc<StyleSheet>) -> Element {
    ui! {
        button(
            label = move || format!("{} {}", emoji, count.get()),
            on_click = move || {
                count.set(count.get() + 1);
            },
            style = chip.clone(),
        )
    }
}

fn bar_sheet() -> Rc<StyleSheet> {
    static_sheet(
        "reaction.bar",
        StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(4.0)),
            background: Some(col(COLOR_BAR_BG)),
            padding_top: Some(px(4.0)),
            padding_right: Some(px(4.0)),
            padding_bottom: Some(px(4.0)),
            padding_left: Some(px(4.0)),
            ..Default::default()
        },
    )
}

fn chip_sheet() -> Rc<StyleSheet> {
    static_sheet(
        "reaction.chip",
        StyleRules {
            background: Some(col(COLOR_CHIP_BG)),
            color: Some(col(COLOR_COUNT)),
            padding_top: Some(px(8.0)),
            padding_right: Some(px(8.0)),
            padding_bottom: Some(px(8.0)),
            padding_left: Some(px(8.0)),
            border_top_left_radius: Some(px(6.0)),
            border_top_right_radius: Some(px(6.0)),
            border_bottom_left_radius: Some(px(6.0)),
            border_bottom_right_radius: Some(px(6.0)),
            ..Default::default()
        },
    )
}
