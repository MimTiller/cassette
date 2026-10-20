//! `live_dm_view` — the LIVE direct-messages surface: a list of the
//! signed-in user's real DM conversations (fetched from the backend's
//! `list_dms`) plus a conversation view with a working composer (sends via
//! `send_dm_message`) and a "start a new DM" row (calls `start_dm` by the
//! other user's numeric id).
//!
//! This is separate from the mock `dm_view` module (which renders the
//! `&'static` data in `crate::data`) and is a *new, independent* entry
//! point: it has its own `open` toggle signal, its own list/message state,
//! and its own full-screen overlay region. It does not touch or remove the
//! mock DM screen.
//!
//! Live-mode conventions (mirroring `app.rs`'s `live_servers` /
//! `live_channels`):
//! - `live_dms` is a `Signal<Vec<Dm>>` populated via `spawn_async` calling
//!   the `#[server]` fn, on first open.
//! - `live_dm_messages` / `live_dm_revision` hold the fetched history for
//!   the currently-selected DM (the `revision` bump is the rebuild trigger,
//!   the same idiom as `live_messages` / `live_revision`).
//! - Rows reuse `crate::app::message_row` for visual consistency with the
//!   live channel message list.
//!
//! Sending: the composer calls `send_dm_message`, and the message appears
//! through the realtime gateway's inbound `WsServerMsg::DmMessage` handling
//! (see `dm_incoming_effect` below) — the same "server echoes it back as a
//! frame, the effect buffers and bumps the revision" path as the live
//! channel composer in `app.rs`.

use std::cell::RefCell;
use std::rc::Rc;

use runtime_core::{
    AlignItems, Color, Element, FlexDirection, FlexWrap, FontWeight, JustifyContent, KeyEvent,
    KeyOutcome, ObjectFit, Overflow, Position, Signal, StyleRules, TextTransform, Tokenized,
    TouchPhase, TouchResponse, signal, switch, ui,
};
use runtime_core::icon;
use icons_lucide::{PLUS, SMILE};

use crate::app::{format_when, message_row, same_group};
use crate::dms::{self, Dm};
use crate::style_helpers::{borderless_input, col, pct, px, section_label};

// --- palette -----------------------------------------------------------------
// The same Discord dark palette as `app.rs` / `dm_view.rs`, kept local so the
// module is self-contained (mirrors how `dm_view.rs` keeps its own constants).
const BG_SIDEBAR: &str = "#2b2d31";
const BG_MAIN: &str = "#313338";
const BG_ACTIVE: &str = "#404249";
const BG_HOVER: &str = "#35373c";
const COMPOSER: &str = "#383a40";
const TEXT: &str = "#f2f3f5";
const TEXT_MUTED: &str = "#949ba4";
const BORDER: &str = "#26272b";
const BUTTON: &str = "#5865f2";

fn grow(v: f32) -> Tokenized<f32> {
    Tokenized::Literal(v)
}

fn border_width(px: f32) -> Tokenized<f32> {
    Tokenized::Literal(px)
}

/// One buffered live DM message: the wire fields needed to render a row —
/// `author_username`, `content`, `created_at` (UTC epoch ms), and
/// `edited_at` (UTC epoch ms, `None` until the first edit).
type LiveDmMsg = (String, String, i64, Option<i64>);

/// Convert the wire's UTC epoch *milliseconds* to the unix *seconds*
/// `format_when` expects (the gateway path does the same `/1000` on ingest).
fn ms_to_secs(ms: i64) -> i64 {
    ms / 1000
}

/// The signed-in user's numeric id (`0` = not logged in). Used to decide
/// which participant of a DM is "the other" one to show in the list.
fn current_user_id() -> i64 {
    crate::auth::CURRENT_USER_ID.load(std::sync::atomic::Ordering::SeqCst)
}

// --- conversation list -------------------------------------------------------

