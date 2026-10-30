//! `join_modal` — the "Join a Server" modal for the server rail.
//!
//! A centered card over a dimmed full-screen backdrop (the build-once/toggle
//! idiom of `invite_modal.rs`: when `open` is false the wrapper collapses to
//! zero size; when true it fills the window). It has three states:
//!
//! - **Input** (default): a `text_input` for the invite code above a **Join**
//!   button.
//! - **Success**: after a successful `redeem_invite` RPC, a brief
//!   confirmation (the returned role) above a **Done** button.
//! - **Error**: when the backend rejects the code (unknown, expired, fully
//!   spent), the backend's own message is shown inline above a **Try again**
//!   button that drops back to the input state (the input keeps the typed
//!   code so it can be corrected).
//!
//! The `open` / `code` / `result` / `error` / `busy` signals are owned by the
//! caller (`app.rs`), which resets `code`/`result`/`error`/`busy` on open so
//! a re-open always starts fresh. The only client-side validation is
//! non-emptiness; all real rejection (and its wording) comes from the
//! backend. On success the caller's `live_servers` list is refreshed so the
//! newly-joined server appears in the rail's live section (re-fetched via the
//! same `crate::servers::list_servers` RPC the rail already uses).
//!
//! Idioms: the three states are one `switch` keyed on `(result, error)`
//! (the `switch(key, render)` pattern used throughout `app.rs`); each arm
//! builds its own `Element` (Elements aren't `Copy`, so a pre-built one
//! would be moved on the first rebuild). `Signal` is `Copy`, so every `move`
//! closure captures a copy and stays `Fn`. Buttons that carry an `.on_touch`
//! chain are built as `let` bindings (the `close_btn` idiom in
//! `invite_modal.rs`) so the `.method` chain lives outside the `ui!` child
//! position.
use crate::theme;
use runtime_core::{
    component, switch, ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent,
    Overflow, Position, Signal, StyleRules, TouchPhase, TouchResponse,
};

use crate::style_helpers::{borderless_input, col, pct, px, rounded, with_radius};

// Discord dark palette (kept local so the component is self-contained).

