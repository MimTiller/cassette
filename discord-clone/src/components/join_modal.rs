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
use std::rc::Rc;

use runtime_core::{
    component, switch, ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent,
    Overflow, Position, Signal, StyleRules, TouchPhase, TouchResponse,
};

use crate::style_helpers::{col, pct, px, rounded};

// Discord dark palette (kept local so the component is self-contained).

/// The "Join a Server" modal. `code` is the invite code being typed;
/// `result` is `Some(RedeemResult)` once the backend responds successfully;
/// `error` holds the backend's rejection message on failure; `busy` is true
/// while the RPC is in flight (the Join button shows a pending label);
/// `servers` is the caller's live server list, re-fetched on a successful
/// redeem so the new server appears in the rail; `on_enter` is called with the
/// joined server's id on success so the caller auto-enters the new server
/// (the enter-live pattern the rail's tile handler and Create modal use).
/// All signals are owned by the caller and reset on re-open.
#[component]
pub fn JoinServerModal(
    open: Signal<bool>,
    code: Signal<String>,
    result: Signal<Option<discord_server::invites::RedeemResult>>,
    error: Signal<String>,
    busy: Signal<bool>,
    servers: Signal<Vec<crate::servers::ServerSummary>>,
    on_enter: Option<Rc<dyn Fn(i64)>>,
) -> Element {
    let close = open;
    let c = code;
    let r = result;
    let e = error;
    let b = busy;
    let srv = servers;
    let enter = on_enter;

    // Shared card/field/button styles (one static pass).
    let card_style = StyleRules {
        padding_top: Some(px(20.0)),
        padding_bottom: Some(px(20.0)),
        padding_left: Some(px(20.0)),
        padding_right: Some(px(20.0)),
        ..crate::style_helpers::modal_card(360.0)
    };
    let title_style = StyleRules {
        font_size: Some(px(18.0)),
        font_weight: Some(FontWeight::Bold),
        color: Some(col(theme::TEXT_STRONG)),
        ..Default::default()
    };
    let input_style = StyleRules {
        margin_top: Some(px(16.0)),
        ..crate::style_helpers::text_field_style()
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
    // `spawn_then_in` anchored on the app shell: redeem and the list refresh
    // happen in the future, every signal write happens in the callback, so a
    // torn-down scope can't land a write on a freed slot.
    let on_join: Rc<dyn Fn()> = Rc::new(move || {
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
        let enter = enter.clone();
        runtime_core::spawn_then_in(
            &crate::app::shell_alive(),
            async move {
                let joined = crate::invite::redeem_invite(&code_owned).await;
                let list = crate::servers::list_servers().await.ok().unwrap_or_else(Vec::new);
                (joined, list)
            },
            move |(joined, list)| {
                match joined {
                    Ok(res) => {
                        let sid = res.server_id;
                        r2.set(Some(res));
                        e2.set(String::new());
                        srv2.set(list);
                        // Auto-enter the joined server: the caller's handler
                        // selects it and pulls its channels/roster/unread.
                        if let Some(cb) = enter {
                            cb(sid);
                        }
                    }
                    Err(msg) => {
                        r2.set(None);
                        e2.set(msg);
                    }
                }
                b2.set(false);
            },
        );
    });

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
                let close_c = ui! { crate::controls::modal_close_button(move || close.set(false)) };
                ui! {
                    view(style = card_style.clone()) {
                        text(style = title_style.clone()) { "Join a Server".to_string() }
                        text(style = ok_style.clone()) { role }
                        view(style = StyleRules {
                            margin_top: Some(px(8.0)),
                            ..crate::style_helpers::footer_strip()
                        }) {
                            { done_btn }
                        }
                        { close_c }
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
                let close_c = ui! { crate::controls::modal_close_button(move || close.set(false)) };
                ui! {
                    view(style = card_style.clone()) {
                        text(style = title_style.clone()) { "Join a Server".to_string() }
                        text(style = err_style.clone()) { err.clone() }
                        view(style = StyleRules {
                            margin_top: Some(px(8.0)),
                            ..crate::style_helpers::footer_strip()
                        }) {
                            { retry_btn }
                        }
                        { close_c }
                    }
                }
            } else {
                // --- Input arm: code field + Join button -----------------------
                let close_c = ui! { crate::controls::modal_close_button(move || close.set(false)) };
                let join_label = if busy.get() { "Joining…" } else { "Join" };
                let join_fn = on_join.clone();
                // The input carries `on_change` so typing reaches `code`
                // (`value =` alone only fills the box), and is built in its own
                // `ui!` so the sibling splat below cannot parse as its children.
                let code_input = ui! {
                    text_input(
                        value = code,
                        on_change = move |v: String| code.set(v),
                        placeholder = "Invite code",
                        test_id = "join-server-code-input",
                        style = input_style.clone(),
                    )
                };
                ui! {
                    view(style = card_style.clone()) {
                        text(style = title_style.clone()) { "Join a Server".to_string() }
                        { code_input }
                        { close_c }
                        view(style = StyleRules {
                            margin_top: Some(px(8.0)),
                            ..crate::style_helpers::footer_strip()
                        }) {
                            view(style = btn_style.clone(), test_id = "join-server-join") {
                                text(style = btn_label_style.clone()) { join_label.to_string() }
                            }
                            .on_touch(move |ev| {
                                if ev.phase == TouchPhase::Ended {
                                    join_fn();
                                }
                                TouchResponse::CONSUMED
                            })
                        }
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
