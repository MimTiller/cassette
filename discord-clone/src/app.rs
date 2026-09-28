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
use std::collections::HashMap;
use std::rc::Rc;

use runtime_core::{
    AlignItems, AlignSelf, Color, Element, FlexDirection, FontWeight, JustifyContent, KeyEvent,
    KeyOutcome, Overflow, Position, Signal, StyleRules, TextAlign, TextTransform, Tokenized,
    TouchPhase, TouchResponse,
    signal, switch, ui,
};
use runtime_core::icon;
use icons_lucide::{
    CHEVRON_DOWN, COMPASS, HASH, HEADPHONES, MIC, MIC_OFF, PLUS, SETTINGS, SMILE, VOLUME_2,
};

use runtime_shared::time::local_offset_minutes;

use crate::data::{SERVERS, Category, Member, Status, USER};
use crate::member_count_display::member_count_display;
use crate::message_search_bar::MessageSearchBar;
use crate::pin_count_badge::PinCountBadge;
use crate::profile_settings::profile_settings;
use crate::style_helpers::{col, pct, px};
use crate::typing_indicator::TypingIndicator;

// Robot test ids: stable, `'static` strings the automation robot
// (`find_element`/`tap`) matches on to locate interactive elements. The
// per-server / per-channel / per-voice ids are derived from the mock ids in
// `data.rs` — fixed at compile time — so each maps to a `&'static str` (the
// `test_id` builder requires a static string, so a runtime `format!` would
// not typecheck).
fn server_test_id(id: u32) -> &'static str {
    match id {
        1 => "server-1",
        2 => "server-2",
        3 => "server-3",
        _ => "server",
    }
}
fn channel_test_id(id: u32) -> &'static str {
    match id {
        101 => "channel-101",
        102 => "channel-102",
        103 => "channel-103",
        201 => "channel-201",
        202 => "channel-202",
        301 => "channel-301",
        _ => "channel",
    }
}
fn voice_test_id(id: u32) -> &'static str {
    match id {
        104 => "voice-104",
        105 => "voice-105",
        203 => "voice-203",
        302 => "voice-302",
        _ => "voice",
    }
}

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
/// Discord's unread badge red (the pill on channels + the rail count).
const BADGE: &str = "#f23f43";
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
        // Top-align so the avatar sits against the message head and a
        // multi-line body never stretches it into a vertical pill.
        align_items: Some(AlignItems::FlexStart),
        ..Default::default()
    };
    let avatar_style = StyleRules {
        width: Some(px(40.0)),
        height: Some(px(40.0)),
        flex_shrink: Some(grow(0.0)),
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
        flex_shrink: Some(grow(0.0)),
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

/// A voice-channel row in the sidebar: a speaker icon (`VOLUME_2`) + name,
/// with the connected members listed beneath it in smaller, indented rows.
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
        let label = v.name.clone();
        let row = ui! {
            view(style = row_style, test_id = voice_test_id(v.id)) {
                icon(VOLUME_2)
                    .color(|| Color::from(TEXT_MUTED))
                    .size(14.0)
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
    // Font slightly smaller than the group headers + a touch of negative
    // letter-spacing so "MEMBER COUNT — N" stays on one line within the panel
    // width (no white-space/nowrap property exists, so size is the lever).
    // Left padding matches the group headers below (ONLINE, IDLE, …).
    let header_style = StyleRules {
        padding_top: Some(px(14.0)),
        padding_bottom: Some(px(6.0)),
        padding_left: Some(px(12.0)),
        font_size: Some(px(10.0)),
        letter_spacing: Some(Tokenized::Literal(-0.5)),
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
///
/// The strip is two rows (the 240px sidebar can't fit the identity, the
/// Profile/Log Out pills, and the voice-action icons on one line): row 1 is
/// the avatar + name/status, row 2 is a control row — the mic-mute, deafen
/// and settings-gear icon buttons (Discord's user-panel actions) on the left,
/// the Leave button (voice only) and the Profile / Log Out pills on the right.
fn user_panel(
    user_status: Signal<Status>,
    joined_voice: Signal<Option<(u32, u32)>>,
    authed: Signal<bool>,
    profile_mode: Signal<bool>,
    muted: Signal<bool>,
    deafened: Signal<bool>,
) -> Element {
    switch(
        move || (user_status.get(), voice_name_for(joined_voice.get())),
        move |(status, voice_name): &(Status, Option<String>)| {
            let user = USER;
            let voice_name = voice_name.clone();
            // Two rows (see the fn doc): identity on top, the icon-button
            // control row beneath.
            let panel_style = StyleRules {
                padding_top: Some(px(8.0)),
                padding_bottom: Some(px(8.0)),
                padding_left: Some(px(8.0)),
                padding_right: Some(px(8.0)),
                gap: Some(px(8.0)),
                flex_direction: Some(FlexDirection::Column),
                background: Some(col(USER_BAR)),
                ..Default::default()
            };
            let row_style = StyleRules {
                min_height: Some(px(32.0)),
                gap: Some(px(8.0)),
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
                background: Some(col(user.color)),
                // Fixed 32px circle: never squeeze it when the row is tight.
                flex_shrink: Some(grow(0.0)),
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
                // Only this column absorbs row shrinkage (the avatar is
                // fixed): the name must keep ~40px so "you" renders in full
                // instead of clipping to "yo".
                min_width: Some(px(40.0)),
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
                        font_size: Some(px(10.0)),
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
                        height: Some(px(24.0)),
                        padding_left: Some(px(8.0)),
                        padding_right: Some(px(8.0)),
                        background: Some(col(BG_SIDEBAR)),
                        color: Some(col(TEXT)),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        font_size: Some(px(11.0)),
                        font_weight: Some(FontWeight::SemiBold),
                        flex_shrink: Some(grow(0.0)),
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
                        font_size: Some(px(11.0)),
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
            // Compact pill: 8px padding + 11px type so it fits the 240px
            // sidebar row next to the avatar and the Profile pill.
            let logout_style = StyleRules {
                height: Some(px(24.0)),
                padding_left: Some(px(8.0)),
                padding_right: Some(px(8.0)),
                background: Some(col(BG_SIDEBAR)),
                color: Some(col(TEXT_MUTED)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                font_size: Some(px(11.0)),
                font_weight: Some(FontWeight::SemiBold),
                // The row must shrink the name column, never the pills:
                // a squashed pill is what made "Log Out" wrap to two lines.
                flex_shrink: Some(grow(0.0)),
                border_top_left_radius: Some(px(6.0)),
                border_top_right_radius: Some(px(6.0)),
                border_bottom_left_radius: Some(px(6.0)),
                border_bottom_right_radius: Some(px(6.0)),
                ..Default::default()
            };
            let logout = authed.clone();
            let logout_text_style = StyleRules {
                color: Some(col(TEXT_MUTED)),
                font_size: Some(px(11.0)),
                font_weight: Some(FontWeight::SemiBold),
                ..Default::default()
            };
            let logout_btn = ui! {
                view(style = logout_style, test_id = "logout") {
                    text(style = logout_text_style) { "Log Out".to_string() }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        logout.set(false);
                    }
                    TouchResponse::CONSUMED
                })
            };
            // Profile Settings entry: a "Profile" pill (styled like Log Out)
            // that opens the full-screen `profile_settings` screen. `app_shell`
            // owns `profile_mode` and renders the overlay when it flips on.
            let profile_open = profile_mode.clone();
            // Compact like Log Out, so both pills fit one 240px row.
            let profile_btn = ui! {
                view(style = StyleRules {
                    height: Some(px(24.0)),
                    padding_left: Some(px(8.0)),
                    padding_right: Some(px(8.0)),
                    background: Some(col(BG_SIDEBAR)),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    font_size: Some(px(11.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    flex_shrink: Some(grow(0.0)),
                    border_top_left_radius: Some(px(6.0)),
                    border_top_right_radius: Some(px(6.0)),
                    border_bottom_left_radius: Some(px(6.0)),
                    border_bottom_right_radius: Some(px(6.0)),
                    ..Default::default()
                }, test_id = "profile") {
                    text(style = StyleRules {
                        color: Some(col(TEXT_MUTED)),
                        font_size: Some(px(11.0)),
                        font_weight: Some(FontWeight::SemiBold),
                        ..Default::default()
                    }) { "Profile".to_string() }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        profile_open.set(true);
                    }
                    TouchResponse::CONSUMED
                })
            };
            // Mic / deafen / settings icon buttons (Discord's user-panel
            // voice actions). The signals are created in `app_shell` and
            // owned here — pure local UI state, no backend call. The mic
            // swaps to a slashed glyph when muted; Lucide has no
            // headphones-off, so deafened colors the icon red instead.
            let icon_btn_style = StyleRules {
                width: Some(px(20.0)),
                height: Some(px(20.0)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                flex_shrink: Some(grow(0.0)),
                ..Default::default()
            };
            let mut muted_state = muted.clone();
            let mic_btn = ui! {
                view(style = icon_btn_style.clone(), test_id = "user-mic") {
                    if muted_state.get() {
                        icon(MIC_OFF).color(|| Color::from(TEXT_MUTED)).size(15.0)
                    } else {
                        icon(MIC).color(|| Color::from(TEXT_MUTED)).size(15.0)
                    }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        muted_state.set(!muted_state.get());
                    }
                    TouchResponse::CONSUMED
                })
            };
            let mut deafened_state = deafened.clone();
            let deafen_btn = ui! {
                view(style = icon_btn_style.clone(), test_id = "user-deafen") {
                    if deafened_state.get() {
                        icon(HEADPHONES).color(|| Color::from("#f23f43")).size(15.0)
                    } else {
                        icon(HEADPHONES).color(|| Color::from(TEXT_MUTED)).size(15.0)
                    }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        deafened_state.set(!deafened_state.get());
                    }
                    TouchResponse::CONSUMED
                })
            };
            let settings_btn = ui! {
                view(style = icon_btn_style, test_id = "user-settings") {
                    icon(SETTINGS).color(|| Color::from(TEXT_MUTED)).size(15.0)
                }
                .on_touch(|_ev| TouchResponse::CONSUMED)
            };
            // Row 2: the icon buttons left, the pills (plus Leave, voice only)
            // pushed to the right.
            let controls_row = ui! {
                view(style = row_style.clone()) {
                    mic_btn
                    deafen_btn
                    settings_btn
                    view(style = StyleRules { flex_grow: Some(grow(1.0)), ..Default::default() }) {}
                    if let Some(btn) = leave_btn {
                        btn
                    }
                    profile_btn
                    logout_btn
                }
            };
            ui! {
                view(style = panel_style) {
                    view(style = row_style) {
                        avatar
                        view(style = text_stack) {
                            text(style = name_style) { name }
                            second_line
                        }
                    }
                    controls_row
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

    // The signed-in account's numeric id (`CURRENT_USER_ID`) and the signed
    // session token (`CURRENT_TOKEN`) live in `crate::auth`. The
    // `server::configure` credential provider reads `current_token()` to
    // thread the `Authorization: Bearer` header; the id rides the realtime
    // gateway's connect URL. `0`/empty mean "not logged in".

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
            // Real login: spawn the backend `#[server]` login call and resolve
            // the session before flipping `authed`. On success `auth::login`
            // records the numeric id and the signed session token
            // process-wide; the client threads the token back as
            // `Authorization: Bearer <token>` on backend calls. On failure the
            // server's message is shown inline.
            error.set(String::new());
            runtime_core::driver::spawn_async(async move {
                match crate::auth::login(&em, &pw).await {
                    Ok(_) => {
                        authed.set(true);
                    }
                    Err(message) => {
                        error.set(message);
                    }
                }
            });
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
                view(style = guest_style_i, test_id = "login-guest") {
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
                        test_id = "login-email",
                        on_change = on_email_change,
                        placeholder = "you@example.com",
                        style = field_style.clone(),
                    )
                    text(style = label_style.clone()) { "Password".to_string() }
                    text_input(
                        value = password,
                        test_id = "login-password",
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
                        test_id = "login-submit",
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
    // Point the backend `#[server]` RPC stubs at a base URL and attach the
    // signed-in user id as the `x-user-id` header on every outgoing call.
    // Runs once per process (the `app()` entry), so no "only once" guard is
    // needed.
    crate::auth::configure_server();
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
    // Mic-mute / deafen toggles for the user panel's voice-action icon row.
    // Pure local UI state, owned here and passed into `user_panel`; the mic
    // swaps to a slashed glyph when muted, deafen colors the icon red.
    let muted: Signal<bool> = signal(false);
    let deafened: Signal<bool> = signal(false);
    // (channel_id, author, body, ts_secs) of every message the user has sent
    // this session, so each channel keeps its own additions. Read at rebuild.
    let sent: Rc<RefCell<Vec<(u32, &'static str, String, i64)>>> = Rc::new(RefCell::new(Vec::new()));
    // Bumped on every send. `switch` dedups on its scrutinee VALUE (equal
    // re-fire keeps the mounted arm), so re-setting `active_channel` to its
    // current value does NOT rebuild the message list — this counter does.
    let revision: Signal<u32> = signal(0);

    // --- live/real message state -----------------------------------------
    // Separate from the mock `sent`/`revision` above: when `active_live_channel`
    // is `Some(id)` the message list reads from here instead of the mock path.
    // channel_id -> Vec<(author_id, author_label, content, created_at_secs)>,
    // oldest-first (the gateway's History replies arrive oldest-first, and new
    // frames append in order). Mutating the Vec does NOT trigger reactivity, so
    // `live_revision` is bumped on every frame the buffer absorbs and folded
    // into the list `switch`'s key — exactly the mock `revision` idiom.
    let live_messages: Rc<RefCell<HashMap<i64, Vec<(i64, String, String, i64)>>>> =
        Rc::new(RefCell::new(HashMap::new()));
    let live_revision: Signal<u32> = signal(0);
    // Channel-category collapse state: one signal per category id, all
    // starting expanded (false). Held in an Rc<HashMap> so both the sidebar
    // `switch` key and its render loop can reach it. The key reads every
    // category's collapsed value (see below), so when the CategoryHeader
    // toggles its signal the key changes and the (non-reactive, plain-Rust)
    // row loop re-runs, re-gating each category's rows on its collapsed flag.
    // The map lives in `app_shell` scope, so state survives `switch` rebuilds
    // across server/channel/voice changes.
    let collapsed: Rc<RefCell<HashMap<u32, Signal<bool>>>> = Rc::new(RefCell::new(HashMap::new()));
    {
        let s = active_server.get();
        if let Some(server) = SERVERS.iter().find(|x| x.id == s) {
            for cat in server.categories {
                collapsed.borrow_mut().entry(cat.id).or_insert_with(|| signal(false));
            }
        }
    }
    // Live (backend) servers the signed-in user belongs to, fetched once from
    // the `#[server]` RPC after `configure_server()` has pointed the stubs at
    // a base URL. Rendered as its own section below the mock rows; see the
    // rail for why these rows are not yet interactive.
    let live_servers: Signal<Vec<crate::servers::ServerSummary>> = signal(Vec::new());
    runtime_core::driver::spawn_async(async move {
        if let Ok(servers) = crate::servers::list_servers().await {
            live_servers.set(servers);
        }
    });
    // Live-mode selection, parallel to (and independent of) the mock
    // `active_server`/`active_channel` signals. `active_live_server` is the id
    // of a *real* backend server the user tapped (None = mock path is active);
    // `live_channels` holds that server's channels, fetched on demand. Tapping
    // a mock rail row sets `active_live_server` back to None, so the two modes
    // never overlap and the mock path keeps its exact existing behaviour.
    let active_live_server: Signal<Option<i64>> = signal(None);
    let live_channels: Signal<Vec<crate::servers::ChannelSummary>> = signal(Vec::new());
    // The live channel the user tapped inside the active live server. Reset to
    // None whenever `active_live_server` changes (new live server selected, or
    // mock mode re-entered) — a channel id from a different server is stale.
    let active_live_channel: Signal<Option<i64>> = signal(None);

    // Realtime gateway: the signed session token authenticates the connect
    // (the backend verifies it). It rides the connect URL's `?args=` query —
    // a browser websocket cannot set request headers, so the token can't go on
    // an Authorization header here the way the REST calls do. Non-async hook;
    // call it once here. An empty token (guest, before a successful login)
    // simply gets no working socket, so the status label below shows
    // Closed/Error.
    let gateway = discord_server::server_settings::message_gateway(
        crate::auth::current_token(),
    );
    // Compact single-line status label for the 72px rail: "● Open" etc.
    // (the wide `messaging: {:?}` form clipped/wrapped). The label is
    // reactive: `switch` re-runs the arm whenever the status string
    // changes, so the rail tracks Connecting -> Open -> Closed/Error live.
    // Per-status color: green Open, amber Connecting, red Closed/Error —
    // inline hex like the rest of the rail's status accents.
    let gateway_status = switch(
        move || format!("{:?}", gateway.status()),
        move |status: &String| {
            let (label, color) = match status.as_str() {
                "Open" => ("Open", "#23a55a"),
                "Connecting" => ("Connecting", "#f0b232"),
                // Closed and Error share the red accent.
                _ => ("Closed", "#f23f43"),
            };
            // The status dot: a small filled `view` circle in the same 11px
            // shape as the member-list presence dots (Lucide's `circle` is an
            // outline, so a plain colored view reads better at this size).
            let dot_style = StyleRules {
                width: Some(px(11.0)),
                height: Some(px(11.0)),
                background: Some(col(color)),
                border_top_left_radius: Some(px(6.0)),
                border_top_right_radius: Some(px(6.0)),
                border_bottom_left_radius: Some(px(6.0)),
                border_bottom_right_radius: Some(px(6.0)),
                ..Default::default()
            };
            let status_style = StyleRules {
                font_size: Some(px(10.0)),
                color: Some(col(color)),
                text_align: Some(TextAlign::Center),
                ..Default::default()
            };
            ui! {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    gap: Some(px(6.0)),
                    ..Default::default()
                }) {
                    view(style = dot_style) {}
                    text(style = status_style) { label.to_string() }
                }
            }
        },
    );

    // Reactive "watch every frame" effect: `switch` re-runs its arm whenever
    // `gateway.incoming()`'s value changes, so the arm fires once per inbound
    // frame. This one runs a SIDE EFFECT (absorb the message into `live_messages`
    // and bump `live_revision`), not a render — so it returns an inert 0-height
    // `view` (the no-op-render idiom; the element itself is never seen, mounted
    // in the rail below). Only `Message` frames are buffered; `History` replies
    // surface as a run of `Message { new: false }` frames, which land here too.
    // `created_at` arrives as epoch MILLISECONDS from the server; the client's
    // `format_when`/`now_secs` work in SECONDS, so divide by 1000 on ingest.
    let live_buf = live_messages.clone();
    let live_rev = live_revision.clone();
    let gateway_incoming_effect = switch(
        move || gateway.incoming().get(),
        move |frame: &Option<discord_server::server_settings::WsServerMsg>| {
            if let Some(discord_server::server_settings::WsServerMsg::Message {
                channel_id,
                author_id,
                content,
                created_at,
                ..
            }) = frame
            {
                let label = format!("user {}", author_id);
                live_buf
                    .borrow_mut()
                    .entry(*channel_id)
                    .or_default()
                    .push((*author_id, label, content.clone(), *created_at / 1000));
                live_rev.set(live_rev.get() + 1);
            }
            ui! {
                view(style = StyleRules {
                    height: Some(px(0.0)),
                    ..Default::default()
                }) {}
            }
        },
    );

    // Profile Settings screen: a "Profile" entry in the user panel opens it as
    // a full-screen overlay (built once, shown/hidden by this signal — the same
    // toggle idiom as the DM rail). No re-mount, no rebuild of the underlying tree.
    let profile_mode: Signal<bool> = signal(false);

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
        let live_server = active_live_server.clone();
        let live_channel = active_live_channel.clone();
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
            view(style = row_style, test_id = server_test_id(s.id)) {
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
                // The badge: a red numbered pill for @mentions, a small
                // plain white dot for unread (no mention), nothing for read.
                // The row is the positioning ancestor (`position: Relative`
                // above); the mock totals are computed once at build — swap
                // for a gateway-fed signal later.
                if s.total_mentions() > 0 {
                    view(style = move || StyleRules {
                        position: Some(Position::Absolute),
                        right: Some(px(-4.0)),
                        bottom: Some(px(-4.0)),
                        min_width: Some(px(18.0)),
                        height: Some(px(18.0)),
                        background: Some(col(BADGE)),
                        border_top_left_radius: Some(px(9.0)),
                        border_top_right_radius: Some(px(9.0)),
                        border_bottom_left_radius: Some(px(9.0)),
                        border_bottom_right_radius: Some(px(9.0)),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        padding_left: Some(px(4.0)),
                        padding_right: Some(px(4.0)),
                        ..Default::default()
                    }) {
                        text(style = move || StyleRules {
                            font_size: Some(px(12.0)),
                            font_weight: Some(FontWeight::Bold),
                            color: Some(col(TEXT)),
                            ..Default::default()
                        }) { s.total_mentions().to_string() }
                    }
                } else if s.total_unread() > 0 {
                    // Plain dot: unread but no mention — Discord shows a
                    // small white indicator instead of the red badge.
                    view(style = StyleRules {
                        position: Some(Position::Absolute),
                        right: Some(px(-4.0)),
                        bottom: Some(px(-4.0)),
                        width: Some(px(10.0)),
                        height: Some(px(10.0)),
                        background: Some(col(TEXT)),
                        border_top_left_radius: Some(px(5.0)),
                        border_top_right_radius: Some(px(5.0)),
                        border_bottom_left_radius: Some(px(5.0)),
                        border_bottom_right_radius: Some(px(5.0)),
                        ..Default::default()
                    }) {}
                }
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    // Selecting a mock server exits live mode so the channel
                    // sidebar shows the mock channels, not the live ones.
                    live_server.set(None);
                    live_channel.set(None);
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

    // Live (backend) servers — a visually separate section below the mock
    // rows. Clicking a row enters live mode: it sets `active_live_server`
    // (so the channel sidebar swaps to the live branch) and fetches that
    // server's real channels into `live_channels`. The active row is
    // highlighted reactively (its key is `live_servers`, and the render
    // closure also reads `active_live_server` reactively). Tapping a mock
    // rail row clears `active_live_server`, exiting live mode.
    let live_section = switch(
        move || live_servers.get(),
        move |servers: &Vec<crate::servers::ServerSummary>| {
            let divider_style = StyleRules {
                width: Some(px(32.0)),
                height: Some(px(1.0)),
                background: Some(col(BORDER)),
                ..Default::default()
            };
            // Uppercase "LIVE" caption under the thin divider: 10px, muted,
            // centered with a touch of letter spacing (the old "(live)" text
            // looked unstyled and the lowercase parens wasted rail width).
            let label_style = StyleRules {
                font_size: Some(px(10.0)),
                color: Some(col(TEXT_MUTED)),
                letter_spacing: Some(Tokenized::Literal(0.5)),
                text_align: Some(TextAlign::Center),
                text_transform: Some(TextTransform::Uppercase),
                ..Default::default()
            };
            let row_style = StyleRules {
                width: Some(px(48.0)),
                height: Some(px(48.0)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                background: Some(col(BG_SIDEBAR)),
                border_top_left_radius: Some(px(16.0)),
                border_top_right_radius: Some(px(16.0)),
                border_bottom_left_radius: Some(px(16.0)),
                border_bottom_right_radius: Some(px(16.0)),
                ..Default::default()
            };
            let initial_style = StyleRules {
                font_size: Some(px(18.0)),
                font_weight: Some(FontWeight::Bold),
                color: Some(col(TEXT_CHANNEL)),
                ..Default::default()
            };
            let rows: Vec<Element> = servers.iter().map(|s| {
                let initial = s.name.chars().next().unwrap_or('?').to_string();
                let sid = s.id;
                let active_live = active_live_server.clone();
                let live_ch = live_channels.clone();
                let live_ch_sel = active_live_channel.clone();
                // Static base for the squircle; the reactive closure below only
                // overrides the background so the tapped row lights up without
                // a full rebuild.
                let base = row_style.clone();
                let reactive_style = move || StyleRules {
                    background: Some(col(if active_live.get() == Some(sid) {
                        TEXT
                    } else {
                        BG_SIDEBAR
                    })),
                    ..base.clone()
                };
                ui! {
                    view(style = reactive_style) {
                        text(style = initial_style.clone()) { initial }
                    }
                    // Enter live mode: select this real server and pull its
                    // channels (async `#[server]` RPC, same pattern as the
                    // `live_servers` fetch above). Any previously selected live
                    // channel is stale now (it belonged to a different server),
                    // so reset it alongside `active_live_server`.
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            active_live.set(Some(sid));
                            live_ch_sel.set(None);
                            runtime_core::driver::spawn_async(async move {
                                if let Ok(channels) =
                                    crate::servers::list_channels(sid).await
                                {
                                    live_ch.set(channels);
                                }
                            });
                        }
                        TouchResponse::CONSUMED
                    })
                }
            }).collect();
            ui! {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Column),
                    align_items: Some(AlignItems::Center),
                    gap: Some(px(10.0)),
                    ..Default::default()
                }) {
                    view(style = divider_style) {}
                    text(style = label_style) { "live".to_string() }
                    rows
                }
            }
        },
    );

    // Bottom rail actions: Discord's Add-a-Server and Explore circles,
    // rendered below the live-servers section. Both are no-op stubs for now.
    let rail_actions = {
        let circle_style = StyleRules {
            width: Some(px(48.0)),
            height: Some(px(48.0)),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::Center),
            background: Some(col(BG_SIDEBAR)),
            border_top_left_radius: Some(px(24.0)),
            border_top_right_radius: Some(px(24.0)),
            border_bottom_left_radius: Some(px(24.0)),
            border_bottom_right_radius: Some(px(24.0)),
            ..Default::default()
        };
        ui! {
            view(style = circle_style.clone()) {
                icon(PLUS).color(|| Color::from("#23a55a")).size(20.0)
            }
            .on_touch(|_ev| TouchResponse::CONSUMED)
            // TODO: create-server flow
            view(style = circle_style) {
                icon(COMPASS).color(|| Color::from(TEXT_MUTED)).size(20.0)
            }
            .on_touch(|_ev| TouchResponse::CONSUMED)
            // TODO: explore flow
        }
    };

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

    // Profile panel toggle, cloned so the `move` switch closure below can
    // own this copy while the overlay (built later) keeps the original.
    let user_profile_mode = profile_mode.clone();
    let sidebar = switch(
        move || {
            // Fingerprint (id, name, kind) so the key is `PartialEq` even
            // though `ChannelSummary` itself isn't; it changes when a live
            // server's channels are fetched, forcing the live branch below to
            // rebuild and show them.
            let live_fp: Vec<(i64, String, String)> = live_channels
                .get()
                .iter()
                .map(|c| (c.id, c.name.clone(), c.kind.clone()))
                .collect();
            (
                active_server.get(),
                active_channel.get(),
                joined_voice.get(),
                active_live_server.get(),
                live_fp,
            )
        },
        move |(server_id, _channel_id, _voice, _live_server, _live_fp): &(
            u32,
            u32,
            Option<(u32, u32)>,
            Option<i64>,
            Vec<(i64, String, String)>,
        )| {
            // `switch`'s render is `Fn` — clone the captured styles so the
            // closure borrows them rather than moving them out (whiteboard idiom).
            let sidebar_style = sidebar_style.clone();
            let sidebar_header_style = sidebar_header_style.clone();
            let sidebar_title_style = sidebar_title_style.clone();
            let chevron_style = chevron_style.clone();
            let cat_style = cat_style.clone();
            let channel = active_channel;

            // Live (backend) mode: build the live channel rows up front
            // (harmless when not in live mode — `live_channels` is empty).
            let live = live_channels.get();
                // Static row style mirroring the mock channel rows' geometry
                // (34px pill, inset 8px) so live rows line up with the mock
                // ones. `row_style` is a `Fn` closure, so it can be captured
                // by the per-row `ui!` below without being moved.
                let live_row_style = move || StyleRules {
                    height: Some(px(34.0)),
                    margin_left: Some(px(8.0)),
                    margin_right: Some(px(8.0)),
                    padding_left: Some(px(12.0)),
                    padding_right: Some(px(8.0)),
                    background: Some(col("transparent")),
                    border_top_left_radius: Some(px(8.0)),
                    border_top_right_radius: Some(px(8.0)),
                    border_bottom_left_radius: Some(px(8.0)),
                    border_bottom_right_radius: Some(px(8.0)),
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    ..Default::default()
                };
                let live_glyph_style = move || StyleRules {
                    width: Some(px(20.0)),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    flex_shrink: Some(grow(0.0)),
                    font_size: Some(px(15.0)),
                    color: Some(col(TEXT_MUTED)),
                    ..Default::default()
                };
                let live_label_style = move || StyleRules {
                    flex_grow: Some(grow(1.0)),
                    flex_shrink: Some(grow(0.0)),
                    font_size: Some(px(15.0)),
                    color: Some(col(TEXT_CHANNEL)),
                    ..Default::default()
                };
                if _live_server.is_some() {
                    // Build the live channel rows here, inside the live arm, so
                    // they're a local of this block. Splatting a captured outer
                    // `Vec<Element>` out of the inner `ui!` closure would be a
                    // move out of an `Fn` closure (`Element` is not `Clone`).
                    let live_sel = active_live_channel.clone();
                    let gw = gateway.clone();
                    let live_rows: Vec<Element> = live
                        .iter()
                        .map(|c| {
                            // '#' for text channels, a speaker icon for voice —
                            // the same icons the mock rows use below.
                            let name = c.name.clone();
                            let is_text = c.kind == "text";
                            let cid = c.id;
                            // Reactive, per-row — the mock path's rows work the
                            // same way: background fills with the active colour
                            // when selected, and the glyph/label go semi-bold
                            // (label also brightens) — nothing changes when no
                            // live channel is active, so the unselected rows
                            // render exactly as before.
                            let row_style = move || StyleRules {
                                background: Some(col(if live_sel.get() == Some(cid) {
                                    BG_ACTIVE
                                } else {
                                    "transparent"
                                })),
                                ..live_row_style()
                            };
                            let glyph_style = move || StyleRules {
                                font_weight: Some(if live_sel.get() == Some(cid) {
                                    FontWeight::SemiBold
                                } else {
                                    FontWeight::Normal
                                }),
                                ..live_glyph_style()
                            };
                            let label_style = move || StyleRules {
                                font_weight: Some(if live_sel.get() == Some(cid) {
                                    FontWeight::SemiBold
                                } else {
                                    FontWeight::Normal
                                }),
                                color: Some(col(if live_sel.get() == Some(cid) {
                                    TEXT
                                } else {
                                    TEXT_CHANNEL
                                })),
                                ..live_label_style()
                            };
                            let row = if is_text {
                                // Voice rows stay non-clickable for now — a later
                                // task handles voice selection. Text rows select
                                // the channel and request its history over the
                                // gateway (the echo/History frames land in
                                // `gateway.incoming()`). `.on_touch` chains on the
                                // `view` node inside `ui!`, so each row kind gets
                                // its own `ui!` block.
                                ui! {
                                    view(style = row_style) {
                                        icon(HASH)
                                            .color(|| Color::from(TEXT_MUTED))
                                            .size(14.0)
                                        text(style = label_style) { name }
                                    }
                                    .on_touch(move |ev| {
                                        if ev.phase == TouchPhase::Ended {
                                            live_sel.set(Some(cid));
                                            gw.send(
                                                discord_server::server_settings::WsClientMsg::History {
                                                    channel_id: cid,
                                                },
                                            );
                                        }
                                        TouchResponse::CONSUMED
                                    })
                                }
                            } else {
                                ui! {
                                    view(style = row_style) {
                                        icon(VOLUME_2)
                                            .color(|| Color::from(TEXT_MUTED))
                                            .size(14.0)
                                        text(style = label_style) { name }
                                    }
                                }
                            };
                            row
                        })
                        .collect();
                    // Resolve the channels content as a local `Element` *outside*
                    // the `ui!` below. An `if/else` written directly inside `ui!`
                    // wraps its branches in a reactivity closure that captures
                    // `live_rows`, and that closure is `Fn` (re-runs) so it can't
                    // move the captured `Vec<Element>` into `view { live_rows }`
                    // (`Element` is not `Clone`). The mock branch avoids this by
                    // splatting `rows` directly, never in a conditional.
                    let live_content: Element = if live_rows.is_empty() {
                        ui! {
                            text(style = cat_style.clone()) {
                                "No channels yet".to_string()
                            }
                        }
                    } else {
                        ui! {
                            view { live_rows }
                        }
                    };
                    ui! {
                        view(style = sidebar_style.clone()) {
                            view(style = sidebar_header_style.clone()) {
                                view(style = sidebar_title_style.clone()) {
                                    text(style = cat_style.clone()) {
                                        "SERVER".to_string()
                                    }
                                    text(style = chevron_style.clone()) {
                                        "⌃".to_string()
                                    }
                                }
                            }
                            live_content
                        }
                    }
                } else {
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
                            // Positioning ancestor for the absolutely-positioned
                            // unread pill (default Static would anchor it to the
                            // sidebar instead of the row).
                            position: Some(Position::Relative),
                            flex_direction: Some(FlexDirection::Row),
                            align_items: Some(AlignItems::Center),
                            ..Default::default()
                        }
                    };
                    let label_style = move || StyleRules {
                        flex_grow: Some(grow(1.0)),
                        flex_shrink: Some(grow(0.0)),
                        font_size: Some(px(15.0)),
                        font_weight: Some(if active.get() == c.id {
                            FontWeight::SemiBold
                        } else {
                            FontWeight::Normal
                        }),
                        color: Some(col(if active.get() == c.id || c.unread > 0 {
                            TEXT
                        } else {
                            TEXT_CHANNEL
                        })),
                        ..Default::default()
                    };
                    let name = c.name.to_string();
                    rows.push(ui! {
                        view(style = row_style, test_id = channel_test_id(c.id)) {
                            icon(HASH)
                                .color(|| Color::from(TEXT_MUTED))
                                .size(14.0)
                            text(style = label_style) { name }
                            // The unread pill on the row's right edge, vertically
                            // centered in the 34px row. Mock count per channel;
                            // the real value comes from the gateway later.
                            if c.mentions > 0 {
                                view(style = StyleRules {
                                    position: Some(Position::Absolute),
                                    top: Some(px(9.0)),
                                    right: Some(px(8.0)),
                                    min_width: Some(px(16.0)),
                                    height: Some(px(16.0)),
                                    background: Some(col(BADGE)),
                                    border_top_left_radius: Some(px(8.0)),
                                    border_top_right_radius: Some(px(8.0)),
                                    border_bottom_left_radius: Some(px(8.0)),
                                    border_bottom_right_radius: Some(px(8.0)),
                                    align_items: Some(AlignItems::Center),
                                    justify_content: Some(JustifyContent::Center),
                                    padding_left: Some(px(4.0)),
                                    padding_right: Some(px(4.0)),
                                    ..Default::default()
                                }) {
                                    text(style = StyleRules {
                                        font_size: Some(px(11.0)),
                                        font_weight: Some(FontWeight::Bold),
                                        color: Some(col(TEXT)),
                                        ..Default::default()
                                    }) { c.mentions.to_string() }
                                }
                            }
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
            let user = user_panel(user_status, joined_voice, authed, user_profile_mode, muted, deafened);
            ui! {
                view(style = sidebar_style) {
                    view(style = sidebar_header_style) {
                        icon(CHEVRON_DOWN)
                            .color(|| Color::from(TEXT_MUTED))
                            .size(11.0)
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
    let topic_style = StyleRules { font_size: Some(px(13.0)), color: Some(col(TEXT_MUTED)), max_width: Some(px(220.0)), height: Some(px(18.0)), overflow: Some(Overflow::Hidden), ..Default::default() };

    // Live-mode header: a live channel's name has no mock counterpart, so it's
    // looked up from `live_channels` (fetched when the live server was
    // selected) and the topic is the static "No topic set yet" (backend
    // channels have no topic column). `live_channels` is a `Copy` `Signal`, so
    // capturing it here is cheap; the arm re-runs whenever
    // `active_live_channel` (in the key) changes. `None` falls through to the
    // mock header below, byte-for-byte unchanged.
    let header = switch(
        move || (active_server.get(), active_channel.get(), active_live_channel.get()),
        move |(server_id, channel_id, live_cid): &(u32, u32, Option<i64>)| {
            let header_style = header_style.clone();
            let hash_style = hash_style.clone();
            let title_style = title_style.clone();
            let topic_style = topic_style.clone();
            if let Some(live_id) = *live_cid {
                let title = live_channels
                    .get()
                    .iter()
                    .find(|c| c.id == live_id)
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                let topic = "No topic set yet".to_string();
                return ui! {
                    view(style = header_style) {
                        text(style = hash_style) { "#".to_string() }
                        text(style = title_style) { title }
                        text(style = topic_style) { topic }
                    }
                };
            }
            let server = SERVERS.iter().find(|x| x.id == *server_id).unwrap();
            let channel = server.find_text_channel(*channel_id).unwrap();
            let title = channel.name.to_string();
            let topic = channel.topic.to_string();
            // Roster plus the signed-in user, who counts as a member like
            // anyone else (same idiom as the member panel header).
            let total = (server.members.len() + 1) as i64;
            ui! {
                view(style = header_style) {
                    text(style = hash_style) { "#".to_string() }
                    text(style = title_style) { title }
                    text(style = topic_style) { topic }
                    // Spacer: eats the row's remaining width so the member
                    // count, pin badge, and search bar sit at the far right.
                    view(style = StyleRules { flex_grow: Some(grow(1.0)), ..Default::default() }) {}
                    member_count_display(12.into(), total.into())
                    PinCountBadge()
                    // Compact message-search widget. It can render mock result
                    // rows below its input, so wrap it in a fixed-width `view`
                    // to keep the header row from stretching taller/wider.
                    view(style = StyleRules {
                        width: Some(px(180.0)),
                        ..Default::default()
                    }) {
                        MessageSearchBar()
                    }
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
    let live_read = live_messages.clone();
    let sent_messages = switch(
        move || (
            active_server.get(),
            active_channel.get(),
            revision.get(),
            active_live_channel.get(),
            live_revision.get(),
        ),
        move |(server_id, channel_id, _revision, live_cid, _live_rev): &(u32, u32, u32, Option<i64>, u32)| {
            // Live/real channel active: render the gateway-fed buffer instead of
            // the mock path. `None` falls through to the mock render unchanged.
            if let Some(live_id) = *live_cid {
                let now = crate::data::now_secs();
                let mine: Vec<(i64, String, String, i64)> = live_read
                    .borrow()
                    .get(&live_id)
                    .cloned()
                    .unwrap_or_default();
                if mine.is_empty() {
                    // No history and nothing received yet: a centered, muted
                    // hint instead of a blank pane. The wrapper `flex_grow`s to
                    // fill the (tall) scroll pane, then centers the single line.
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
                            }) { "No messages yet -- say hi!".to_string() }
                        }
                    };
                }
                let mut rows: Vec<Element> = Vec::with_capacity(mine.len());
                for (_aid, author, body, ts) in mine.iter() {
                    let when = format_when(*ts, now);
                    rows.push(message_row(author.as_str(), BUTTON, body.as_str(), when));
                }
                return ui! {
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Column),
                        ..Default::default()
                    }) { rows }
                };
            }
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

    // Typing indicator row — sits directly above the composer. Mocked to fire
    // only in #general (server 1, channel 101), where it reads "dyno is
    // typing...". Other channels render an empty row with the exact same
    // line-box height as the indicator (identical paddings + 12px font) so the
    // composer's y-offset never jumps on channel switch. Keyed on
    // (server, channel) exactly like the header / message-list switches.
    let typing_row = switch(
        move || (active_server.get(), active_channel.get()),
        move |(server_id, channel_id): &(u32, u32)| {
            if *server_id == 1 && *channel_id == 101 {
                // `TypingIndicator`'s own root view already carries the row
                // paddings (4/4/16/16) + a 12px line, so it is mounted bare —
                // no wrapping view (that would double the padding). A `ui!`
                // block with a single component child returns that child's
                // `Element` directly.
                ui! { TypingIndicator(name = "dyno".to_string()) }
            } else {
                // Empty spacer matching the indicator's exact line box: same
                // paddings + a 12px (empty) text node, so the composer's
                // y-offset never jumps when leaving #general.
                ui! {
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        padding_top: Some(px(4.0)),
                        padding_bottom: Some(px(4.0)),
                        padding_left: Some(px(16.0)),
                        padding_right: Some(px(16.0)),
                        ..Default::default()
                    }) {
                        text(style = StyleRules {
                            font_size: Some(px(12.0)),
                            ..Default::default()
                        }) { "".to_string() }
                    }
                }
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
    // Circular muted-grey icon buttons flanking the composer input —
    // the '+' attachment affordance on the left, the emoji picker on
    // the right (both no-ops for now).
    let icon_button_style = StyleRules {
        width: Some(px(28.0)),
        height: Some(px(28.0)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        background: Some(col(COMPOSER)),
        border_top_left_radius: Some(px(14.0)),
        border_top_right_radius: Some(px(14.0)),
        border_bottom_left_radius: Some(px(14.0)),
        border_bottom_right_radius: Some(px(14.0)),
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
    // Live-send handles: `gateway` is a `Copy` `UseSocket`, so clone it in so
    // the (shared) `move` handler can send over the global socket. Only used
    // when `active_live_channel` is `Some`; otherwise the mock path runs.
    let live_chan = active_live_channel.clone();
    let gateway_send = gateway;
    let do_send = move || {
        let body = send_composer.get();
        if body.trim().is_empty() {
            return;
        }
        // Live/real mode: send over the gateway socket to the live channel.
        // The server echoes it back as a `Message` frame, which the incoming
        // effect buffers and bumps `live_revision`, rebuilding the list.
        if let Some(live_cid) = live_chan.get() {
            gateway_send.send(discord_server::server_settings::WsClientMsg::Send {
                channel_id: live_cid,
                content: body,
            });
            send_composer.set(String::new());
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

    // Profile Settings screen (built once; its fields are bound to local
    // component state). Rendered as a full-screen overlay on top of the rest
    // of the app: when `profile_mode` is off it collapses to zero size, when
    // on it fills the whole window. A "Done" pill in the header flips
    // `profile_mode` back off to close it.
    let profile_el = profile_settings();
    let profile_close = profile_mode.clone();
    let profile_header = ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::SpaceBetween),
            padding_top: Some(px(16.0)),
            padding_bottom: Some(px(16.0)),
            padding_left: Some(px(16.0)),
            padding_right: Some(px(16.0)),
            ..Default::default()
        }) {
            text(style = StyleRules {
                font_size: Some(px(18.0)),
                font_weight: Some(FontWeight::Bold),
                color: Some(col(TEXT)),
                ..Default::default()
            }) { "Profile".to_string() }
            view(style = StyleRules {
                height: Some(px(26.0)),
                padding_left: Some(px(12.0)),
                padding_right: Some(px(12.0)),
                background: Some(col(BG_SIDEBAR)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                border_top_left_radius: Some(px(6.0)),
                border_top_right_radius: Some(px(6.0)),
                border_bottom_left_radius: Some(px(6.0)),
                border_bottom_right_radius: Some(px(6.0)),
                ..Default::default()
            }) {
                text(style = StyleRules {
                    color: Some(col(TEXT)),
                    font_size: Some(px(12.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    ..Default::default()
                }) { "Done".to_string() }
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    profile_close.set(false);
                }
                TouchResponse::CONSUMED
            })
        }
    };
    let profile_open_chk = profile_mode.clone();
    let profile_wrap = move || StyleRules {
        position: Some(Position::Absolute),
        left: Some(px(0.0)),
        top: Some(px(0.0)),
        width: Some(if profile_open_chk.get() { pct(100.0) } else { px(0.0) }),
        height: Some(if profile_open_chk.get() { pct(100.0) } else { px(0.0) }),
        background: Some(col(BG_MAIN)),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    let profile_region = ui! {
        view(style = profile_wrap) {
            profile_header
            view(style = StyleRules { flex_grow: Some(grow(1.0)), overflow: Some(Overflow::Hidden), ..Default::default() }) { profile_el }
        }
    };

    ui! {
        view(style = StyleRules {
            width: Some(pct(100.0)),
            height: Some(pct(100.0)),
            background: Some(col(BG_MAIN)),
            flex_direction: Some(FlexDirection::Row),
            // Positioned ancestor for the full-screen Profile overlay, so
            // `Position::Absolute` children anchor to the whole window.
            position: Some(Position::Relative),
            overflow: Some(Overflow::Hidden),
            ..Default::default()
        }) {
            view(style = rail_style) { server_rows live_section rail_actions gateway_status gateway_incoming_effect }
            sidebar
            view(style = main_style) {
                header
                scroll_view(style = StyleRules { flex_grow: Some(grow(1.0)), ..Default::default() }) { sent_messages }
                typing_row
                view(style = composer_style) {
                    view(style = icon_button_style.clone()) {
                        icon(PLUS).color(|| Color::from(TEXT_MUTED)).size(16.0)
                    }
                    .on_touch(|_ev| TouchResponse::CONSUMED)
                    // TODO: attachment picker
                    text_input(value = composer, on_change = on_change, placeholder = "Message", style = input_style, test_id = "composer").on_key_down(on_enter)
                    view(style = icon_button_style) {
                        icon(SMILE).color(|| Color::from(TEXT_MUTED)).size(16.0)
                    }
                    .on_touch(|_ev| TouchResponse::CONSUMED)
                    // TODO: emoji picker
                    button(label = "Send".to_string(), on_click = on_click, style = send_style, test_id = "send")
                }
            }
            members_switch
            // Painted last so it sits on top of everything when open.
            profile_region
        }
    }
}