/// The "Join a Server" modal. `code` is the invite code being typed;
/// `result` is `Some(RedeemResult)` once the backend responds successfully;
/// `error` holds the backend's rejection message on failure; `busy` is true
/// while the RPC is in flight (the Join button shows a pending label);
/// `servers` is the caller's live server list, re-fetched on a successful
/// redeem so the new server appears in the rail. All signals are owned by the
/// caller and reset on re-open.
#[component]
pub fn JoinServerModal(
    open: Signal<bool>,
    code: Signal<String>,
    result: Signal<Option<discord_server::invites::RedeemResult>>,
    error: Signal<String>,
    busy: Signal<bool>,
    servers: Signal<Vec<crate::servers::ServerSummary>>,
) -> Element {
    let close = open;
    let c = code;
    let r = result;
    let e = error;
    let b = busy;
    let srv = servers;

    // Shared card/field/button styles (one static pass).
    let card_style = StyleRules {
        width: Some(px(360.0)),
        padding_top: Some(px(20.0)),
        padding_bottom: Some(px(20.0)),
        padding_left: Some(px(20.0)),
        padding_right: Some(px(20.0)),
        background: Some(col(theme::BG_SECONDARY)),
        flex_direction: Some(FlexDirection::Column),
        ..rounded(12.0)
    };
    let title_style = StyleRules {
        font_size: Some(px(18.0)),
        font_weight: Some(FontWeight::Bold),
        color: Some(col(theme::TEXT_STRONG)),
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
        ..with_radius(borderless_input(), 8.0)
    };
    let btn_style = StyleRules {
        height: Some(px(40.0)),
        margin_top: Some(px(14.0)),
        background: Some(col(theme::ACCENT)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        flex_direction: Some(FlexDirection::Row),
        ..rounded(8.0)
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
    let ok_style = StyleRules {
        font_size: Some(px(14.0)),
        color: Some(col(theme::SUCCESS)),
        margin_top: Some(px(14.0)),
        ..Default::default()
    };

    // --- Join handler --------------------------------------------------------
    // `move` closure: `Signal` is `Copy`, so every captured copy stays `Fn`
    // across taps. The only client-side gate is non-emptiness; the backend's
    // rejection (unknown / expired / fully spent) is surfaced verbatim.
    let on_join = move || {
        let raw = c.get();
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return;
        }
        b.set(true);
        let code_owned = crate::invite::code_from_input(trimmed);
        let r2 = r;
        let e2 = e;
        let b2 = b;
        let srv2 = srv;
        runtime_core::driver::spawn_async(async move {
            match crate::invite::redeem_invite(&code_owned).await {
                Ok(res) => {
                    r2.set(Some(res));
                    e2.set(String::new());
                    // The backend persisted the membership; re-fetch the live
                    // server list (the same RPC the rail's live section uses)
                    // so the newly-joined server appears without a reload.
                    if let Ok(list) = crate::servers::list_servers().await {
                        srv2.set(list);
                    }
                }
                Err(msg) => {
                    r2.set(None);
                    e2.set(msg);
                }
            }
            b2.set(false);
        });
    };

    // --- Body: one switch keyed on (result, error) ---------------------------
    // Priority: success > error > input. Each arm builds its own `Element`
    // inline (the `invite_modal.rs` idiom — `Element` is not `Copy`). Buttons
    // that carry an `.on_touch` chain are built as `let` bindings so the
    // method chain lives outside the `ui!` child position.
    let body = switch(
        move || (result.get().map(|r| r.server_id), error.get().clone()),
        move |(res, err): &(Option<i64>, String)| {
            if res.is_some() {
                // --- Success arm: confirmation + Done --------------------------
                let full = result.get();
                let role = match full.as_ref() {
                    Some(rr) if rr.already_member => {
                        format!("You're already a member of this server (role: {})", rr.role)
                    }
                    Some(rr) => format!("Joined! Your role is: {}", rr.role),
                    None => "Joined!".to_string(),
                };
                let done_btn = ui! {
                    view(style = btn_style.clone(), test_id = "join-server-done") {
                        text(style = btn_label_style.clone()) { "Done".to_string() }
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            close.set(false);
                        }
                        TouchResponse::CONSUMED
                    })
                };
                ui! {
                    view(style = card_style.clone()) {
                        text(style = title_style.clone()) { "Join a Server".to_string() }
                        text(style = ok_style.clone()) { role }
                        { done_btn }
                    }
                }
            } else if !err.is_empty() {
                // --- Error arm: backend message + Try again --------------------
                let retry_btn = ui! {
                    view(style = btn_style.clone(), test_id = "join-server-retry") {
                        text(style = btn_label_style.clone()) { "Try again".to_string() }
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            // Drop back to the input state; the typed code stays in
                            // `code` so it can be corrected and resubmitted.
                            error.set(String::new());
                        }
                        TouchResponse::CONSUMED
                    })
                };
                ui! {
                    view(style = card_style.clone()) {
                        text(style = title_style.clone()) { "Join a Server".to_string() }
                        text(style = err_style.clone()) { err.clone() }
                        { retry_btn }
                    }
                }
            } else {
                // --- Input arm: code field + Join button -----------------------
                let join_label = if busy.get() { "Joining…" } else { "Join" };
                // The button is inlined as a `view` tag (not a `{ splat }`) so
                // that `text_input(...)` is followed by a *call*, not a block —
                // a `call { … }` would attach the block as the `text_input`'s
                // (non-existent) children.
                ui! {
                    view(style = card_style.clone()) {
                        text(style = title_style.clone()) { "Join a Server".to_string() }
                        // Two-way: `text_input(value = …)` alone only fills
                        // the box, so typing never reached `code`.
                        { crate::controls::text_field(code, "Invite code", input_style.clone()) }
                        view(style = btn_style.clone(), test_id = "join-server-join") {
                            text(style = btn_label_style.clone()) { join_label.to_string() }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                on_join();
                            }
                            TouchResponse::CONSUMED
                        })
                    }
                }
            }
        },
    );

    // --- Full-screen backdrop + centered card ---------------------------------
    // The card is built as a `let` so its `.on_touch` (consume, so a tap on the
    // card does NOT close) sits outside the `ui!` child position. A tap on the
    // backdrop (outside the card) closes the modal.
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
