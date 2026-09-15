//! `app()` — the Discord-style layout: a server rail, a channel sidebar (with
//! category sections and voice-channel presence rows), a main message view with
//! a composer, and a member list. State is a few signals (active server,
//! active channel, composer text) plus a per-session log of sent messages; the
//! tree is declarative around them.
//!
//! Idioms, each verified against a working example in `examples/`:
//! - styles are plain `StyleRules` (via `crate::style_helpers` `px` / `pct` /
//!   `col`) — one pass of static CSS, no live style engine;
//! - active-state highlight is a *reactive* style closure
//!   (`move || StyleRules { … }`), re-run on every signal change — the
//!   `IntoStyleProp for Fn()->StyleRules` dynamic path (`style_attach.rs`);
//! - clickable rows are the `view` builder's `.on_touch(|ev| …)` chain (the
//!   `ui!` macro's `view` tag is `style` + children only) — `whiteboard-demo`;
//! - the channel list, header, and message list are `switch(key, render)`
//!   keyed on `(server_id, channel_id)` so they rebuild when the selection
//!   changes — the same pattern `whiteboard-demo` uses to rebuild its canvas
//!   list (and `switch`'s render is `Fn`, so captured `StyleRules` are cloned
//!   inside the closure, exactly as whiteboard does);
//! - a precomputed `Element` / `Vec<Element>` splices in as a bare child —
//!   `whiteboard-demo` `scroll_view { rows }`.
//!
//! Timestamps: message rows show "Today at …" / "Yesterday at …" lines,
//! computed at rebuild from the runtime's wall clock (`runtime_shared::time`
//! — `epoch_millis()` / `local_offset_minutes()`; the web backend installs a
//! `js Date`-backed source during bootstrap). No `chrono`/`time` needed: a
//! days-ago comparison plus a civil-time formatter is all a "Today at" label
//! needs.

use std::cell::RefCell;
use std::rc::Rc;

use runtime_core::{
    AlignItems, AlignSelf, Element, FlexDirection, FontWeight, JustifyContent, KeyEvent, KeyOutcome,
    Overflow, Position, Signal, StyleRules, TextAlign, TextTransform, Tokenized, TouchPhase,
    TouchResponse,
    signal, switch, ui,
};

use runtime_shared::time::local_offset_minutes;

use crate::data::{SERVERS, Category, Member, Status, USER};
use crate::style_helpers::{col, pct, px};

// Discord dark palette (Blurple-flavoured).
const BG_RAIL: &str = "#1e1f22";
const BG_SIDEBAR: &str = "#2b2d31";
const BG_MAIN: &str = "#313338";
const BG_ACTIVE: &str = "#404249";
/// Discord's row hover colour (lighter than `#2b2d31`, darker than active).
const BG_HOVER: &str = "#35373c";
const TEXT: &str = "#f2f3f5";
const TEXT_MUTED: &str = "#949ba4";
const TEXT_CHANNEL: &str = "#c8cacf";
const BORDER: &str = "#26272b";
const COMPOSER: &str = "#383a40";
const BUTTON: &str = "#5865f2";
/// Active-server indicator pill on the rail (Discord's white sliver).
const PILL: &str = "#f2f3f5";
/// The user-profile section at the bottom of the sidebar sits a shade darker
/// than the channel list above it.
const USER_BAR: &str = "#232428";

fn grow(v: f32) -> Tokenized<f32> {
    Tokenized::Literal(v)
}

fn border_width(px: f32) -> Tokenized<f32> {
    Tokenized::Literal(px)
}

// --- timestamps --------------------------------------------------------------

const DAY_MS: i64 = 86_400_000;

/// Civil local time (day number, hour, minute) for a Unix-seconds instant.
/// Pure math on the wall clock — no date library: all a "Today at …" label
/// needs is *which day* (relative to now) and the clock time.
fn local_day_clock(unix_secs: i64) -> (i64, u32, u32) {
    let local_ms = unix_secs * 1_000 + local_offset_minutes() as i64 * 60_000;
    // Floor-divide: local_ms can be negative for pre-1970 instants.
    let day = local_ms.div_euclid(DAY_MS);
    let rem = local_ms.rem_euclid(DAY_MS);
    let h = (rem / 3_600_000) as u32;
    let m = ((rem % 3_600_000) / 60_000) as u32;
    (day, h, m)
}

