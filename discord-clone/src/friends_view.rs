//! `friends_view` — the LIVE friends-list panel, opened from the "Friends"
//! entry at the top of the live DM list column.
//!
//! Layout: a column — a "Friends" header line (icon + title), a row of tab
//! pills (Online / All / Pending / Add Friend) INLINE in one header row, then
//! the active tab's rows inside a scrollable list. Each friend row is ONE
//! line: avatar chip, username on the left, and the actions (Message / Remove
//! for accepted, Accept / Decline for incoming, Cancel for outgoing) on the
//! right, with even row spacing.
//!
//! State convention (mirrors `live_dm_view` / app.rs's live idiom): `snap` is
//! a `Signal<Vec<FriendRow>>` snapshot of `list_friends`, flat + tagged so the
//! panel's `switch` can key on the whole vector. The ACTOR half of the
//! server's sync path: every mutating RPC here re-fetches and `snap.set()`s
//! the refreshed list (which rebuilds the switch). The OTHER user's half lives
//! in app.rs's `gateway_incoming_effect`: its `FriendChanged` branch re-fetches
//! `list_friends` and sets the same signal — the snapshot IS the rebuild key,
//! so no extra revision counter is needed.
//!
//! Online tab: presence comes from the app_shell `live_presence` map
//! (`Presence` frames). "Online" = accepted friends whose recorded status is
//! NOT `offline`; a friend with no entry yet falls in, because presence frames
//! only reach members of shared servers and friends need not share one — so
//! Online is a subset of All that only ever hides known-offline rows.
//! `presence_rev` is folded into the `switch` key so a status change
//! re-renders. (Report note: the app already tracks presence — Online is NOT
//! just a copy of All.)
//!
//! Rows carry: avatar chip (uploaded image once the shared client avatar
//! registry knows the user — fed by `crate::friends` on every list landing —
//! else the initial), username, and actions: accepted rows get Message
//! (opens/creates the DM through the existing live-DM functions via the
//! app_shell `on_message` callback) and Remove; incoming rows get Accept /
//! Decline; outgoing rows get Cancel. The Add Friend tab is a username input +
//! Send button; the server's error text (`user not found`, `cannot friend
//! yourself`, ...) shows inline under the input on failure.

use crate::theme;
use std::cell::RefCell;
use std::rc::Rc;
use std::collections::HashMap;

use runtime_core::{
    AlignItems, Element, FlexDirection, FontWeight, JustifyContent, KeyEvent,
    KeyOutcome, ObjectFit, Overflow, Signal, StyleRules, Tokenized, TouchEvent,
    TouchPhase, TouchResponse, switch, ui,
};
use runtime_core::icon;
use icons_lucide::{PLUS, USERS};

use crate::avatar_chip::{avatar_chip, registered_avatar, mountable_avatar_url};
use crate::friends::{FriendList};
use crate::style_helpers::{borderless_input, col, px};

// --- palette (same Discord dark palette as live_dm_view.rs) ------------------

fn grow(v: f32) -> Tokenized<f32> {
    Tokenized::Literal(v)
}

/// One row of the panel snapshot: `(tag, user_id, username, avatar_url)` with
/// tag 0 = incoming, 1 = outgoing, 2 = accepted.
pub type FriendRow = (u8, i64, String, Option<String>);

/// Flatten a wire [`FriendList`] into the panel snapshot.
pub fn snapshot(list: FriendList) -> Vec<FriendRow> {
    let mut out: Vec<FriendRow> = Vec::new();
    for group in [
        (0u8, list.incoming.into_iter()),
        (1u8, list.outgoing.into_iter()),
        (2u8, list.accepted.into_iter()),
    ] {
        for u in group.1 {
            out.push((group.0, u.user_id, u.username, u.avatar_url));
        }
    }
    out
}

