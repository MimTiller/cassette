//! `invite_modal` — the "Invite People" modal for the live member panel.
//!
//! A full-screen overlay (the same build-once/toggle idiom as `profile_mode`:
//! when `open` is false the wrapper collapses to zero size; when true it fills
//! the window). It has three visual states:
//!
//! - **Create** (default): the `InviteExpirySelector` radio group (30 min / 1
//!   day / never) above a **Create** button.
//! - **Code**: after a successful `create_invite` RPC, the resulting code is
//!   shown via `InviteLinkDisplay` above a **Done** button.
//! - **Error**: when the backend rejects the call (e.g. insufficient
//!   privileges), the backend's message is shown inline.
//!
//! The `expiry` / `created` / `created_err` signals are owned by the caller
//! (`app_shell`), which also resets them when the modal is opened so a
//! re-open always starts fresh.
//!
//! Idioms: the three states are a `switch` keyed on `(created, created_err)`
//! (the `switch(key, render)` pattern used throughout `app.rs`); each arm
//! builds its own `Element` (Elements aren't `Copy`, so a pre-built one
//! would be moved on the first rebuild). `Signal` is `Copy`, so every `move`
//! closure captures a copy and stays `Fn`.

use crate::theme;
use runtime_core::{
    component, switch, ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent,
    Overflow, Position, Signal, StyleRules, TouchPhase, TouchResponse,
};

use crate::invite_expiry_selector::InviteExpirySelector;
use crate::invite_link_display::InviteLinkDisplay;
use crate::style_helpers::{col, pct, px};

// Discord dark palette (kept local so the component is self-contained).

const MIN_30_SECS: i64 = 30 * 60;
const DAY_SECS: i64 = 24 * 60 * 60;