/// The "Start a new DM" row at the top of the live DM list: a numeric user-id
/// input (the backend's `start_dm` takes `user_id: i64` — there is no
/// username-lookup endpoint) + a Go button that calls `start_dm` and then
/// selects the resulting conversation (idempotent: an existing DM is returned
/// as-is).
///
/// `id_text` is owned in `app_shell` scope (not here) so the draft survives a
/// list rebuild: when `start_dm` lands, `live_dms` is updated and the list
/// `switch` re-runs its arm, re-creating this row — an in-row signal would
/// drop the typed id at that exact moment.
fn live_dm_new_row(
    live_dms: Signal<Vec<Dm>>,
    active: Signal<Option<i64>>,
    msgs: Rc<RefCell<Vec<LiveDmMsg>>>,
    rev: Signal<u32>,
    id_text: Signal<String>,
) -> Element {
    let on_change = move |s: String| {
        id_text.set(s);
    };
    let row_style = StyleRules {
        height: Some(px(48.0)),
        margin_left: Some(px(8.0)),
        margin_right: Some(px(8.0)),
        margin_top: Some(px(6.0)),
        margin_bottom: Some(px(6.0)),
        padding_left: Some(px(10.0)),
        padding_right: Some(px(10.0)),
        align_items: Some(AlignItems::Center),
        gap: Some(px(6.0)),
        background: Some(col(BG_HOVER)),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        ..Default::default()
    };
    let input_style = StyleRules {
        flex_grow: Some(grow(1.0)),
        height: Some(px(30.0)),
        padding_left: Some(px(8.0)),
        padding_right: Some(px(8.0)),
        background: Some(col(COMPOSER)),
        color: Some(col(TEXT)),
        border_top_left_radius: Some(px(6.0)),
        border_top_right_radius: Some(px(6.0)),
        border_bottom_left_radius: Some(px(6.0)),
        border_bottom_right_radius: Some(px(6.0)),
        ..borderless_input()
    };
    let go_style = StyleRules {
        height: Some(px(30.0)),
        padding_left: Some(px(12.0)),
        padding_right: Some(px(12.0)),
        background: Some(col(BUTTON)),
        color: Some(col(TEXT)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        border_top_left_radius: Some(px(6.0)),
        border_top_right_radius: Some(px(6.0)),
        border_bottom_left_radius: Some(px(6.0)),
        border_bottom_right_radius: Some(px(6.0)),
        ..Default::default()
    };

    // One action shared by the Go button and the input's Enter key: parse the
    // id, call `start_dm`, then select the resulting DM (fetching its history,
    // the same idiom as `live_dm_row`'s touch handler).
    let id_read = id_text.clone();
    let do_start = Rc::new(move || {
        let raw = id_read.get();
        match raw.trim().parse::<i64>() {
            Ok(user_id) if user_id > 0 => {
                let dms_sig = live_dms.clone();
                let act = active.clone();
                let m = msgs.clone();
                let r = rev.clone();
                runtime_core::driver::spawn_async(async move {
                    match dms::start_dm(user_id).await {
                        Ok(dm) => {
                            // Idempotent: the backend returns the existing DM
                            // if one is already open with this user, so we can
                            // always just select the returned channel id.
                            let dm_id = dm.channel_id;
                            if !dms_sig.get().iter().any(|d| d.channel_id == dm_id) {
                                let mut list = dms_sig.get();
                                list.insert(0, dm);
                                dms_sig.set(list);
                            }
                            act.set(Some(dm_id));
                            crate::auth::save_last_view_dm(dm_id);
                            if let Ok(history) = dms::list_dm_messages(dm_id, None).await {
                                m.borrow_mut().clear();
                                m.borrow_mut()
                                    .extend(history.into_iter().map(|msg| {
                                        (
                                            msg.author_username,
                                            msg.content,
                                            msg.created_at,
                                            msg.edited_at,
                                        )
                                    }));
                                r.set(r.get() + 1);
                            }
                        }
                        Err(_) => {
                            // A failed start (unknown user, self-DM) leaves the
                            // selection untouched; the id draft is kept so the
                            // user can correct it.
                        }
                    }
                });
            }
            _ => {
                // Not a valid id: ignore (the input keeps the draft so the
                // user can fix it — no separate error element, to keep the
                // row a stable single-line layout).
            }
        }
    });
    let on_click = {
        let start = do_start.clone();
        move || (start)()
    };
    let on_enter = {
        let start = do_start.clone();
        move |ev: &KeyEvent| {
            if ev.key == "Enter" && !ev.shift && !ev.ctrl && !ev.alt && !ev.meta {
                (start)();
                KeyOutcome::PreventDefault
            } else {
                KeyOutcome::Default
            }
        }
    };
    ui! {
        view(style = row_style) {
            text_input(value = id_text, on_change = on_change, placeholder = "user id", style = input_style, test_id = "live-dm-new-id").on_key_down(on_enter)
            button(label = "Go".to_string(), on_click = on_click, style = go_style, test_id = "live-dm-new-go")
        }
    }
}

/// One row in the live DM list: an avatar chip (the other participant's
/// initial), their username, and a "live" hint line. Tapping it selects the
/// DM (fetches its history into `live_dm_messages` + bumps `live_dm_revision`).
fn live_dm_row(
    dm: Dm,
    active: Signal<Option<i64>>,
    msgs: Rc<RefCell<Vec<LiveDmMsg>>>,
    rev: Signal<u32>,
) -> Element {
    // The "other" participant: the one whose id isn't the signed-in user.
    // If the id isn't known yet (rare; the list is only fetched post-login),
    // fall back to the first participant.
    let me = current_user_id();
    let other = dm
        .participants
        .iter()
        .find(|p| p.user_id != me)
        .cloned()
        .or_else(|| dm.participants.first().cloned());
    let dm_id = dm.channel_id;
    let name = other
        .as_ref()
        .map(|p| p.username.clone())
        .unwrap_or_else(|| format!("user #{}", dm_id));
    let initial = name.chars().next().unwrap_or('?').to_string();
    // The other participant's uploaded avatar (from the shared client
    // registry, fed by `list_dms`), if any — drawn as a round-cropped image
    // over the chip area; otherwise the pre-existing initial chip.
    let avatar_img = crate::auth::avatar_for(name.as_str());

    let hovered = signal(false);
    let row_style = move || {
        let on = active.get() == Some(dm_id);
        StyleRules {
            height: Some(px(56.0)),
            margin_left: Some(px(8.0)),
            margin_right: Some(px(8.0)),
            padding_left: Some(px(10.0)),
            padding_right: Some(px(10.0)),
            align_items: Some(AlignItems::Center),
            gap: Some(px(10.0)),
            background: Some(col(if on {
                BG_ACTIVE
            } else if hovered.get() {
                BG_HOVER
            } else {
                "transparent"
            })),
            border_top_left_radius: Some(px(8.0)),
            border_top_right_radius: Some(px(8.0)),
            border_bottom_left_radius: Some(px(8.0)),
            border_bottom_right_radius: Some(px(8.0)),
            ..Default::default()
        }
    };
    let avatar_style = StyleRules {
        width: Some(px(34.0)),
        height: Some(px(34.0)),
        flex_shrink: Some(grow(0.0)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        background: Some(col(BUTTON)),
        border_top_left_radius: Some(px(10.0)),
        border_top_right_radius: Some(px(10.0)),
        border_bottom_left_radius: Some(px(10.0)),
        border_bottom_right_radius: Some(px(10.0)),
        ..Default::default()
    };
    let initial_style = StyleRules {
        font_size: Some(px(15.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT)),
        ..Default::default()
    };
    let name_style = StyleRules {
        font_size: Some(px(15.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT)),
        flex_grow: Some(grow(1.0)),
        ..Default::default()
    };

    // The chip: uploaded image (round-cropped) when the shared client
    // registry knows the participant, else the pre-existing initial chip.
    let avatar_chip = if let Some(url) = &avatar_img {
        ui! {
            image(
                src = url.clone(),
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
        ui! {
            view(style = avatar_style) {
                text(style = initial_style) { initial }
            }
        }
    };

    // Selecting a DM: set `active` (rebuilds the list to highlight it) and
    // fetch its history. Clone the `Rc` before handing it to the `async move`
    // block (it outlives the `on_touch` `Fn` closure — a plain capture would
    // be a move out of the closure, E0507).
    ui! {
        view(style = row_style) {
            { avatar_chip }
            text(style = name_style) { name }
        }
        .on_hover(move |on| hovered.set(on))
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                active.set(Some(dm_id));
                // Persist as this account's last open DM so a refresh re-opens
                // THIS conversation (keyed by user id in localStorage).
                crate::auth::save_last_view_dm(dm_id);
                let msgs = msgs.clone();
                let rev = rev.clone();
                runtime_core::driver::spawn_async(async move {
                    if let Ok(history) = dms::list_dm_messages(dm_id, None).await {
                        msgs.borrow_mut().clear();
                        msgs.borrow_mut().extend(history.into_iter().map(|m| {
                            (
                                m.author_username,
                                m.content,
                                m.created_at,
                                m.edited_at,
                            )
                        }));
                        rev.set(rev.get() + 1);
                    }
                });
            }
            TouchResponse::CONSUMED
        })
    }
}

/// The live DM list column: a "DIRECT MESSAGES" section label, a
/// "start a new DM" row, and a scrollable list of `live_dms` rows. The
/// `switch` key is `live_dms` (the fetched list), so when the fetch lands the
/// rows appear; `active`/`revision` are also read so the highlight + message
/// pane stay in sync. `new_id` is the "start a new DM" draft, owned in
/// `app_shell` scope so it survives a list rebuild.
fn live_dm_list(
    live_dms: Signal<Vec<Dm>>,
    active: Signal<Option<i64>>,
    _msgs: Rc<RefCell<Vec<LiveDmMsg>>>,
    rev: Signal<u32>,
    new_id: Signal<String>,
    friends_entry: Rc<dyn Fn() -> Element>,
) -> Element {
    let column_style = StyleRules {
        width: Some(px(240.0)),
        background: Some(col(BG_SIDEBAR)),
        border_right_width: Some(border_width(1.0)),
        border_right_color: Some(col(BORDER)),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    let section_style = StyleRules {
        padding_top: Some(px(14.0)),
        padding_bottom: Some(px(6.0)),
        padding_left: Some(px(16.0)),
        padding_right: Some(px(16.0)),
        ..section_label()
    };

    let list = switch(
        move || live_dms.get(),
        move |dms: &Vec<Dm>| {
            // `switch`'s render is `Fn` (called on every key change) — clone
            // the captured styles so the closure borrows them rather than
            // moving them out (the whiteboard idiom, cf. `app.rs`'s sidebar).
            let column_style = column_style.clone();
            let section_style = section_style.clone();
            let new_row = live_dm_new_row(
                live_dms.clone(),
                active.clone(),
                _msgs.clone(),
                rev.clone(),
                new_id.clone(),
            );
            let rows: Vec<Element> = dms
                .iter()
                .cloned()
                .map(|dm| live_dm_row(dm, active.clone(), _msgs.clone(), rev.clone()))
                .collect();
            let entry = (friends_entry)();
            ui! {
                view(style = column_style) {
                    text(style = section_style) { "Direct Messages".to_string() }
                    { entry }
                    new_row
                    view(style = StyleRules {
                        flex_grow: Some(grow(1.0)),
                        overflow: Some(Overflow::Hidden),
                        ..Default::default()
                    }) {
                        scroll_view(style = StyleRules {
                            flex_grow: Some(grow(1.0)),
                            ..Default::default()
                        }) { rows }
                    }
                }
            }
        },
    );
    list
}

// --- conversation ------------------------------------------------------------

/// The active live DM conversation: a header (the other participant's name)
/// + the fetched message history + a working composer. The composer mirrors
/// the live channel composer in `app.rs` (a `Signal<String>`, a styled
/// `text_input`, Enter-to-send); sending calls `send_dm_message` and the
/// resulting message arrives back through the gateway's `DmMessage` frame,
/// which `dm_incoming_effect` appends to `msgs` (see below).
fn live_dm_conversation(
    live_dms: Signal<Vec<Dm>>,
    active: Signal<Option<i64>>,
    msgs: Rc<RefCell<Vec<LiveDmMsg>>>,
    rev: Signal<u32>,
) -> Element {
    let pane_style = StyleRules {
        flex_grow: Some(grow(1.0)),
        background: Some(col(BG_MAIN)),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };

    // --- composer (built inside the `switch` render below) ----------------
    // Mirrors the channel composer in `app.rs` (a `Signal<String>`, a styled
    // `text_input`, and an Enter-to-send `on_key_down` handler), scoped to the
    // currently-selected live DM. Like the channel composer there is NO
    // visible Send button — Enter is the only submit affordance (Discord has
    // none). The send reads `active` at call time, so the same action works
    // as the selection changes.
    //
    // The `text` draft signal is created HERE (function scope) so the typed
    // value survives a `switch` rebuild — the composer *element* is rebuilt
    // inside the render closure (which is `Fn`, called on every key change, so
    // it cannot move an out-of-scope `Element` out), but it re-binds to the
    // same `text` signal, so the draft persists. Every other building block
    // (the `on_change`/send closures and the styles) is `Copy`/`Clone` or
    // reconstructible from `Copy` captures, so the element is cheaply rebuilt
    // in place.
    let text: Signal<String> = signal(String::new());
    let composer_style = StyleRules {
        padding_top: Some(px(12.0)),
        padding_bottom: Some(px(12.0)),
        padding_left: Some(px(16.0)),
        padding_right: Some(px(16.0)),
        border_top_width: Some(border_width(1.0)),
        border_top_color: Some(col(BORDER)),
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        gap: Some(px(8.0)),
        ..Default::default()
    };
    let input_style = StyleRules {
        flex_grow: Some(grow(1.0)),
        height: Some(px(40.0)),
        padding_left: Some(px(12.0)),
        padding_right: Some(px(12.0)),
        background: Some(col(COMPOSER)),
        color: Some(col(TEXT)),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        ..borderless_input()
    };

    // --- message list ------------------------------------------------------
    // Rebuilds when `rev` bumps (a fetched history load or an inbound
    // `DmMessage` frame) — the same value-dedup `switch` key trick the live
    // channel list in `app.rs` uses. The whole pane (header + rows + composer)
    // is built inside this render, since the header name depends on `active`.
    let body = switch(
        move || (active.get(), rev.get()),
        move |(_cid, _r): &(Option<i64>, u32)| {
            // `switch`'s render is `Fn` (re-called on every key change) — clone
            // the captured styles so we borrow them rather than move them out.
            let pane_style = pane_style.clone();
            let composer_style = composer_style.clone();
            let input_style = input_style.clone();
            let cid = _cid.unwrap_or(0);

            // --- composer (rebuilt in place, bound to the shared `text`) ----
            let on_change = move |s: String| {
                text.set(s);
            };
            let text_read = text.clone();
            let act_read = active.clone();
            let do_send = move || {
                let cid = match act_read.get() {
                    Some(c) => c,
                    None => return,
                };
                let body = text_read.get();
                if body.trim().is_empty() {
                    return;
                }
                text_read.set(String::new());
                runtime_core::driver::spawn_async(async move {
                    // The server pushes a `DmMessage` frame to every participant
                    // (the sender included); `dm_incoming_effect` appends it to
                    // `msgs` and bumps `rev`, rebuilding the list. On RPC failure
                    // (e.g. not a participant) the message simply never appears —
                    // no optimistic append, mirroring the channel composer.
                    let _ = dms::send_dm_message(cid, body).await;
                });
            };
            let on_send = Rc::new(do_send);
            let on_enter = {
                let send_key = on_send.clone();
                move |ev: &KeyEvent| {
                    if ev.key == "Enter" && !ev.shift && !ev.ctrl && !ev.alt && !ev.meta {
                        (send_key)();
                        KeyOutcome::PreventDefault
                    } else {
                        KeyOutcome::Default
                    }
                }
            };
            let composer_el = ui! {
                view(style = composer_style) {
                    text_input(value = text, on_change = on_change, placeholder = "Message", style = input_style, test_id = "live-dm-composer").on_key_down(on_enter)
                }
            };

            if cid == 0 {
                // No DM selected yet: a centered, muted hint.
                return ui! {
                    view(style = pane_style) {
                        view(style = StyleRules {
                            flex_grow: Some(grow(1.0)),
                            align_items: Some(AlignItems::Center),
                            justify_content: Some(JustifyContent::Center),
                            padding_top: Some(px(24.0)),
                            ..Default::default()
                        }) {
                            text(style = StyleRules {
                                font_size: Some(px(14.0)),
                                color: Some(col(TEXT_MUTED)),
                                ..Default::default()
                            }) { "Select a conversation".to_string() }
                        }
                    }
                };
            }
            // Header: the other participant's name.
            let other_name = live_dms
                .get()
                .iter()
                .find(|dm| dm.channel_id == cid)
                .and_then(|dm| {
                    let me = current_user_id();
                    dm.participants
                        .iter()
                        .find(|p| p.user_id != me)
                        .cloned()
                        .or_else(|| dm.participants.first().cloned())
                })
                .map(|p| p.username)
                .unwrap_or_else(|| format!("user #{}", cid));
            let header_style = StyleRules {
                padding_top: Some(px(10.0)),
                padding_bottom: Some(px(10.0)),
                padding_left: Some(px(16.0)),
                padding_right: Some(px(16.0)),
                border_bottom_width: Some(border_width(1.0)),
                border_bottom_color: Some(col(BORDER)),
                ..Default::default()
            };
            let title_style = StyleRules {
                font_size: Some(px(15.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(TEXT)),
                ..Default::default()
            };

            let now = crate::data::now_secs();
            let mine: Vec<LiveDmMsg> = msgs.borrow().clone();
            if mine.is_empty() {
                // No messages yet: a centered, muted hint (plus the composer).
                return ui! {
                    view(style = pane_style) {
                        view(style = header_style) {
                            text(style = title_style) { other_name }
                        }
                        view(style = StyleRules {
                            flex_grow: Some(grow(1.0)),
                            align_items: Some(AlignItems::Center),
                            justify_content: Some(JustifyContent::Center),
                            ..Default::default()
                        }) {
                            text(style = StyleRules {
                                font_size: Some(px(14.0)),
                                color: Some(col(TEXT_MUTED)),
                                ..Default::default()
                            }) { "No messages yet".to_string() }
                        }
                        composer_el
                    }
                };
            }
            // Message-grouping state carried across rows (oldest-first):
            // the previous row's author + timestamp. An edited message keeps
            // its own header (a marker on a headerless row would be orphaned).
            let mut rows: Vec<Element> = Vec::with_capacity(mine.len());
            let mut prev: Option<(String, i64)> = None;
            for (author, body, ts, edited_at) in mine.iter() {
                let ts_s = ms_to_secs(*ts);
                let when = format_when(ts_s, now);
                let grouped = prev
                    .as_ref()
                    .map(|(pa, pts)| same_group(pa, *pts, author.as_str(), ts_s))
                    .unwrap_or(false)
                    && edited_at.is_none();
                rows.push(message_row(
                    author.as_str(),
                    BUTTON,
                    body.as_str(),
                    when,
                    crate::app::format_full_when(ts_s),
                    edited_at.is_some(),
                    grouped,
                ));
                prev = Some((author.clone(), ts_s));
            }
            ui! {
                view(style = pane_style) {
                    view(style = header_style) {
                        text(style = title_style) { other_name }
                    }
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
                            }) { rows }
                        }
                    }
                    composer_el
                }
            }
        },
    );

    body
}