// --- shared styles ------------------------------------------------------------
fn pill_style() -> StyleRules {
    StyleRules {
        height: Some(px(28.0)),
        padding_left: Some(px(14.0)),
        padding_right: Some(px(14.0)),
        background: Some(col(theme::BG_SECONDARY)),
        color: Some(col(theme::TEXT_STRONG)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        flex_shrink: Some(grow(0.0)),
        ..Default::default()
    }
}

fn action_style() -> StyleRules {
    StyleRules {
        height: Some(px(26.0)),
        padding_left: Some(px(10.0)),
        padding_right: Some(px(10.0)),
        background: Some(col(theme::BG_SECONDARY)),
        color: Some(col(theme::TEXT_STRONG)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        border_top_left_radius: Some(px(6.0)),
        border_top_right_radius: Some(px(6.0)),
        border_bottom_left_radius: Some(px(6.0)),
        border_bottom_right_radius: Some(px(6.0)),
        flex_shrink: Some(grow(0.0)),
        ..Default::default()
    }
}

// One friend row: a fixed-height line laid out left-to-right (`flex_direction:
// Row` — the framework default is Column, and a Column row is what stacked the
// chip, name, and buttons on top of each other). The name grows and the action
// pills are pinned after it, so every row reads as one line at even spacing.
fn row_style() -> StyleRules {
    StyleRules {
        height: Some(px(52.0)),
        margin_left: Some(px(8.0)),
        margin_right: Some(px(8.0)),
        padding_left: Some(px(10.0)),
        padding_right: Some(px(10.0)),
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        gap: Some(px(10.0)),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        ..Default::default()
    }
}

fn name_style() -> StyleRules {
    StyleRules {
        font_size: Some(px(15.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(theme::TEXT_STRONG)),
        flex_grow: Some(grow(1.0)),
        ..Default::default()
    }
}

/// The avatar chip: the uploaded image (round-cropped) when the shared client
/// avatar registry (`crate::auth::avatar_for`, fed by `crate::friends` on
/// every list landing) carries a RESOLVABLE URL, else the initial-letter
/// circle. The snapshot's own `avatar_url` is consulted only as a fallback —
/// the web backend refuses to mount an unresolvable src (broken-image
/// placeholder + leaking alt text), so the shared helper's registry gate is
/// what prevents that class of bug.
fn friend_avatar_chip(name: &str, avatar: Option<String>) -> Element {
    if let Some(url) = registered_avatar(name) {
        ui! {
            image(
                src = crate::auth::media_url(url.clone()),
                alt = format!("{} avatar", name),
                style = StyleRules {
                    width: Some(px(34.0)),
                    height: Some(px(34.0)),
                    flex_shrink: Some(grow(0.0)),
                    object_fit: Some(ObjectFit::Cover),
                    border_top_left_radius: Some(px(10.0)),
                    border_top_right_radius: Some(px(10.0)),
                    border_bottom_left_radius: Some(px(10.0)),
                    border_bottom_right_radius: Some(px(10.0)),
                    ..Default::default()
                },
            )
        }
    } else if let Some(url) = avatar {
        // The row's own snapshot URL, gated the same way: only data:/http(s):
        // with a non-empty base64 body may mount.
        if mountable_avatar_url(url.as_str()) {
            ui! {
                image(
                    src = crate::auth::media_url(url.clone()),
                    alt = format!("{} avatar", name),
                    style = StyleRules {
                        width: Some(px(34.0)),
                        height: Some(px(34.0)),
                        flex_shrink: Some(grow(0.0)),
                        object_fit: Some(ObjectFit::Cover),
                        border_top_left_radius: Some(px(10.0)),
                        border_top_right_radius: Some(px(10.0)),
                        border_bottom_left_radius: Some(px(10.0)),
                        border_bottom_right_radius: Some(px(10.0)),
                        ..Default::default()
                    },
                )
            }
        } else {
            avatar_chip(name, theme::ACCENT, 34.0, 10.0)
        }
    } else {
        avatar_chip(name, theme::ACCENT, 34.0, 10.0)
    }
}

/// A small action button whose click runs `click` (keyed `test_id` by label so
/// the Robot tools can target it; the friend it acts on is the button's row).
fn friend_action(label: &str, id: i64, click: Rc<dyn Fn()>) -> Element {
    let _ = id; // ids disambiguate rows visually; test_id stays label-static
    ui! {
        button(
            label = label.to_string(),
            on_click = move || (click)(),
            style = action_style(),
            test_id = "friends-act",
        )
    }
}

/// One friend row: avatar chip, username, then the caller-supplied actions —
/// all inline (the row's `flex_direction: Row`, set in `row_style`).
fn friend_row(name: String, avatar: Option<String>, actions: Vec<Element>) -> Element {
    let chip = friend_avatar_chip(name.as_str(), avatar.clone());
    ui! {
        view(style = row_style()) {
            { chip }
            text(style = name_style()) { crate::auth::shown_name(&name) }
            { actions }
        }
    }
}

/// A centered muted hint (shown when a tab has no rows).
fn centered_hint(text: String) -> Element {
    ui! {
        view(style = StyleRules {
            flex_grow: Some(grow(1.0)),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::Center),
            ..Default::default()
        }) {
            text(
                style = StyleRules {
                    font_size: Some(px(14.0)),
                    color: Some(col(theme::TEXT_MUTED)),
                    ..Default::default()
                },
            ) { text }
        }
    }
}

/// The whole friends area: header + tab pills + the active tab's rows.
///
/// `snap` is the fetched list snapshot (set by the actor's RPC re-fetches here
/// AND by app.rs's gateway `FriendChanged` branch for the other user);
/// `active_tab` 0 = Online, 1 = All, 2 = Pending, 3 = Add Friend; `add_username`
/// / `add_error` the Add Friend draft + inline server error (owned in app_shell
/// scope so the draft survives rebuilds); `presence` / `presence_rev` the
/// app_shell live-presence map + revision; `on_message(uid)` the app_shell
/// callback that opens/creates the live DM with `uid` through the existing
/// live-DM functions.
pub fn friends_screen(
    snap: Signal<Vec<FriendRow>>,
    active_tab: Signal<u8>,
    add_username: Signal<String>,
    add_error: Signal<String>,
    presence: Rc<RefCell<HashMap<i64, String>>>,
    presence_rev: Signal<u32>,
    on_message: Rc<dyn Fn(i64)>,
) -> Element {
    // --- tab pills (built once; a tap sets active_tab) ----------------------
    // One inline header row: the container's `flex_direction: Row` (below, in
    // the assembled ui!) places the four pills side by side — stacked full-
    // width bars was the Column-default bug.
    let tab_read = active_tab.clone();
    let pills: Vec<Element> = {
        let mut out: Vec<Element> = Vec::new();
        for (label, tab) in [
            ("Online".to_string(), 0u8),
            ("All".to_string(), 1u8),
            ("Pending".to_string(), 2u8),
            ("Add Friend".to_string(), 3u8),
        ].into_iter() {
            let id_str = match tab {
                0u8 =>
                    "friends-tab-online",
                1u8 =>
                    "friends-tab-all",
                2u8 =>
                    "friends-tab-pending",
                _ =>
                    "friends-tab-add",
            };
            let set_tab = {
                let t = tab_read.clone();
                let v = tab;
                move |ev: &TouchEvent| {
                    if ev.phase == TouchPhase::Ended {
                        t.set(v);
                    }
                    TouchResponse::CONSUMED
                }
            };
            out.push(
                ui! {
                    view(style = pill_style(), test_id = id_str) {
                        text(
                            style = StyleRules {
                                font_size: Some(px(12.0)),
                                font_weight: Some(FontWeight::SemiBold),
                                ..Default::default()
                            },
                        ) { label }
                    }
                    .on_touch(set_tab)
                }
            );
        }
        out
    };

    // --- body: one switch over (tab, snapshot, error, presence_rev) ---------
    // Presence is read at build time; presence_rev in the key re-renders the
    // Online tab when any tracked user's status changes.
    let pres_map = presence.clone();
    let body = switch(
        move || (
            active_tab.get(),
            snap.get(),
            add_error.get(),
            presence_rev.get(),
        ),
        move |key: &(u8, Vec<FriendRow>, String, u32)| {
            let (tab, rows, err, _rev) = key;
            let pres = pres_map.borrow().clone();
            let on_msg = on_message.clone();

            // Build one row (with its actions) for a snapshot entry.
            let row_el =
                move |row: &FriendRow| -> Element {
                    let (tag, id, name, avatar) = row.clone();
                    let actions: Vec<Element> =
                        match tag {
                            // incoming: Accept + Decline
                            0u8 => {
                                let accept = {
                                    let s = snap.clone();
                                    let uid = id;
                                    move || {
                                        runtime_core::driver::spawn_async(
                                            async move {
                                                if let Ok(list) = crate::friends::accept_friend_request(uid).await {
                                                    s.set(snapshot(list));
                                                }
                                            },
                                        );
                                    }
                                };
                                let decline = {
                                    let s = snap.clone();
                                    let uid = id;
                                    move || {
                                        runtime_core::driver::spawn_async(
                                            async move {
                                                if let Ok(list) = crate::friends::decline_or_cancel(uid).await {
                                                    s.set(snapshot(list));
                                                }
                                            },
                                        );
                                    }
                                };
                                vec![
                                    friend_action("Accept", id, Rc::new(accept)),
                                    friend_action("Decline", id, Rc::new(decline)),
                                ]
                            }
                            // outgoing: Cancel
                            1u8 => {
                                let cancel = {
                                    let s = snap.clone();
                                    let uid = id;
                                    move || {
                                        runtime_core::driver::spawn_async(
                                            async move {
                                                if let Ok(list) = crate::friends::decline_or_cancel(uid).await {
                                                    s.set(snapshot(list));
                                                }
                                            },
                                        );
                                    }
                                };
                                vec![friend_action("Cancel", id, Rc::new(cancel))]
                            }
                            // accepted: Message + Remove
                            _ => {
                                let msg = {
                                    let f = on_msg.clone();
                                    let uid = id;
                                    move || {
                                        (f)(uid);
                                    }
                                };
                                let remove = {
                                    let s = snap.clone();
                                    let uid = id;
                                    move || {
                                        runtime_core::driver::spawn_async(
                                            async move {
                                                if let Ok(list) = crate::friends::remove_friend(uid).await {
                                                    s.set(snapshot(list));
                                                }
                                            },
                                        );
                                    }
                                };
                                vec![
                                    friend_action("Message", id, Rc::new(msg)),
                                    friend_action("Remove", id, Rc::new(remove)),
                                ]
                            }
                        };
                    friend_row(name, avatar, actions)
                };

            if *tab == 3u8 {
                // --- Add Friend: username input + Send + inline error -----
                let do_send = {
                    let name_read = add_username.clone();
                    let err_sig = add_error.clone();
                    let s = snap.clone();
                    move || {
                        let raw = name_read.get().trim().to_string();
                        if raw.is_empty() {
                            return;
                        }
                        runtime_core::driver::spawn_async(
                            async move {
                                match crate::friends::send_friend_request(raw).await {
                                    Ok(list) => {
                                        err_sig.set(String::new());
                                        s.set(snapshot(list));
                                    }
                                    Err(reason) => err_sig.set(reason),
                                }
                            },
                        );
                    }
                };
                let on_enter = {
                    let send = do_send.clone();
                    move |ev: &KeyEvent| {
                        if ev.key == "Enter"
                            && !ev.shift
                            && !ev.ctrl
                            && !ev.alt
                            && !ev.meta
                        {
                            (send)();
                            KeyOutcome::PreventDefault
                        } else {
                            KeyOutcome::Default
                        }
                    }
                };
                return ui! {
                    view(style = StyleRules {
                        flex_grow: Some(grow(1.0)),
                        overflow: Some(Overflow::Hidden),
                        ..Default::default()
                    }) {
                        view(style = StyleRules {
                            flex_grow: Some(grow(1.0)),
                            flex_direction: Some(FlexDirection::Column),
                            align_items: Some(AlignItems::Center),
                            justify_content: Some(JustifyContent::Center),
                            padding_top: Some(px(24.0)),
                            gap: Some(px(10.0)),
                            ..Default::default()
                        }) {
                            view(style = StyleRules {
                                flex_direction: Some(FlexDirection::Row),
                                align_items: Some(AlignItems::Center),
                                justify_content: Some(JustifyContent::Center),
                                gap: Some(px(10.0)),
                                ..Default::default()
                            }) {
                                icon(PLUS).color(|| theme::live(theme::TEXT_MUTED)).size(18.0)
                                text_input(
                                    value = add_username,
                                    on_change = move |s: String| {
                                        add_username.set(s);
                                    },
                                    placeholder = "username",
                                    style = StyleRules {
                                        height: Some(px(32.0)),
                                        padding_left: Some(px(10.0)),
                                        padding_right: Some(px(10.0)),
                                        background: Some(col(theme::BG_INPUT)),
                                        color: Some(col(theme::TEXT_STRONG)),
                                        border_top_left_radius: Some(px(6.0)),
                                        border_top_right_radius: Some(px(6.0)),
                                        border_bottom_left_radius: Some(px(6.0)),
                                        border_bottom_right_radius: Some(px(6.0)),
                                        ..borderless_input()
                                    },
                                    test_id = "friends-add-username",
                                ).on_key_down(on_enter)
                                button(
                                    label = "Send".to_string(),
                                    on_click = {
                                        let send = do_send.clone();
                                        move || (send)()
                                    },
                                    style = StyleRules {
                                        height: Some(px(32.0)),
                                        padding_left: Some(px(14.0)),
                                        padding_right: Some(px(14.0)),
                                        background: Some(col(theme::ACCENT)),
                                        color: Some(col(theme::TEXT_STRONG)),
                                        align_items: Some(AlignItems::Center),
                                        justify_content: Some(JustifyContent::Center),
                                        border_top_left_radius: Some(px(6.0)),
                                        border_top_right_radius: Some(px(6.0)),
                                        border_bottom_left_radius: Some(px(6.0)),
                                        border_bottom_right_radius: Some(px(6.0)),
                                        ..Default::default()
                                    },
                                    test_id = "friends-add-send",
                                )
                            }
                            view(style = StyleRules {
                                align_items: Some(AlignItems::FlexStart),
                                ..Default::default()
                            }) {
                                text(
                                    style = StyleRules {
                                        font_size: Some(px(13.0)),
                                        color: Some(col(theme::DANGER)),
                                        ..Default::default()
                                    },
                                ) { err.clone() }
                            }
                        }
                    }
                };
            }

            let row_elems: Vec<Element> = {
                let mut out: Vec<Element> = Vec::new();
                for row in rows.iter() {
                    let keep =
                        match *tab {
                            // Online: accepted, not known-offline
                            0u8 =>
                                row.0 == 2u8
                                    && match pres.get(&row.1) {
                                        Some(status) => status.as_str() != "offline",
                                        None => true,
                                    },
                            // All: every accepted friend
                            1u8 => row.0 == 2u8,
                            // Pending: incoming + outgoing
                            _ => row.0 == 0u8 || row.0 == 1u8,
                        };
                    if keep {
                        out.push((row_el)(row));
                    }
                }
                out
            };
            if row_elems.is_empty() {
                let hint =
                    match *tab {
                        0u8 => "No friends online".to_string(),
                        1u8 => "No friends yet".to_string(),
                        _ => "No pending requests".to_string(),
                    };
                return ui! {
                    view(style = StyleRules {
                        flex_grow: Some(grow(1.0)),
                        overflow: Some(Overflow::Hidden),
                        ..Default::default()
                    }) {
                        centered_hint(hint)
                    }
                };
            }
            ui! {
                view(style = StyleRules {
                    flex_grow: Some(grow(1.0)),
                    overflow: Some(Overflow::Hidden),
                    ..Default::default()
                }) {
                    scroll_view(style = StyleRules {
                        flex_grow: Some(grow(1.0)),
                        ..Default::default()
                    }) {
                        view(style = StyleRules {
                            flex_direction: Some(FlexDirection::Column),
                            ..Default::default()
                        }) { row_elems }
                    }
                }
            }
        },
    );

    // The outer column carries NO explicit width: it flex-grows into the main
    // area the live DM screen gives it (pct-based fills would pin it narrow —
    // the narrow-column clipping bug class).
    ui! {
        view(style = StyleRules {
            flex_grow: Some(grow(1.0)),
            background: Some(col(theme::BG_PRIMARY)),
            flex_direction: Some(FlexDirection::Column),
            overflow: Some(Overflow::Hidden),
            ..Default::default()
        }) {
            view(style = StyleRules {
                align_items: Some(AlignItems::Center),
                gap: Some(px(10.0)),
                flex_direction: Some(FlexDirection::Row),
                padding_left: Some(px(16.0)),
                padding_bottom: Some(px(8.0)),
                ..Default::default()
            }) {
                icon(USERS).color(|| theme::live(theme::TEXT_MUTED)).size(18.0)
                text(
                    style = StyleRules {
                        font_size: Some(px(16.0)),
                        font_weight: Some(FontWeight::Bold),
                        color: Some(col(theme::TEXT_STRONG)),
                        ..Default::default()
                    },
                ) { "Friends".to_string() }
            }
            view(style = StyleRules {
                margin_top: Some(px(10.0)),
                margin_bottom: Some(px(10.0)),
                gap: Some(px(8.0)),
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                padding_left: Some(px(16.0)),
                padding_right: Some(px(16.0)),
                ..Default::default()
            }) { pills }
            { body }
        }
    }
}