/// Convert a day number (days since 1970-01-01) to a civil (year, month, day)
/// triple using the standard 4-00-04 leap-year rule. Only exercised by the
/// "older than a day" fallback in `format_when` — mock seed messages never
/// reach it, but the branch shouldn't emit garbage if it ever does.
fn civil_ymd_from_day(day: i64) -> (i64, u32, u32) {
    // Howard Hinnant's `days_to_ymd` — pure integer math, valid for all
    // positive day numbers (and most negative ones).
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

/// Discord-style message time line: "Today at 10:14 AM", "Yesterday at …",
/// or "9/12/2026" once it's older than a day.
fn format_when(unix_secs: i64, now_secs: i64) -> String {
    let (day, h, m) = local_day_clock(unix_secs);
    let (now_day, _, _) = local_day_clock(now_secs);
    let (suffix, h12) = {
        let h12 = h % 12;
        let h12 = if h12 == 0 { 12 } else { h12 };
        (if h < 12 { "AM" } else { "PM" }, h12)
    };
    let time = format!("{h12}:{m:02} {suffix}");
    match now_day - day {
        0 => format!("Today at {time}"),
        1 => format!("Yesterday at {time}"),
        // Older than a day: fall back to a month/day/year built from the day
        // number. (Mock data is always ≤ ~1 day old, so this is the safety
        // branch — kept simple on purpose.)
        _ => {
            let (y, m, d) = civil_ymd_from_day(day);
            format!("{m}/{d}/{y}")
        }
    }
}

// --- small reusable rows -----------------------------------------------------

/// One message row: avatar chip, then author + "Today at …" line, then body.
/// Takes `&str`s so it serves both the static seed data and runtime-authored
/// messages (the strings are copied into the node, so the returned `Element`
/// is self-contained).
fn message_row(author: &str, color: &str, body: &str, when: String) -> Element {
    let row_style = StyleRules {
        padding_top: Some(px(8.0)),
        padding_bottom: Some(px(2.0)),
        padding_left: Some(px(16.0)),
        padding_right: Some(px(16.0)),
        gap: Some(px(10.0)),
        flex_direction: Some(FlexDirection::Row),
        ..Default::default()
    };
    let avatar_style = StyleRules {
        width: Some(px(40.0)),
        height: Some(px(40.0)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        background: Some(col(color)),
        border_top_left_radius: Some(px(20.0)),
        border_top_right_radius: Some(px(20.0)),
        border_bottom_left_radius: Some(px(20.0)),
        border_bottom_right_radius: Some(px(20.0)),
        ..Default::default()
    };
    let avatar_text_style = StyleRules {
        font_size: Some(px(15.0)),
        font_weight: Some(FontWeight::Bold),
        color: Some(col(TEXT)),
        ..Default::default()
    };
    let stack_style = StyleRules {
        flex_direction: Some(FlexDirection::Column),
        flex_grow: Some(grow(1.0)),
        gap: Some(px(2.0)),
        ..Default::default()
    };
    let meta_style = StyleRules {
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Baseline),
        gap: Some(px(6.0)),
        ..Default::default()
    };
    let author_style = StyleRules {
        font_size: Some(px(15.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT)),
        ..Default::default()
    };
    let when_style = StyleRules {
        font_size: Some(px(11.0)),
        color: Some(col(TEXT_MUTED)),
        ..Default::default()
    };
    let body_style = StyleRules {
        font_size: Some(px(15.0)),
        color: Some(col(TEXT)),
        flex_grow: Some(grow(1.0)),
        ..Default::default()
    };
    let initial = author.chars().next().unwrap_or(' ').to_string();
    ui! {
        view(style = row_style) {
            view(style = avatar_style) {
                text(style = avatar_text_style) { initial }
            }
            view(style = stack_style) {
                view(style = meta_style) {
                    text(style = author_style) { author.to_string() }
                    text(style = when_style) { when }
                }
                text(style = body_style) { body.to_string() }
            }
        }
    }
}

/// One member row: avatar chip with the status dot overlaid on its
/// bottom-right corner, then the name. Offline rows render dimmed,
/// Discord-style. Pure data in, self-contained `Element` out (like
/// `message_row`) so `member_panel` can splice them into a column.
fn member_row(m: &Member) -> Element {
    let dim = m.status == Status::Offline;
    let row_style = StyleRules {
        padding_top: Some(px(4.0)),
        padding_bottom: Some(px(4.0)),
        padding_left: Some(px(10.0)),
        padding_right: Some(px(12.0)),
        gap: Some(px(10.0)),
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        ..Default::default()
    };
    let avatar_style = StyleRules {
        width: Some(px(32.0)),
        height: Some(px(32.0)),
        position: Some(Position::Relative),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        background: Some(col(m.color)),
        border_top_left_radius: Some(px(16.0)),
        border_top_right_radius: Some(px(16.0)),
        border_bottom_left_radius: Some(px(16.0)),
        border_bottom_right_radius: Some(px(16.0)),
        ..Default::default()
    };
    let initial_style = StyleRules {
        font_size: Some(px(14.0)),
        font_weight: Some(FontWeight::Bold),
        color: Some(col(TEXT)),
        ..Default::default()
    };
    // The status dot: absolutely positioned at the avatar's bottom-right,
    // with a 2px ring matching the sidebar bg so it reads as "attached".
    let dot_style = StyleRules {
        position: Some(Position::Absolute),
        right: Some(px(0.0)),
        bottom: Some(px(0.0)),
        width: Some(px(11.0)),
        height: Some(px(11.0)),
        background: Some(col(m.status.color())),
        border_top_width: Some(border_width(2.0)),
        border_right_width: Some(border_width(2.0)),
        border_bottom_width: Some(border_width(2.0)),
        border_left_width: Some(border_width(2.0)),
        border_top_color: Some(col(BG_SIDEBAR)),
        border_right_color: Some(col(BG_SIDEBAR)),
        border_bottom_color: Some(col(BG_SIDEBAR)),
        border_left_color: Some(col(BG_SIDEBAR)),
        border_top_left_radius: Some(px(6.0)),
        border_top_right_radius: Some(px(6.0)),
        border_bottom_left_radius: Some(px(6.0)),
        border_bottom_right_radius: Some(px(6.0)),
        ..Default::default()
    };
    let name_style = StyleRules {
        font_size: Some(px(15.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(if dim { TEXT_MUTED } else { TEXT })),
        ..Default::default()
    };
    let initial = m.name.chars().next().unwrap_or(' ').to_string();
    ui! {
        view(style = row_style) {
            view(style = avatar_style) {
                text(style = initial_style) { initial }
                view(style = dot_style) {}
            }
            text(style = name_style) { m.name.to_string() }
        }
    }
}

/// A voice-channel row in the sidebar: "🔊 name" (a speaker glyph rendered as
/// the Unicode speaker character — the `icon` primitive isn't available in
/// the `ui!` macro here, so a text glyph keeps the row dependency-free), with
/// the connected members listed beneath it in smaller, indented rows.
///
/// Interactive: clicking the row **joins** the channel (Discord's click-to-
/// join). While joined, the user's row appears in the member list beneath
/// the name and the profile strip turns into a voice-connection bar. Clicking
/// a joined row again leaves. Join state is the app-level `joined_voice`
/// signal — this function's output is only valid for `server_id` (the signal
/// is keyed by server), and it's rebuilt whenever that signal changes (the
/// sidebar `switch`'s key includes it).
fn voice_section(
    cat: &Category,
    server_id: u32,
    active_server: Signal<u32>,
    joined_voice: Signal<Option<(u32, u32)>>,
    user_status: Signal<Status>,
) -> Vec<Element> {
    let mut out: Vec<Element> = Vec::new();
    if cat.voice.is_empty() {
        return out;
    }
    for v in cat.voice {
        // Voice rows get the same compact pill + hover treatment as text
        // rows (Discord treats voice rows identically here).
        let hovered = signal(false);
        let joined = joined_voice.clone();
        let row_style = move || StyleRules {
            height: Some(px(34.0)),
            margin_left: Some(px(8.0)),
            margin_right: Some(px(8.0)),
            padding_left: Some(px(12.0)),
            padding_right: Some(px(8.0)),
            background: Some(col(if hovered.get() {
                BG_HOVER
            } else {
                "transparent"
            })),
            border_top_left_radius: Some(px(8.0)),
            border_top_right_radius: Some(px(8.0)),
            border_bottom_left_radius: Some(px(8.0)),
            border_bottom_right_radius: Some(px(8.0)),
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(6.0)),
            ..Default::default()
        };
        let v_label_style = move || StyleRules {
            font_size: Some(px(15.0)),
            font_weight: Some(if joined.get() == Some((server_id, v.id)) {
                FontWeight::SemiBold
            } else {
                FontWeight::Normal
            }),
            color: Some(col(if joined.get() == Some((server_id, v.id)) {
                TEXT
            } else {
                TEXT_CHANNEL
            })),
            ..Default::default()
        };
        let label = format!("🔊 {}", v.name);
        let row = ui! {
            view(style = row_style) {
                text(style = v_label_style) { label }
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    // Join/leave toggle, Discord-style: only one channel at a
                    // time, and a voice join always belongs to the server you
                    //'re looking at. Switching servers clears the join
                    // (handled in the rail handler).
                    let is_here = active_server.get() == server_id;
                    match joined.get() {
                        Some((sv, _)) if sv == server_id => {
                            // Already in a voice channel on this server: a
                            // click leaves it (even on a different row).
                            joined.set(None);
                        }
                        None if is_here => joined.set(Some((server_id, v.id))),
                        // Joined on a different server: switching to this
                        // server already cleared the join — but if it
                        // somehow survived, don't double-join.
                        _ => {}
                    }
                }
                TouchResponse::CONSUMED
            })
            .on_hover(move |on| hovered.set(on))
        };
        out.push(row);
        // Connected members, Discord's voice layout (avatar + name, indented).
        for sp in v.speakers {
            let row_style = StyleRules {
                padding_top: Some(px(2.0)),
                padding_bottom: Some(px(2.0)),
                padding_left: Some(px(44.0)),
                padding_right: Some(px(8.0)),
                gap: Some(px(8.0)),
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                ..Default::default()
            };
            let av_style = StyleRules {
                width: Some(px(20.0)),
                height: Some(px(20.0)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                background: Some(col(sp.color)),
                border_top_left_radius: Some(px(10.0)),
                border_top_right_radius: Some(px(10.0)),
                border_bottom_left_radius: Some(px(10.0)),
                border_bottom_right_radius: Some(px(10.0)),
                ..Default::default()
            };
            let av_text = StyleRules {
                font_size: Some(px(9.0)),
                font_weight: Some(FontWeight::Bold),
                color: Some(col(TEXT)),
                ..Default::default()
            };
            let name_style = StyleRules {
                font_size: Some(px(13.0)),
                color: Some(col(TEXT_MUTED)),
                ..Default::default()
            };
            let initial = sp.name.chars().next().unwrap_or(' ').to_string();
            out.push(ui! {
                view(style = row_style) {
                    view(style = av_style) {
                        text(style = av_text) { initial }
                    }
                    text(style = name_style) { sp.name.to_string() }
                }
            });
        }
        // While joined, the user's own row appears beneath the seed members —
        // Discord shows you in the channel you're in. Built as a plain row
        // (no `member_row` — the dot ring is a separate concern and this one
        // reuses the sidebar's 20px avatar scale).
        if joined_voice.get() == Some((server_id, v.id)) {
            let you = USER;
            let ustatus = user_status.get();
            let row_style = StyleRules {
                padding_top: Some(px(2.0)),
                padding_bottom: Some(px(2.0)),
                padding_left: Some(px(44.0)),
                padding_right: Some(px(8.0)),
                gap: Some(px(8.0)),
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                ..Default::default()
            };
            let av_style = StyleRules {
                width: Some(px(20.0)),
                height: Some(px(20.0)),
                position: Some(Position::Relative),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                background: Some(col(you.color)),
                border_top_left_radius: Some(px(10.0)),
                border_top_right_radius: Some(px(10.0)),
                border_bottom_left_radius: Some(px(10.0)),
                border_bottom_right_radius: Some(px(10.0)),
                ..Default::default()
            };
            let av_text = StyleRules {
                font_size: Some(px(9.0)),
                font_weight: Some(FontWeight::Bold),
                color: Some(col(TEXT)),
                ..Default::default()
            };
            let dot_style = StyleRules {
                position: Some(Position::Absolute),
                right: Some(px(0.0)),
                bottom: Some(px(0.0)),
                width: Some(px(8.0)),
                height: Some(px(8.0)),
                background: Some(col(ustatus.color())),
                border_top_width: Some(border_width(1.5)),
                border_right_width: Some(border_width(1.5)),
                border_bottom_width: Some(border_width(1.5)),
                border_left_width: Some(border_width(1.5)),
                border_top_color: Some(col(BG_SIDEBAR)),
                border_right_color: Some(col(BG_SIDEBAR)),
                border_bottom_color: Some(col(BG_SIDEBAR)),
                border_left_color: Some(col(BG_SIDEBAR)),
                border_top_left_radius: Some(px(4.0)),
                border_top_right_radius: Some(px(4.0)),
                border_bottom_left_radius: Some(px(4.0)),
                border_bottom_right_radius: Some(px(4.0)),
                ..Default::default()
            };
            let name_style = StyleRules {
                font_size: Some(px(13.0)),
                color: Some(col(TEXT)),
                ..Default::default()
            };
            let initial = you.name.chars().next().unwrap_or(' ').to_string();
            out.push(ui! {
                view(style = row_style) {
                    view(style = av_style) {
                        text(style = av_text) { initial }
                        view(style = dot_style) {}
                    }
                    text(style = name_style) { you.name.to_string() }
                }
            });
        }
    }
    out
}

