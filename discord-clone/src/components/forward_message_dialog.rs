//! `forward_message_dialog` — a self-contained forward-message dialog for the
//! Discord clone. It renders a hardcoded local list of exactly two fake
//! channel names, each as a clickable row. Clicking a row sets a local
//! `forwarded_to` signal to that channel's name; once set, the two rows are
//! replaced by a single text line reading `"Forwarded to {name}"`.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape
//! `delete_message_confirm` / `kick_confirm_dialog` ship in).
//!
//! Idioms, verified against `components/delete_message_confirm.rs` and
//! `components/timeout_duration_picker.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors, written INLINE (created fresh per evaluation, never
//!   captured across the `when` boundary);
//! - the two channel rows are authored with a keyed
//!   `for (i, name) in …iter().enumerate(), key = i` inside `ui!` (the same
//!   shape `category_filter_chips` uses), where each row is a `view` whose
//!   `.on_touch(|ev| …)` chain (firing on `TouchPhase::Ended`) sets the local
//!   `forwarded_to` signal to that row's name;
//! - the rows/result swap is a node-level reactive
//!   `if let Some(r) = forwarded_to.get() { … } else { … }` inside `ui!`,
//!   which lowers to the `when` primitive (mounts the chosen branch, unmounts
//!   the other when a row sets the signal) — the same shape
//!   `delete_message_confirm` uses for its button/result swap.

use crate::theme;
use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, StyleRules, TouchPhase,
    TouchResponse,
};

// Discord dark palette (kept local so the component is self-contained).

// The two hardcoded fake channel names — exactly two, local only. Kept as a
// static slice so the keyed `for` renders one row per entry and the loop index
// doubles as the per-row identity.
const CHANNELS: [&str; 2] = ["general", "random"];

/// The forward-message dialog. Renders the hardcoded local list of exactly two
/// fake channel names as clickable rows. Tapping a row sets the local
/// `forwarded_to` signal to that channel's name; once set, the rows are
/// replaced by a text line reading `"Forwarded to {name}"`. No props, no
/// backend.
#[component]
#[allow(dead_code)] // declared but not wired into any screen yet
pub fn ForwardMessageDialog() -> Element {
    // Local result state: `None` until a row is tapped. Both the per-row
    // `.on_touch` handlers and the reactive `if let` below read/write the
    // same signal (`Signal` is `Copy`, so each `move` closure captures a copy
    // of the same underlying signal and stays `Fn`). No backend call.
    let forwarded_to = signal(Option::<String>::None);

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Stretch),
            gap: Some(px(12.0)),
            width: Some(px(280.0)),
            padding_top: Some(px(20.0)),
            padding_bottom: Some(px(20.0)),
            padding_left: Some(px(20.0)),
            padding_right: Some(px(20.0)),
            background: Some(col(theme::BG_SECONDARY)),
            border_top_left_radius: Some(px(8.0)),
            border_top_right_radius: Some(px(8.0)),
            border_bottom_left_radius: Some(px(8.0)),
            border_bottom_right_radius: Some(px(8.0)),
            ..Default::default()
        }) {
            // The prompt line — local literal, no lookup, no backend call.
            text(style = StyleRules {
                font_size: Some(px(16.0)),
                color: Some(col(theme::TEXT_NORMAL)),
                ..Default::default()
            }) { "Forward this message to:".to_string() }

            // Reactive swap: `if let Some(r) = forwarded_to.get() {…} else {…}`
            // lowers to a `when` (mounts the chosen branch, unmounts the
            // other when a row sets the signal). The styles are inline so
            // nothing is captured across the `when` boundary (`StyleRules`
            // is not `Copy`).
            if let Some(r) = forwarded_to.get() {
                // Once a channel is chosen, show the confirmation line in
                // place of the list. `r` is the chosen channel name.
                text(style = StyleRules {
                    font_size: Some(px(16.0)),
                    color: Some(col(theme::TEXT_STRONG)),
                    ..Default::default()
                }) { format!("Forwarded to {}", r) }
            } else {
                // One row per hardcoded channel name. `i` is the loop index
                // (and the `key`); the per-row `.on_touch` closure only reads
                // the (Copy) `name` and (Copy) `forwarded_to` signal, so it
                // stays `Fn`. The built `StyleRules` is a fresh inline value
                // each evaluation.
                for (i, name) in CHANNELS.iter().enumerate(), key = i {
                    view(style = StyleRules {
                        height: Some(px(44.0)),
                        margin_top: Some(px(2.0)),
                        margin_bottom: Some(px(2.0)),
                        padding_left: Some(px(12.0)),
                        padding_right: Some(px(12.0)),
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        background: Some(col(theme::BG_PRIMARY)),
                        border_top_left_radius: Some(px(6.0)),
                        border_top_right_radius: Some(px(6.0)),
                        border_bottom_left_radius: Some(px(6.0)),
                        border_bottom_right_radius: Some(px(6.0)),
                        ..Default::default()
                    }) {
                        // The channel-hash label — a static muted "#", no
                        // reactivity (text content must be a `TextContent`).
                        text(style = StyleRules {
                            font_size: Some(px(15.0)),
                            color: Some(col(theme::TEXT_MUTED)),
                            margin_right: Some(px(6.0)),
                            ..Default::default()
                        }) { "#".to_string() }
                        // The channel name — static per row.
                        text(style = StyleRules {
                            font_size: Some(px(15.0)),
                            color: Some(col(theme::TEXT_STRONG)),
                            ..Default::default()
                        }) { name.to_string() }
                    }.on_touch(move |ev| {
                        // Tap: set `forwarded_to` to THIS row's channel name on
                        // `TouchPhase::Ended`. `name` and `forwarded_to` are
                        // `Copy`, so this `move` closure stays `Fn`. No backend
                        // call.
                        if ev.phase == TouchPhase::Ended {
                            forwarded_to.set(Some(name.to_string()));
                        }
                        TouchResponse::CONSUMED
                    })
                }
            }
        }
    }
}
