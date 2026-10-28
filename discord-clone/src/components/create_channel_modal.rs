//! `create_channel_modal` — the "New Channel" modal for a live server's
//! channel sidebar.
//!
//! A centered card over a dimmed full-screen backdrop (the build-once/toggle
//! idiom of `join_modal.rs` / `invite_modal.rs`: when `open` is false the
//! wrapper collapses to zero size; when true it fills the window). It has
//! three visual states:
//!
//! - **Input** (default): a channel-name `text_input`, a two-button kind
//!   toggle ("Text" / "Voice"), and a **Create** button.
//! - **Error**: when the backend rejects the call (e.g. insufficient
//!   privileges), the backend's own message is shown inline above the Create
//!   button (the input keeps the typed name so it can be corrected).
//!
//! The `open` / `name` / `kind` / `busy` / `error` signals are owned by the
//! caller (`app.rs`), which resets them on open so a re-open starts fresh.
//! `server_id` is the caller's `active_live_server` (read at submit time — the
//! modal is built once and lives across live-server switches). `channels` is
//! the caller's live channel list; on a successful create it is re-fetched via
//! `crate::servers::list_channels` so the new channel appears without a reload
//! (the same refresh `join_modal.rs` uses for `live_servers`). The only
//! client-side gate is a non-empty name; all real rejection and its wording
//! comes from the backend.
//!
//! Idioms: the states are one `switch` keyed on `(error, busy)`; each arm
//! builds its own `Element` (`Element` is not `Copy`). `Signal` is `Copy`, so
//! every `move` closure captures a copy and stays `Fn`. Buttons carrying an
//! `.on_touch` chain are built as `let` bindings (the `close_btn` idiom in
//! `invite_modal.rs`) so the method chain lives outside the `ui!` child
//! position.
use crate::theme;
use runtime_core::{
    component, switch, ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent,
    Overflow, Position, Signal, StyleRules, TouchPhase, TouchResponse,
};

use crate::style_helpers::{borderless_input, col, pct, px};

// Discord dark palette (kept local so the component is self-contained).

