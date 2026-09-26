#![allow(dead_code)]
//! `setup_checklist_item` — a self-contained "server setup checklist" row for
//! the Discord clone. A single row is an icon (a checkmark when the task is
//! done, an empty circle when it isn't) next to the task's label; when the
//! task is done the label is drawn in a muted color with a strikethrough.
//!
//! Everything is hardcoded local state — no backend call. The row's `done`
//! flag is a real `Signal<bool>` prop so a later `.set` flips the row's icon
//! and label on the next rebuild; the demo at the bottom hardcodes exactly
//! one instance done (`true`) and one instance not done (`false`). Declared
//! in `lib.rs` so it compiles as part of the crate, but referenced by nothing
//! (the same shape `reaction_summary` / `connection_quality_indicator` /
//! `verified_badge` ship in) — not wired into any screen yet.
//!
//! Idioms, each verified against this crate's `components/reaction_summary.rs`
//! / `components/server_stats_card.rs` / `components/permission_override_row.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the row's two appearances are selected by a `switch` keyed on the `done`
//!   signal — the framework's live unit of reactivity (the `ui!` tree
//!   evaluates once, so a `.set` on the signal only re-renders what sits
//!   behind a `switch` key). The render is an `Fn` closure (it rebuilds many
//!   times), so the per-arm colors/flags are chosen into local `Copy` `&str` /
//!   `bool` bindings at the top of the closure — nothing non-`Copy` crosses
//!   the `switch` boundary;
//! - the `label` prop is a `String` (the macro rewrites it to
//!   `Reactive<String>`); it is read via `.get()` inside the `ui!` body so it
//!   bakes to a reactive `TextSource`; the `done` prop is a `Signal<bool>`
//!   (threaded through un-wrapped) and drives the `switch` key.

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, switch, ui, AlignItems, Element, FlexDirection, FontWeight, Signal,
    StyleRules,
};

// Discord dark palette (kept local so the component is self-contained).
const DONE_ICON: &str = "✓"; // the checkmark, shown when the task is done
const TODO_ICON: &str = "○"; // the empty circle, shown when the task is pending
const DONE_COLOR: &str = "#57f287"; // the green checkmark (Discord "online" green)
const TODO_ICON_COLOR: &str = "#6d7178"; // the muted circle (Discord muted gray)
const LABEL_ACTIVE: &str = "#dbdee1"; // a pending task's label (Discord primary text)
const LABEL_DONE: &str = "#6d7178"; // a done task's label (muted, crossed out)

/// One server-setup checklist row: an icon (checkmark when done, empty circle
/// when not) beside the task's label, the label struck through + muted once
/// the task is done. `label` is the hardcoded task text; `done` is the local
/// completion flag (a `Signal` so a later `.set` flips the row live).
#[component]
#[allow(non_snake_case)]
pub fn SetupChecklistItem(label: String, done: Signal<bool>) -> Element {
    // `switch` keyed on the flag: rebuilds the row only when the value
    // changes. The render is `Fn` (many rebuilds), so each arm's icon/color/
    // strike are picked into local `Copy` `&str` / `bool` bindings up front.
    switch(
        move || done.get(),
        move |is_done: &bool| {
            let (icon, icon_color, text_color, strike) = if *is_done {
                (DONE_ICON, DONE_COLOR, LABEL_DONE, true)
            } else {
                (TODO_ICON, TODO_ICON_COLOR, LABEL_ACTIVE, false)
            };
            // The label is hardcoded and never changes, so read it into an
            // owned `String` each rebuild and bake it to a static text body —
            // the same shape the icon's `icon.to_string()` uses (avoids moving
            // the non-`Copy` reactive `label` handle out of the multi-call
            // `Fn` render closure).
            let label_text = label.get().to_string();

            ui! {
                // The row: a horizontal flex of the icon next to the label.
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    gap: Some(px(8.0)),
                    ..Default::default()
                }) {
                    // The status icon — a static string body per arm, so it bakes
                    // to a static `TextSource` (no reactive slot for the glyph).
                    text(style = StyleRules {
                        font_size: Some(px(16.0)),
                        font_weight: Some(FontWeight::Bold),
                        color: Some(col(icon_color)),
                        ..Default::default()
                    }) { icon.to_string() }
                    // The task label — `label.get()` bakes to a reactive
                    // `TextSource`; the arm's `strike` applies the strikethrough
                    // only on the done arm (the `Option<bool>` field, so `None`
                    // on the pending arm means "no decoration").
                    text(style = StyleRules {
                        font_size: Some(px(14.0)),
                        font_weight: Some(FontWeight::Normal),
                        color: Some(col(text_color)),
                        strikethrough: Some(strike),
                        ..Default::default()
                    }) {
                        label_text
                    }
                }
            }
        },
    )
}

/// Two hardcoded instances of the row — one task already done (`true`) and one
/// still pending (`false`) — stacked in a column. The demo exists only to
/// exercise both arms of `SetupChecklistItem` so the component compiles with
/// real, hardcoded inputs; it is not wired into any screen.
#[component]
#[allow(non_snake_case)]
pub fn SetupChecklistDemo() -> Element {
    // The local completion flags: exactly one true, one false (hardcoded).
    let done_one = signal(true);
    let done_two = signal(false);

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            gap: Some(px(10.0)),
            ..Default::default()
        }) {
            SetupChecklistItem(label = "Add a welcome channel", done = done_one)
            SetupChecklistItem(label = "Invite your first member", done = done_two)
        }
    }
}