/// The "Invite People" full-screen modal. `server_id` is the caller's
/// `active_live_server` (`None` when not in live mode — the Create button is
/// a no-op, the same inert-outside-live idiom as the channel-management
/// modals); `expiry` is the selected option index (0 = 30 min, 1 = 1 day,
/// 2 = never); `created` is `Some(invite)` once the backend responds
/// successfully; `created_err` holds the backend's rejection message on
/// failure. All five signals are owned by the caller and reset on re-open.
#[component]
#[allow(non_snake_case)]
pub fn InviteModal(
    open: Signal<bool>,
    server_id: Signal<Option<i64>>,
    expiry: Signal<usize>,
    created: Signal<Option<discord_server::invites::Invite>>,
    created_err: Signal<String>,
) -> Element {
    let close = open;

    // --- Close button ("Done" pill in the header) --------------------------
    let close_label_style = StyleRules {
        color: Some(col(theme::TEXT_STRONG)),
        font_size: Some(px(12.0)),
        font_weight: Some(FontWeight::SemiBold),
        ..Default::default()
    };
    let close_pill_style = StyleRules {
        height: Some(px(26.0)),
        padding_left: Some(px(12.0)),
        padding_right: Some(px(12.0)),
        background: Some(col(theme::BG_SECONDARY)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        border_top_left_radius: Some(px(6.0)),
        border_top_right_radius: Some(px(6.0)),
        border_bottom_left_radius: Some(px(6.0)),
        border_bottom_right_radius: Some(px(6.0)),
        ..Default::default()
    };
    let close_btn = ui! {
        view(style = close_pill_style, test_id = "invite-close") {
            text(style = close_label_style) { "Done".to_string() }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                close.set(false);
            }
            TouchResponse::CONSUMED
        })
    };

    // --- Create button ------------------------------------------------------
    // Captures `server_id` / `expiry` / `created` / `created_err` (all `Signal`,
    // `Copy`) — the `move` closure stays `Fn` across taps. `server_id` is the
    // caller's `active_live_server`, read at submit time: `None` outside live
    // mode makes Create a no-op (the same inert-outside-live idiom as the
    // channel-management modals).
    let sid = server_id;
    let exp = expiry;
    let cr = created;
    let cerr = created_err;
    let on_create = move || {
        let Some(sid0) = sid.get() else {
            return;
        };
        let idx = exp.get();
        let now = runtime_shared::time::epoch_millis() / 1_000;
        let expires_at: Option<i64> = match idx {
            0 => Some(now + MIN_30_SECS),
            1 => Some(now + DAY_SECS),
            _ => None, // "Never" — no expiry
        };
        runtime_core::driver::spawn_async(async move {
            match crate::invite::create_invite(sid0, expires_at, None).await {
                Ok(invite) => {
                    cr.set(Some(invite));
                    cerr.set(String::new());
                }
                Err(msg) => cerr.set(msg),
            }
        });
    };

    let create_btn_style = StyleRules {
        height: Some(px(40.0)),
        margin_top: Some(px(16.0)),
        background: Some(col(theme::ACCENT)),
        color: Some(col(theme::TEXT_STRONG)),
        font_size: Some(px(15.0)),
        font_weight: Some(FontWeight::SemiBold),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        flex_direction: Some(FlexDirection::Row),
        ..Default::default()
    };
    let create_btn_label = StyleRules {
        font_size: Some(px(15.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(theme::TEXT_STRONG)),
        ..Default::default()
    };

    // --- Error line ---------------------------------------------------------
    let err_style = StyleRules {
        font_size: Some(px(13.0)),
        color: Some(col(theme::DANGER)),
        text_align: Some(runtime_core::TextAlign::Center),
        margin_top: Some(px(12.0)),
        ..Default::default()
    };

    // --- Header -------------------------------------------------------------
    let header_style = StyleRules {
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::SpaceBetween),
        padding_top: Some(px(16.0)),
        padding_bottom: Some(px(16.0)),
        padding_left: Some(px(16.0)),
        padding_right: Some(px(16.0)),
        ..Default::default()
    };
    let header_title = StyleRules {
        font_size: Some(px(18.0)),
        font_weight: Some(FontWeight::Bold),
        color: Some(col(theme::TEXT_STRONG)),
        ..Default::default()
    };
    let header = ui! {
        view(style = header_style) {
            text(style = header_title) { "Invite People".to_string() }
            { close_btn }
        }
    };

    // --- Body: a switch keyed on (created, created_err) ---------------------
    // Each arm builds its own `Element` (Elements aren't `Copy`). The create
    // arm (created == None) shows the expiry selector + Create button; the
    // code arm (created == Some) shows the `InviteLinkDisplay`; the error arm
    // shows the backend's message.
    let body = switch(
        move || (created.get().map(|i| i.code.clone()), created_err.get().clone()),
        move |(created, err): &(Option<String>, String)| {
            if created.is_some() {
                // Code arm: the real code from the backend.
                let code = created.as_ref().unwrap().clone();
                let wrap = StyleRules {
                    padding_top: Some(px(16.0)),
                    padding_bottom: Some(px(16.0)),
                    padding_left: Some(px(16.0)),
                    padding_right: Some(px(16.0)),
                    ..Default::default()
                };
                ui! {
                    view(style = wrap) {
                        InviteLinkDisplay(code = code)
                    }
                }
            } else if !err.is_empty() {
                // Error arm.
                ui! {
                    view(style = StyleRules {
                        padding_top: Some(px(16.0)),
                        padding_bottom: Some(px(16.0)),
                        padding_left: Some(px(16.0)),
                        padding_right: Some(px(16.0)),
                        ..Default::default()
                    }) {
                        text(style = err_style.clone()) { err.clone() }
                    }
                }
            } else {
                // Create arm: expiry selector + Create button. Both are built
                // fresh inside this arm (the `switch` render is `Fn` and re-runs,
                // so a pre-built `Element` from the outer scope would be moved
                // on the first rebuild — the `Fn`-rebuild idiom the doc header
                // names). `on_create` is `Copy` (its captures are all `Copy`
                // signals), and the two styles are cloned, so the arm closure
                // captures copies and stays `Fn`.
                let on_create = on_create;
                let style = create_btn_style.clone();
                let lbl = create_btn_label.clone();
                let expiry_sel = ui! {
                    InviteExpirySelector(value = expiry)
                };
                let create_btn = ui! {
                    view(style = style, test_id = "invite-create") {
                        text(style = lbl) { "Create".to_string() }
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            on_create();
                        }
                        TouchResponse::CONSUMED
                    })
                };
                ui! {
                    view(style = StyleRules {
                        padding_top: Some(px(16.0)),
                        padding_bottom: Some(px(16.0)),
                        padding_left: Some(px(16.0)),
                        padding_right: Some(px(16.0)),
                        ..Default::default()
                    }) {
                        { expiry_sel }
                        { create_btn }
                    }
                }
            }
        },
    );

    // --- Full-screen wrapper (collapses to zero when `open` is false) -------
    let open_chk = open;
    let wrap_style = move || StyleRules {
        position: Some(Position::Absolute),
        left: Some(px(0.0)),
        top: Some(px(0.0)),
        width: Some(if open_chk.get() { pct(100.0) } else { px(0.0) }),
        height: Some(if open_chk.get() { pct(100.0) } else { px(0.0) }),
        background: Some(col(theme::BG_PRIMARY)),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };

    ui! {
        view(style = wrap_style) {
            { header }
            view(style = StyleRules {
                flex_grow: Some(runtime_core::Tokenized::Literal(1.0)),
                overflow: Some(Overflow::Hidden),
                ..Default::default()
            }) {
                body
            }
        }
    }
}