/// The member-list panel for the right side of the window: a
/// "MEMBER COUNT — N" header (Discord's style), then members grouped by
/// status (online → idle → DND → offline). `user` is the signed-in user
/// rendered as a regular member entry under their *live* presence status —
/// cycling the status in the profile strip regroups them here (the panel
/// is rebuilt on `user_status` changes; see `members_switch`). Spliced in
/// as the right-most column for whichever server is active.
fn member_panel(server: &crate::data::Server, user: &Member) -> Element {
    let mut body: Vec<Element> = Vec::new();
    // Header: "MEMBER COUNT — N" where N is the roster plus the signed-in
    // user, who counts as a member like anyone else.
    let header = format!("MEMBER COUNT — {}", server.members.len() + 1);
    let header_style = StyleRules {
        padding_top: Some(px(14.0)),
        padding_bottom: Some(px(6.0)),
        padding_left: Some(px(12.0)),
        font_size: Some(px(11.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT_MUTED)),
        text_transform: Some(TextTransform::Uppercase),
        ..Default::default()
    };
    body.push(ui! { text(style = header_style) { header } });

    // Status groups in Discord order: Online, Idle, DND, Offline. The
    // signed-in user is appended to their live-status group (last, Discord's
    // "you" placement for the mock roster).
    for status in [Status::Online, Status::Idle, Status::DoNotDisturb, Status::Offline] {
        let group: Vec<&Member> = server.members.iter().filter(|m| m.status == status).collect();
        let with_user = user.status == status;
        if group.is_empty() && !with_user {
            continue;
        }
        let group_label = format!("{} — {}", status.label(), group.len() + usize::from(with_user));
        let group_style = StyleRules {
            padding_top: Some(px(10.0)),
            padding_bottom: Some(px(2.0)),
            padding_left: Some(px(12.0)),
            font_size: Some(px(11.0)),
            font_weight: Some(FontWeight::SemiBold),
            color: Some(col(TEXT_MUTED)),
            text_transform: Some(TextTransform::Uppercase),
            ..Default::default()
        };
        body.push(ui! { text(style = group_style) { group_label } });
        for m in group.iter() {
            body.push(member_row(m));
        }
        if with_user {
            body.push(member_row(user));
        }
    }

    // Section wrapper: fixed-width right-side panel (Discord's member list
    // lives on the right). The left border separates it from the main view;
    // the panel owns its own bg.
    ui! {
        view(style = StyleRules {
            width: Some(px(240.0)),
            background: Some(col(BG_SIDEBAR)),
            border_left_width: Some(border_width(1.0)),
            border_left_color: Some(col(BORDER)),
            flex_direction: Some(FlexDirection::Column),
            overflow: Some(Overflow::Hidden),
            ..Default::default()
        }) {
            scroll_view(style = StyleRules {
                flex_grow: Some(grow(1.0)),
                ..Default::default()
            }) { body }
        }
    }
}

