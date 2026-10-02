//! `server_settings_panel` — the LIVE server's own settings overlay: rename the
//! server, set/clear its icon (a URL, empty clears), and transfer ownership
//! to another member by numeric user id.
//!
//! A full-screen card over a dimmed backdrop (the build-once/toggle idiom of
//! `delete_channel_modal.rs` / `create_channel_modal.rs`). The `open` /
//! `busy` / `error` / `servers` / `server_id` / `name` / `icon` /
//! `new_owner_id` signals are all owned by the caller (`app.rs`), which seeds
//! `name`/`icon` from the active live server on open. `server_id` is read at
//! submit time (`None` outside live mode = a no-op), so the panel is inert
//! outside live mode — the same shape the channel-management modals use.
//!
//! Permission model: **no client-side permission logic beyond the
//! owner-only gate on the transfer-ownership section**. The backend enforces
//! every real permission check (owner to rename, owner/admin to change the
//! icon, owner to transfer, and that the target is a member); on a rejection
//! the backend's own message is shown verbatim in the red `error` line. After
//! any successful op the caller's `servers` list (and thus the rail's live
//! section) is re-fetched via `crate::servers::list_servers`, so the renamed
//! name / new icon appear without a reload.
//!
//! Idioms (verified against `delete_channel_modal.rs`): one `switch` keyed on
//! `busy`; each section's button label flips to its busy verb; `Signal` is
//! `Copy`, so every `move` closure captures a copy and stays `Fn`; the
//! section buttons are built *inside* the render closure (a pre-built
//! `Element` moved into an `Fn` render closure is gone after the first
//! rebuild, E0507). The owner gate is a build-time lookup of the signed-in
//! user's own row in `live_members` (`role == "owner"`).
use runtime_core::{
    component, switch, ui, AlignItems, Element, FlexDirection, FontWeight,
    JustifyContent, Overflow, Position, Signal, StyleRules, TouchPhase, TouchResponse,
};

use crate::style_helpers::{col, pct, px};

// Discord dark palette (kept local so the component is self-contained).
const CARD: &str = "#2b2d31";
const INPUT: &str = "#1e1f22";
const BTN_SAVE: &str = "#4e5058";
const BTN_TRANSFER: &str = "#248046";
const TEXT: &str = "#f2f3f5";
const TEXT_MUTED: &str = "#949ba4";
const ERROR: &str = "#f23f43";

