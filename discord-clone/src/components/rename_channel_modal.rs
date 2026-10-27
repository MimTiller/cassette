//! `rename_channel_modal` — the "Rename Channel" modal for a live channel row.
//!
//! A centered card over a dimmed full-screen backdrop (the build-once/toggle
//! idiom of `join_modal.rs` / `create_channel_modal.rs`). It has two visual
//! states:
//!
//! - **Input** (default): a channel-name `text_input` (pre-filled with the
//!   channel's current name) above a **Rename** button.
//! - **Error**: when the backend rejects the call (e.g. insufficient
//!   privileges), the backend's own message is shown inline above the Rename
//!   button (the input keeps the typed name so it can be corrected).
//!
//! The `open` / `name` / `busy` / `error` signals are owned by the caller
//! (`app.rs`), which resets them and seeds `name` with the channel's current
//! name on open. `server_id` and `channel_id` are read at submit time (the
//! modal is built once and lives across re-opens). `channels` is the caller's
//! live channel list; on a successful rename it is re-fetched via
//! `crate::servers::list_channels` so the new name appears without a reload.
//! The only client-side gate is a non-empty name; all real rejection and its
//! wording comes from the backend.
//!
//! Idioms: the states are one `switch` keyed on `error`; each arm builds its
//! own `Element` (`Element` is not `Copy`). `Signal` is `Copy`, so every
//! `move` closure captures a copy and stays `Fn`. The Rename button carries an
//! `.on_touch` chain built as a `let` binding (the `close_btn` idiom) so the
//! method chain lives outside the `ui!` child position.
use crate::theme;
use runtime_core::{
    component, switch, ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent,
    Overflow, Position, Signal, StyleRules, TouchPhase, TouchResponse,
};

use crate::style_helpers::{borderless_input, col, pct, px};

// Discord dark palette (kept local so the component is self-contained).

