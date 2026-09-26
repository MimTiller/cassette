//! A panel of pinned messages, seeded with exactly two hardcoded mock
//! entries (author + text). Each entry carries an "Unpin" button that
//! removes it from the panel's local component state — a plain list
//! mutation, no backend call.
//!
//! Self-contained: no backend call, no shared refs, not yet wired into
//! any screen. Drop it in a `ui!` tree as `PinnedMessagesPanel()` and it
//! renders as-is; the pinned list lives entirely inside this component.

use std::rc::Rc;

use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, Signal, StyleRules, StyleSheet,
};

use crate::style_helpers::{col, px, static_sheet};

/// Panel background — a Discord-style dark surface.
const COLOR_PANEL_BG: &str = "#2f3136";
/// Author name, bright against the dark surface.
const COLOR_AUTHOR: &str = "#dcddde";
/// Message body text, dimmed.
const COLOR_TEXT: &str = "#b5bac1";
/// Empty-state placeholder text.
const COLOR_EMPTY: &str = "#80848e";

/// One pinned message: a stable id (list key + unpin target), author, text.
/// `PartialEq` is required by `Signal::set`'s equality guard; `Clone` by
/// `Signal::get` and the `for`-loop row splat.
#[derive(Clone, PartialEq)]
struct PinnedMsg {
    id: usize,
    author: &'static str,
    text: &'static str,
}

const MOCK_PINNED: [PinnedMsg; 2] = [
    PinnedMsg {
        id: 0,
        author: "nitro_nov",
        text: "Welcome to the server — read the rules channel first!",
    },
    PinnedMsg {
        id: 1,
        author: "pixel_pilot",
        text: "Voice hangout every Friday at 8pm CT. Bring snacks.",
    },
];

#[component]
pub fn PinnedMessagesPanel() -> Element {
    let panel = panel_sheet();
    // The full pinned list as one reactive vector. Unpinning filters an id
    // out of it; `ui!` re-renders the rows from the signal on each change.
    let pinned = signal(MOCK_PINNED.to_vec());
    ui! {
        view(style = panel) {
            if pinned.get().is_empty() {
                text(style = empty_sheet()) { "No pinned messages." }
            }
            for msg in pinned.get() {
                pinned_row(msg, pinned.clone())
            }
        }
    }
}

/// One pinned-message row: author over body text, with an Unpin button.
/// The button's closure owns its own clone of the list signal, so it can
/// mutate the list in place without borrowing across closures.
fn pinned_row(msg: PinnedMsg, pinned: Signal<Vec<PinnedMsg>>) -> Element {
    let author_sheet = author_sheet();
    let text_sheet = text_sheet();
    let unpin_sheet = unpin_sheet();
    // Each closure that mutates the list takes its own clone of the signal
    // handle so ownership stays clean (two move closures, one signal).
    let click_signal = pinned;
    ui! {
        view(style = row_sheet()) {
            view {
                text(style = author_sheet) { msg.author }
                text(style = text_sheet) { msg.text }
            }
            button(
                label = "Unpin",
                on_click = move || {
                    let id = msg.id;
                    let mut next = click_signal.get();
                    next.retain(|m| m.id != id);
                    click_signal.set(next);
                },
                style = unpin_sheet,
            )
        }
    }
}

fn panel_sheet() -> Rc<StyleSheet> {
    static_sheet(
        "pinned.panel",
        StyleRules {
            flex_direction: Some(FlexDirection::Column),
            gap: Some(px(10.0)),
            background: Some(col(COLOR_PANEL_BG)),
            padding_top: Some(px(10.0)),
            padding_right: Some(px(10.0)),
            padding_bottom: Some(px(10.0)),
            padding_left: Some(px(10.0)),
            ..Default::default()
        },
    )
}

fn row_sheet() -> Rc<StyleSheet> {
    static_sheet(
        "pinned.row",
        StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::FlexStart),
            gap: Some(px(8.0)),
            ..Default::default()
        },
    )
}

fn author_sheet() -> Rc<StyleSheet> {
    static_sheet(
        "pinned.author",
        StyleRules {
            color: Some(col(COLOR_AUTHOR)),
            padding_bottom: Some(px(2.0)),
            ..Default::default()
        },
    )
}

fn text_sheet() -> Rc<StyleSheet> {
    static_sheet(
        "pinned.text",
        StyleRules {
            color: Some(col(COLOR_TEXT)),
            ..Default::default()
        },
    )
}

fn unpin_sheet() -> Rc<StyleSheet> {
    static_sheet(
        "pinned.unpin",
        StyleRules {
            background: Some(col("#40444b")),
            padding_top: Some(px(4.0)),
            padding_right: Some(px(8.0)),
            padding_bottom: Some(px(4.0)),
            padding_left: Some(px(8.0)),
            border_top_left_radius: Some(px(4.0)),
            border_top_right_radius: Some(px(4.0)),
            border_bottom_left_radius: Some(px(4.0)),
            border_bottom_right_radius: Some(px(4.0)),
            ..Default::default()
        },
    )
}

fn empty_sheet() -> Rc<StyleSheet> {
    static_sheet(
        "pinned.empty",
        StyleRules {
            color: Some(col(COLOR_EMPTY)),
            ..Default::default()
        },
    )
}
