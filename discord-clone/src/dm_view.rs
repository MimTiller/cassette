//! `dm_view` — the direct-messages UI: a DM list (recent conversations) plus a
//! DM conversation view (header, message list, composer).
//!
//! This is the same message view + composer as a text channel, pointed at the
//! per-user `Dm` conversations in `crate::data` instead of a channel. It is a
//! **new, self-contained module** — it does not touch `app.rs`'s existing
//! channel view or composer. It reuses two standalone pieces from `app.rs`
//! (both promoted to `pub(crate)` for that):
//! - `crate::app::message_row(author, color, body, when, edited, grouped)`
//!   renders each message row (avatar chip + author/"Today at …" line +
//!   body; the `grouped` form drops the header for same-author messages
//!   inside the grouping window — see `crate::app::same_group`);
//! - `crate::app::format_when(unix_secs, now_secs)` renders the time line.
//!
//! The composer itself (a `Signal<String>` + a styled `text_input` with an
//! Enter-to-send `on_key_down` handler) is mirrored here rather than imported,
//! because in `app.rs` it lives inline inside `app_shell`, not as a reusable
//! component. All data is the existing `&'static` mock in `crate::data`; sent
//! DMs are kept in a per-DM session log (mirroring the channel `sent` log).
//!
//! Entry points:
//! - `dm_screen()` — the whole DM area: the list column + the active
//!   conversation. This is the surface a navigation entry would mount.
//! - `dm_list(active)` — the recent-conversations column.
//! - `dm_conversation(dm_id, ...)` — one conversation (header + messages +
//!   composer).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use runtime_core::{
    AlignItems, Element, FlexDirection, FontWeight, JustifyContent, KeyEvent, KeyOutcome,
    Overflow, Signal, StyleRules, Tokenized, TouchPhase, TouchResponse,
    signal, switch, ui,
};

use crate::app::{format_when, message_row, same_group};
use crate::data::{DMS, USER, find_dm, now_secs};
use crate::read_receipt_indicator::ReadReceiptIndicator;
use crate::style_helpers::{borderless_input, col, px, section_label};

// --- palette -----------------------------------------------------------------
// The same Discord dark palette as `app.rs`, kept local so the module is
// self-contained (mirrors how `profile_settings.rs` keeps its own constants).
const BG_SIDEBAR: &str = "#2b2d31";
const BG_MAIN: &str = "#313338";
const BG_ACTIVE: &str = "#404249";
const BG_HOVER: &str = "#35373c";
const TEXT: &str = "#f2f3f5";
const TEXT_MUTED: &str = "#949ba4";
const BORDER: &str = "#26272b";
const COMPOSER: &str = "#383a40";
const BUTTON: &str = "#5865f2";

fn grow(v: f32) -> Tokenized<f32> {
    Tokenized::Literal(v)
}

fn border_width(px: f32) -> Tokenized<f32> {
    Tokenized::Literal(px)
}

/// Mock "unread" badge counts per DM (the other participant's id). A real build
/// would derive this from a read-cursor; here it is a static seed so the list
/// shows the familiar red unread pill. `pub(crate)` so `app_shell` can seed the
/// reactive per-DM unread map from it (the map is the single source of truth
/// once the screen is up — a read DM maps to 0).
pub(crate) fn mock_unread(dm_id: u32) -> u32 {
    match dm_id {
        1 => 1,
        2 => 2,
        4 => 1,
        _ => 0,
    }
}

// --- DM list -----------------------------------------------------------------

