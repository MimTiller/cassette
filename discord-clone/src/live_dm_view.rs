//! `live_dm_view` — the LIVE direct-messages surface: a list of the
//! signed-in user's real DM conversations (fetched from the backend's
//! `list_dms`) plus a read-only view of a conversation's message history
//! (fetched from `list_dm_messages`).
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
//! Read/view only: sending a new DM and starting a new DM conversation
//! (`start_dm` / `send_dm_message`) are out of scope here.

use std::cell::RefCell;
use std::rc::Rc;

use runtime_core::{
    AlignItems, Element, FlexDirection, FontWeight, JustifyContent, Overflow, Signal, StyleRules,
    TextTransform, Tokenized, TouchPhase, TouchResponse, signal, switch, ui,
};

use crate::app::{format_when, message_row};
use crate::dms::{self, Dm};
use crate::style_helpers::{col, pct, px};

// --- palette -----------------------------------------------------------------
// The same Discord dark palette as `app.rs` / `dm_view.rs`, kept local so the
// module is self-contained (mirrors how `dm_view.rs` keeps its own constants).
const BG_SIDEBAR: &str = "#2b2d31";
const BG_MAIN: &str = "#313338";
const BG_ACTIVE: &str = "#404249";
const BG_HOVER: &str = "#35373c";
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

    // Selecting a DM: set `active` (rebuilds the list to highlight it) and
    // fetch its history. Clone the `Rc` before handing it to the `async move`
    // block (it outlives the `on_touch` `Fn` closure — a plain capture would
    // be a move out of the closure, E0507).
    ui! {
        view(style = row_style) {
            view(style = avatar_style) {
                text(style = initial_style) { initial }
            }
            text(style = name_style) { name }
        }
        .on_hover(move |on| hovered.set(on))
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                active.set(Some(dm_id));
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

/// The live DM list column: a "DIRECT MESSAGES" section label + a scrollable
/// list of `live_dms` rows. The `switch` key is `live_dms` (the fetched list),
/// so when the fetch lands the rows appear; `active`/`revision` are also read
/// so the highlight + message pane stay in sync.
fn live_dm_list(
    live_dms: Signal<Vec<Dm>>,
    active: Signal<Option<i64>>,
    _msgs: Rc<RefCell<Vec<LiveDmMsg>>>,
    rev: Signal<u32>,
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
        font_size: Some(px(11.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT_MUTED)),
        text_transform: Some(TextTransform::Uppercase),
        ..Default::default()
    };

    let list = switch(
        move || live_dms.get(),
        move |dms: &Vec<Dm>| {
            // `switch`'s render is `Fn` (called on every key change) — clone
            // the captured styles so the closure borrows them rather than
            // moving them out (the whiteboard idiom, cf. `app.rs`'s sidebar).
            let column_style = column_style.clone();
            let section_style = section_style.clone();
            let rows: Vec<Element> = dms
                .iter()
                .cloned()
                .map(|dm| live_dm_row(dm, active.clone(), _msgs.clone(), rev.clone()))
                .collect();
            ui! {
                view(style = column_style) {
                    text(style = section_style) { "Direct Messages".to_string() }
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
/// + the fetched message history, rendered with the same `message_row` the
/// live channel list uses. Read-only — no composer (sending is out of scope).
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

    let body = switch(
        move || (active.get(), rev.get()),
        move |(_cid, _r): &(Option<i64>, u32)| {
            // `switch`'s render is `Fn` (re-called on every key change) — clone
            // the captured style so we borrow it rather than move it out.
            let pane_style = pane_style.clone();
            let cid = _cid.unwrap_or(0);
            if cid == 0 {
                // No DM selected yet: a centered, muted hint.
                return ui! {
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
                    }
                };
            }
            let rows: Vec<Element> = mine
                .iter()
                .map(|(author, body, ts, edited_at)| {
                    let when = format_when(ms_to_secs(*ts), now);
                    message_row(author.as_str(), BUTTON, body.as_str(), when, edited_at.is_some())
                })
                .collect();
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
                }
            }
        },
    );

    body
}

// --- full-screen overlay -----------------------------------------------------

/// The whole live DM area: the list column + the active conversation. This is
/// the surface the rail entry mounts (inside its own full-screen overlay
/// region built in `app.rs`, the same paint-last idiom as the mock `dm_region`).
///
/// `live_dms` is the fetched conversation list, `active` the selected DM id,
/// `msgs`/`rev` the fetched-history buffer + rebuild counter. All are owned in
/// `app_shell` scope so state survives a close/re-open.
pub fn live_dm_screen(
    live_dms: Signal<Vec<Dm>>,
    active: Signal<Option<i64>>,
    msgs: Rc<RefCell<Vec<LiveDmMsg>>>,
    rev: Signal<u32>,
) -> Element {
    ui! {
        view(style = StyleRules {
            width: Some(pct(100.0)),
            height: Some(pct(100.0)),
            flex_direction: Some(FlexDirection::Row),
            overflow: Some(Overflow::Hidden),
            ..Default::default()
        }) {
            live_dm_list(live_dms.clone(), active.clone(), msgs.clone(), rev.clone())
            live_dm_conversation(live_dms.clone(), active.clone(), msgs.clone(), rev.clone())
        }
    }
}
