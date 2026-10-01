//! `delete_channel_modal` — the "Delete Channel" confirmation modal for a live
//! channel row.
//!
//! A centered card over a dimmed full-screen backdrop (the build-once/toggle
//! idiom of `join_modal.rs` / `create_channel_modal.rs`). It has two visual
//! states:
//!
//! - **Confirm** (default): a `"Delete this channel?"` prompt naming the
//!   channel (with a note that it can't be undone) above a **Cancel** button
//!   and a red **Delete** button.
//! - **Error**: when the backend rejects the call (e.g. a non-owner trying to
//!   delete a channel), the backend's own message is shown inline above the
//!   buttons, so the user can retry or cancel.
//!
//! The `open` / `busy` / `error` signals are owned by the caller (`app.rs`),
//! which resets `error` on open. `server_id` / `channel_id` / `channel_name`
//! are read at submit time (the modal is built once and lives across re-opens).
//! `channels` is the caller's live channel list; on a successful delete it is
//! re-fetched via `crate::servers::list_channels` so the row disappears without
//! a reload. There is no client-side permission check — the backend decides
//! and its message is surfaced verbatim.
//!
//! Idioms: the states are one `switch` keyed on `error`; each arm builds its
//! own `Element` (`Element` is not `Copy`). `Signal` is `Copy`, so every
//! `move` closure captures a copy and stays `Fn`. The buttons carry
//! `.on_touch` chains built as `let` bindings (the `close_btn` idiom in
//! `invite_modal.rs`) so the method chain lives outside the `ui!` child
//! position.
use runtime_core::{
    component, switch, ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent,
    Overflow, Position, Signal, StyleRules, TouchPhase, TouchResponse,
};

use crate::style_helpers::{col, pct, px};

// Discord dark palette (kept local so the component is self-contained).
const CARD: &str = "#2b2d31";
const BTN_CANCEL: &str = "#4e5058"; // Cancel button (Discord muted gray)
const BTN_DANGER: &str = "#da373c"; // Delete button (Discord danger red)
const TEXT: &str = "#f2f3f5";
const TEXT_MUTED: &str = "#949ba4";