/// The "New Channel" modal for a live server. `server_id` is the caller's
/// `active_live_server` (`None` when not in live mode — submit is a no-op);
/// `name` is the typed channel name; `kind` is `"text"` or `"voice"`; `busy`
/// is true while the RPC is in flight; `error` holds the backend's rejection
/// message; `channels` is the live channel list, re-fetched on a successful
/// create. All signals are owned by the caller and reset on re-open.
#[component]
pub fn CreateChannelModal(
    open: Signal<bool>,
    server_id: Signal<Option<i64>>,
    name: Signal<String>,
    kind: Signal<String>,
    busy: Signal<bool>,
    error: Signal<String>,
    channels: Signal<Vec<crate::servers::ChannelSummary>>,
) -> Element {
    let close = open;
    let sid = server_id;
    let n = name;
    let k = kind;
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
    let seg_row_style = StyleRules {
        margin_top: Some(px(12.0)),
        flex_direction: Some(FlexDirection::Row),
        gap: Some(px(8.0)),
        ..Default::default()
    };
    // The kind segment buttons: one `StyleRules` per button, driven by whether
    // the current `kind` signal matches (re-runs reactively on toggle).
    let seg_style = move |selected: bool| StyleRules {
        height: Some(px(32.0)),
        flex_grow: Some(runtime_core::Tokenized::Literal(1.0)),
        background: Some(col(if selected { theme::ACCENT } else { theme::BUTTON_SECONDARY })),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        ..Default::default()
    };
    let seg_label_style = StyleRules {
        font_size: Some(px(14.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(theme::TEXT_STRONG)),
        ..Default::default()
    };
    let create_btn_style = StyleRules {
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

    // --- Create handler ------------------------------------------------------
    // `move` closure: every captured copy is `Copy`, so it stays `Fn` across
    // taps. The client-side gates are: (1) the in-flight `busy` guard — without
    // it a fast double-tap fires two `create_channel` RPCs and mints two
    // duplicate channels (the backend has no unique-name constraint and never
    // rejects "already taken", so the client guard is the only protection);
    // the same guard the `create_server_modal` uses. (2) A non-empty name (and
    // a live server). The backend's rejection is surfaced verbatim in `error`.
    // The `name` signal is the input's source of truth. `value = name` is the
    // signal→widget leg only: keystrokes arrive via `on_change` (per the
    // `input_with_submit.rs` recipe — "without this, the signal never updates
    // and `get()` is always empty"). Without this callback the `name` signal
    // stayed empty while the DOM field showed the typed text, so Create always
    // hit the empty-name guard and silently did nothing. `Signal` is `Copy`,
    // so this closure captures a copy of `n` and stays `Fn` on each keystroke.
    let on_name_change = move |s: String| n.set(s);

    let on_create = move || {
        if b.get() {
            return;
        }
        let raw = n.get();
        let trimmed = raw.trim().to_string();
        let Some(sid0) = sid.get() else {
            return;
        };
        if trimmed.is_empty() {
            return;
        }
        let kind0 = k.get();
        b.set(true);
        e.set(String::new());
        let e2 = e;
        let b2 = b;
        let ch2 = ch;
        let close2 = close;
        runtime_core::driver::spawn_async(async move {
            match crate::servers::create_channel(sid0, &trimmed, &kind0, None).await {
                Ok(_) => {
                    // The backend persisted the channel; re-fetch the live
                    // channel list (the same RPC the live sidebar's row fetch
                    // uses) so the new channel appears without a reload, then
                    // close the modal.
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

    // --- Body: one switch keyed on (error, busy) -----------------------------
    // Error > input. The busy state only changes the Create button label, so
    // it stays on the input arm. Each arm builds its own `Element`.
    //
    // The interactive elements (the two kind segments + the Create button) are
    // built *inside* the render closure: `Element` is not `Copy`, and a
    // pre-built one splatted into an `Fn` render closure is moved out on the
    // first rebuild and gone the second (E0507). Building them here per rebuild
    // is the `login_screen`/`whiteboard` idiom in this file. Every closure they
    // reference (`seg_style`, `on_create`) captures only `Copy` signals, so it
    // is itself `Fn` + `Copy` and can be captured by the `move` render closure.
    let body = switch(
        // `kind == "category"` is in the key: a category gets its own title
        // and no Text/Voice toggle (the server menu's Create Category opens
        // this modal with that kind preset).
        move || (error.get().clone(), busy.get(), k.get() == "category"),
        move |(err, _busy, is_category): &(String, bool, bool)| {
            // Kind segments: each sets `kind`; the active fill reads `kind`
            // reactively (the style closure re-runs on toggle).
            let text_seg = ui! {
                view(style = move || seg_style(k.get() == "text"), test_id = "create-channel-kind-text") {
                    text(style = seg_label_style.clone()) { "Text".to_string() }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        k.set("text".to_string());
                    }
                    TouchResponse::CONSUMED
                })
            };
            let voice_seg = ui! {
                view(style = move || seg_style(k.get() == "voice"), test_id = "create-channel-kind-voice") {
                    text(style = seg_label_style.clone()) { "Voice".to_string() }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        k.set("voice".to_string());
                    }
                    TouchResponse::CONSUMED
                })
            };
            // The Create label is a static `String` computed at element-build
            // time (the `join_label` idiom in `join_modal.rs`): an inline
            // `if/else` returning `&str` inside `text { … }` is parsed as a
            // structural child, not text content, so it is pre-computed.
            let create_label = if b.get() { "Creating…".to_string() } else { "Create".to_string() };
            let (title, sub, placeholder) = if *is_category {
                ("New Category", "Group channels under a heading", "Category name")
            } else {
                ("New Channel", "Add a channel to this server", "Channel name")
            };
            let kind_row = || -> Element {
                if *is_category {
                    return ui! { view(style = StyleRules { height: Some(px(0.0)), ..Default::default() }) {} };
                }
                ui! { view(style = seg_row_style.clone()) { { text_seg } { voice_seg } } }
            };
            let create_btn = ui! {
                view(style = create_btn_style.clone(), test_id = "create-channel-create") {
                    text(style = btn_label_style.clone()) { create_label }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        on_create();
                    }
                    TouchResponse::CONSUMED
                })
            };
            if !err.is_empty() {
                ui! {
                    view(style = card_style.clone()) {
                        text(style = title_style.clone()) { title.to_string() }
                        text(style = sub_style.clone()) { sub.to_string() }
                        view(style = StyleRules { flex_direction: Some(FlexDirection::Column), ..Default::default() }) {
                            text_input(
                                value = name.clone(),
                                on_change = on_name_change.clone(),
                                placeholder = placeholder.to_string(),
                                style = input_style.clone(),
                                test_id = "create-channel-name-input",
                            )
                        }
                        { kind_row() }
                        text(style = err_style.clone()) { err.clone() }
                        { create_btn }
                    }
                }
            } else {
                ui! {
                    view(style = card_style.clone()) {
                        text(style = title_style.clone()) { title.to_string() }
                        text(style = sub_style.clone()) { sub.to_string() }
                        view(style = StyleRules { flex_direction: Some(FlexDirection::Column), ..Default::default() }) {
                            text_input(
                                value = name.clone(),
                                on_change = on_name_change.clone(),
                                placeholder = placeholder.to_string(),
                                style = input_style.clone(),
                                test_id = "create-channel-name-input",
                            )
                        }
                        { kind_row() }
                        { create_btn }
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
