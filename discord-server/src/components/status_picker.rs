//! A radio-group-style presence status picker with exactly four options —
//! Online, Away, Do Not Disturb, Invisible — one selectable at a time.
//!
//! Each option renders a small colored dot (green / yellow / red / gray)
//! next to its label. The active choice is tracked in local component state;
//! when the selection moves, the chosen row's label gains a leading check
//! mark while the others stay plain — a reactive repaint, no rebuild. The
//! dot colors are fixed per status: they encode the status itself, not the
//! selection.
//!
//! Self-contained: no backend call, no shared refs, not yet wired into any
//! screen. Drop it in a `ui!` tree as `StatusPicker()` and it renders as-is;
//! the selection lives entirely inside this component.

use std::rc::Rc;

use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, Signal, StyleRules, StyleSheet,
};

use crate::style_helpers::{col, px, static_sheet};

/// Picker background — a Discord-style dark surface.
const COLOR_PICKER_BG: &str = "#2f3136";
/// Option label text.
const COLOR_LABEL: &str = "#b5bac1";

/// Online — green.
const COLOR_ONLINE: &str = "#23a559";
/// Away — yellow.
const COLOR_AWAY: &str = "#f0b232";
/// Do Not Disturb — red.
const COLOR_DND: &str = "#f23f43";
/// Invisible — gray.
const COLOR_INVISIBLE: &str = "#80848e";

#[component]
pub fn StatusPicker() -> Element {
    let picker = picker_sheet();
    let row = row_sheet();
    let label = label_sheet();
    // One selection signal shared by all rows: which option index is active.
    // Radio-group semantics — picking a row sets this, so exactly one is
    // ever selected. Local state, no external wiring.
    let selected = signal(0usize);
    ui! {
        view(style = picker) {
            status_row(0, "Online", COLOR_ONLINE, row.clone(), label.clone(), selected.clone())
            status_row(1, "Away", COLOR_AWAY, row.clone(), label.clone(), selected.clone())
            status_row(2, "Do Not Disturb", COLOR_DND, row.clone(), label.clone(), selected.clone())
            status_row(3, "Invisible", COLOR_INVISIBLE, row.clone(), label.clone(), selected.clone())
        }
    }
}

/// One selectable status option: a fixed-color status dot beside its label.
///
/// The dot is a plain (non-interactive) `view` carrying the status color.
/// The label `button` is the clickable element: its label is a reactive
/// closure over the shared `selected` signal, so when the selection moves
/// exactly the chosen row repaints a leading "✓ " while the rest stay plain
/// — no rebuild.
fn status_row(
    index: usize,
    label_text: &'static str,
    dot_color: &'static str,
    row: Rc<StyleSheet>,
    label: Rc<StyleSheet>,
    selected: Signal<usize>,
) -> Element {
    let dot = dot_sheet(dot_color);
    // Two `move` closures, each needing the shared selection: hand each its
    // own clone of the signal handle so they own their capture.
    let label_signal = selected.clone();
    let click_signal = selected;
    ui! {
        view(style = row) {
            view(style = dot)
            button(
                label = move || {
                    if label_signal.get() == index {
                        format!("✓ {}", label_text)
                    } else {
                        label_text.to_string()
                    }
                },
                on_click = move || click_signal.set(index),
                style = label,
            )
        }
    }
}

fn picker_sheet() -> Rc<StyleSheet> {
    static_sheet(
        "status.picker",
        StyleRules {
            flex_direction: Some(FlexDirection::Column),
            gap: Some(px(2.0)),
            background: Some(col(COLOR_PICKER_BG)),
            padding_top: Some(px(6.0)),
            padding_right: Some(px(6.0)),
            padding_bottom: Some(px(6.0)),
            padding_left: Some(px(6.0)),
            ..Default::default()
        },
    )
}

fn row_sheet() -> Rc<StyleSheet> {
    static_sheet(
        "status.row",
        StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(8.0)),
            ..Default::default()
        },
    )
}

/// The status dot: a small circle in the option's status color.
fn dot_sheet(color: &str) -> Rc<StyleSheet> {
    // A circle is a square with a radius of half its size.
    static_sheet(
        &format!("status.dot.{}", color),
        StyleRules {
            width: Some(px(14.0)),
            height: Some(px(14.0)),
            background: Some(col(color)),
            border_top_left_radius: Some(px(7.0)),
            border_top_right_radius: Some(px(7.0)),
            border_bottom_left_radius: Some(px(7.0)),
            border_bottom_right_radius: Some(px(7.0)),
            ..Default::default()
        },
    )
}

fn label_sheet() -> Rc<StyleSheet> {
    static_sheet(
        "status.label",
        StyleRules {
            color: Some(col(COLOR_LABEL)),
            padding_top: Some(px(4.0)),
            padding_right: Some(px(8.0)),
            padding_bottom: Some(px(4.0)),
            padding_left: Some(px(8.0)),
            ..Default::default()
        },
    )
}