/// The recent-conversations list, newest first. `active` is the currently open
/// DM id (used to highlight the selected row); `unread`/`unread_rev` are the
/// reactive per-DM badge-count state (seeded from `mock_unread` in `app_shell`
/// — the map is the single source of truth, a read DM maps to 0) and the
/// rebuild counter the list `switch` folds into its key so a reset re-renders
/// the rows.
pub fn dm_list(
    active: Signal<u32>,
    unread: Rc<RefCell<HashMap<u32, u32>>>,
    unread_rev: Signal<u32>,
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

    // "RECENT" header — the small uppercase section label, mirroring the
    // channel sidebar's category headers.
    let section_style = StyleRules {
        padding_top: Some(px(14.0)),
        padding_bottom: Some(px(6.0)),
        padding_left: Some(px(16.0)),
        padding_right: Some(px(16.0)),
        ..section_label()
    };
    let section_label = "Direct Messages".to_string();

    // Recency order: sort by newest message age (smallest = most recent) so the
    // list reads "recent conversations". `DMS` is already roughly ordered, but
    // this makes the ordering explicit and stable (ties broken by id).
    let mut ordered: Vec<u32> = DMS.iter().map(|d| d.id).collect();
    ordered.sort_by_key(|id| {
        (find_dm(*id).map(|d| d.last_age_secs()).unwrap_or(0), *id)
    });

    // The rows live inside a `switch` keyed on the unread-revision signal so a
    // badge reset re-runs this loop and re-reads each row's count from the
    // shared `unread` map — the channel sidebar's exact idiom (the rows rebuild
    // on the key change, re-borrowing the map for the fresh counts). Each row's
    // hover signal is still owned by the row's own closures.
    let list = switch(
        move || unread_rev.get(),
        move |_rev: &u32| {
            let rows: Vec<Element> = ordered
                .iter()
                .filter_map(|id| find_dm(*id).map(|dm| dm_row(dm.id, active, unread.clone())))
                .collect();
            ui! {
                view(style = column_style.clone()) {
                    text(style = section_style.clone()) { section_label.clone() }
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

/// One DM row: avatar chip (with a status dot), the participant's name, a
/// preview of their last message, the last-message time, and an unread badge
/// (read from the shared reactive `unread` map).
fn dm_row(
    dm_id: u32,
    active: Signal<u32>,
    unread: Rc<RefCell<HashMap<u32, u32>>>,
) -> Element {
    // `find_dm` is total over the `ordered` ids, so this is safe.
    let dm = find_dm(dm_id).expect("dm id came from DMS");
    let name = dm.user.name.to_string();
    let tint = dm.user.color;
    let status = dm.user.status;
    let preview = dm
        .messages
        .last()
        .map(|m| {
            // Prefix the other participant's messages; "you" lines read as
            // "you: …" like Discord's list preview.
            if m.author == USER.name {
                format!("you: {}", m.body)
            } else {
                m.body.to_string()
            }
        })
        .unwrap_or_else(|| "No messages yet".to_string());
    let when = {
        let now = now_secs();
        format_when(dm.messages.last().map(|m| m.ts_secs(now)).unwrap_or(now), now)
    };
    // The badge count from the shared reactive map (seeded from `mock_unread`
    // in `app_shell`; a DM maps to 0 once it's opened). `unwrap_or(0)` is
    // defensive — every DM has a seeded entry.
    let unread = unread.borrow().get(&dm_id).copied().unwrap_or(0);

    let hovered = signal(false);
    let row_style = move || {
        let on = active.get() == dm_id;
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

    // Avatar chip: a rounded-square tinted circle + the initial, with a small
    // presence dot at its lower-left (mirrors Discord's list avatar).
    let avatar_style = StyleRules {
        width: Some(px(34.0)),
        height: Some(px(34.0)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        background: Some(col(tint)),
        border_top_left_radius: Some(px(12.0)),
        border_top_right_radius: Some(px(12.0)),
        border_bottom_left_radius: Some(px(12.0)),
        border_bottom_right_radius: Some(px(12.0)),
        position: Some(runtime_core::Position::Relative),
        ..Default::default()
    };
    let avatar_initial = name.chars().next().unwrap_or(' ').to_string();
    let avatar_initial_style = StyleRules {
        font_size: Some(px(14.0)),
        font_weight: Some(FontWeight::Bold),
        color: Some(col(TEXT)),
        ..Default::default()
    };
    let dot_style = StyleRules {
        position: Some(runtime_core::Position::Absolute),
        right: Some(px(-3.0)),
        bottom: Some(px(-3.0)),
        width: Some(px(11.0)),
        height: Some(px(11.0)),
        background: Some(col(status.color())),
        border_top_left_radius: Some(px(6.0)),
        border_top_right_radius: Some(px(6.0)),
        border_bottom_left_radius: Some(px(6.0)),
        border_bottom_right_radius: Some(px(6.0)),
        border_top_width: Some(border_width(2.0)),
        border_right_width: Some(border_width(2.0)),
        border_bottom_width: Some(border_width(2.0)),
        border_left_width: Some(border_width(2.0)),
        border_top_color: Some(col(BG_SIDEBAR)),
        border_right_color: Some(col(BG_SIDEBAR)),
        border_bottom_color: Some(col(BG_SIDEBAR)),
        border_left_color: Some(col(BG_SIDEBAR)),
        ..Default::default()
    };

    let stack_style = StyleRules {
        flex_direction: Some(FlexDirection::Column),
        flex_grow: Some(grow(1.0)),
        gap: Some(px(3.0)),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    let name_style = move || StyleRules {
        font_size: Some(px(15.0)),
        font_weight: Some(if active.get() == dm_id {
            FontWeight::SemiBold
        } else {
            FontWeight::Normal
        }),
        color: Some(col(TEXT)),
        ..Default::default()
    };
    let meta_style = StyleRules {
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Baseline),
        gap: Some(px(6.0)),
        ..Default::default()
    };
    let preview_style = StyleRules {
        font_size: Some(px(13.0)),
        color: Some(col(TEXT_MUTED)),
        flex_grow: Some(grow(1.0)),
        // Clip to a single line (the `ui!` text primitive wraps; a fixed-height
        // row + overflow hidden keeps it to the line).
        ..Default::default()
    };
    let when_style = StyleRules {
        font_size: Some(px(11.0)),
        color: Some(col(TEXT_MUTED)),
        ..Default::default()
    };
    let unread_style = StyleRules {
        min_width: Some(px(18.0)),
        height: Some(px(18.0)),
        padding_left: Some(px(5.0)),
        padding_right: Some(px(5.0)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        background: Some(col("#f23f43")),
        color: Some(col(TEXT)),
        font_size: Some(px(11.0)),
        font_weight: Some(FontWeight::Bold),
        border_top_left_radius: Some(px(9.0)),
        border_top_right_radius: Some(px(9.0)),
        border_bottom_left_radius: Some(px(9.0)),
        border_bottom_right_radius: Some(px(9.0)),
        ..Default::default()
    };
    let unread_label = unread.to_string();

    ui! {
        view(style = row_style) {
            view(style = avatar_style) {
                text(style = avatar_initial_style) { avatar_initial }
                view(style = dot_style) {}
            }
            view(style = stack_style) {
                text(style = name_style) { name }
                view(style = meta_style) {
                    text(style = preview_style) { preview }
                    text(style = when_style) { when }
                    if unread > 0 {
                        text(style = unread_style) { unread_label }
                    }
                }
            }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                active.set(dm_id);
            }
            TouchResponse::CONSUMED
        })
        .on_hover(move |on| hovered.set(on))
    }
}

// --- DM conversation ---------------------------------------------------------

/// One conversation: a header (participant + presence), the message list, and
/// a composer. `dm_id` selects the conversation from the mock data; `sent` and
/// `revision` are the per-DM session log + rebuild counter (mirroring the
/// channel `sent`/`revision` pair in `app.rs`).
pub fn dm_conversation(dm_id: u32, sent: Rc<RefCell<Vec<(u32, &'static str, String, i64)>>> , revision: Signal<u32>) -> Element {
    // `find_dm` is total over `DMS` ids.
    let dm = find_dm(dm_id).expect("dm id came from DMS");

    // --- header ------------------------------------------------------------
    let header_style = StyleRules {
        padding_top: Some(px(10.0)),
        padding_bottom: Some(px(10.0)),
        padding_left: Some(px(16.0)),
        padding_right: Some(px(16.0)),
        border_bottom_width: Some(border_width(1.0)),
        border_bottom_color: Some(col(BORDER)),
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        gap: Some(px(10.0)),
        ..Default::default()
    };
    let header_avatar_style = StyleRules {
        width: Some(px(30.0)),
        height: Some(px(30.0)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        background: Some(col(dm.user.color)),
        border_top_left_radius: Some(px(10.0)),
        border_top_right_radius: Some(px(10.0)),
        border_bottom_left_radius: Some(px(10.0)),
        border_bottom_right_radius: Some(px(10.0)),
        position: Some(runtime_core::Position::Relative),
        ..Default::default()
    };
    let header_initial = dm.user.name.chars().next().unwrap_or(' ').to_string();
    let header_initial_style = StyleRules {
        font_size: Some(px(13.0)),
        font_weight: Some(FontWeight::Bold),
        color: Some(col(TEXT)),
        ..Default::default()
    };
    let header_dot_style = StyleRules {
        position: Some(runtime_core::Position::Absolute),
        right: Some(px(-2.0)),
        bottom: Some(px(-2.0)),
        width: Some(px(10.0)),
        height: Some(px(10.0)),
        background: Some(col(dm.user.status.color())),
        border_top_left_radius: Some(px(5.0)),
        border_top_right_radius: Some(px(5.0)),
        border_bottom_left_radius: Some(px(5.0)),
        border_bottom_right_radius: Some(px(5.0)),
        border_top_width: Some(border_width(2.0)),
        border_right_width: Some(border_width(2.0)),
        border_bottom_width: Some(border_width(2.0)),
        border_left_width: Some(border_width(2.0)),
        border_top_color: Some(col(BG_MAIN)),
        border_right_color: Some(col(BG_MAIN)),
        border_bottom_color: Some(col(BG_MAIN)),
        border_left_color: Some(col(BG_MAIN)),
        ..Default::default()
    };
    let header_stack_style = StyleRules {
        flex_direction: Some(FlexDirection::Column),
        gap: Some(px(1.0)),
        ..Default::default()
    };
    let header_name_style = StyleRules {
        font_size: Some(px(15.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT)),
        ..Default::default()
    };
    let header_status_style = StyleRules {
        font_size: Some(px(12.0)),
        color: Some(col(TEXT_MUTED)),
        ..Default::default()
    };
    let header_name = dm.user.name.to_string();
    let header_status = format!(
        "— {}",
        dm.user.status.name()
    );

    // --- message list ------------------------------------------------------
    // Rebuilds when `revision` bumps (a send) — the same value-dedup `switch`
    // key trick the channel message list uses. `sent_read` is a dedicated
    // clone so this closure and `on_send` don't both `move` the same `Rc`.
    let sent_read = sent.clone();
    let messages = switch(
        move || revision.get(),
        move |_: &u32| {
            let now = now_secs();
            let mut rows: Vec<Element> = Vec::with_capacity(dm.messages.len());
            // Message-grouping state (seed then this session's sends —
            // oldest-first): the previous row's author + timestamp.
            let mut prev: Option<(&str, i64)> = None;
            for m in dm.messages.iter() {
                let ts = m.ts_secs(now);
                let when = format_when(ts, now);
                let grouped = prev
                    .map(|(pa, pts)| same_group(pa, pts, m.author, ts))
                    .unwrap_or(false);
                rows.push(message_row(
                    m.author,
                    m.color,
                    m.body,
                    when,
                    crate::app::format_full_when(ts),
                    false,
                    grouped,
                ));
                prev = Some((m.author, ts));
            }
            // The messages the current user sent in this DM this session, in
            // send order (so the LAST entry is the newest). Collected first so
            // the loop below can know which one is most recent.
            let mine: Vec<(u32, &'static str, String, i64)> = sent_read
                .borrow()
                .clone()
                .into_iter()
                .filter(|(dmid, _, _, _)| *dmid == dm.id)
                .collect();
            for (i, (_dmid, author, body, ts)) in mine.iter().enumerate() {
                let when = format_when(*ts, now);
                let grouped = prev
                    .map(|(pa, pts)| same_group(pa, pts, author, *ts))
                    .unwrap_or(false);
                let base = message_row(
                    author,
                    BUTTON,
                    body.as_str(),
                    when,
                    crate::app::format_full_when(*ts),
                    false,
                    grouped,
                );
                // Read receipt: Discord's "seen" double-check is a DM
                // convention (not a server-channel one), shown on the most
                // recent message the current user sent. `mine` is in send
                // order, so `i + 1 == mine.len()` holds exactly for that
                // newest message — a deliberate SINGLE instance on the newest
                // sent message, NOT a per-row widget (the graveyard
                // anti-pattern). The other sent rows render `base` unchanged.
                if i + 1 == mine.len() {
                    rows.push(ui! {
                        view(style = StyleRules {
                            flex_direction: Some(FlexDirection::Column),
                            ..Default::default()
                        }) {
                            { base }
                            // Right-aligned, just under the newest sent
                            // message — Discord's receipt sits at the tail of
                            // the line. The indicator is self-styled.
                            view(style = StyleRules {
                                flex_direction: Some(FlexDirection::Row),
                                justify_content: Some(JustifyContent::FlexEnd),
                                padding_top: Some(px(1.0)),
                                padding_right: Some(px(16.0)),
                                ..Default::default()
                            }) {
                                ReadReceiptIndicator()
                            }
                        }
                    });
                } else {
                    rows.push(base);
                }
                prev = Some((author, *ts));
            }
            ui! {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Column),
                    ..Default::default()
                }) { rows }
            }
        },
    );

    // --- composer ----------------------------------------------------------
    // Mirrors the channel composer in `app.rs` (a `Signal<String>`, a styled
    // `text_input`, and an Enter-to-send `on_key_down` handler), scoped to this
    // DM. The Send button and the Enter key share one `Rc`'d send action.
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
    let send_style = StyleRules {
        height: Some(px(40.0)),
        padding_left: Some(px(18.0)),
        padding_right: Some(px(18.0)),
        background: Some(col(BUTTON)),
        color: Some(col(TEXT)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        ..Default::default()
    };

    let text: Signal<String> = signal(String::new());
    let on_change = move |s: String| {
        text.set(s);
    };
    let text_read = text.clone();
    let sent_write = sent.clone();
    let revision_bump = revision.clone();
    let do_send = move || {
        let body = text_read.get();
        if body.trim().is_empty() {
            return;
        }
        sent_write.borrow_mut().push((dm.id, USER.name, body, now_secs()));
        text_read.set(String::new());
        // Bump so the message-list `switch` rebuilds (its key dedups on value).
        revision_bump.set(revision_bump.get() + 1);
    };
    let on_send = Rc::new(do_send);
    let on_click = {
        let send_btn = on_send.clone();
        move || (send_btn)()
    };
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

    // --- assemble ----------------------------------------------------------
    ui! {
        view(style = StyleRules {
            flex_grow: Some(grow(1.0)),
            background: Some(col(BG_MAIN)),
            flex_direction: Some(FlexDirection::Column),
            overflow: Some(Overflow::Hidden),
            ..Default::default()
        }) {
            view(style = header_style) {
                view(style = header_avatar_style) {
                    text(style = header_initial_style) { header_initial }
                    view(style = header_dot_style) {}
                }
                view(style = header_stack_style) {
                    text(style = header_name_style) { header_name }
                    text(style = header_status_style) { header_status }
                }
            }
            scroll_view(style = StyleRules { flex_grow: Some(grow(1.0)), ..Default::default() }) { messages }
            view(style = composer_style) {
                text_input(value = text, on_change = on_change, placeholder = "Message", style = input_style).on_key_down(on_enter)
                button(label = "Send".to_string(), on_click = on_click, style = send_style)
            }
        }
    }
}

// --- screen (list + active conversation) -------------------------------------

/// The whole DM area: the recent-conversations list plus the active
/// conversation. This is the surface a navigation entry would mount (e.g. a
/// rail icon or a sidebar section). `active` selects the open conversation;
/// `sent`/`revision` are the per-DM session log + rebuild counter; `unread` /
/// `unread_rev` are the reactive per-DM badge-count state (owned in `app_shell`
/// so it survives a close/re-open).
pub fn dm_screen(
    active: Signal<u32>,
    sent: Rc<RefCell<Vec<(u32, &'static str, String, i64)>>>,
    revision: Signal<u32>,
    unread: Rc<RefCell<HashMap<u32, u32>>>,
    unread_rev: Signal<u32>,
) -> Element {
    // Default to the most recent conversation (the first entry of the list).
    let default_active: Signal<u32> = active;
    let list = dm_list(default_active.clone(), unread.clone(), unread_rev);

    // The conversation view rebuilds when the selected DM changes.
    let conv = switch(
        move || active.get(),
        move |dm_id: &u32| dm_conversation(*dm_id, sent.clone(), revision.clone()),
    );

    // Clear a DM's unread badge the moment it's opened, so the red pill drops
    // as soon as the user reads it. Side-effect `switch` keyed on the active DM
    // (the channel `mention_reset_effect` idiom in `app.rs`): it fires when
    // `active` changes value — the per-row click handler can't reach the shared
    // `unread` map out of its own scope. Bumping `unread_rev` re-keys the list
    // `switch`, which re-reads the now-zero count and drops the badge. The count
    // is reset optimistically (no mark-read RPC exists for the mock DM path —
    // the backend `mark_read` is channel-scoped), which is correct: opening a
    // DM has read its messages by definition.
    let dm_reset_active = active;
    let dm_reset_buf = unread.clone();
    let dm_reset_rev = unread_rev;
    let dm_reset_effect = switch(
        move || dm_reset_active.get(),
        move |dm_id: &u32| {
            if dm_reset_buf.borrow().get(dm_id).copied().unwrap_or(0) > 0 {
                dm_reset_buf.borrow_mut().insert(*dm_id, 0);
                dm_reset_rev.set(dm_reset_rev.get() + 1);
            }
            ui! {
                view(style = StyleRules {
                    height: Some(px(0.0)),
                    ..Default::default()
                }) {}
            }
        },
    );

    ui! {
        view(style = StyleRules {
            flex_grow: Some(grow(1.0)),
            flex_direction: Some(FlexDirection::Row),
            overflow: Some(Overflow::Hidden),
            ..Default::default()
        }) {
            list
            conv
            // Zero-height side-effect arm: mounted in the live tree so the reset
            // `switch` fires when `active` changes (same placement as
            // `mention_reset_effect` in the channel rail).
            dm_reset_effect
        }
    }
}