/// The bottom-left user profile section: avatar (with status dot), name, and
/// status line. Identity comes from the data model's `USER` constant; the
/// live presence status comes from the `user_status` signal, so the strip can
/// show Idle / DND without touching the data.
///
/// While the user is in a voice channel, the name + status line becomes a
/// voice-connection line: a green "Voice Connected" (Discord's actual text),
/// the channel name beneath it, and a **Leave** button that disconnects.
/// The whole strip is wrapped in a `switch` keyed on `(status, voice_name?)`
/// so any of those changes re-renders the strip (the key is value-deduped,
/// like every other `switch` here).
fn user_panel(
    user_status: Signal<Status>,
    joined_voice: Signal<Option<(u32, u32)>>,
    authed: Signal<bool>,
) -> Element {
    switch(
        move || (user_status.get(), voice_name_for(joined_voice.get())),
        move |(status, voice_name): &(Status, Option<String>)| {
            let user = USER;
            let voice_name = voice_name.clone();
            let row_style = StyleRules {
                padding_top: Some(px(8.0)),
                padding_bottom: Some(px(8.0)),
                padding_left: Some(px(8.0)),
                padding_right: Some(px(8.0)),
                gap: Some(px(10.0)),
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                background: Some(col(USER_BAR)),
                ..Default::default()
            };
            let avatar_style = StyleRules {
                width: Some(px(32.0)),
                height: Some(px(32.0)),
                position: Some(Position::Relative),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                background: Some(col(user.color)),
                border_top_left_radius: Some(px(16.0)),
                border_top_right_radius: Some(px(16.0)),
                border_bottom_left_radius: Some(px(16.0)),
                border_bottom_right_radius: Some(px(16.0)),
                ..Default::default()
            };
            let initial_style = StyleRules {
                font_size: Some(px(14.0)),
                font_weight: Some(FontWeight::Bold),
                color: Some(col(TEXT)),
                ..Default::default()
            };
            let dot_style = StyleRules {
                position: Some(Position::Absolute),
                right: Some(px(0.0)),
                bottom: Some(px(0.0)),
                width: Some(px(11.0)),
                height: Some(px(11.0)),
                background: Some(col(status.color())),
                border_top_width: Some(border_width(2.0)),
                border_right_width: Some(border_width(2.0)),
                border_bottom_width: Some(border_width(2.0)),
                border_left_width: Some(border_width(2.0)),
                border_top_color: Some(col(USER_BAR)),
                border_right_color: Some(col(USER_BAR)),
                border_bottom_color: Some(col(USER_BAR)),
                border_left_color: Some(col(USER_BAR)),
                border_top_left_radius: Some(px(6.0)),
                border_top_right_radius: Some(px(6.0)),
                border_bottom_left_radius: Some(px(6.0)),
                border_bottom_right_radius: Some(px(6.0)),
                ..Default::default()
            };
            let text_stack = StyleRules {
                flex_direction: Some(FlexDirection::Column),
                flex_grow: Some(grow(1.0)),
                overflow: Some(Overflow::Hidden),
                ..Default::default()
            };
            let name_style = StyleRules {
                font_size: Some(px(14.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(TEXT)),
                ..Default::default()
            };
            let initial = user.name.chars().next().unwrap_or(' ').to_string();
            let name = user.name.to_string();

            // In a voice channel? Replace the name/status pair with the
            // Discord-style connection line: green "Voice Connected" + the
            // channel name, and a Leave button.
            let second_line = match voice_name {
                Some(ref channel) => {
                    let vc_style = StyleRules {
                        font_size: Some(px(12.0)),
                        font_weight: Some(FontWeight::SemiBold),
                        color: Some(col("#23a559")),
                        ..Default::default()
                    };
                    let ch_style = StyleRules {
                        font_size: Some(px(11.0)),
                        color: Some(col(TEXT_MUTED)),
                        overflow: Some(Overflow::Hidden),
                        ..Default::default()
                    };
                    ui! {
                        text(style = vc_style) { "Voice Connected".to_string() }
                        text(style = ch_style) { channel.clone() }
                    }
                }
                None => {
                    let status_style = StyleRules {
                        font_size: Some(px(12.0)),
                        color: Some(col(TEXT_MUTED)),
                        ..Default::default()
                    };
                    let status = status.name().to_string();
                    ui! {
                        text(style = status_style) { status }
                    }
                }
            };

            // The Leave button only exists while connected. Built as a
            // view + on_touch — the app's standard click pattern (the
            // runtime's `button` helper takes a fixed `label` prop, which
            // doesn't fit a styled pill; a view gives full style control).
            let leave_btn = match voice_name {
                Some(_) => {
                    let leave_style = StyleRules {
                        height: Some(px(26.0)),
                        padding_left: Some(px(10.0)),
                        padding_right: Some(px(10.0)),
                        background: Some(col(BG_SIDEBAR)),
                        color: Some(col(TEXT)),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        font_size: Some(px(12.0)),
                        font_weight: Some(FontWeight::SemiBold),
                        border_top_left_radius: Some(px(6.0)),
                        border_top_right_radius: Some(px(6.0)),
                        border_bottom_left_radius: Some(px(6.0)),
                        border_bottom_right_radius: Some(px(6.0)),
                        ..Default::default()
                    };
                    // `None` clears the join; the sidebar `switch` key
                    // includes `joined_voice`, so the voice rows rebuild too.
                    let leave = joined_voice.clone();
                    let leave_text_style = StyleRules {
                        color: Some(col(TEXT)),
                        font_size: Some(px(12.0)),
                        font_weight: Some(FontWeight::SemiBold),
                        ..Default::default()
                    };
                    Some(ui! {
                        view(style = leave_style) {
                            text(style = leave_text_style) { "Leave".to_string() }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                leave.set(None);
                            }
                            TouchResponse::CONSUMED
                        })
                    })
                }
                None => None,
            };

            // Click the avatar to cycle the live status (Discord's
            // avatar-click status menu, simplified to a one-click cycle:
            // Online → Idle → DND → Online). The touch target is the avatar
            // only, not the whole row — the Leave button lives in the same
            // row while connected, and its own `on_touch` owns that region.
            let avatar = ui! {
                view(style = avatar_style) {
                    text(style = initial_style) { initial }
                    view(style = dot_style) {}
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        let touch = user_status;
                        touch.set(touch.get().next());
                    }
                    TouchResponse::CONSUMED
                })
            };
            // Log Out: flips the shared `authed` signal back to false, which
            // unmounts this tree and re-mounts the login screen (the `switch`
            // key in `app()` re-evaluates on it). Same view+on_touch pill the
            // Leave button uses, so it fits the row's styling.
            let logout_style = StyleRules {
                height: Some(px(26.0)),
                padding_left: Some(px(10.0)),
                padding_right: Some(px(10.0)),
                background: Some(col(BG_SIDEBAR)),
                color: Some(col(TEXT_MUTED)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                font_size: Some(px(12.0)),
                font_weight: Some(FontWeight::SemiBold),
                border_top_left_radius: Some(px(6.0)),
                border_top_right_radius: Some(px(6.0)),
                border_bottom_left_radius: Some(px(6.0)),
                border_bottom_right_radius: Some(px(6.0)),
                ..Default::default()
            };
            let logout = authed.clone();
            let logout_text_style = StyleRules {
                color: Some(col(TEXT_MUTED)),
                font_size: Some(px(12.0)),
                font_weight: Some(FontWeight::SemiBold),
                ..Default::default()
            };
            let logout_btn = ui! {
                view(style = logout_style) {
                    text(style = logout_text_style) { "Log Out".to_string() }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        logout.set(false);
                    }
                    TouchResponse::CONSUMED
                })
            };
            ui! {
                view(style = row_style) {
                    avatar
                    view(style = text_stack) {
                        text(style = name_style) { name }
                        second_line
                    }
                    if let Some(btn) = leave_btn {
                        btn
                    }
                    logout_btn
                }
            }
        },
    )
}