// --- gateway inbound ---------------------------------------------------------

/// Watch the realtime gateway for inbound `WsServerMsg::DmMessage` frames and
/// append them to the live DM message buffer. This is the client half of the
/// send path: the composer calls `send_dm_message` and the server pushes the
/// frame back to every participant (the sender included), so a freshly sent
/// message — and any message the other participant sends — lands here,
/// deduped on `message_id`, and the list rebuilds via `rev`.
///
/// `frame_seq` must be `gateway.incoming()` (the per-frame value signal); this
/// is the same "watch every frame" side-effect idiom as `app.rs`'s
/// `gateway_incoming_effect`, but scoped to DM frames only and mounted inside
/// the live DM overlay so it only does work when the overlay is open.
pub fn dm_incoming_effect(
    frame_seq: Signal<Option<discord_server::server_settings::WsServerMsg>>,
    active: Signal<Option<i64>>,
    msgs: Rc<RefCell<Vec<LiveDmMsg>>>,
    rev: Signal<u32>,
) -> Element {
    let dm_buf = msgs.clone();
    let dm_rev = rev.clone();
    let dm_active = active.clone();
    switch(
        move || frame_seq.get(),
        move |frame: &Option<discord_server::server_settings::WsServerMsg>| {
            if let Some(discord_server::server_settings::WsServerMsg::DmMessage {
                message_id,
                dm_id,
                author_username,
                author_avatar,
                content,
                created_at,
                ..
            }) = frame
            {
                let dm_id = *dm_id;
                // The author's avatar rides the frame: register it so the DM
                // pane's rows (and the rail) draw it instead of the initial.
                crate::auth::remember_avatar(author_username.as_str(), author_avatar.clone());
                // Only buffer frames for the currently-open conversation —
                // the buffer holds exactly one DM's history at a time, and
                // re-selecting a DM re-fetches its history anyway, so
                // off-screen DMs never accumulate stale rows.
                if dm_active.get() == Some(dm_id) {
                    let changed = {
                        let mut buf = dm_buf.borrow_mut();
                        // Dedup: a `DmMessage` frame can be delivered more than
                        // once (reconnect replay, a sender's second socket).
                        // `LiveDmMsg` carries no wire `message_id`, so dedup on
                        // the wire fields the frame does carry — a replay has
                        // identical (author, content, created_at).
                        if buf.iter().any(|m| {
                            m.0 == *author_username
                                && m.1 == *content
                                && m.2 == *created_at
                        }) {
                            return ui! {
                                view(style = StyleRules {
                                    height: Some(px(0.0)),
                                    ..Default::default()
                                }) {}
                            };
                        }
                        buf.push((
                            author_username.clone(),
                            content.clone(),
                            *created_at,
                            None,
                        ));
                        true
                    };
                    if changed {
                        dm_rev.set(dm_rev.get() + 1);
                    }
                }
            }
            ui! {
                view(style = StyleRules {
                    height: Some(px(0.0)),
                    ..Default::default()
                }) {}
            }
        },
    )
}