/// The LIVE server-settings panel. `open` gates the overlay; `server_id` is
/// the target (read at submit time, `None` = no-op); `name`/`icon`/
/// `new_owner_id` are the three inputs (seeded by the caller on open);
/// `busy` is true while an RPC is in flight; `error` holds the backend's
/// rejection message; `servers` is the caller's live server list, re-fetched
/// on any successful op so the rail's name/icon refresh; `members` is the
/// active live server's roster, used only to gate the owner-only
/// transfer-ownership section.
#[component]
#[allow(non_snake_case)]
pub fn ServerSettingsPanel(
    open: Signal<bool>,
    server_id: Signal<Option<i64>>,
    name: Signal<String>,
    icon: Signal<String>,
    new_owner_id: Signal<String>,
    busy: Signal<bool>,
    error: Signal<String>,
    servers: Signal<Vec<crate::servers::ServerSummary>>,
    members: Signal<Vec<crate::moderation::MemberInfo>>,
) -> Element {
    let close = open;
    let sid = server_id;
    let nm = name;
    let ic = icon;
    let owner = new_owner_id;
    let b = busy;
    let e = error;
    let srv = servers;
    let mbrs = members;

    // --- Shared styles (one static pass) ------------------------------------
    let card_style = StyleRules {
        width: Some(px(380.0)),
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
        align_items: Some(AlignItems::Stretch),
        ..Default::default()
    };
    let header_style = StyleRules {
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::SpaceBetween),
        margin_bottom: Some(px(16.0)),
        ..Default::default()
    };
    let title_style = StyleRules {
        font_size: Some(px(18.0)),
        font_weight: Some(FontWeight::Bold),
        color: Some(col(TEXT)),
        ..Default::default()
    };
    let close_style = StyleRules {
        height: Some(px(26.0)),
        padding_left: Some(px(12.0)),
        padding_right: Some(px(12.0)),
        background: Some(col(BTN_SAVE)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        border_top_left_radius: Some(px(6.0)),
        border_top_right_radius: Some(px(6.0)),
        border_bottom_left_radius: Some(px(6.0)),
        border_bottom_right_radius: Some(px(6.0)),
        flex_direction: Some(FlexDirection::Row),
        flex_shrink: Some(runtime_core::Tokenized::Literal(0.0)),
        ..Default::default()
    };
    let close_label_style = StyleRules {
        font_size: Some(px(12.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT)),
        ..Default::default()
    };
    let section_style = StyleRules {
        flex_direction: Some(FlexDirection::Column),
        margin_top: Some(px(14.0)),
        gap: Some(px(6.0)),
        ..Default::default()
    };
    let label_style = StyleRules {
        font_size: Some(px(11.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT_MUTED)),
        text_transform: Some(runtime_core::TextTransform::Uppercase),
        letter_spacing: Some(runtime_core::Tokenized::Literal(0.5)),
        ..Default::default()
    };
    let input_style = StyleRules {
        height: Some(px(36.0)),
        background: Some(col(INPUT)),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        padding_left: Some(px(10.0)),
        padding_right: Some(px(10.0)),
        align_items: Some(AlignItems::Center),
        ..Default::default()
    };
    let save_btn_style = StyleRules {
        height: Some(px(34.0)),
        background: Some(col(BTN_SAVE)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        flex_direction: Some(FlexDirection::Row),
        ..Default::default()
    };
    let transfer_btn_style = StyleRules {
        height: Some(px(34.0)),
        background: Some(col(BTN_TRANSFER)),
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
        font_size: Some(px(14.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT)),
        ..Default::default()
    };
    let err_style = StyleRules {
        font_size: Some(px(12.0)),
        color: Some(col(ERROR)),
        margin_top: Some(px(10.0)),
        ..Default::default()
    };

    // --- Handlers ------------------------------------------------------------
    // Each is a `Copy` `move` closure (it captures only `Copy` `Signal`s), so a
    // render-closure `.on_touch` can call it and the `async move` block gets its
    // own copies — the exact shape of `delete_channel_modal.rs`'s `on_delete`.
    // No client-side permission check: the backend decides and its message is
    // surfaced verbatim in `error`; on success the caller's `servers` list is
    // re-fetched (and the name/icon inputs are nudged to the new values).
    let do_rename = move || {
        let Some(sid0) = sid.get() else {
            return;
        };
        let new_name = nm.get();
        e.set(String::new());
        b.set(true);
        let b2 = b;
        let e2 = e;
        let srv2 = srv;
        let nm2 = nm;
        let ic2 = ic;
        runtime_core::driver::spawn_async(async move {
            match crate::servers::rename_server(sid0, &new_name).await {
                Ok(r) => {
                    if let Ok(list) = crate::servers::list_servers().await {
                        srv2.set(list);
                    }
                    nm2.set(r.name.clone());
                    ic2.set(r.icon_url.clone().unwrap_or_default());
                }
                Err(msg) => e2.set(msg),
            }
            b2.set(false);
        });
    };
    let do_icon = move || {
        let Some(sid0) = sid.get() else {
            return;
        };
        let raw = ic.get();
        let trimmed = raw.trim();
        let icon_opt: Option<String> = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
        e.set(String::new());
        b.set(true);
        let b2 = b;
        let e2 = e;
        let srv2 = srv;
        let ic2 = ic;
        runtime_core::driver::spawn_async(async move {
            match crate::servers::set_server_icon(sid0, icon_opt).await {
                Ok(r) => {
                    if let Ok(list) = crate::servers::list_servers().await {
                        srv2.set(list);
                    }
                    ic2.set(r.icon_url.clone().unwrap_or_default());
                }
                Err(msg) => e2.set(msg),
            }
            b2.set(false);
        });
    };
    let do_transfer = move || {
        let Some(sid0) = sid.get() else {
            return;
        };
        let raw = owner.get();
        match raw.trim().parse::<i64>() {
            Ok(new_owner_id) => {
                e.set(String::new());
                b.set(true);
                let b2 = b;
                let e2 = e;
                let srv2 = srv;
                let mbrs2 = mbrs;
                runtime_core::driver::spawn_async(async move {
                    match crate::servers::transfer_server_ownership(sid0, new_owner_id).await {
                        Ok(_) => {
                            if let Ok(list) = crate::servers::list_servers().await {
                                srv2.set(list);
                            }
                            // Ownership just changed, so re-pull the roster: the
                            // owner-only gate on the transfer section reads the
                            // caller's `role` from it, and it needs to flip off
                            // for the (now former) owner without a server re-select.
                            if let Ok(members) = crate::moderation::list_members(sid0).await {
                                mbrs2.set(members);
                            }
                        }
                        Err(msg) => e2.set(msg),
                    }
                    b2.set(false);
                });
            }
            Err(_) => {
                e.set("Enter the new owner's numeric user id".to_string());
            }
        }
    };

    // Owner-only gate on the transfer-ownership section: computed *inside* the
    // render closure (below) so it re-evaluates on every rebuild — in
    // particular after a successful transfer, when `do_transfer` re-pulls the
    // roster and the `busy`/`error` key flips, so the section flips off for the
    // (now former) owner without a server re-select. The signed-in user's own
    // row in the active live server's roster is checked for `role == "owner"`;
    // `CURRENT_USER_ID` is `0` when signed out, which can't match a real member
    // id, so the section simply never appears. Everything else is left to the
    // backend's real permission checks.

    // --- Body: one switch keyed on `(busy, error)` ---------------------------
    // Each section's button label flips to its busy verb while an RPC is in
    // flight; the error key re-runs the closure when the backend's message
    // lands. The buttons are built *inside* the render closure (a pre-built
    // `Element` moved into an `Fn` render closure is gone after the first
    // rebuild, E0507). `do_rename`/`do_icon`/`do_transfer` capture only
    // `Copy` signals, so they're `Copy` + `Fn` and can be invoked here.
    let body = switch(
        move || (b.get(), e.get()),
        move |(busy, err): &(bool, String)| {
            let self_id: i64 = crate::auth::CURRENT_USER_ID
                .load(std::sync::atomic::Ordering::SeqCst);
            let is_owner: bool = mbrs
                .get()
                .iter()
                .any(|m| m.user_id == self_id && m.role == "owner");
            let rename_label = if *busy { "Renaming…" } else { "Save name" };
            let icon_label = if *busy { "Saving…" } else { "Save icon" };
            let transfer_label = if *busy { "Transferring…" } else { "Transfer" };
            let rename_btn = ui! {
                view(style = save_btn_style.clone(), test_id = "server-rename-save") {
                    text(style = btn_label_style.clone()) { rename_label.to_string() }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        do_rename();
                    }
                    TouchResponse::CONSUMED
                })
            };
            let icon_btn = ui! {
                view(style = save_btn_style.clone(), test_id = "server-icon-save") {
                    text(style = btn_label_style.clone()) { icon_label.to_string() }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        do_icon();
                    }
                    TouchResponse::CONSUMED
                })
            };
            let transfer_btn = ui! {
                view(style = transfer_btn_style.clone(), test_id = "server-transfer-confirm") {
                    text(style = btn_label_style.clone()) { transfer_label.to_string() }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        do_transfer();
                    }
                    TouchResponse::CONSUMED
                })
            };
            let row_col = move || StyleRules {
                flex_direction: Some(FlexDirection::Column),
                ..Default::default()
            };
            let name_row = ui! {
                view(style = row_col()) {
                    text(style = label_style.clone()) { "Server name".to_string() }
                    text_input(value = nm, placeholder = "New server name", style = input_style.clone())
                    rename_btn
                }
            };
            let icon_row = ui! {
                view(style = row_col()) {
                    text(style = label_style.clone()) { "Server icon (URL)".to_string() }
                    text_input(value = ic, placeholder = "https://… (empty to clear)", style = input_style.clone())
                    icon_btn
                }
            };
            let transfer_row = ui! {
                view(style = row_col()) {
                    text(style = label_style.clone()) { "Transfer ownership".to_string() }
                    text(style = StyleRules {
                        font_size: Some(px(11.0)),
                        color: Some(col(TEXT_MUTED)),
                        ..Default::default()
                    }) { "Pass ownership to another member's numeric user id.".to_string() }
                    text_input(value = owner, placeholder = "New owner user id", style = input_style.clone())
                    transfer_btn
                }
            };
            let header = ui! {
                view(style = header_style.clone()) {
                    text(style = title_style.clone()) { "Server Settings".to_string() }
                    view(style = close_style.clone()) {
                        text(style = close_label_style.clone()) { "Done".to_string() }
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            close.set(false);
                        }
                        TouchResponse::CONSUMED
                    })
                }
            };
            // The inline error line and the owner-gated transfer section are
            // each either the real element or an empty (zero-size) `view`, so
            // the card is a single `ui!` with no nested in-`ui!` conditionals
            // (those wrap their branch in a stored reactivity closure that
            // can't hold the switch's `&String` `err` borrow — the reason the
            // other modals lift their conditions out to a top-level `if/else`).
            let err_el: Element = if !err.is_empty() {
                ui! {
                    text(style = err_style.clone()) { err.to_string() }
                }
            } else {
                ui! {
                    view {}
                }
            };
            let transfer_el: Element = if is_owner {
                transfer_row
            } else {
                ui! {
                    view {}
                }
            };
            ui! {
                view(style = card_style.clone()) {
                    { header }
                    { name_row }
                    { icon_row }
                    { transfer_el }
                    { err_el }
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