/// The name of the voice channel the user is currently joined to, resolved
/// from the `(server_id, voice_id)` pair in `joined_voice`. Used to build the
/// `user_panel` switch key (value-deduped — `String` compares by content, so
/// two joins of the same channel on different servers would be equal, which
/// is exactly the "same visible state, no rebuild" behavior we want).
fn voice_name_for(joined: Option<(u32, u32)>) -> Option<String> {
    let (server_id, voice_id) = joined?;
    let server = SERVERS.iter().find(|s| s.id == server_id)?;
    Some(server.find_voice_channel(voice_id)?.name.to_string())
}

// --- mock login screen -------------------------------------------------------

/// The mock login screen, shown while `authed` is false. No backend: any
/// non-empty email + password signs in (the password field is `secure`, so
/// it masks input like a real login); empty fields show an inline error
/// instead. "Continue as guest" skips the fields entirely — the mock has one
/// identity ("you"), so guest == you. State is local to this function — it
/// unmounts the moment `authed` flips, so no session survives a reload.
fn login_screen(authed: Signal<bool>) -> Element {
    // "you" is the only identity in this mock build; authing in just flips
    // the shared signal the app tree is keyed on.
    let email: Signal<String> = signal(String::new());
    let password: Signal<String> = signal(String::new());
    let error: Signal<String> = signal(String::new());

    // Card + text styles (static — built once, cloned into the render).
    let card_style = StyleRules {
        width: Some(px(380.0)),
        padding_top: Some(px(40.0)),
        padding_bottom: Some(px(32.0)),
        padding_left: Some(px(32.0)),
        padding_right: Some(px(32.0)),
        background: Some(col(BG_SIDEBAR)),
        border_top_width: Some(border_width(1.0)),
        border_right_width: Some(border_width(1.0)),
        border_bottom_width: Some(border_width(1.0)),
        border_left_width: Some(border_width(1.0)),
        border_top_color: Some(col(BORDER)),
        border_right_color: Some(col(BORDER)),
        border_bottom_color: Some(col(BORDER)),
        border_left_color: Some(col(BORDER)),
        border_top_left_radius: Some(px(12.0)),
        border_top_right_radius: Some(px(12.0)),
        border_bottom_left_radius: Some(px(12.0)),
        border_bottom_right_radius: Some(px(12.0)),
        flex_direction: Some(FlexDirection::Column),
        align_items: Some(AlignItems::Center),
        ..Default::default()
    };
    let logo_style = StyleRules {
        width: Some(px(64.0)),
        height: Some(px(64.0)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        background: Some(col(BUTTON)),
        border_top_left_radius: Some(px(20.0)),
        border_top_right_radius: Some(px(20.0)),
        border_bottom_left_radius: Some(px(20.0)),
        border_bottom_right_radius: Some(px(20.0)),
        ..Default::default()
    };
    let logo_text_style = StyleRules {
        font_size: Some(px(30.0)),
        font_weight: Some(FontWeight::Bold),
        color: Some(col(TEXT)),
        ..Default::default()
    };
    let heading_style = StyleRules {
        margin_top: Some(px(20.0)),
        font_size: Some(px(22.0)),
        font_weight: Some(FontWeight::Bold),
        color: Some(col(TEXT)),
        ..Default::default()
    };
    let sub_style = StyleRules {
        margin_top: Some(px(4.0)),
        margin_bottom: Some(px(24.0)),
        font_size: Some(px(14.0)),
        color: Some(col(TEXT_MUTED)),
        text_align: Some(TextAlign::Center),
        ..Default::default()
    };
    let label_style = StyleRules {
        align_self: Some(AlignSelf::Stretch),
        margin_top: Some(px(12.0)),
        margin_bottom: Some(px(6.0)),
        font_size: Some(px(11.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT_MUTED)),
        text_transform: Some(TextTransform::Uppercase),
        ..Default::default()
    };
    let field_style = StyleRules {
        align_self: Some(AlignSelf::Stretch),
        height: Some(px(40.0)),
        padding_left: Some(px(12.0)),
        padding_right: Some(px(12.0)),
        background: Some(col(COMPOSER)),
        color: Some(col(TEXT)),
        font_size: Some(px(15.0)),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        ..Default::default()
    };
    let submit_style = StyleRules {
        align_self: Some(AlignSelf::Stretch),
        margin_top: Some(px(20.0)),
        height: Some(px(40.0)),
        background: Some(col(BUTTON)),
        color: Some(col(TEXT)),
        font_size: Some(px(15.0)),
        font_weight: Some(FontWeight::SemiBold),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        ..Default::default()
    };
    let footer_style = StyleRules {
        margin_top: Some(px(16.0)),
        font_size: Some(px(13.0)),
        color: Some(col(TEXT_MUTED)),
        ..Default::default()
    };

    // The shared login action: validate, then auth in. `authed` flipping is
    // what unmounts this screen and mounts the app tree (the `switch` key in
    // `app()` re-evaluates on it). Every captured signal is `Copy`.
    let do_login = {
        let authed = authed.clone();
        move || {
            let em = email.get();
            let pw = password.get();
            if em.trim().is_empty() || pw.trim().is_empty() {
                error.set("Email and password are required.".to_string());
                return;
            }
            // No backend: any non-empty email + password signs in as "you".
            // A real login would `spawn` a network call here and resolve the
            // session before flipping `authed`.
            error.set(String::new());
            authed.set(true);
        }
    };
    let on_submit_click = move || do_login();
    let on_pw_enter = move |ev: &KeyEvent| {
        if ev.key == "Enter" && !ev.shift && !ev.ctrl && !ev.alt && !ev.meta {
            do_login();
            KeyOutcome::PreventDefault
        } else {
            KeyOutcome::Default
        }
    };
    // "Continue as guest" — skips the fields entirely (the mock has no real
    // accounts, so guest == you).
    let on_guest = move || authed.set(true);

    // The form. The inline error is reactive: a nested `switch` keyed on the
    // `error` signal rebuilds the form whenever a failed submit sets it (the
    // same `switch(key, render)` pattern the sidebar/message list use —
    // `ui!` doesn't re-read signal state on its own, only on rebuild).
    let em_change = email.clone();
    let pw_change = password.clone();
    let on_email_change = move |s: String| em_change.set(s);
    let on_pw_change = move |s: String| pw_change.set(s);
    // The "Continue as guest" pill's parts. The pill `Element` is built *inside*
    // the render closure (Elements aren't `Copy` and the closure is `Fn`, so a
    // pre-built one would be moved on the first rebuild and gone the second).
    let guest_style = StyleRules {
        margin_top: Some(px(8.0)),
        padding_top: Some(px(2.0)),
        padding_bottom: Some(px(2.0)),
        font_size: Some(px(13.0)),
        font_weight: Some(FontWeight::SemiBold),
        ..Default::default()
    };
    let guest_text_style = StyleRules {
        color: Some(col(BUTTON)),
        font_size: Some(px(13.0)),
        font_weight: Some(FontWeight::SemiBold),
        ..Default::default()
    };
    let form = switch(
        move || error.get(),
        move |err: &String| {
            // The inline error, shown only while non-empty (a failed submit
            // sets it; a successful sign-in clears it and unmounts the screen).
            let err_style = StyleRules {
                margin_top: Some(px(12.0)),
                font_size: Some(px(13.0)),
                color: Some(col("#f23f43")),
                text_align: Some(TextAlign::Center),
                ..Default::default()
            };
            let err_text = err.clone();
            let err_el = if err_text.is_empty() {
                None
            } else {
                Some(ui! { text(style = err_style) { err_text } })
            };
            let guest = on_guest.clone();
            let guest_style_i = guest_style.clone();
            let guest_text_style_i = guest_text_style.clone();
            let guest_btn = ui! {
                view(style = guest_style_i) {
                    text(style = guest_text_style_i) { "Continue as guest".to_string() }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        guest();
                    }
                    TouchResponse::CONSUMED
                })
            };
            ui! {
                // `ui!` moves each style into its render closure (the
                // whiteboard idiom: clone outer styles per use inside a
                // `switch` render).
                view(style = StyleRules {
                    align_self: Some(AlignSelf::Stretch),
                    flex_direction: Some(FlexDirection::Column),
                    ..Default::default()
                }) {
                    text(style = label_style.clone()) { "Email".to_string() }
                    text_input(
                        value = email,
                        on_change = on_email_change,
                        placeholder = "you@example.com",
                        style = field_style.clone(),
                    )
                    text(style = label_style.clone()) { "Password".to_string() }
                    text_input(
                        value = password,
                        on_change = on_pw_change,
                        placeholder = "••••••••",
                        secure = true,
                        style = field_style.clone(),
                    )
                    .on_key_down(on_pw_enter)
                    // The inline error, spliced in only when present.
                    if let Some(el) = err_el {
                        el
                    }
                    button(
                        label = "Sign In".to_string(),
                        on_click = on_submit_click,
                        style = submit_style.clone(),
                    )
                    text(style = footer_style.clone()) { "No account?".to_string() }
                    guest_btn
                }
            }
        },
    );

    // Full-screen backdrop (Discord's login page: dark, centred card).
    ui! {
        view(style = StyleRules {
            width: Some(pct(100.0)),
            height: Some(pct(100.0)),
            background: Some(col(BG_RAIL)),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::Center),
            overflow: Some(Overflow::Hidden),
            ..Default::default()
        }) {
            view(style = card_style) {
                view(style = logo_style) {
                    text(style = logo_text_style) { "D".to_string() }
                }
                text(style = heading_style) { "Welcome back!".to_string() }
                text(style = sub_style) { "We're so excited to see you again!".to_string() }
                form
            }
        }
    }
}