/// The "Delete Channel" confirmation modal. `server_id` / `channel_id` are the
/// target (read at submit time); `channel_name` is shown in the prompt; `busy`
/// is true while the RPC is in flight; `error` holds the backend's rejection
/// message; `channels` is the live channel list, re-fetched on a successful
/// delete. All signals are owned by the caller and reset on re-open.
#[component]
#[allow(non_snake_case)]
pub fn DeleteChannelModal(
    open: Signal<bool>,
    server_id: Signal<Option<i64>>,
    channel_id: Signal<Option<i64>>,
    channel_name: Signal<String>,
    busy: Signal<bool>,
    error: Signal<String>,
    channels: Signal<Vec<crate::servers::ChannelSummary>>,
) -> Element {
    let close = open;
    let sid = server_id;
    let cid = channel_id;
    let nm = channel_name;
    let b = busy;
    let e = error;
    let ch = channels;

    // Shared card/button styles (one static pass).
    let card_style = StyleRules {
        width: Some(px(340.0)),
        padding_top: Some(px(20.0)),
        padding_bottom: Some(px(20.0)),
        padding_left: Some(px(20.0)),
        padding_right: Some(px(20.0)),
        background: Some(col(CARD)),
        border_top_left_radius: Some(px(12.0)),
        border_top_right_radius: Some(px(12.0)),
        border_bottom_left_radius: Some(px(12.0)),
        border_bottom_right_radius: Some(px(12.0)),
        flex_direction: Some(FlexDirection::Column),
        align_items: Some(AlignItems::Center),
        ..Default::default()
    };
    let title_style = StyleRules {
        font_size: Some(px(18.0)),
        font_weight: Some(FontWeight::Bold),
        color: Some(col(TEXT)),
        text_align: Some(runtime_core::TextAlign::Center),
        ..Default::default()
    };
    let prompt_style = StyleRules {
        font_size: Some(px(14.0)),
        color: Some(col(TEXT)),
        margin_top: Some(px(12.0)),
        text_align: Some(runtime_core::TextAlign::Center),
        ..Default::default()
    };
    let warn_style = StyleRules {
        font_size: Some(px(12.0)),
        color: Some(col(TEXT_MUTED)),
        margin_top: Some(px(6.0)),
        text_align: Some(runtime_core::TextAlign::Center),
        ..Default::default()
    };
    let btn_row_style = StyleRules {
        margin_top: Some(px(20.0)),
        width: Some(px(100.0)), // fills the card width
        flex_direction: Some(FlexDirection::Row),
        gap: Some(px(10.0)),
        ..Default::default()
    };
    let cancel_btn_style = StyleRules {
        height: Some(px(40.0)),
        flex_grow: Some(runtime_core::Tokenized::Literal(1.0)),
        background: Some(col(BTN_CANCEL)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        flex_direction: Some(FlexDirection::Row),
        ..Default::default()
    };
    let delete_btn_style = StyleRules {
        height: Some(px(40.0)),
        flex_grow: Some(runtime_core::Tokenized::Literal(1.0)),
        background: Some(col(BTN_DANGER)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        flex_direction: Some(FlexDirection::Row),
        ..Default::default()
    };
    let btn_label_style = StyleRules {
        font_size: Some(px(15.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT)),
        ..Default::default()
    };
    let err_style = StyleRules {
        font_size: Some(px(13.0)),
        color: Some(col("#f23f43")),
        margin_top: Some(px(12.0)),
        text_align: Some(runtime_core::TextAlign::Center),
        ..Default::default()
    };

    // --- Delete handler ------------------------------------------------------
    // `move` closure: every captured copy is `Copy`, so it stays `Fn` across
    // taps. No client-side permission check — the backend decides and its
    // message is surfaced verbatim in `error`.
    let on_delete = move || {
        let Some(sid0) = sid.get() else {
            return;
        };
        let Some(cid0) = cid.get() else {
            return;
        };
        b.set(true);
        e.set(String::new());
        let e2 = e;
        let b2 = b;
        let ch2 = ch;
        let close2 = close;
        runtime_core::driver::spawn_async(async move {
            match crate::servers::delete_channel(sid0, cid0).await {
                Ok(_) => {
                    if let Ok(list) = crate::servers::list_channels(sid0).await {
                        ch2.set(list);
                    }
                    close2.set(false);
                }
                Err(msg) => e2.set(msg),
            }
            b2.set(false);
        });
    };

    // --- Body: one switch keyed on `error` -----------------------------------
    // Error > confirm. Each arm builds its own `Element`.
    //
    // The Cancel / Delete buttons are built *inside* the render closure (not
    // pre-built `let`s splatted in): `Element` is not `Copy`, and a pre-built
    // one moved into an `Fn` render closure is gone after the first rebuild
    // (E0507). `on_delete` / `close` capture only `Copy` signals, so the
    // closures stay `Fn` + `Copy`.
    let body = switch(
        move || error.get().clone(),
        move |err: &String| {
            let name = nm.get();
            let cancel_btn = ui! {
                view(style = cancel_btn_style.clone(), test_id = "delete-channel-cancel") {
                    text(style = btn_label_style.clone()) { "Cancel".to_string() }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        close.set(false);
                    }
                    TouchResponse::CONSUMED
                })
            };
            // Static label computed at build time (see `join_modal.rs`).
            let delete_label = if b.get() { "Deleting…".to_string() } else { "Delete".to_string() };
            let delete_btn = ui! {
                view(style = delete_btn_style.clone(), test_id = "delete-channel-delete") {
                    text(style = btn_label_style.clone()) { delete_label }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        on_delete();
                    }
                    TouchResponse::CONSUMED
                })
            };
            if !err.is_empty() {
                ui! {
                    view(style = card_style.clone()) {
                        text(style = title_style.clone()) { "Delete Channel".to_string() }
                        text(style = prompt_style.clone()) {
                            format!("Delete {}?", name)
                        }
                        text(style = err_style.clone()) { err.clone() }
                        view(style = btn_row_style.clone()) {
                            { cancel_btn }
                            { delete_btn }
                        }
                    }
                }
            } else {
                ui! {
                    view(style = card_style.clone()) {
                        text(style = title_style.clone()) { "Delete Channel".to_string() }
                        text(style = prompt_style.clone()) {
                            format!("Delete {}?", name)
                        }
                        text(style = warn_style.clone()) {
                            "This can't be undone. Messages in the channel will be removed."
                                .to_string()
                        }
                        view(style = btn_row_style.clone()) {
                            { cancel_btn }
                            { delete_btn }
                        }
                    }
                }
            }
        },
    );

    // --- Full-screen backdrop + centered card --------------------------------
    let open_chk = open;
    let wrap_style = move || StyleRules {
        position: Some(Position::Absolute),
        left: Some(px(0.0)),
        top: Some(px(0.0)),
        width: Some(if open_chk.get() { pct(100.0) } else { px(0.0) }),
        height: Some(if open_chk.get() { pct(100.0) } else { px(0.0) }),
        background: Some(col("rgba(0, 0, 0, 0.6)")),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    let card = ui! {
        view {
            { body }
        }
        .on_touch(move |_ev| TouchResponse::CONSUMED)
    };
    ui! {
        view(style = wrap_style) {
            { card }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                close.set(false);
            }
            TouchResponse::CONSUMED
        })
    }
}