// --- full-screen overlay -----------------------------------------------------

/// The whole live DM area: the list column + the active conversation + the
/// gateway `DmMessage` inbound effect. This is the surface the rail entry
/// mounts (inside its own full-screen overlay region built in `app.rs`, the
/// same paint-last idiom as the mock `dm_region`).
///
/// `live_dms` is the fetched conversation list, `active` the selected DM id,
/// `msgs`/`rev` the fetched-history buffer + rebuild counter, `new_id` the
/// "start a new DM" draft, and `frame_seq` the gateway's per-frame value
/// signal (the effect watches it for `DmMessage` frames). All are owned in
/// `app_shell` scope so state survives a close/re-open.
pub fn live_dm_screen(
    live_dms: Signal<Vec<Dm>>,
    active: Signal<Option<i64>>,
    msgs: Rc<RefCell<Vec<LiveDmMsg>>>,
    rev: Signal<u32>,
    new_id: Signal<String>,
    frame_seq: Signal<Option<discord_server::server_settings::WsServerMsg>>,
    friends_entry: Rc<dyn Fn() -> Element>,
    friends_open: Signal<bool>,
    friends_panel: Rc<dyn Fn() -> Element>,
) -> Element {
    let effect = dm_incoming_effect(frame_seq, active.clone(), msgs.clone(), rev.clone());
    // Main area: the selected DM conversation, or — while the Friends panel
    // is open (toggled by the "Friends" entry at the top of the list column)
    // — the panel itself. `Element` is not `Clone`, so the panel and the
    // entry both enter as factories; all their state is signals owned in
    // app_shell scope, so rebuilding per render is cheap and state-stable.
    let conv_msgs = msgs.clone();
    let conv_dms = live_dms.clone();
    let conv_active = active.clone();
    let conv_rev = rev.clone();
    let main = switch(
        move || friends_open.get(),
        move |open: &bool| {
            if *open {
                (friends_panel)()
            } else {
                live_dm_conversation(
                    conv_dms.clone(),
                    conv_active.clone(),
                    conv_msgs.clone(),
                    conv_rev.clone(),
                )
            }
        },
    );
    ui! {
        view(style = StyleRules {
            width: Some(pct(100.0)),
            height: Some(pct(100.0)),
            flex_direction: Some(FlexDirection::Row),
            overflow: Some(Overflow::Hidden),
            ..Default::default()
        }) {
            live_dm_list(live_dms.clone(), active.clone(), msgs.clone(), rev.clone(), new_id, friends_entry)
            main
            effect
        }
    }
}