// --- the tree ----------------------------------------------------------------

pub fn app() -> Element {
    // Mock auth gate: starts signed out (login screen). There's no backend, so
    // "auth" is just a `bool` signal — the login screen sets it true on Sign In
    // (or Continue as guest) and a Log Out in the user panel flips it back. A
    // real build would replace this with a session/token and a network login.
    let authed: Signal<bool> = signal(false);
    let authed_copy = authed; // `Signal` is `Copy`; the render closure only
                              // receives the current value, so pass the signal in.
    switch(
        move || authed.get(),
        move |authed: &bool| {
            if *authed {
                app_shell(authed_copy)
            } else {
                login_screen(authed_copy)
            }
        },
    )
}

fn app_shell(authed: Signal<bool>) -> Element {
    let active_server: Signal<u32> = signal(SERVERS[0].id);
    let active_channel: Signal<u32> = signal(SERVERS[0].first_text_channel().id);
    let composer: Signal<String> = signal(String::new());
    // The voice channel the user is currently joined to, keyed by (server_id,
    // voice_channel_id). Joining a voice row in the sidebar sets it; leaving
    // (from the profile strip) or switching servers clears it. `None` = not
    // in a voice channel. Drives the voice-row membership lists AND the
    // profile strip's voice-connection bar.
    let joined_voice: Signal<Option<(u32, u32)>> = signal(None);
    // The signed-in user's live presence status. A signal (not the `USER`
    // constant) because the profile strip can change it; the member panel
    // renders the user under this status, so a change regroups them there too.
    let user_status: Signal<Status> = signal(Status::Online);
    // (channel_id, author, body, ts_secs) of every message the user has sent
    // this session, so each channel keeps its own additions. Read at rebuild.
    let sent: Rc<RefCell<Vec<(u32, &'static str, String, i64)>>> = Rc::new(RefCell::new(Vec::new()));
    // Bumped on every send. `switch` dedups on its scrutinee VALUE (equal
    // re-fire keeps the mounted arm), so re-setting `active_channel` to its
    // current value does NOT rebuild the message list — this counter does.
    let revision: Signal<u32> = signal(0);

    // --- server rail -----------------------------------------------------
    let rail_style = StyleRules {
        width: Some(px(72.0)),
        background: Some(col(BG_RAIL)),
        flex_direction: Some(FlexDirection::Column),
        align_items: Some(AlignItems::Center),
        gap: Some(px(10.0)),
        padding_top: Some(px(12.0)),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };

    let server_rows: Vec<Element> = SERVERS.iter().map(|s| {
        let active = active_server.clone();
        let channel = active_channel.clone();
        let voice = joined_voice.clone();
        // Reactive style: re-runs whenever `active_server` changes.
        let row_style = move || StyleRules {
            width: Some(px(48.0)),
            height: Some(px(48.0)),
            // Positioning ancestor for the absolutely-positioned pill.
            position: Some(Position::Relative),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::Center),
            background: Some(col(if active.get() == s.id {
                TEXT
            } else {
                BG_SIDEBAR
            })),
            // Discord's squircle is a 16px radius on a 48px icon, not a full
            // circle.
            border_top_left_radius: Some(px(16.0)),
            border_top_right_radius: Some(px(16.0)),
            border_bottom_left_radius: Some(px(16.0)),
            border_bottom_right_radius: Some(px(16.0)),
            ..Default::default()
        };
        let label_style = move || StyleRules {
            font_size: Some(px(18.0)),
            font_weight: Some(FontWeight::Bold),
            color: Some(col(if active.get() == s.id {
                BG_RAIL
            } else {
                TEXT_CHANNEL
            })),
            ..Default::default()
        };
        ui! {
            view(style = row_style) {
                // The active-server pill: Discord's white vertical sliver on
                // the far-left edge of the rail. The row is the positioning
                // ancestor (default Static is not, so the pill would anchor
                // to the rail — we want it at the row's left edge). The row
                // is 48px centered in the 72px rail (12px offset), so
                // left: -12 puts the pill flush with the screen edge.
                view(style = move || StyleRules {
                    position: Some(Position::Absolute),
                    left: Some(px(-12.0)),
                    top: Some(px(14.0)),
                    width: Some(px(4.0)),
                    height: Some(px(20.0)),
                    background: Some(col(PILL)),
                    border_top_left_radius: Some(px(2.0)),
                    border_top_right_radius: Some(px(2.0)),
                    border_bottom_left_radius: Some(px(2.0)),
                    border_bottom_right_radius: Some(px(2.0)),
                    opacity: Some(Tokenized::Literal(if active.get() == s.id {
                        1.0
                    } else {
                        0.0
                    })),
                    ..Default::default()
                }) {}
                text(style = label_style) { s.initial }
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    active.set(s.id);
                    if let Some(srv) = SERVERS.iter().find(|x| x.id == s.id) {
                        // Default to the first *text* channel — voice channels
                        // aren't selectable in this build.
                        channel.set(srv.first_text_channel().id);
                    }
                    // Switching servers drops the voice join: you can't stay
                    // connected in a channel you're no longer looking at
                    // (Discord's per-server voice behavior, simplified).
                    voice.set(None);
                }
                TouchResponse::CONSUMED
            })
        }
    }).collect();

    // --- channel sidebar -------------------------------------------------
    let sidebar_style = StyleRules {
        width: Some(px(240.0)),
        background: Some(col(BG_SIDEBAR)),
        border_right_width: Some(border_width(1.0)),
        border_right_color: Some(col(BORDER)),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    let sidebar_header_style = StyleRules {
        padding_top: Some(px(14.0)),
        padding_bottom: Some(px(14.0)),
        padding_left: Some(px(16.0)),
        padding_right: Some(px(16.0)),
        border_bottom_width: Some(border_width(1.0)),
        border_bottom_color: Some(col(BORDER)),
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        gap: Some(px(6.0)),
        ..Default::default()
    };
    let sidebar_title_style = StyleRules {
        font_size: Some(px(16.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT)),
        ..Default::default()
    };
    let chevron_style = StyleRules {
        font_size: Some(px(11.0)),
        color: Some(col(TEXT_MUTED)),
        ..Default::default()
    };
    // Category section header: the small uppercase "TEXT CHANNELS" label.
    let cat_style = StyleRules {
        padding_top: Some(px(14.0)),
        padding_bottom: Some(px(4.0)),
        padding_left: Some(px(16.0)),
        padding_right: Some(px(16.0)),
        font_size: Some(px(11.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT_MUTED)),
        text_transform: Some(TextTransform::Uppercase),
        ..Default::default()
    };

    let sidebar = switch(
        move || (active_server.get(), active_channel.get(), joined_voice.get()),
        move |(server_id, _channel_id, _voice): &(u32, u32, Option<(u32, u32)>)| {
            // `switch`'s render is `Fn` — clone the captured styles so the
            // closure borrows them rather than moving them out (whiteboard idiom).
            let sidebar_style = sidebar_style.clone();
            let sidebar_header_style = sidebar_header_style.clone();
            let sidebar_title_style = sidebar_title_style.clone();
            let chevron_style = chevron_style.clone();
            let cat_style = cat_style.clone();
            let channel = active_channel;
            let server = SERVERS.iter().find(|x| x.id == *server_id).unwrap();
            let mut rows: Vec<Element> = Vec::new();
            // Category sections, in data order: each section is a small
            // uppercase header, its text-channel rows, then its voice rows.
            for cat in server.categories {
                let cat_label = cat.title.to_string();
                let cat_style = cat_style.clone();
                rows.push(ui! { text(style = cat_style) { cat_label } });
                for c in cat.text {
                    let active = channel;
                    // Discord's channel rows are compact (~34px) with a
                    // rounded pill (inset 8px each side) that fills with the
                    // hover colour, and the active colour when selected — the
                    // row itself stays transparent otherwise.
                    let hovered = signal(false);
                    let row_style = move || {
                        let on = active.get() == c.id;
                        StyleRules {
                            height: Some(px(34.0)),
                            margin_left: Some(px(8.0)),
                            margin_right: Some(px(8.0)),
                            padding_left: Some(px(12.0)),
                            padding_right: Some(px(8.0)),
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
                            align_items: Some(AlignItems::Center),
                            ..Default::default()
                        }
                    };
                    let label_style = move || StyleRules {
                        font_size: Some(px(15.0)),
                        font_weight: Some(if active.get() == c.id {
                            FontWeight::SemiBold
                        } else {
                            FontWeight::Normal
                        }),
                        color: Some(col(if active.get() == c.id {
                            TEXT
                        } else {
                            TEXT_CHANNEL
                        })),
                        ..Default::default()
                    };
                    let label = format!(
                        "#{}",
                        c.name
                    );
                    rows.push(ui! {
                        view(style = row_style) {
                            text(style = label_style) { label }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                active.set(c.id);
                            }
                            TouchResponse::CONSUMED
                        })
                        .on_hover(move |on| hovered.set(on))
                    });
                }
                rows.extend(voice_section(
                    cat,
                    server.id,
                    active_server.clone(),
                    joined_voice.clone(),
                    user_status.clone(),
                ));
            }
            let title = server.name.to_string();
            // The user profile is the bottom-left of the sidebar (Discord
            // convention). The member list is a separate right-side panel —
            // see `members_switch` further down — so it's not built here.
            let user = user_panel(user_status, joined_voice, authed);
            ui! {
                view(style = sidebar_style) {
                    view(style = sidebar_header_style) {
                        text(style = chevron_style) { "▾".to_string() }
                        text(style = sidebar_title_style) { title }
                    }
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
                    user
                }
            }
        },
    );

    // --- main view -------------------------------------------------------
    let header_style = StyleRules {
        padding_top: Some(px(10.0)),
        padding_bottom: Some(px(10.0)),
        padding_left: Some(px(16.0)),
        padding_right: Some(px(16.0)),
        border_bottom_width: Some(border_width(1.0)),
        border_bottom_color: Some(col(BORDER)),
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        gap: Some(px(8.0)),
        ..Default::default()
    };
    let hash_style = StyleRules { font_size: Some(px(20.0)), color: Some(col(TEXT_MUTED)), ..Default::default() };
    let title_style = StyleRules { font_size: Some(px(15.0)), font_weight: Some(FontWeight::SemiBold), color: Some(col(TEXT)), ..Default::default() };
    let topic_style = StyleRules { font_size: Some(px(13.0)), color: Some(col(TEXT_MUTED)), flex_grow: Some(grow(1.0)), ..Default::default() };

    let header = switch(
        move || (active_server.get(), active_channel.get()),
        move |(server_id, channel_id): &(u32, u32)| {
            let header_style = header_style.clone();
            let hash_style = hash_style.clone();
            let title_style = title_style.clone();
            let topic_style = topic_style.clone();
            let server = SERVERS.iter().find(|x| x.id == *server_id).unwrap();
            let channel = server.find_text_channel(*channel_id).unwrap();
            let title = channel.name.to_string();
            let topic = channel.topic.to_string();
            ui! {
                view(style = header_style) {
                    text(style = hash_style) { "#".to_string() }
                    text(style = title_style) { title }
                    text(style = topic_style) { topic }
                }
            }
        },
    );

    // Message list — the channel's seed messages plus any sent here this session.
    // `sent_read` is a dedicated clone so this closure and `on_send` don't both
    // `move`-capture the same non-`Copy` `Rc`. The key includes `revision`
    // because `switch` dedups on scrutinee VALUE — without it, a send (which
    // keeps the same channel) would not rebuild the list.
    let sent_read = sent.clone();
    let sent_messages = switch(
        move || (active_server.get(), active_channel.get(), revision.get()),
        move |(server_id, channel_id, _revision): &(u32, u32, u32)| {
            let server = SERVERS.iter().find(|x| x.id == *server_id).unwrap();
            let channel = server.find_text_channel(*channel_id).unwrap();
            let cid = *channel_id;
            let now = crate::data::now_secs();
            let mut rows: Vec<Element> = Vec::with_capacity(channel.messages.len());
            for m in channel.messages.iter() {
                let when = format_when(m.ts_secs(now), now);
                rows.push(message_row(m.author, m.color, m.body, when));
            }
            let mine: Vec<(u32, &'static str, String, i64)> = sent_read.borrow().clone();
            for (_mcid, author, body, ts) in mine.iter().filter(|(mcid, _, _, _)| *mcid == cid) {
                let when = format_when(*ts, now);
                rows.push(message_row(author, BUTTON, body.as_str(), when));
            }
            ui! {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Column),
                    ..Default::default()
                }) { rows }
            }
        },
    );

    // --- composer --------------------------------------------------------
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
        ..Default::default()
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

    let on_change = move |s: String| {
        composer.set(s);
    };
    // One send action shared by the Send button AND the Enter key on the
    // composer. Every captured signal/Rc is cloned behind an `Rc` so the
    // handler can be `Fn` (callable from both the `button on_click` and the
    // `text_input on_key_down` closures).
    let send_composer = composer.clone();
    let sent_write = sent.clone();
    let revision_bump = revision.clone();
    let do_send = move || {
        let body = send_composer.get();
        if body.trim().is_empty() {
            return;
        }
        let cid = active_channel.get();
        sent_write.borrow_mut().push((cid, "you", body, crate::data::now_secs()));
        send_composer.set(String::new());
        // Bump the revision so the message-list `switch` rebuilds (its key
        // dedups on value, so re-setting the same channel alone would not).
        revision_bump.set(revision_bump.get() + 1);
    };
    // Button click → send.
    let on_send = Rc::new(do_send);
    let send_btn = on_send.clone();
    let on_click = move || (send_btn)();
    // Enter on the composer → send. Plain Enter only; Shift/Ctrl/Alt/Meta+
    // Enter fall through to the platform default. `PreventDefault` stops the
    // input's default Enter handling (no newline / no stray submit) now that
    // we've taken the action.
    let send_key = on_send.clone();
    let on_enter = move |ev: &KeyEvent| {
        if ev.key == "Enter" && !ev.shift && !ev.ctrl && !ev.alt && !ev.meta {
            (send_key)();
            KeyOutcome::PreventDefault
        } else {
            KeyOutcome::Default
        }
    };

    // --- member list (right side) ----------------------------------------
    // Rebuilds when the active server changes (each server has its own
    // roster) AND when the signed-in user's live status changes — the user
    // regroups to their new status bucket, and the member count follows.
    // Rendered as the right-most top-level column.
    let members_switch = switch(
        move || (active_server.get(), user_status.get()),
        move |(server_id, user_status): &(u32, Status)| {
            let server = SERVERS.iter().find(|x| x.id == *server_id).unwrap();
            member_panel(server, &USER.as_member(*user_status))
        },
    );

    let main_style = StyleRules {
        flex_grow: Some(grow(1.0)),
        background: Some(col(BG_MAIN)),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };

    ui! {
        view(style = StyleRules {
            width: Some(pct(100.0)),
            height: Some(pct(100.0)),
            background: Some(col(BG_MAIN)),
            flex_direction: Some(FlexDirection::Row),
            overflow: Some(Overflow::Hidden),
            ..Default::default()
        }) {
            view(style = rail_style) { server_rows }
            sidebar
            view(style = main_style) {
                header
                scroll_view(style = StyleRules { flex_grow: Some(grow(1.0)), ..Default::default() }) { sent_messages }
                view(style = composer_style) {
                    text_input(value = composer, on_change = on_change, placeholder = "Message", style = input_style).on_key_down(on_enter)
                    button(label = "Send".to_string(), on_click = on_click, style = send_style)
                }
            }
            members_switch
        }
    }
}