/// The "Rename Channel" modal. `server_id` / `channel_id` are the target
/// (read at submit time); `name` is the typed new name (seeded with the
/// channel's current name by the caller on open); `busy` is true while the
/// RPC is in flight; `error` holds the backend's rejection message; `channels`
/// is the live channel list, re-fetched on a successful rename. All signals
/// are owned by the caller and reset on re-open.
#[component]
#[allow(non_snake_case)]
pub fn RenameChannelModal(
    open: Signal<bool>,
    server_id: Signal<Option<i64>>,
    channel_id: Signal<Option<i64>>,
    name: Signal<String>,
    busy: Signal<bool>,
    error: Signal<String>,
    channels: Signal<Vec<crate::servers::ChannelSummary>>,
) -> Element {
    let close = open;
    let sid = server_id;
    let cid = channel_id;
    let n = name;
    let b = busy;
    let e = error;
    let ch = channels;

    // Shared card/field/button styles (one static pass).
    let card_style = StyleRules {
        width: Some(px(340.0)),
        padding_top: Some(px(20.0)),
        padding_bottom: Some(px(20.0)),
        padding_left: Some(px(20.0)),
        padding_right: Some(px(20.0)),
        background: Some(col(theme::BG_SECONDARY)),
        border_top_left_radius: Some(px(12.0)),
        border_top_right_radius: Some(px(12.0)),
        border_bottom_left_radius: Some(px(12.0)),
        border_bottom_right_radius: Some(px(12.0)),
        flex_direction: Some(FlexDirection::Column),
        ..Default::default()
    };
    let title_style = StyleRules {
        font_size: Some(px(18.0)),
        font_weight: Some(FontWeight::Bold),
        color: Some(col(theme::TEXT_STRONG)),
        ..Default::default()
    };
    let sub_style = StyleRules {
        font_size: Some(px(13.0)),
        color: Some(col(theme::TEXT_MUTED)),
        margin_top: Some(px(4.0)),
        ..Default::default()
    };
    let input_style = StyleRules {
        height: Some(px(40.0)),
        margin_top: Some(px(16.0)),
        padding_left: Some(px(12.0)),
        padding_right: Some(px(12.0)),
        background: Some(col(theme::BG_TERTIARY)),
        color: Some(col(theme::TEXT_NORMAL)),
        font_size: Some(px(15.0)),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        ..borderless_input()
    };
    let rename_btn_style = StyleRules {
        height: Some(px(40.0)),
        margin_top: Some(px(16.0)),
        background: Some(col(theme::ACCENT)),
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
        color: Some(col(theme::TEXT_STRONG)),
        ..Default::default()
    };
    let err_style = StyleRules {
        font_size: Some(px(13.0)),
        color: Some(col(theme::DANGER)),
        margin_top: Some(px(12.0)),
        ..Default::default()
    };

    // The `name` signal is the input's source of truth. `value = name` is the
    // signal→widget leg only: keystrokes arrive via `on_change` (per the
    // `input_with_submit.rs` recipe — "without this, the signal never updates
    // and `get()` is always empty"). Without this callback the `name` signal
    // stayed empty while the DOM field showed the typed text, so Rename always
    // hit the empty-name guard and silently did nothing. `Signal` is `Copy`,
    // so this closure captures a copy of `n` and stays `Fn` on each keystroke.
    let on_name_change = move |s: String| n.set(s);

    // --- Rename handler ------------------------------------------------------
    // `move` closure: every captured copy is `Copy`, so it stays `Fn` across
    // taps. The only client-side gate is a non-empty name (and a known
    // channel); the backend's rejection is surfaced verbatim in `error`.
    let on_rename = move || {
        let raw = n.get();
        let trimmed = raw.trim().to_string();
        let Some(sid0) = sid.get() else {
            return;
        };
        let Some(cid0) = cid.get() else {
            return;
        };
        if trimmed.is_empty() {
            return;
        }
        b.set(true);
        e.set(String::new());
        let e2 = e;
        let b2 = b;
        let ch2 = ch;
        let close2 = close;
        runtime_core::driver::spawn_async(async move {
            match crate::servers::rename_channel(sid0, cid0, &trimmed).await {
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
    // Error > input. Each arm builds its own `Element`.
    //
    // The Rename button is built *inside* the render closure (not a pre-built
    // `let` splatted in): `Element` is not `Copy`, and a pre-built one moved
    // into an `Fn` render closure is gone after the first rebuild (E0507). The
    // `on_rename` closure captures only `Copy` signals, so it is `Fn` + `Copy`.
    let body = switch(
        move || error.get().clone(),
        move |err: &String| {
            // Static label computed at build time (see `join_modal.rs`); the
            // button is rebuilt per arm, so the label is pre-computed rather
            // than an inline `if/else` (which `ui!` would parse as a structural
            // child).
            let rename_label = if b.get() { "Renaming…".to_string() } else { "Rename".to_string() };
            let rename_btn = ui! {
                view(style = rename_btn_style.clone(), test_id = "rename-channel-rename") {
                    text(style = btn_label_style.clone()) { rename_label }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        on_rename();
                    }
                    TouchResponse::CONSUMED
                })
            };
            if !err.is_empty() {
                ui! {
                    view(style = card_style.clone()) {
                        text(style = title_style.clone()) { "Rename Channel".to_string() }
                        text_input(
                            value = name.clone(),
                            on_change = on_name_change.clone(),
                            placeholder = "Channel name",
                            style = input_style.clone(),
                            test_id = "rename-channel-name-input",
                        )
                        text(style = err_style.clone()) { err.clone() }
                        { rename_btn }
                    }
                }
            } else {
                ui! {
                    view(style = card_style.clone()) {
                        text(style = title_style.clone()) { "Rename Channel".to_string() }
                        text(style = sub_style.clone()) { "Give this channel a new name".to_string() }
                        view {
                            text_input(value = name.clone(), on_change = on_name_change.clone(), placeholder = "Channel name", style = input_style.clone(), test_id = "rename-channel-name-input")
                        }
                        { rename_btn }
                    }
                }
            }
        },
    );

    // --- Full-screen backdrop + centered card --------------------------------
    // The card wrapper consumes its own touch so a tap on the card does NOT
    // close; a tap on the backdrop (outside the card) closes the modal.
    let open_chk = open;
    let wrap_style = move || StyleRules {
        position: Some(Position::Absolute),
        left: Some(px(0.0)),
        top: Some(px(0.0)),
        width: Some(if open_chk.get() { pct(100.0) } else { px(0.0) }),
        height: Some(if open_chk.get() { pct(100.0) } else { px(0.0) }),
        background: Some(col(theme::OVERLAY)),
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
