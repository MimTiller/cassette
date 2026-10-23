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

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use runtime_core::{
    after_animation_frame, after_ms_detached, AlignItems, AlignSelf, Color, Cursor, Element,
    FlexDirection, FlexWrap, FontFamily, FontWeight, JustifyContent, KeyEvent, KeyOutcome, Overflow,
    PanEvent, PanRecognizer, Position, Ref, ReadSignal, ScrollViewHandle, Signal, StyleRules,
    TextAlign, TextTransform, Tokenized, TouchEvent, TouchPhase, TouchResponse, ViewHandle, memo,
    pan, signal, switch, ui, viewport_size, ObjectFit,
};
use runtime_core::icon;
use runtime_core::primitives::overlay::BackdropMode;
use runtime_core::primitives::portal::{AnchorTarget, ElementAlign, ElementSide};
// The file-picker SDK (also a `profile_settings` dep): the composer's attach
// button opens the platform's native multi-image picker and stages each
// picked file through `stage_attachment` (the same encode as avatars).
use file_picker::{FilePicker, PickRequest, PickOutcome, MediaKind};
use icons_lucide::{
    CHEVRON_DOWN, CHEVRON_UP, CIRCLE_HELP, COMPASS, GAVEL, GLOBE, HASH, HEADPHONES, MAIL,
    MESSAGE_CIRCLE, MIC, MIC_OFF, PIN, PLUS, SETTINGS, SMILE, SMILE_PLUS,
    USER_PLUS, USERS, VOLUME_2,
};
use crate::idle_timer_badge::idle_timer_badge;
use crate::server_settings_panel::ServerSettingsPanel;

use runtime_shared::time::local_offset_minutes;

use crate::data::{DMS, SERVERS, Category, Member, Server, Status, TextChannel, USER, member_profile};
use crate::message_search_bar::MessageSearchBar;
use crate::pin_count_badge::PinCountBadge;
use crate::ban_list_row::BanListRow;
use crate::timeout_duration_picker::TimeoutDurationPicker;
use crate::member_role_list::MemberRoleList;
use crate::verified_badge::VerifiedBadge;
use crate::icon_change_confirm::IconChangeConfirm;
use crate::ban_reason_input::BanReasonInput;
use crate::delete_account_warning::DeleteAccountWarning;

use crate::user_bio_block::user_bio_block;
use crate::audit_log_row::AuditLogRow;
use crate::blocked_user_row::BlockedUserRow;
use crate::server_region_selector::ServerRegionSelector;
use crate::server_icon_upload_placeholder::ServerIconUploadPlaceholder;
use crate::category_header::CategoryHeader;
use crate::mic_selector::MicSelector;

use crate::link_embed_preview::LinkEmbedPreview;
use crate::code_block::code_block;
use crate::screen_share_indicator::ScreenShareIndicator;
use crate::two_factor_toggle::TwoFactorToggle;

use crate::send_failed_indicator::SendFailedIndicator;
use crate::slowmode_indicator::SlowmodeIndicator;
use crate::latency_badge::LatencyBadge;
use crate::jump_to_bottom_button::JumpToBottomButton;
use crate::load_earlier_button::load_earlier_button;
use crate::connection_quality_indicator::ConnectionQualityIndicator;
use crate::voice_join_button::VoiceJoinButton;
use crate::volume_slider::VolumeSlider;
use crate::soundboard_button::SoundboardButton;
use crate::edited_label::edited_label;
use crate::settings_suite::UserSettings;
use crate::keyboard_shortcuts_overlay::keyboard_shortcuts_overlay;
use crate::profile_settings::profile_settings;
use crate::dm_view::dm_screen;
use crate::style_helpers::{borderless_input, col, pct, px, section_label};
use crate::join_modal::JoinServerModal;
use crate::create_server_modal::create_server_modal;
use crate::create_channel_modal::CreateChannelModal;
use crate::rename_channel_modal::RenameChannelModal;
use crate::delete_channel_modal::DeleteChannelModal;
use crate::discover_servers_modal::discover_servers_modal;
use crate::typing_indicator::TypingIndicator;
use crate::invite_link_display::InviteLinkDisplay;
use crate::invite_expiry_selector::InviteExpirySelector;
use crate::thread_list_row::ThreadListRow;
use crate::slash_command_dropdown::SlashCommandDropdown;
use crate::emoji_picker::EmojiPicker;
use crate::settings_tab_bar::SettingsTabBar;
use crate::setup_checklist_item::SetupChecklistDemo;
use crate::emoji_upload_slot::EmojiUploadSlot;
use crate::rules_accept_checkbox::rules_accept_checkbox;
use crate::nickname_edit_field::NicknameEditField;
use crate::custom_status_input::CustomStatusInput;
use crate::notification_permission_prompt::NotificationPermissionPrompt;
use crate::channel_topic_banner::ChannelTopicBanner;
use crate::char_counter::CharCounter;
use crate::kick_confirm_dialog::KickConfirmDialog;
use crate::delete_message_confirm::DeleteMessageConfirm;
use crate::transfer_ownership_confirm::TransferOwnershipConfirm;
use crate::invite_modal::InviteModal;

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
/// Discord's elevated surface — one step above `BG_MAIN`, for the channel
/// anchor chip and other small raised elements that should sit visually above
/// the message pane background.
const BG_ELEVATED: &str = "#383a40";
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
/// Reaction-pill palette — kept identical to `components/reaction_summary.rs`
/// (the unwired single-example pill this feature generalizes) so the live
/// pills reuse that exact visual language: a blurple border + tinted fill
/// when the current user reacted, a muted-gray border on a neutral fill
/// otherwise.
const REACTED_BG: &str = "#363b63";
const REACTED_BORDER: &str = "#5865f2";
const PLAIN_BG: &str = "#313338";
const PLAIN_BORDER: &str = "#4e5058";
/// The user-profile section at the bottom of the sidebar sits a shade darker
/// than the channel list above it.
const USER_BAR: &str = "#232428";
/// Discord's message-grouping window: consecutive messages from the same
/// author closer together than this (seconds) collapse into one group —
/// avatar + name + timestamp on the first only, tightly-spaced body-only
/// rows after. Discord's own window is ~7 minutes (420s).
const GROUP_WINDOW_SECS: i64 = 420;

/// "Does `cur` continue `prev`'s message group?" — same author and the gap
/// between the two instants inside `GROUP_WINDOW_SECS`. Used by the mock
/// channel, live channel, and both DM render paths (the list is always
/// oldest-first, so "previous" is the row rendered just above).
pub(crate) fn same_group(prev_author: &str, prev_ts: i64, cur_author: &str, cur_ts: i64) -> bool {
    prev_author == cur_author && cur_ts >= prev_ts && cur_ts - prev_ts < GROUP_WINDOW_SECS
}
/// One staged composer attachment: a unique staging key (for the thumbnail
/// strip's `switch` so an 'x' removes exactly that entry), the file name
/// (shown in the mock fallback line + carried to the wire frame), the MIME
/// type, and the encoded `data:image/…;base64,` URL.
type PendingImage = (i64, String, String, String);

/// Validate one image and stage it in the composer's pending list: the
/// shared client-side limit gate (max 4 per message, and the type/size
/// checks via `crate::profile_settings::validate_and_encode_attachment`,
/// the same encode the avatar upload uses). Rejections land visibly in the
/// `err` signal; accepted images append to `pending` with a fresh key from
/// `next_key`. Reached by BOTH the file-picker callback and the robot
/// `attach.stage` hook, so an E2E test stages exactly what the picker would.
pub(crate) fn stage_attachment(
    pending: Signal<Vec<PendingImage>>,
    next_key: Signal<i64>,
    err: Signal<String>,
    filename: String,
    mime: String,
    bytes: &[u8],
) {
    if pending.get().len() >= crate::profile_settings::ATTACHMENT_MAX_COUNT {
        err.set("Too many images (max 4 per message)".to_string());
        return;
    }
    match crate::profile_settings::validate_and_encode_attachment(mime.as_str(), bytes) {
        Ok(url) => {
            let key = next_key.get();
            next_key.set(key + 1);
            let mut staged = pending.get();
            staged.push((key, filename, mime, url));
            pending.set(staged);
            err.set(String::new());
        }
        Err(msg) => err.set(msg),
    }
}

/// The images attached to one live message, rendered as a row UNDER the
/// body: each image sits in a definite 400x300 box with `object_fit`
/// Contain preserving the aspect ratio (the same definite-box sizing the
/// avatar chips use — width/height + object_fit + no flex_shrink drift).
/// A reactive style fed from the image's `on_load` natural size is NOT
/// used: that never took effect in the live app (the node rendered 0x0).
fn attachment_row(atts: &[discord_server::server_settings::MessageAttachment]) -> Element {
    let cells: Vec<Element> = atts.iter().map(|a| {
        let data = crate::auth::media_url(a.url.clone());
        let name = a.filename.clone();
        ui! {
            image(
                src = data,
                alt = name,
                style = StyleRules {
                    width: Some(px(400.0)),
                    height: Some(px(300.0)),
                    flex_shrink: Some(grow(0.0)),
                    object_fit: Some(ObjectFit::Contain),
                    ..Default::default()
                },
            )
        }
    }).collect();
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(8.0)),
            margin_top: Some(px(6.0)),
            margin_bottom: Some(px(6.0)),
            ..Default::default()
        }) { cells }
    }
}

/// Robot test hook (`attach.stage`): stage one composer image attachment
/// from base64 bytes — the file-picker callback's validate → encode path
/// (`stage_attachment` above, which the picker's async callback runs),
/// callable through the robot bridge so an E2E test can attach a real
/// image without driving the native file-open dialog (which no in-page
/// tool can tap). Compiled only into robot builds.
runtime_vocabulary::robot_only! {
use runtime_vocabulary::robot_fn::{RobotArg, RobotCall, RobotFunction, RobotReply, register as robot_register};
use runtime_shared::__serde_json as robot_json;

pub(crate) fn register_send_now_hook(on_send: Rc<dyn Fn()>) {
    let hook: RobotCall = Rc::new(
        move |args: &robot_json::Value| -> Result<RobotReply, String> {
            (on_send)();
            Ok(RobotReply::Ready(robot_json::Value::Null))
        },
    );
    robot_register(
        RobotFunction {
            name: "send.now",
            doc: "Fire the composer's send action (the same Rc Enter's on_key_down calls — body + staged attachments). No args.",
            args: &[],
            returns: "()",
            is_async: false,
            call: Some(hook),
        },
    );
}

pub(crate) fn register_attach_stage_hook(
    pending: runtime_core::Signal<Vec<PendingImage>>,
    next_key: runtime_core::Signal<i64>,
    err: runtime_core::Signal<String>,
) {
    let hook: RobotCall = Rc::new(
        move |args: &robot_json::Value| -> Result<RobotReply, String> {
            let mime = args.get("mime").and_then(|v| v.as_str()).ok_or("attach.stage: 'mime' must be a string")?;
            let data_b64 = args.get("data_b64").and_then(|v| v.as_str()).ok_or("attach.stage: 'data_b64' must be a string")?;
            let filename = args.get("filename").and_then(|v| v.as_str()).unwrap_or("image");
            let bytes = crate::profile_settings::b64_decode(data_b64)?;
            stage_attachment(pending.clone(), next_key.clone(), err.clone(), filename.to_string(), mime.to_string(), &bytes);
            let reply = RobotReply::Ready(robot_json::Value::Null);
            Ok(reply)
        },
    );
    robot_register(
        RobotFunction {
            name: "attach.stage",
            doc: "Stage a composer image attachment from base64 bytes ({mime, data_b64, filename?}) — the file-picker callback's encode path for tests that cannot drive the native file dialog.",
            args: &[
                RobotArg { name: "mime", ty: "String" },
                RobotArg { name: "data_b64", ty: "String" },
                RobotArg { name: "filename", ty: "String" },
            ],
            returns: "()",
            is_async: false,
            call: Some(hook),
        },
    );
}
}

/// One buffered live (gateway-fed) message: the server's wire `message_id`,
/// then `author_id` + `author_label`, `content`, `created_at_secs`, and
/// `edited_at` (the `MessageEdited` timestamp in seconds, `None` until edited).
/// The display fields mirror the old 4-tuple; `message_id` + `edited_at` are
/// what the per-row edit/delete affordance and the "(edited)" marker need on
/// top of them (the mock path carries no wire id, so it renders `None`).
type LiveMsg = (i64, i64, String, String, i64, Option<i64>, Option<i64>);

/// Insert a `LiveMsg` into a channel's buffer in (created_at_secs, message_id)
/// ascending order, skipping a duplicate `message_id` if one is already present
/// (dedup on absorption). Returns `true` if the message was added.
///
/// This keeps the buffer sorted oldest-first no matter which way a frame arrives:
/// a live `new: true` frame (the newest message) sorts to the end, and a
/// scrollback page — the server's `before_id` cursor returns strictly-older
/// messages — stacks in at the front, all in one call. It is the single write
/// path for `live_messages`, so the buffer's ordering invariant is maintained in
/// one place rather than assumed by every call site.
fn insert_live_msg(
    buf: &mut Vec<LiveMsg>,
    msg: (i64, i64, String, String, i64, Option<i64>, Option<i64>),
) -> bool {
    let (id, ..) = msg;
    // Dedup: a `message_id` already in the buffer is a re-delivery (the server
    // may echo a History page that overlaps the live tail), so skip it.
    if buf.iter().any(|(e, ..)| *e == id) {
        return false;
    }
    // Binary-search the position by (created_at_secs, message_id), the same key
    // the server's `ORDER BY created_at ASC, id ASC` uses for a page. Each buffer
    // element's (created_at_secs, message_id) is compared as an owned tuple so the
    // closure's two sides have the same type.
    let pos = buf
        .partition_point(|e| (e.4, e.0) < (msg.4, msg.0))
        .min(buf.len());
    buf.insert(pos, msg);
    true
}

/// The "start of conversation" anchor for a live channel — a centered, muted
/// line (e.g. "Jump to the start of the #general conversation") shown ABOVE the
/// earliest loaded message. It is the empty-state's sibling for channels that
/// have content: where a zero-message channel shows a centered "No messages
/// yet -- say hi!", a one-message channel (or any freshly opened one) shows this
/// anchor as the first row so the pane reads as "you're at the top of a real
/// conversation" rather than a single orphaned message floating in a void.
///
/// It is a plain `fn -> Element` (not `#[component]`) like `live_edit_row` — it
/// takes only a channel name and a static style, so there is no props struct to
/// generate. `channel_name` is looked up by the caller from `live_channels`.
#[allow(non_snake_case)]
fn channel_anchor(channel_name: &str) -> Element {
    ui! {
        view(style = StyleRules {
            align_items: Some(AlignItems::Center),
            padding_top: Some(px(16.0)),
            padding_bottom: Some(px(8.0)),
            ..Default::default()
        }) {
            view(style = StyleRules {
                width: Some(px(16.0)),
                height: Some(px(16.0)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                background: Some(col(BG_ELEVATED)),
                border_top_left_radius: Some(px(8.0)),
                border_top_right_radius: Some(px(8.0)),
                border_bottom_left_radius: Some(px(8.0)),
                border_bottom_right_radius: Some(px(8.0)),
                ..Default::default()
            }) {
                text(style = StyleRules {
                    font_size: Some(px(12.0)),
                    color: Some(col(TEXT_MUTED)),
                    ..Default::default()
                }) { "#".to_string() }
            }
            text(style = StyleRules {
                margin_left: Some(px(8.0)),
                font_size: Some(px(13.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(TEXT_MUTED)),
                ..Default::default()
            }) { format!("You're looking at the beginning of the #{} conversation.", channel_name) }
        }
    }
}


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
pub(crate) fn format_when(unix_secs: i64, now_secs: i64) -> String {
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

/// Full absolute date/time for a message's instant — the hover tooltip's
/// "Saturday, September 12, 2026 at 3:42 PM" form (the counterpart to
/// `format_when`'s short relative label). Pure integer math over the same
/// `local_day_clock` / `civil_ymd_from_day` helpers; the weekday is derived
/// from the day number (1970-01-01 was a Thursday).
pub(crate) fn format_full_when(unix_secs: i64) -> String {
    let (day, h, m) = local_day_clock(unix_secs);
    let (y, mo, d) = civil_ymd_from_day(day);
    // 1970-01-01 (day 0) was a Thursday, so `day.rem_euclid(7)` indexes a
    // Thursday-first weekday table.
    let weekday = [
        "Thursday", "Friday", "Saturday", "Sunday", "Monday", "Tuesday", "Wednesday",
    ][day.rem_euclid(7) as usize];
    let month = [
        "January", "February", "March", "April", "May", "June", "July", "August",
        "September", "October", "November", "December",
    ][(mo - 1) as usize];
    let h12 = {
        let h12 = h % 12;
        if h12 == 0 {
            12
        } else {
            h12
        }
    };
    let suffix = if h < 12 { "AM" } else { "PM" };
    format!("{weekday}, {month} {d}, {y} at {h12}:{m:02} {suffix}")
}

// --- small reusable rows -----------------------------------------------------

/// The inline content of a message body, split on Discord spoiler markers.
/// Discord marks spoiler text with `||…||`; everything between a pair of
/// `||` is rendered through `SpoilerTag` (the blurred, click-to-reveal tag)
/// while the rest renders as plain text. The split runs are laid out in a
/// `flex_direction: Row` + `flex_wrap: Wrap` container so an inline spoiler
/// sits on the same line as the surrounding words and wraps like the rest of
/// the body. A body with no `||…||` marker renders exactly as one plain text
/// run (the previous behavior), so this is a content-dependent upgrade, not an
/// unconditional per-row widget.
fn message_body(body: &str) -> Element {
    // A body that carries real Markdown syntax (emphasis, code, links, quotes,
    // lists…) renders the WHOLE thing through the `markdown` SDK — one scene
    // payload whose caps-generic handler builds the semantic DOM (styled runs,
    // `<pre>` blocks, `<blockquote>`). Bodies with no Markdown sigils — every
    // plain chat line, and every mock seed row — skip the SDK entirely and
    // keep the exact node shape they had before, so plain messages keep their
    // current metrics and grouped consecutive rows stay tight. A body
    // carrying `||…||` spoilers keeps the spoiler path (SDK v1 has no spoiler
    // construct); the two features never stack on one row.
    if !body.contains("||") && looks_like_markdown(body) {
        return Element::from(markdown::markdown(linkify_bare_urls(body), md_chat_theme()));
    }
    // Split the body into `(is_spoiler, text)` runs. A `||…||` pair becomes a
    // spoiler run; the surrounding slices become plain runs. We walk the body
    // by finding each marker pair in turn so the runs stay in document order.
    let mut runs: Vec<(bool, String)> = Vec::new();
    let chars: Vec<char> = body.chars().collect();
    let len = chars.len();
    let mut i = 0usize;
    let mut plain = String::new();
    while i < len {
        if i + 1 < len && chars[i] == '|' && chars[i + 1] == '|' {
            // Opening marker: flush the accumulated plain run, then look for
            // the closing `||` after it.
            if !plain.is_empty() {
                runs.push((false, std::mem::take(&mut plain)));
            }
            let start = i + 2;
            let mut close = None;
            let mut j = start;
            while j + 1 < len {
                if chars[j] == '|' && chars[j + 1] == '|' {
                    close = Some(j);
                    break;
                }
                j += 1;
            }
            match close {
                Some(c) => {
                    // Spoiler body is the slice between the two markers.
                    let inner: String = chars[start..c].iter().collect();
                    runs.push((true, inner));
                    i = c + 2;
                }
                None => {
                    // Unmatched marker: treat the `||` as literal text.
                    plain.push('|');
                    plain.push('|');
                    i += 2;
                }
            }
        } else {
            plain.push(chars[i]);
            i += 1;
        }
    }
    if !plain.is_empty() {
        runs.push((false, plain));
    }
    if runs.is_empty() {
        // Empty body: render an empty string (keeps the row shape stable).
        runs.push((false, String::new()));
    }

    let runs_el: Vec<Element> = runs
        .iter()
        .map(|(is_spoiler, text)| {
            if *is_spoiler {
                // The click-to-reveal spoiler. The generated component fn takes
                // a `Reactive<String>` `text` prop; `String: Into<Reactive<String>>`
                // (the `From<T> for Reactive<T>` impl), so `…into()` bakes a
                // static prop. (The `text = …` named-prop form only parses
                // inside `ui!`; here we call the fn directly, positionally.)
                ui! {
                    view(style = StyleRules {
                        margin_left: Some(px(2.0)),
                        margin_right: Some(px(2.0)),
                        ..Default::default()
                    }) {
                        { crate::spoiler_tag::SpoilerTag(text.to_string().into()) }
                    }
                }
            } else {
                // Plain run: an inline text node in the same flow as the
                // spoilers, so the row stays on one visual line when it fits.
                ui! {
                    text(style = StyleRules {
                        font_size: Some(px(15.0)),
                        color: Some(col(TEXT)),
                        ..Default::default()
                    }) { text.to_string() }
                }
            }
        })
        .collect();

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            flex_wrap: Some(FlexWrap::Wrap),
            flex_grow: Some(grow(1.0)),
            ..Default::default()
        }) { runs_el }
    }
}

/// Wrap whitespace-delimited tokens starting with `http://`/`https://` in
/// `<…>` autolinks so the SDK renders them as links (Discord linkifies bare
/// URLs; the SDK parser enables strikethrough but not GFM autolinking, and
/// `<url>` is plain CommonMark autolink syntax). URLs inside `[t](u)` or an
/// existing `<u>` never start a token with the scheme, so they're untouched;
/// trailing sentence punctuation is moved outside the link marker. Code is
/// protected per-region, not whole-body: inline `…` spans and ``` fenced blocks
/// pass through verbatim while URLs outside them still linkify, so "`code`
/// https://x" links but "`https://x`" doesn't.
fn linkify_bare_urls(body: &str) -> String {
    let bytes = body.as_bytes();
    let n = bytes.len();
    let is_ws = |c: u8| c == b' ' || c == b'\t' || c == b'\n' || c == b'\r';
    let is_tail_punct = |c: u8| c == b'.' || c == b',' || c == b'!' || c == b'?'
        || c == b';' || c == b':' || c == b'\'' || c == b'"';
    // True when the line starting at `pos` opens with ``` (the fence marker),
    // after up to 3 leading spaces (CommonMark's indent allowance).
    let fence_at_line_start = |pos: usize| -> bool {
        let mut j = pos;
        let mut spaces = 0;
        while j < n && spaces < 4 && (bytes[j] == b' ' || bytes[j] == b'\t') {
            j += 1;
            spaces += 1;
        }
        j + 2 < n && bytes[j] == b'`' && bytes[j + 1] == b'`' && bytes[j + 2] == b'`'
    };
    // Index just past the newline ending the line at `pos` (or end of body).
    let line_end = |pos: usize| -> usize {
        if pos >= n {
            return n;
        }
        match body[pos..].find('\n') {
            Some(off) => pos + off + 1,
            None => n
        }
    };
    let mut out = String::new();
    let mut i = 0;
    while i < n {
        let line_start = i == 0 || bytes[i - 1] == b'\n';
        // Fenced code block: copy the opening fence line through the closing
        // fence line verbatim, then resume linkifying after it.
        if line_start && fence_at_line_start(i) {
            let open_end = line_end(i);
            out += &body[i..open_end];
            i = open_end;
            while i < n {
                let next_start = i == 0 || bytes[i - 1] == b'\n';
                if next_start && fence_at_line_start(i) {
                    let close_end = line_end(i);
                    out += &body[i..close_end];
                    i = close_end;
                    break;
                }
                // interior line: verbatim
                let end = line_end(i);
                out += &body[i..end];
                i = end;
            }
            continue;
        }
        // Inline code span: from this backtick through the NEXT backtick on
        // the same line, verbatim. An unmatched backtick falls through to the
        // token loop below and renders as-is, same as pulldown-cmark does.
        if bytes[i] == b'`' {
            let mut k = i + 1;
            while k < n && bytes[k] != b'`' && bytes[k] != b'\n' {
                k += 1;
            }
            if k < n && bytes[k] == b'`' {
                out += &body[i..k + 1];
                i = k + 1;
                continue;
            }
        }
        if is_ws(bytes[i]) {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }
        let start = i;
        while i < n && !is_ws(bytes[i]) {
            i += 1;
        }
        let tok = &body[start..i];
        if tok.starts_with("http://") || tok.starts_with("https://") {
            let scheme_len = if tok.starts_with("https://") { 8 } else { 7 };
            let mut end = i - start;
            while end > scheme_len && is_tail_punct(bytes[start + end - 1]) {
                end -= 1;
            }
            out += "<";
            out += &tok[..end];
            out += ">";
            out += &tok[end..];
        } else {
            out += tok;
        }
    }
    out
}

#[cfg(test)]
mod linkify_tests {
    use super::*;

    /// A bare URL in prose is wrapped in `<…>` autolink syntax.
    #[test]
    fn plain_url_gets_linkified() {
        assert_eq!(linkify_bare_urls("see https://example.com now"),
            "see <https://example.com> now");
    }

    /// Inline code no longer vetoes linkification for the whole body: the
    /// `` `code` `` span passes through verbatim and the URL after it links.
    #[test]
    fn url_outside_inline_code_links() {
        assert_eq!(linkify_bare_urls("`code` https://example.com"),
            "`code` <https://example.com>");
    }

    /// A URL INSIDE an inline code span stays untouched (no markers inserted).
    #[test]
    fn url_inside_inline_code_stays_untouched() {
        assert_eq!(linkify_bare_urls("run `https://example.com` here"),
            "run `https://example.com` here");
    }

    /// A URL inside a ``` fenced block stays untouched — verbatim through the
    /// fence — while a URL after the closing fence still links.
    #[test]
    fn url_inside_fenced_block_stays_untouched() {
        assert_eq!(linkify_bare_urls("```\nhttps://inside.example\n```\nafter https://outside.example"),
            "```\nhttps://inside.example\n```\nafter <https://outside.example>");
    }

    /// Trailing sentence punctuation sits OUTSIDE the autolink marker.
    #[test]
    fn trailing_punctuation_trimmed() {
        assert_eq!(linkify_bare_urls("https://example.com."),
            "<https://example.com>.");
        assert_eq!(linkify_bare_urls("go https://example.com, ok"),
            "go <https://example.com>, ok");
    }
}

/// Chat-body theme for the `markdown` SDK: the dark Discord palette at the
/// message-body text size (15px). `code_bg` sits a hair below `BG_MAIN`
/// (#313338) so inline code reads as a subtle chip rather than a boxy
/// highlight; links use Discord's blurple (`BUTTON`).
fn md_chat_theme() -> markdown::MdTheme {
    markdown::MdTheme {
        text: TEXT.to_string(),
        muted: TEXT_MUTED.to_string(),
        heading: TEXT.to_string(),
        link: BUTTON.to_string(),
        code_fg: TEXT.to_string(),
        code_bg: "#232529".to_string(),
        quote_fg: TEXT_CHANNEL.to_string(),
        base_size: 15.0,
        heading_scale: [1.6, 1.3, 1.15, 1.05, 1.0, 0.9],
        mono_family: None,
    }
}

/// Cheap super-set test for Markdown worth routing through the SDK: does the
/// body contain an inline sigil (emphasis/code/strike/link brackets) or open
/// a line with a block construct (heading, quote, list bullet, rule)? False
/// positives (a stray `_`, an `[` in prose) are harmless — pulldown-cmark
/// needs well-formed spans, so plain-looking text parses back to one plain
/// run and renders at the same size/color as the `text` node it replaces.
fn looks_like_markdown(body: &str) -> bool {
    if body.contains("**")
        || body.contains("__")
        || body.contains("_")
        || body.contains("*")
        || body.contains("~~")
        || body.contains("`")
        || body.contains("](")
        || body.contains("[")
        || body.contains("http://")
        || body.contains("https://")
    {
        return true;
    }
    for line in body.split('\n') {
        let t = line.trim_start();
        if t.starts_with("#") || t.starts_with(">") || t.starts_with("- ")
            || t.starts_with("+ ") || t.starts_with("---")
        {
            return true;
        }
        // Ordered list marker: `1. ` etc. (a digit run then `. `). ASCII, so
        // the byte slice offset equals the char count.
        let ds: Vec<char> = t.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !ds.is_empty() && t[ds.len()..].starts_with(". ") {
            return true;
        }
    }
    false
}

/// Extract a `LinkEmbedPreview` prop pair `(host, mock_title)` from a message
/// body that contains a URL-looking string (`http://…` / `https://…`). Returns
/// `None` for a body with no URL — the caller then renders no preview at all,
/// so this is a content-dependent upgrade (only messages with a link get a
/// card), never a per-row widget. The host is parsed straight from the URL;
/// the title is a small deterministic mock derived from the URL's last path
/// segment (there is no real fetch), so it varies per message.
fn link_preview_props(body: &str) -> Option<(String, String)> {
    // Locate the URL substring: from the scheme to the next whitespace (or end).
    let s = body
        .find("http://")
        .or_else(|| body.find("https://"))?;
    let e = body[s..]
        .find(char::is_whitespace)
        .map(|i| s + i)
        .unwrap_or(body.len());
    let url = &body[s..e];
    // Drop the scheme to get "host[/path…][?query…]".
    let after = url.splitn(2, "://").nth(1).unwrap_or("");
    // Host = up to the first `/`, `?`, or `:` (port).
    let host = after
        .split(|c| c == '/' || c == '?' || c == ':')
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    if host.is_empty() {
        return None;
    }
    // Path = everything after the first `/`; the LAST non-empty segment is the
    // mock title source (title-cased) — for `…/owner/repo/…` that's the
    // resource name (Discord's card shows the page's own title, and the repo
    // path is the closest stand-in). `.rev().find` walks from the end, so
    // trailing query/empty segments are skipped. Falls back to a generic label.
    let path = after.splitn(2, "/").nth(1).unwrap_or("");
    let last = path
        .split('/')
        .rev()
        .find(|seg| !seg.trim().is_empty())
        .unwrap_or("");
    let title = if last.trim().is_empty() {
        "Linked page".to_string()
    } else {
        let clean = last.trim();
        let mut chars = clean.chars();
        match chars.next() {
            Some(c) => c.to_uppercase().to_string() + chars.as_str(),
            None => clean.to_string(),
        }
    };
    Some((host, title))
}

/// One message row: avatar chip, then author + "Today at …" line, then body.
/// Takes `&str`s so it serves both the static seed data and runtime-authored
/// messages (the strings are copied into the node, so the returned `Element`
/// is self-contained). `edited` gates the "(edited)" marker: live rows pass
/// the message's real `edited_at.is_some()`; the mock path passes `true` to
/// preserve its previous (always-on placeholder) rendering. `grouped` is the
/// Discord message-grouping form: the row continues the previous message's
/// group (same author within `GROUP_WINDOW_SECS`), so it renders no avatar
/// and no author/time header — just the body, tightly spaced (3px instead of
/// 8px above) and left-aligned under where the name starts (66px = 16px pad
/// + 40px avatar + 10px gap), with no right margin (a body that wide is
/// full-width by construction). `same_group` computes the flag at each call
/// site.
pub(crate) fn message_row(
    author: &str,
    color: &str,
    body: &str,
    when: String,
    full: String,
    edited: bool,
    grouped: bool,
) -> Element {
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
    let initial = author.chars().next().unwrap_or(' ').to_string();
    // The author's avatar chip: the uploaded image (round-cropped, cover-fit)
    // when the shared client avatar registry (`crate::auth::avatar_for`, fed
    // by login / `list_members` / gateway frames) knows a URL for this author,
    // else the pre-existing colored initial circle. Mock identities are never
    // registered, so guest/demo rows render exactly as before.
    let avatar_chip = if let Some(url) = crate::auth::avatar_for(author) {
        let url = crate::auth::media_url(url.clone());
        ui! {
            image(
                src = url,
                alt = format!("{} avatar", author),
                style = StyleRules {
                    width: Some(px(40.0)),
                    height: Some(px(40.0)),
                    flex_shrink: Some(grow(0.0)),
                    object_fit: Some(ObjectFit::Cover),
                    border_top_left_radius: Some(px(20.0)),
                    border_top_right_radius: Some(px(20.0)),
                    border_bottom_left_radius: Some(px(20.0)),
                    border_bottom_right_radius: Some(px(20.0)),
                    ..Default::default()
                },
            )
        }
    } else {
        ui! {
            view(style = avatar_style) {
                text(style = avatar_text_style) { initial }
            }
        }
    };
    if grouped {
        // Grouped form (Discord convention): no avatar, no author/time
        // header. Just the body, tightly spaced above the row it follows
        // (3px vs the 8px of a fresh header row) and left-aligned under
        // where the name would start: 16px pad + 40px avatar + 10px gap.
        // No right padding — at this width the body is full-width anyway.
        let grouped_style = StyleRules {
            padding_top: Some(px(3.0)),
            padding_bottom: Some(px(2.0)),
            padding_left: Some(px(66.0)),
            padding_right: Some(px(16.0)),
            ..Default::default()
        };
        return ui! {
            view(style = grouped_style) {
                // `message_body` renders markdown-bearing bodies through the
                // SDK; a plain body is one `text` run at the same 15px/TEXT
                // styling, so grouped consecutive rows keep their tight look.
                { message_body(body) }
                if edited { edited_label() }
            }
        };
    }
    // Content-dependent link preview: only a body that actually contains a
    // `http(s)://…` string gets a `LinkEmbedPreview` card rendered below it.
    // `link_preview_props` returns `None` for a URL-less body, so no card is
    // rendered for the other rows — this is a deliberate single instance on
    // the one link-bearing seed message, NOT a per-row widget (the graveyard
    // anti-pattern). `site_name` + `title` are parsed from the URL; the
    // description is a small mock line.
    let link = link_preview_props(body);
    ui! {
        view(style = row_style) {
            { avatar_chip }
            view(style = stack_style) {
                view(style = meta_style) {
                    text(style = author_style) { author.to_string() }
                    if author == "miri" { VerifiedBadge() }
                    // The relative "when" label is a real hover target now:
                    // `message_timestamp_tooltip` shows the short label at rest
                    // and the full absolute date/time (`full`) while hovered.
                    crate::message_timestamp_tooltip::message_timestamp_tooltip(when, full)
                }
                // The body, split on `||…||` spoiler markers: plain runs render
                // as text and each spoiler run as a `SpoilerTag` (blurred,
                // click-to-reveal). Content-dependent — a marker-less body is a
                // single plain run (the previous rendering).
                { message_body(body) }
                if edited { edited_label() }
                // The link preview card, only when the body carries a URL.
                if let Some((host, title)) = &link {
                    crate::link_embed_preview::LinkEmbedPreview(
                        host.clone().into(),
                        title.clone().into(),
                        "Shared a link — preview shown automatically.".to_string().into(),
                    )
                }
            }
        }
    }
}

// --- per-row hover action bar + overflow menu (channel message list) --------
// These wrap `message_row` for the CHANNEL message list only (the mock rows
// that also carry a `reaction_area` + the shared reaction `picker`). The wrapper
// adds a `Position::Relative` box around the base row, drives a per-row `hovered`
// signal from the box's `on_hover` (the voice-channel-row idiom), and floats the
// `MessageActionBar` over the row's top-right. DM rows and the live/edit rows
// call `message_row` directly and never reach this, so they keep rendering
// exactly as before (no per-row controls added to a loop that shouldn't have them).

/// The small overflow menu that the action bar's "more" button opens. Rendered
/// only while `open` is true (the `switch` primitive, same show/hide idiom as
/// `MessageActionBar`); it drops just below the bar. `on_pin` shows the pin
/// toast; `on_translate` reveals the message's translate control; every entry
/// closes the menu on tap.
fn message_overflow_menu(
    open: Signal<bool>,
    on_copy: Rc<dyn Fn()>,
    on_pin: Rc<dyn Fn()>,
    on_translate: Rc<dyn Fn()>,
    // Optional owner-only entries (Edit / Delete) — the caller passes them only
    // for the current user's own messages (live rows, `is_mine`); mock rows
    // pass `None` and the menu keeps its two shared entries. When present they
    // render as the menu's last two rows, so the whole message-action set
    // (react / reply / copy / pin / translate / edit / delete) lives in this one
    // hover-gated surface instead of a separate always-visible inline bar.
    on_edit: Option<Rc<dyn Fn()>>,
    on_delete: Option<Rc<dyn Fn()>>,
) -> Element {
    switch(
        move || open.get(),
        move |show: &bool| {
            if !show {
                return ui! {
                    view(style = StyleRules {
                        width: Some(px(0.0)),
                        height: Some(px(0.0)),
                        ..Default::default()
                    }) {}
                };
            }
            let pin = on_pin.clone();
            let translate = on_translate.clone();
            let edit = on_edit.clone();
            let delete = on_delete.clone();
            // A generic tap row: one `Rc<dyn Fn()>` per entry, fired on touch
            // end, then the menu closes. (The earlier `MoreMenuAction` enum
            // hard-coded the two shared actions; Edit/Delete arrived after, so
            // the row now takes its handler directly.)
            let menu_row = |label: &'static str, action: Rc<dyn Fn()>, open: Signal<bool>| -> Element {
                let row_style = StyleRules {
                    padding_top: Some(px(8.0)),
                    padding_bottom: Some(px(8.0)),
                    padding_left: Some(px(12.0)),
                    padding_right: Some(px(12.0)),
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    ..Default::default()
                };
                let label_style = StyleRules {
                    font_size: Some(px(14.0)),
                    color: Some(col(TEXT)),
                    ..Default::default()
                };
                let open_c = open;
                let touch = move |ev: &runtime_shared::TouchEvent| {
                    if ev.phase == TouchPhase::Ended {
                        action();
                        open_c.set(false);
                    }
                    TouchResponse::CONSUMED
                };
                ui! {
                    view(style = row_style) {
                        text(style = label_style) { label.to_string() }
                    }
                    .on_touch(touch)
                }
            };
            ui! {
                view(style = StyleRules {
                    position: Some(Position::Absolute),
                    top: Some(px(40.0)),
                    right: Some(px(12.0)),
                    width: Some(px(190.0)),
                    flex_direction: Some(FlexDirection::Column),
                    // Clip, don't scroll: this is a short fixed list (3–5 rows)
                    // that must never scroll; `hidden` keeps the surface from
                    // showing a scrollbar if a row ever wraps past the box.
                    overflow: Some(Overflow::Hidden),
                    background: Some(col("#111214")),
                    border_top_left_radius: Some(px(8.0)),
                    border_top_right_radius: Some(px(8.0)),
                    border_bottom_left_radius: Some(px(8.0)),
                    border_bottom_right_radius: Some(px(8.0)),
                    border_top_width: Some(border_width(1.0)),
                    border_right_width: Some(border_width(1.0)),
                    border_bottom_width: Some(border_width(1.0)),
                    border_left_width: Some(border_width(1.0)),
                    border_top_color: Some(col(BORDER)),
                    border_right_color: Some(col(BORDER)),
                    border_bottom_color: Some(col(BORDER)),
                    border_left_color: Some(col(BORDER)),
                    ..Default::default()
                }) {
                    { menu_row("Copy text", on_copy.clone(), open) }
                    { menu_row("Pin message", pin.clone(), open) }
                    { menu_row("Translate message", translate, open) }
                    // Owner-only entries: present only when the caller supplied
                    // handlers (live rows the current user owns). Each renders a
                    // zero-size placeholder otherwise so the branch still yields
                    // an `Element` for the `ui!` child slot.
                    {
                        if let Some(edit) = edit {
                            menu_row("Edit", edit, open)
                        } else {
                            ui! {
                                view(style = StyleRules {
                                    width: Some(px(0.0)),
                                    height: Some(px(0.0)),
                                    ..Default::default()
                                }) {}
                            }
                        }
                    }
                    {
                        if let Some(delete) = delete {
                            menu_row("Delete", delete, open)
                        } else {
                            ui! {
                                view(style = StyleRules {
                                    width: Some(px(0.0)),
                                    height: Some(px(0.0)),
                                    ..Default::default()
                                }) {}
                            }
                        }
                    }
                }
            }
        },
    )
}

/// A channel message row with the hover `MessageActionBar` + overflow menu.
/// Wraps `message_row` (the base content) in a `Position::Relative` box and
/// floats the action bar over the row's top-right while hovered. React points
/// the SHARED reaction `picker` at this row (the same picker `reaction_area`'s
/// add button opens — no second reaction UI); Reply populates the composer's
/// `reply_ctx` (read by `ReplyPreviewBar`); More opens `message_overflow_menu`
/// whose "Pin message" shows `PinConfirmToast`.
fn message_row_with_actions(
    author: &str,
    color: &str,
    body: &str,
    when: String,
    full: String,
    edited: bool,
    grouped: bool,
    key: MessageKey,
    picker: Signal<Option<MessageKey>>,
    reply_ctx: Signal<Option<(String, String)>>,
    // The wire id of the message being replied to. `on_reply` (below) sets it
    // from this row's `key` (a live row carries its real id; a mock row has
    // none), so the composer can attach it to the outbound `Send` frame. The
    // display half (author + snippet) lives in `reply_ctx`, read by the
    // preview bar; this carries only the id the server needs.
    reply_target: Signal<Option<i64>>,
    pin_visible: Signal<bool>,
    translate_key: Signal<Option<MessageKey>>,
    on_pin: Rc<dyn Fn()>,
    // Optional owner-only menu entries (Edit / Delete). The caller passes them
    // only for the current user's own messages (live rows, `is_mine`); mock
    // rows pass `None` and the menu keeps its three shared entries. When
    // present they render as the menu's last two rows, so the whole
    // message-action set lives in this one hover-gated surface.
    on_edit: Option<Rc<dyn Fn()>>,
    on_delete: Option<Rc<dyn Fn()>>,
) -> Element {
    let hovered = signal(false);
    let menu_open = signal(false);
    let base = message_row(author, color, body, when, full, edited, grouped);

    // Reply context: author name + a short snippet of the body. `Signal` is
    // `Copy`, so each `move` handler captures its own copy; the `String`s are
    // cloned per press so the handlers stay `Fn`.
    let author_owned = author.to_string();
    let snippet: String = {
        let s: String = body.chars().take(48).collect();
        if body.chars().count() > 48 {
            format!("{s}…")
        } else {
            s
        }
    };
    let reply_c = reply_ctx;
    let author_r = author_owned.clone();
    let snippet_r = snippet.clone();
    // The reply target's wire id, derived from this row's `key`: a live row
    // carries its real id, a mock row has none. Recorded so the composer's
    // outbound `Send` frame can attach it. `key` is `Copy`, so the handler
    // captures its own copy.
    let reply_t = reply_target;
    let key_r = key;
    let on_reply = Rc::new(move || {
        reply_c.set(Some((author_r.clone(), snippet_r.clone())));
        let id = match key_r {
            MessageKey::Live { id } => id,
            MessageKey::Mock { .. } => return,
        };
        reply_t.set(Some(id));
    });

    // React: point the shared picker at this row's `MessageKey` — the exact
    // entry point `reaction_area`'s add button uses. No second UI.
    let picker_c = picker;
    let on_react = Rc::new(move || {
        picker_c.set(Some(key));
    });

    // More: toggle the overflow menu. The reaction picker's full-window
    // backdrop paints over the row area, so opening the menu while the picker
    // is up would bury the menu's rows (the Edit row intercepted by the
    // picker in the last test) — close the picker as the menu opens.
    let menu_c = menu_open;
    let on_more = Rc::new(move || {
        let opening = !menu_c.get();
        menu_c.set(opening);
        if opening {
            picker_c.set(None);
        }
    });

    // "Pin message" (from the overflow menu): the caller supplies `on_pin`. The
    // MOCK rows flash the "Message pinned!" toast (and auto-dismiss it); the
    // LIVE rows call the backend's real `pin_message` RPC (see the live row
    // builder, which also updates the header pin count). The toast's own "x" is
    // the manual dismiss on the mock path.

    // "Translate message" (from the overflow menu): reveal this row's
    // `TranslateToggle` below the message. Content-agnostic — any message can
    // be translated, so the control is a per-row OPTION (the overflow-menu
    // entry), not a persistent widget: the toggle card only mounts while this
    // row's key is the active one. `translate_key` is the shared "which row is
    // translating" signal (the picker idiom), so opening another row's menu
    // and dismissing both close it. Re-tapping "Translate message" on the same
    // row toggles it off.
    let translate_key_c = translate_key;
    let my_key = key;
    let on_translate = Rc::new(move || {
        let k = translate_key_c;
        k.set(if k.get() == Some(my_key) { None } else { Some(my_key) });
    });

    // "Copy text" (from the overflow menu): write this row's message body to
    // the system clipboard. The clipboard SDK (`clipboard::set_text`) is async
    // and per-backend (navigator.clipboard on web, UIPasteboard on Apple,
    // ClipboardManager on Android, arboard on Linux) — the same idiom the
    // `InviteLinkDisplay` Copy button uses — so the work is spawned via
    // `runtime_core::driver::spawn_async` and the menu closes regardless
    // (a failed write is silent, matching the invite-card's no-lie behavior).
    // `body` is an owned clone so the handler is `Fn` across presses.
    let body_owned = body.to_string();
    let on_copy = Rc::new(move || {
        let text = body_owned.clone();
        runtime_core::driver::spawn_async(async move {
            let _ = clipboard::set_text(&text).await;
        });
    });

    let bar = crate::message_action_bar::message_action_bar(hovered, on_react, on_reply, on_more);
    let menu = message_overflow_menu(menu_open, on_copy, on_pin, on_translate, on_edit, on_delete);

    // The translate control: `original` is the message body; `translated` is a
    // mock rendering (this clone has no translation backend — the toggle
    // demonstrates the reveal/dismiss pattern, and the line visibly swaps).
    // Constructed via the generated props (`.into()` → `Reactive::Static`);
    // the card only renders while this row is the active translator, so it is
    // a deliberate single instance, never spliced into every row.
    let orig = body.to_string();
    let trans = format!("Translated (mock): {body}");

    ui! {
        view(style = StyleRules {
            position: Some(Position::Relative),
            flex_direction: Some(FlexDirection::Column),
            ..Default::default()
        }) {
            { base }
            { bar }
            { menu }
            if translate_key.get() == Some(key) {
                view {
                    crate::translate_toggle::TranslateToggle(orig.clone().into(), trans.clone().into())
                }
            }
        }
        .on_hover(move |on| hovered.set(on))
    }
}

/// The composer's "Replying to …" strip: wraps `reply_preview_bar` and backs
/// its "×" cancel with a callback that clears BOTH the shared `reply_ctx`
/// signal (the display author + snippet) and the `reply_target` signal (the
/// wire id). Each is `Copy`, so the `on_cancel` closure captures its own
/// handle; the bar itself reads a separate copy of `reply_ctx` (same signal).
fn reply_preview(
    reply_ctx: Signal<Option<(String, String)>>,
    reply_target: Signal<Option<i64>>,
) -> Element {
    let clear = reply_ctx;
    let clear_t = reply_target;
    let on_cancel = Rc::new(move || {
        clear.set(None);
        clear_t.set(None);
    });
    crate::reply_preview_bar::reply_preview_bar(reply_ctx, on_cancel)
}

// --- message reactions (mock session-local + live persisted/synced) --------
// Per-message reaction state shared by BOTH the MOCK and the LIVE (gateway)
// message lists. Keyed by `MessageKey`: MOCK rows key by `(channel_id,
// rendered-list-index)` (seed messages first, then this session's sends — a
// stable identity across `revision` bumps, since the list order is
// deterministic), LIVE rows key by their real wire `message_id`. Each value is
// a list of `(emoji, count, reacted_by_me)` pills. Mutating the map does not
// fire reactivity, so the message-list `switch` folds `reaction_rev` into its
// key (the `revision` idiom) and re-reads the map on every rebuild. MOCK pills
// stay session-local (demo content); LIVE pills are persisted server-side:
// clicks call the `add_reaction` / `remove_reaction` RPCs (see `crate::reactions`),
// the gateway's history frames restore persisted pills on a cold load, and
// `WsServerMsg::ReactionAdd` / `ReactionRemove` frames update every connected
// client live.
type Reaction = (String, u32, bool);

/// The identity key for per-message session-local state (the reaction `picker`,
/// the `reactions` pill map, and the `translate_key`). MOCK rows key by their
/// `(channel_id, rendered-list-index)` — the pre-existing stable-enough key —
/// while LIVE (gateway) rows key by their real wire `message_id`. The two are
/// disjoint enum variants, so a live message's id can never collide with a mock
/// `(cid, idx)`; the mock path's existing keys are preserved exactly (wrapped in
/// `Mock { .. }`) and the live path gets its own namespace beside them.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum MessageKey {
    Mock { cid: u32, idx: usize },
    Live { id: i64 },
}

type ReactionMap = HashMap<MessageKey, Vec<Reaction>>;

/// Route a reaction click on a LIVE (gateway) row through the backend RPCs:
/// `add_reaction` / `remove_reaction` persist the `(message, user, emoji)` row
/// and broadcast a `ReactionAdd` / `ReactionRemove` frame to every connected
/// member (the caller's own echo included), so the pill map is updated in
/// `gateway_incoming_effect` — never optimistically here. A rejected call
/// leaves no UI change (no optimistic pill to roll back). Also closes the
/// picker overlay when one was open for this message. MOCK rows never call
/// this (they stay session-local demo state).
fn request_live_reaction(
    key: MessageKey,
    emoji: String,
    remove: bool,
    picker: Signal<Option<MessageKey>>,
) {
    let id = match key {
        MessageKey::Live { id } => id,
        MessageKey::Mock { .. } => return,
    };
    picker.set(None);
    runtime_core::driver::spawn_async(async move {
        if remove {
            let _ = crate::reactions::remove_reaction(id, emoji).await;
        } else {
            let _ = crate::reactions::add_reaction(id, emoji).await;
        }
    });
}

/// Apply the server's authoritative post-state count for one emoji pill to the
/// shared reaction map (the LIVE path). The `ReactionAdd` / `ReactionRemove`
/// broadcast frames carry the total for the emoji, so the client SETS the pill
/// rather than tracking deltas — duplicate or out-of-order frames converge.
/// `reacted_override` is `Some(...)` only when the frame names this client's
/// own user (it sets the "you reacted" highlight); otherwise the existing
/// highlight is kept. Returns whether anything changed (the caller bumps
/// `reaction_rev` on `true` so the message list rebuilds).
fn apply_reaction(
    map: Rc<RefCell<ReactionMap>>,
    key: MessageKey,
    emoji: String,
    count: u32,
    reacted_override: Option<bool>,
) -> bool {
    let mut m = map.borrow_mut();
    let inner = m.entry(key).or_default();
    let pos = inner.iter().position(|(e, _, _)| e == &emoji);
    let changed: bool;
    if count == 0 {
        if let Some(p) = pos {
            inner.remove(p);
            changed = true;
        } else {
            changed = false;
        }
    } else if let Some(p) = pos {
        let (e, c, reacted) = inner[p].clone();
        let reacted = reacted_override.unwrap_or(reacted);
        if c != count || reacted != inner[p].2 {
            inner[p] = (e, count, reacted);
            changed = true;
        } else {
            changed = false;
        }
    } else {
        inner.push((emoji, count, reacted_override.unwrap_or(false)));
        changed = true;
    }
    if m.get(&key).map_or(false, |v| v.is_empty()) {
        m.remove(&key);
    }
    changed
}

/// The reaction strip under one message row: existing reaction pills on the
/// left, a spacer, and an always-visible "add reaction" button (the Lucide
/// SMILE_PLUS glyph) on the right. Tapping the add button opens the picker for
/// this message; tapping a pill toggles the current user's reaction on it
/// (increment + highlight, or decrement + un-highlight, dropping the pill at 0).
/// Each clickable node is a `view(...)` builder with `.on_touch` chained on the
/// builder inside `ui!` (the `ac_el` / composer-emoji-cell idiom) — `Element`
/// itself has no `on_touch` method, so the handler must live on the builder,
/// not on the returned `Element`.
fn reaction_area(
    key: MessageKey,
    reactions: Rc<RefCell<ReactionMap>>,
    reaction_rev: Signal<u32>,
    picker: Signal<Option<MessageKey>>,
) -> Element {
    // Current pills for this message (a clone — the borrow is released before
    // this call returns, so the list `switch` body can borrow again freely).
    let list: Vec<Reaction> = reactions.borrow().get(&key).cloned().unwrap_or_default();

    // One pill per current reaction, built as a `Vec<Element>` and splatted
    // below. Styled like the unwired `components/reaction_summary.rs` pill:
    // `reacted` selects a blurple border + tinted fill, otherwise a muted-gray
    // border on a neutral fill. Each pill's `.on_touch` gets fresh `Rc`/`Copy`
    // handles (`r`/`rev`) plus its own emoji clone (`em` for the label,
    // `em_close` for the closure), so it never moves the outer closure's values.
    // LIVE rows route the tap through `on_toggle` (the RPC-backed pill toggle —
    // the pill map then updates via the server's echo frames); MOCK rows keep
    // the direct session-local mutation.
    let pills: Vec<Element> = list.iter().map(|(emoji, count, reacted)| {
        let (bg, border) = if *reacted {
            (REACTED_BG, REACTED_BORDER)
        } else {
            (PLAIN_BG, PLAIN_BORDER)
        };
        let em = emoji.to_string();
        let em_close = em.clone();
        let cnt = *count;
        let r = reactions.clone();
        let rev = reaction_rev;
        // Whether this tap removes (user already reacted) or adds (user has not)
        // their own reaction. LIVE rows route the tap through the backend RPC
        // (see `request_live_reaction`); MOCK rows mutate the pill map directly.
        let will_remove = *reacted;
        let live_key = match key {
            MessageKey::Live { .. } => true,
            MessageKey::Mock { .. } => false,
        };
        let picker_l = picker;
        ui! {
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(5.0)),
                padding_left: Some(px(8.0)),
                padding_right: Some(px(8.0)),
                padding_top: Some(px(3.0)),
                padding_bottom: Some(px(3.0)),
                height: Some(px(22.0)),
                background: Some(col(bg)),
                border_top_left_radius: Some(px(11.0)),
                border_top_right_radius: Some(px(11.0)),
                border_bottom_left_radius: Some(px(11.0)),
                border_bottom_right_radius: Some(px(11.0)),
                // 1px border in the arm's color — `Tokenized<f32>` (not `px()`),
                // per the `reaction_summary` / `connection_quality_indicator` idiom.
                border_top_width: Some(Tokenized::Literal(1.0)),
                border_right_width: Some(Tokenized::Literal(1.0)),
                border_bottom_width: Some(Tokenized::Literal(1.0)),
                border_left_width: Some(Tokenized::Literal(1.0)),
                border_top_color: Some(col(border)),
                border_right_color: Some(col(border)),
                border_bottom_color: Some(col(border)),
                border_left_color: Some(col(border)),
                ..Default::default()
            }) {
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    ..Default::default()
                }) { em }
                text(style = StyleRules {
                    font_size: Some(px(12.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { cnt.to_string() }
            }
            .on_touch(move |ev| {
                // LIVE rows route the tap through the backend RPC: `request_live_reaction`
                // persists the row and broadcasts a count frame that updates the pill map
                // (for THIS client via its own echo AND every other connected member). No
                // optimistic mutation — the echo is the single source of truth, so a cold
                // reload and a live sync converge identically. MOCK rows keep the direct
                // session-local toggle: not-yet-reacted -> count+1 and highlight; already
                // reacted -> count-1 and un-highlight, dropping the pill at 0.
                if ev.phase == TouchPhase::Ended {
                    if live_key {
                        request_live_reaction(key, em_close.clone(), will_remove, picker_l);
                    } else {
                        let mut m = r.borrow_mut();
                        let inner = m.entry(key).or_default();
                        if let Some(pos) = inner.iter().position(|(e, _, _)| e == &em_close) {
                            let (e, c, was_reacted) = inner[pos].clone();
                            if was_reacted {
                                if c <= 1 {
                                    inner.remove(pos);
                                } else {
                                    inner[pos] = (e, c - 1, false);
                                }
                            } else {
                                inner[pos] = (e, c + 1, true);
                            }
                        }
                        if m.get(&key).map_or(false, |v| v.is_empty()) {
                            m.remove(&key);
                        }
                        rev.set(rev.get() + 1);
                    }
                }
                TouchResponse::CONSUMED
            })
        }
    }).collect();

    // The always-visible "add reaction" button: a small circular glyph target
    // (a `view` + icon, the composer icon-button shape) whose `.on_touch` opens
    // the picker for this message, keyed by the row's `MessageKey`.
    let add = picker;
    let add_key = key;
    let add_button = ui! {
        view(style = StyleRules {
            position: Some(Position::Relative),
            width: Some(px(26.0)),
            height: Some(px(26.0)),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::Center),
            background: Some(col(COMPOSER)),
            border_top_left_radius: Some(px(13.0)),
            border_top_right_radius: Some(px(13.0)),
            border_bottom_left_radius: Some(px(13.0)),
            border_bottom_right_radius: Some(px(13.0)),
            ..Default::default()
        }) {
            icon(SMILE_PLUS).color(|| Color::from(TEXT_MUTED)).size(15.0)
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                add.set(Some(add_key));
            }
            TouchResponse::CONSUMED
        })
    };

    // The strip: pills left, spacer, add button right. `padding_left` of 66
    // aligns it with the message body (row padding 16 + avatar 40 + gap 10) so
    // the pills sit under the text, Discord-style.
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(4.0)),
            padding_top: Some(px(2.0)),
            padding_left: Some(px(66.0)),
            padding_right: Some(px(16.0)),
            ..Default::default()
        }) {
            pills
            view(style = StyleRules {
                flex_grow: Some(grow(1.0)),
                ..Default::default()
            }) {}
            add_button
        }
    }
}

/// The add-reaction picker: a full-window overlay (opaque backdrop + centered
/// emoji grid) shown while `picker` names a message. Tapping a glyph adds a
/// reaction for that message and closes the picker; tapping the backdrop closes
/// it without adding. Reuses the overlay idiom (a `Position::Absolute` child of
/// the root `Relative` ancestor, painted last like `profile_region` /
/// `notif_region`) and the composer's emoji grid.
fn reaction_picker_overlay(
    picker: Signal<Option<MessageKey>>,
    reactions: Rc<RefCell<ReactionMap>>,
    reaction_rev: Signal<u32>,
) -> Element {
    switch(
        move || picker.get(),
        move |p: &Option<MessageKey>| {
            let Some(sel) = p else {
                return ui! {
                    view(style = StyleRules {
                        width: Some(px(0.0)),
                        height: Some(px(0.0)),
                        ..Default::default()
                    }) {}
                };
            };
            let key = *sel;
            // Add (or bump) the picked emoji's reaction for this message, then
            // close the picker. LIVE rows (gateway-keyed) route through the
            // backend RPC (`request_live_reaction`) which persists a row and
            // broadcasts the count frame back into `gateway_incoming_effect` —
            // the pill is NOT mutated optimistically, the echo frame is the
            // single source of truth. MOCK rows keep the session-local mutation.
            let r = reactions.clone();
            let rev = reaction_rev;
            let close = picker;
            let pick_cb: Rc<dyn Fn(String)> = Rc::new(move |glyph: String| {
                let live = matches!(key, MessageKey::Live { .. });
                if live {
                    request_live_reaction(key, glyph, false, close);
                } else {
                    let mut m = r.borrow_mut();
                    let inner = m.entry(key).or_default();
                    if let Some(pos) = inner.iter().position(|(e, _, _)| e == &glyph) {
                        let (e, c, _) = inner[pos].clone();
                        inner[pos] = (e, c + 1, true);
                    } else {
                        inner.push((glyph, 1, true));
                    }
                    rev.set(rev.get() + 1);
                    close.set(None);
                }
            });
            // Opaque full-window backdrop (dismiss-on-outside-tap) with the full
            // emoji picker card centered.
            let backdrop_close = picker;
            ui! {
                view(style = StyleRules {
                    position: Some(Position::Absolute),
                    left: Some(px(0.0)),
                    top: Some(px(0.0)),
                    width: Some(pct(100.0)),
                    height: Some(pct(100.0)),
                    background: Some(col(BG_RAIL)),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    ..Default::default()
                }) {
                    view(style = StyleRules {
                        background: Some(col(BG_ELEVATED)),
                        border_top_left_radius: Some(px(10.0)),
                        border_top_right_radius: Some(px(10.0)),
                        border_bottom_left_radius: Some(px(10.0)),
                        border_bottom_right_radius: Some(px(10.0)),
                        overflow: Some(Overflow::Hidden),
                        ..Default::default()
                    }) {
                        EmojiPicker(on_pick = Some(pick_cb))
                    }
                }
                .on_touch(move |ev| {
                    // Tap on the backdrop (not a glyph) closes the picker.
                    if ev.phase == TouchPhase::Ended {
                        backdrop_close.set(None);
                    }
                    TouchResponse::CONSUMED
                })
            }
        },
    )
}


/// One LIVE (gateway-fed) message row: the `message_row_with_actions` content
/// (the hover `MessageActionBar` + react / reply / pin / translate, exactly
/// like a mock row), plus — for the signed-in user's own messages — the
/// Edit / Delete affordance (inside the overflow menu, hover-gated with the
/// rest) and the inline edit input. Once Edit mode or delete-confirm is
/// actually triggered their Save/Cancel/Delete and Confirm/No rows stay
/// always-visible. The hover bar handles the shared react/reply/pin/translate
/// actions.
///
/// `live_editing` / `live_delete_confirm` / `live_edit_text` are
/// `app_shell`-scope signals that drive per-row edit/delete mode; `gateway` is
/// a `Copy` `UseSocket` handle for sending `EditMessage` / `DeleteMessage`.
/// `key` is the row's `MessageKey` (the real wire `message_id` under `Live`),
/// plus the shared `picker` / `reply_ctx` / `pin_visible` / `translate_key`
/// handles threaded from `app_shell`. The row rebuilds when its state changes
/// because the message-list `switch`'s key folds these signals in (see
/// `sent_messages`).
fn live_edit_row(
    message_id: i64,
    author: &str,
    color: &str,
    body: &str,
    when: String,
    full: String,
    edited: bool,
    is_mine: bool,
    grouped: bool,
    live_editing: Signal<Option<i64>>,
    live_edit_text: Signal<String>,
    live_delete_confirm: Signal<Option<i64>>,
    gateway: server::UseSocket<
        discord_server::server_settings::WsServerMsg,
        discord_server::server_settings::WsClientMsg,
    >,
    // Interaction parity with the mock path: the row's identity key (the real
    // wire `message_id` under the `Live` variant) plus the shared picker /
    // reply / pin / translate handles, so the hover action bar and the
    // overflow-menu actions behave identically to a mock row.
    key: MessageKey,
    picker: Signal<Option<MessageKey>>,
    reply_ctx: Signal<Option<(String, String)>>,
    reply_target: Signal<Option<i64>>,
    pin_visible: Signal<bool>,
    on_pin: Rc<dyn Fn()>,
    translate_key: Signal<Option<MessageKey>>,
) -> Element {
    // Owner-only overflow-menu entries (Edit / Delete) — the same actions the
    // old always-visible inline Edit/Delete buttons did, now living inside the
    // one hover-gated overflow menu (`message_overflow_menu` renders them as
    // its last two rows when present). Built before the base row so the menu
    // can carry them; `None` for other authors' rows, whose menu keeps only
    // the shared entries. The backend enforces the same ownership — this is
    // the affordance gate (the previous inline buttons used the identical
    // `is_mine` check).
    let (menu_edit, menu_delete): (Option<Rc<dyn Fn()>>, Option<Rc<dyn Fn()>>) = if is_mine {
        let seed = body.to_string();
        let editing_sig = live_editing;
        let edit_text = live_edit_text;
        let confirm_sig = live_delete_confirm;
        let mid = message_id;
        (
            Some(Rc::new(move || {
                // Enter edit mode: seed the draft with the current content and
                // flag this message id; the list rebuild (keyed on
                // `live_editing`) mounts the input for this row.
                editing_sig.set(Some(mid));
                edit_text.set(seed.clone());
            })),
            Some(Rc::new(move || {
                confirm_sig.set(Some(mid));
            })),
        )
    } else {
        (None, None)
    };
    // The base row carries the hover `MessageActionBar` + overflow menu
    // (react / reply / pin / translate — plus Edit / Delete above when
    // `is_mine`) exactly like the mock path. The only differences are the
    // real `(edited)` marker (gated on the message's `edited_at` state) and
    // the live `MessageKey`; when `grouped` it is the headerless condensed
    // form (see `message_row`).
    let base = message_row_with_actions(
        author,
        color,
        body,
        when,
        full,
        edited,
        grouped,
        key,
        picker,
        reply_ctx,
        reply_target,
        pin_visible,
        translate_key,
        on_pin,
        menu_edit,
        menu_delete,
    );
    if !is_mine {
        return base;
    }
    let mid = message_id;
    let editing_sig = live_editing;
    let edit_text = live_edit_text;
    let confirm_sig = live_delete_confirm;
    // --- inline edit input (reuses the composer's `text_input` idiom) -------
    // `text_input` takes a reactive `Signal<String>` for `value`, so the draft
    // binds straight to `live_edit_text`. `value` only drives the NATIVE field
    // from the signal; keystrokes report back through `on_change`, which must
    // write the typed text into `live_edit_text` or Save would read the seed
    // instead (the "edit doesn't stick" bug). Cyclic but stable, same idiom as
    // the composer's `on_change = update_on_type`. `autofocus = true` focuses
    // the field at MOUNT — the input mounts when `live_editing` flips to this
    // row (the list `switch` keys on it), so clicking Edit puts the caret in
    // the seeded text with no extra click. Confirmation is the explicit Save
    // button, plus Enter on the input (Enter already submits in the composer
    // and the password fields, so it submits here too); the input is seeded
    // with the current content on Save-cancel paths by the list rebuild.
    let on_edit_type = move |s: String| { edit_text.set(s); };
    // --- Save (shared by the Save pill and Enter on the edit input) --------
    // Reads the DRAFT signal, which the input's `on_change` keeps in sync with
    // the native field.
    let do_save = move || {
        let content = edit_text.get();
        if !content.trim().is_empty() {
            gateway.send(
                discord_server::server_settings::WsClientMsg::EditMessage {
                    message_id: mid,
                    content,
                },
            );
        }
        editing_sig.set(None);
        edit_text.set(String::new());
    };
    let save_fn: Rc<dyn Fn()> = Rc::new(do_save);
    // `Rc` is not `Copy`: the `small_btn` handlers are `Fn` closures that
    // move-capture and can't call it on every press, so the Save pill arm
    // keeps its own Copy-only inline body; this `Rc` serves the Enter handler.
    let save_for_enter = save_fn.clone();
    let on_enter = move |ev: &KeyEvent| {
        if ev.key == "Enter" && !ev.shift && !ev.ctrl && !ev.alt && !ev.meta {
            (save_for_enter)();
            KeyOutcome::PreventDefault
        } else {
            KeyOutcome::Default
        }
    };
    let edit_input = {
        let input_style = StyleRules {
            font_size: Some(px(15.0)),
            color: Some(col(TEXT)),
            background: Some(col(COMPOSER)),
            padding_top: Some(px(6.0)),
            padding_bottom: Some(px(6.0)),
            padding_left: Some(px(10.0)),
            padding_right: Some(px(10.0)),
            border_top_left_radius: Some(px(8.0)),
            border_top_right_radius: Some(px(8.0)),
            border_bottom_left_radius: Some(px(8.0)),
            border_bottom_right_radius: Some(px(8.0)),
            flex_grow: Some(grow(1.0)),
            ..borderless_input()
        };
        ui! {
            text_input(value = live_edit_text, autofocus = true, on_change = on_edit_type, placeholder = "Edit message", style = input_style, test_id = "edit-input")
                .on_key_down(on_enter)
        }
    };
    // --- per-row control buttons ------------------------------------------
    // Compact text pills for the inline edit/confirm controls. Each button
    // carries a `MsgAction` the shared `on_touch` handler matches on — the
    // action is owned per-button so the closure captures nothing borrowed
    // (avoids the `&str`-lifetime trap of a label-matching closure). Edit and
    // Delete live in the '...' overflow menu, not here.
    #[derive(Clone, Copy, PartialEq)]
    enum MsgAction {
        Delete,
        Save,
        Cancel,
        Confirm,
        No,
    }
    let small_btn: Rc<dyn Fn(MsgAction, &str, &'static str, &'static str) -> Element> =
        Rc::new(move |action: MsgAction, label: &str, bg: &'static str, test_id: &'static str| -> Element {
        let s = StyleRules {
            padding_top: Some(px(4.0)),
            padding_bottom: Some(px(4.0)),
            padding_left: Some(px(10.0)),
            padding_right: Some(px(10.0)),
            background: Some(col(bg)),
            border_top_left_radius: Some(px(8.0)),
            border_top_right_radius: Some(px(8.0)),
            border_bottom_left_radius: Some(px(8.0)),
            border_bottom_right_radius: Some(px(8.0)),
            flex_shrink: Some(grow(0.0)),
            font_size: Some(px(12.0)),
            font_weight: Some(FontWeight::SemiBold),
            color: Some(col(TEXT)),
            ..Default::default()
        };
        let label_style = StyleRules {
            font_size: Some(px(12.0)),
            font_weight: Some(FontWeight::SemiBold),
            color: Some(col(TEXT)),
            ..Default::default()
        };
        let touch = move |ev: &runtime_shared::TouchEvent| {
            if ev.phase != TouchPhase::Ended {
                return TouchResponse::CONSUMED;
            }
            match action {
                MsgAction::Delete => confirm_sig.set(Some(mid)),
                // Same action as Save-from-Enter (`save_for_enter` below); kept
                // inline here because `save_fn` is a non-`Copy` `Rc` that an
                // `Fn` touch handler can't move out of on every press, while
                // every handle this arm touches is `Copy`.
                MsgAction::Save => {
                    let content = edit_text.get();
                    if !content.trim().is_empty() {
                        gateway.send(
                            discord_server::server_settings::WsClientMsg::EditMessage {
                                message_id: mid,
                                content,
                            },
                        );
                    }
                    editing_sig.set(None);
                    edit_text.set(String::new());
                },
                MsgAction::Cancel => {
                    editing_sig.set(None);
                    edit_text.set(String::new());
                }
                MsgAction::Confirm => {
                    gateway.send(discord_server::server_settings::WsClientMsg::DeleteMessage {
                        message_id: mid,
                    });
                    confirm_sig.set(None);
                }
                MsgAction::No => confirm_sig.set(None),
            }
            TouchResponse::CONSUMED
        };
        ui! {
            view(style = s) {
                text(style = label_style) { label }
            }
            .on_touch(touch)
            .test_id(test_id)
        }
    });
    // Two mutually-exclusive per-row control states, each rendered beneath the
    // base row in place of nothing else: the edit input (Save / Cancel / Delete)
    // and the delete-confirm strip (Delete this message? / Confirm / No). Edit
    // and Delete are entered from the '...' overflow menu, not inline — see
    // `message_overflow_menu`. The idle row has no inline controls.
    let edit_mode = editing_sig.get() == Some(mid);
    let delete_mode = confirm_sig.get() == Some(mid);
    let save_btn = small_btn(MsgAction::Save, "Save", BUTTON, "save-edit");
    let cancel_btn = small_btn(MsgAction::Cancel, "Cancel", COMPOSER, "cancel-edit");
    let edit_delete_btn = small_btn(MsgAction::Delete, "Delete", BADGE, "row-delete");
    let confirm_btn = small_btn(MsgAction::Confirm, "Confirm", BADGE, "confirm-delete");
    let no_btn = small_btn(MsgAction::No, "No", COMPOSER, "cancel-delete");
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            gap: Some(px(6.0)),
            ..Default::default()
        }) {
            { base }
            if edit_mode {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Column),
                    gap: Some(px(4.0)),
                    ..Default::default()
                }) {
                    { edit_input }
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Row),
                        gap: Some(px(6.0)),
                        ..Default::default()
                    }) {
                        { save_btn }
                        { cancel_btn }
                        { edit_delete_btn }
                    }
                }
            } else if delete_mode {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    gap: Some(px(6.0)),
                    ..Default::default()
                }) {
                    text(style = StyleRules {
                        font_size: Some(px(12.0)),
                        color: Some(col(TEXT_MUTED)),
                        flex_grow: Some(grow(1.0)),
                        ..Default::default()
                    }) { "Delete this message?".to_string() }
                    { confirm_btn }
                    { no_btn }
                }
            }
        }
    }
}

/// One member row: avatar chip with the status dot overlaid on its
/// bottom-right corner, then the name — tinted by the member's role color
/// (Discord colors the username by role; there is no separate role badge in
/// the member list). Compact and fixed-width — fits cleanly inside the 240px
/// member panel column. Tapping the row opens the member profile
/// overlay (`member_profile_panel`), which renders the profile-level
/// components for the selected member.
fn member_row(m: &Member, selected: Signal<Option<&'static str>>) -> Element {
    let dim = m.status == Status::Offline;
    let row_style = StyleRules {
        padding_top: Some(px(4.0)),
        padding_bottom: Some(px(4.0)),
        padding_left: Some(px(10.0)),
        padding_right: Some(px(12.0)),
        gap: Some(px(10.0)),
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        flex_shrink: Some(grow(0.0)),
        overflow: Some(Overflow::Hidden),
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
    // The member's own profile carries the role -> color mapping the row
    // uses to tint the username by role (Discord colors the name rather than
    // showing a separate badge). Offline members stay muted.
    let prof = member_profile(m.name);
    let name_style = StyleRules {
        font_size: Some(px(15.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(if dim { TEXT_MUTED } else { prof.role_color })),
        ..Default::default()
    };
    let initial = m.name.chars().next().unwrap_or(' ').to_string();
    let sel = selected.clone();
    let name_key: &'static str = m.name;
    // Per-server role badge: the member panel exists to show who holds which
    // role, so each row carries its `m.role` as a small chip to the right of
    // the name (the live panel surfaces the same data as a tappable
    // cycle-button from the backend `MemberInfo`).
    let role_chip_style = StyleRules {
        margin_left: Some(px(4.0)),
        padding_top: Some(px(1.0)),
        padding_bottom: Some(px(1.0)),
        padding_left: Some(px(6.0)),
        padding_right: Some(px(6.0)),
        background: Some(col(BG_HOVER)),
        border_top_left_radius: Some(px(6.0)),
        border_top_right_radius: Some(px(6.0)),
        border_bottom_left_radius: Some(px(6.0)),
        border_bottom_right_radius: Some(px(6.0)),
        ..Default::default()
    };
    let role_chip_text_style = StyleRules {
        font_size: Some(px(11.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT_MUTED)),
        ..Default::default()
    };
    let role_label = m.role.to_string();
    ui! {
        view(style = row_style) {
            view(style = avatar_style) {
                text(style = initial_style) { initial }
                view(style = dot_style) {}
            }
            text(style = name_style) { m.name.to_string() }
            view(style = role_chip_style) {
                text(style = role_chip_text_style) { role_label }
            }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                sel.set(Some(name_key));
            }
            TouchResponse::CONSUMED
        })
    }
}

/// The member profile panel: a full-screen overlay showing the
/// profile-level components for the currently-selected member.
/// Uses `switch` keyed on the selected member name so the header
/// updates when a different member is chosen. Tapping the "Close"
/// button (or the backdrop) sets the signal back to `None`.
fn member_profile_panel(selected: Signal<Option<&'static str>>) -> Element {
    switch(
        move || selected.get(),
        move |sel: &Option<&'static str>| {
            if sel.is_none() {
                // Zero-size invisible wrapper (same collapse idiom as the
                // Profile / Notification overlays when closed).
                ui! {
                    view(style = StyleRules {
                        position: Some(Position::Absolute),
                        left: Some(px(0.0)),
                        top: Some(px(0.0)),
                        width: Some(px(0.0)),
                        height: Some(px(0.0)),
                        overflow: Some(Overflow::Hidden),
                        ..Default::default()
                    }) {}
                }
            } else {
                let name = sel.unwrap().to_string();
                // Per-member display data (activity, role, bio, …) keyed by
                // name — distinct per member so no two profiles render
                // identically. Unknown names fall back to the default profile.
                let prof = member_profile(&name);
                let close = selected.clone();
                let header_close = selected.clone();
                let header_style = StyleRules {
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::SpaceBetween),
                    padding_top: Some(px(16.0)),
                    padding_bottom: Some(px(16.0)),
                    padding_left: Some(px(16.0)),
                    padding_right: Some(px(16.0)),
                    flex_shrink: Some(grow(0.0)),
                    ..Default::default()
                };
                let header = ui! {
                    view(style = header_style) {
                        text(style = StyleRules {
                            font_size: Some(px(18.0)),
                            font_weight: Some(FontWeight::Bold),
                            color: Some(col(TEXT)),
                            ..Default::default()
                        }) { format!("Profile — {}", name) }
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
                            flex_shrink: Some(grow(0.0)),
                            ..Default::default()
                        }) {
                            text(style = StyleRules {
                                color: Some(col(TEXT)),
                                font_size: Some(px(12.0)),
                                font_weight: Some(FontWeight::SemiBold),
                                ..Default::default()
                            }) { "Close".to_string() }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                header_close.set(None);
                            }
                            TouchResponse::CONSUMED
                        })
                    }
                };
                let content = ui! {
                    view(style = StyleRules {
                        flex_grow: Some(grow(1.0)),
                        background: Some(col(BG_MAIN)),
                        flex_direction: Some(FlexDirection::Column),
                        overflow: Some(Overflow::Hidden),
                        padding_top: Some(px(8.0)),
                        padding_bottom: Some(px(16.0)),
                        padding_left: Some(px(16.0)),
                        padding_right: Some(px(16.0)),
                        gap: Some(px(12.0)),
                        ..Default::default()
                    }) {
                        user_bio_block(prof.bio.to_string().into())
                        NicknameEditField(default_nickname = prof.nickname.to_string())
                    }
                };
                ui! {
                    view(style = StyleRules {
                        position: Some(Position::Absolute),
                        left: Some(px(0.0)),
                        top: Some(px(0.0)),
                        width: Some(pct(100.0)),
                        height: Some(pct(100.0)),
                        background: Some(col(BG_MAIN)),
                        flex_direction: Some(FlexDirection::Column),
                        overflow: Some(Overflow::Hidden),
                        ..Default::default()
                    }) {
                        header
                        content
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            close.set(None);
                        }
                        TouchResponse::CONSUMED
                    })
                }
            }
        },
    )
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
                if joined.get() == Some((server_id, v.id)) {
                    ConnectionQualityIndicator()
                    ScreenShareIndicator()
                    LatencyBadge()
                }
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
        // Voice controls (join trigger + volume + soundboard) — a compact
        // stack that appears only while the user is joined to this channel,
        // sitting just beneath the row. All local/mock, no backend call.
        if joined_voice.get() == Some((server_id, v.id)) {
            out.push(ui! {
                view(style = StyleRules {
                    padding_top: Some(px(4.0)),
                    padding_bottom: Some(px(4.0)),
                    padding_left: Some(px(44.0)),
                    padding_right: Some(px(8.0)),
                    flex_direction: Some(FlexDirection::Column),
                    gap: Some(px(8.0)),
                    ..Default::default()
                }) {
                    VoiceJoinButton()
                    VolumeSlider()
                    SoundboardButton()
                    MicSelector()
                }
            });
        }
        // "VOICE — N CONNECTED" section header, shown only while joined — the
        // count is the real connected set rendered beneath (the channel's
        // speakers plus your own row), not a hardcoded mock. Matches the
        // member-panel group-label "LABEL — N" idiom.
        if joined_voice.get() == Some((server_id, v.id)) {
            let connected = v.speakers.len() + 1;
            let hdr_style = StyleRules {
                padding_top: Some(px(4.0)),
                padding_left: Some(px(12.0)),
                padding_right: Some(px(8.0)),
                ..Default::default()
            };
            let hdr_text_style = StyleRules {
                font_size: Some(px(11.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(TEXT_MUTED)),
                ..Default::default()
            };
            out.push(ui! {
                view(style = hdr_style) {
                    text(style = hdr_text_style) { format!("VOICE — {} CONNECTED", connected) }
                }
            });
        }
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

/// The member-list panel for the right side of the window: members grouped
/// by status (online → idle → DND → offline). `user` is the signed-in user
/// rendered as a regular member entry under their *live* presence status —
/// cycling the status in the profile strip regroups them here (the panel
/// is rebuilt on `user_status` changes; see `members_switch`). Spliced in
/// as the right-most column for whichever server is active.
fn member_panel(
    server: &crate::data::Server,
    user: &Member,
    selected: Signal<Option<&'static str>>,
    width: Signal<f32>,
    sidebar: Signal<f32>,
    search_corpus: ReadSignal<Vec<(String, String)>>,
) -> Element {
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
    // Message-search bar lives at the top of the right member panel (Discord's
    // "far right" position) instead of in the channel header row. Wrap it in a
    // 12px-padded `view` so it isn't flush against the panel's edges — the bar's
    // own input is flex-sized to this width, so 240px − 24px padding (216px) fits
    // comfortably (the bar's fixed 288px width used to overflow the header's
    // 180px slot and would clip the 240px panel).
    body.push(
        ui! {
            view(style = StyleRules {
                padding_top: Some(px(12.0)),
                padding_bottom: Some(px(12.0)),
                padding_left: Some(px(12.0)),
                padding_right: Some(px(12.0)),
                ..Default::default()
            }) {
                MessageSearchBar(corpus = search_corpus)
            }
        },
    );
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
            ..section_label()
        };
        body.push(ui! { text(style = group_style) { group_label } });
        for m in group.iter() {
            body.push(member_row(m, selected.clone()));
        }
        if with_user {
            body.push(member_row(user, selected.clone()));
        }
    }

    // Section wrapper: right-side member-list panel. Its width is driven by
    // the `width` signal (a thin drag handle on its left edge resizes it);
    // the left border separates it from the main view; the panel owns its own
    // bg. A rules *closure* so the width re-resolves as the signal changes.
    let panel_style = move || {
        let (_s, m) = effective_panel_widths(sidebar, width);
        StyleRules {
            width: Some(px(m)),
            // flex_shrink:0 so the flex row never shrinks the member panel below
            // the viewport-responsive width computed above — when the viewport
            // is wide enough this keeps the user's manual drag width exact.
            flex_shrink: Some(grow(0.0)),
            background: Some(col(BG_SIDEBAR)),
            border_left_width: Some(border_width(1.0)),
            border_left_color: Some(col(BORDER)),
            flex_direction: Some(FlexDirection::Column),
            overflow: Some(Overflow::Hidden),
            ..Default::default()
        }
    };
    ui! {
        view(style = panel_style) {
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
/// the avatar + name/status, row 2 is a control row — the mic-mute, deafen,
/// settings-gear, and keyboard-shortcuts ('?') icon buttons (Discord's
/// user-panel actions) on the left, the Leave button (voice only) and the
/// Profile / Log Out pills on the right.
fn user_panel(
    user_status: Signal<Status>,
    joined_voice: Signal<Option<(u32, u32)>>,
    authed: Signal<bool>,
    profile_mode: Signal<bool>,
    notif_mode: Signal<bool>,
    shortcuts_mode: Signal<bool>,
    muted: Signal<bool>,
    deafened: Signal<bool>,
) -> Element {
    switch(
        move || (user_status.get(), voice_name_for(joined_voice.get())),
        move |(status, voice_name): &(Status, Option<String>)| {
            let user = USER;
            // Real signed-in identity: when genuinely logged in (not guest), the
            // backend's username was stored in `CURRENT_USERNAME` at login and
            // outlives this screen. Guest mode leaves it empty, so the strip
            // falls back to the mock `USER` identity ("Nova") there. The value
            // is set before `authed` flips (login completes first), so a plain
            // read here is stable for the whole mount — no reactivity needed.
            let real_name = crate::auth::current_username();
            // When the signed-in account has an uploaded avatar (shared
            // client registry, fed by login / `list_members`), the profile
            // strip's chip draws the image instead of the initial. Read
            // before `real_name` moves into the display-name branch below.
            let avatar_img = crate::auth::avatar_for(real_name.as_str());
            let display_name = if !real_name.is_empty() {
                real_name
            } else {
                user.name.to_string()
            };
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
            let initial = display_name.chars().next().unwrap_or(' ').to_string();
            let name = display_name;

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
                        padding_left: Some(px(6.0)),
                        padding_right: Some(px(6.0)),
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
            // When the signed-in account has an uploaded avatar, the image
            // sits under the status dot (same 32px circle, cover-cropped);
            // the initial-circle branch renders exactly as before.
            let avatar = if let Some(url) = avatar_img {
                ui! {
                    view(style = StyleRules {
                        width: Some(px(32.0)),
                        height: Some(px(32.0)),
                        position: Some(Position::Relative),
                        flex_shrink: Some(grow(0.0)),
                        border_top_left_radius: Some(px(16.0)),
                        border_top_right_radius: Some(px(16.0)),
                        border_bottom_left_radius: Some(px(16.0)),
                        border_bottom_right_radius: Some(px(16.0)),
                        ..Default::default()
                    }) {
                        image(
                            src = crate::auth::media_url(url.clone()),
                            alt = "Your avatar",
                            style = StyleRules {
                                width: Some(px(32.0)),
                                height: Some(px(32.0)),
                                flex_shrink: Some(grow(0.0)),
                                object_fit: Some(ObjectFit::Cover),
                                border_top_left_radius: Some(px(16.0)),
                                border_top_right_radius: Some(px(16.0)),
                                border_bottom_left_radius: Some(px(16.0)),
                                border_bottom_right_radius: Some(px(16.0)),
                                ..Default::default()
                            },
                        )
                        view(style = dot_style) {}
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            let touch = user_status;
                            touch.set(touch.get().next());
                        }
                        TouchResponse::CONSUMED
                    })
                }
            } else {
                ui! {
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
                }
            };
            // Log Out: flips the shared `authed` signal back to false, which
            // unmounts this tree and re-mounts the login screen (the `switch`
            // key in `app()` re-evaluates on it). Same view+on_touch pill the
            // Leave button uses, so it fits the row's styling.
            // Compact pill: 6px padding + 11px type. This row now also holds
            // the mic/deafen/settings icons, so the old 8px padding (which
            // only had to fit avatar + name + 2 pills) no longer fits — the
            // tight 6px is part of the width budget in
            // `controls_row_fits_224px_sidebar`.
            let logout_style = StyleRules {
                height: Some(px(24.0)),
                padding_left: Some(px(6.0)),
                padding_right: Some(px(6.0)),
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
                        // Drop the persisted session so a refresh after logout
                        // doesn't silently restore it, and clear the one-shot
                        // note so this (a real logout) shows a clean form — no
                        // spurious "session expired".
                        crate::auth::clear_session();
                        // Ask the backend to clear the httpOnly session cookie
                        // too — both halves are needed: storage keys gone AND
                        // cookie gone, or a reload would replay the dead
                        // session.
                        crate::auth::request_logout();
                        *SESSION_NOTE
                            .lock()
                            .unwrap_or_else(|p| p.into_inner()) = None;
                        logout.set(false);
                    }
                    TouchResponse::CONSUMED
                })
            };
            // Profile Settings entry: a "Profile" pill (styled like Log Out)
            // that opens the full-screen `profile_settings` screen. `app_shell`
            // owns `profile_mode` and renders the overlay when it flips on.
            let profile_open = profile_mode.clone();
            // Compact like Log Out (6px padding): the three pills plus the
            // three icon buttons must all fit the 224px control row — see
            // `controls_row_fits_224px_sidebar`.
            let profile_btn = ui! {
                view(style = StyleRules {
                    height: Some(px(24.0)),
                    padding_left: Some(px(6.0)),
                    padding_right: Some(px(6.0)),
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
            // 18px (not 20) + a 6px row gap: the control row must fit all
            // three icons AND the Profile / Log Out / Leave pills in the
            // 224px the 240px sidebar leaves after the panel's 8px side
            // padding — see `controls_row_fits_224px_sidebar` for the
            // arithmetic. 20px icons + 8px gaps overflowed it and clipped
            // Log Out (sidebar `Overflow::Hidden`), which was the regression.
            let icon_btn_style = StyleRules {
                width: Some(px(18.0)),
                height: Some(px(18.0)),
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
            // Settings gear: opens the full-screen Notification Settings
            // overlay (the same build-once/toggle idiom as the Profile pill).
            let notif_open = notif_mode.clone();
            let settings_btn = ui! {
                view(style = icon_btn_style.clone(), test_id = "user-settings") {
                    icon(SETTINGS).color(|| Color::from(TEXT_MUTED)).size(15.0)
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        notif_open.set(true);
                    }
                    TouchResponse::CONSUMED
                })
            };
            // Keyboard shortcuts: a '?' icon next to the gear opens the
            // full-screen shortcuts overlay (same idiom as the gear).
            let shortcuts_open = shortcuts_mode.clone();
            let shortcuts_btn = ui! {
                view(style = icon_btn_style.clone(), test_id = "user-shortcuts") {
                    icon(CIRCLE_HELP).color(|| Color::from(TEXT_MUTED)).size(15.0)
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        shortcuts_open.set(true);
                    }
                    TouchResponse::CONSUMED
                })
            };
            // Idle toggle: a `MOON` icon that flips the user's live presence
            // status to `Status::Idle` (and cycles on to clear it when already
            // idle). It is the status/voice control the profile strip's avatar
            // click implements, exposed as a one-tap icon here with the mic and
            // deafen. It reads/writes the SAME `user_status` signal the avatar
            // uses, so toggling it regroups the user in the member list and
            // updates the strip's status line. Driven through `idle_timer_badge`
            // (the component's correct home is this control row, not the
            // message list).
            let idle_status = user_status.clone();
            let idle_btn = idle_timer_badge(idle_status);
            // Row 2: the icon buttons left, the pills (plus Leave, voice only)
            // pushed to the right. Its own gap (6px, not the identity row's
            // 8px) is deliberate — the extra 2px per gap is part of what lets
            // all three icons plus up to three pills fit the 224px; the
            // identity row above keeps the roomier 8px.
            let controls_row_style = StyleRules {
                min_height: Some(px(24.0)),
                gap: Some(px(6.0)),
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                ..Default::default()
            };
            let controls_row = ui! {
                view(style = controls_row_style) {
                    mic_btn
                    deafen_btn
                    settings_btn
                    shortcuts_btn
                    idle_btn
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

/// Build a mock server created at runtime by the "Add a server" (+) flow.
///
/// The data model is `&'static`-based (every `Server`/`Category`/channel field
/// is a static reference), so a server created while the app is running cannot
/// live on the heap the usual way. The owned values are therefore **leaked** to
/// a stable address and handed back as `&'static` references — the same trick
/// the mock seed data in `data.rs` relies on. In a real build this would be a
/// network call; the leak is a deliberate consequence of the static model and
/// is harmless at this scale (the user can only create a handful of servers).
///
/// The new server gets a single default `general` text channel (empty) and the
/// signed-in user as its only member, so the sidebar, header, and member panel
/// all render without special-casing.
fn create_server(
    name: &str,
    next_id: &Cell<u32>,
    dynamic_servers: &Rc<RefCell<HashMap<u32, &'static Server>>>,
) -> u32 {
    let name = if name.is_empty() { "new-server" } else { name };
    let initial: &str = if name.is_empty() {
        "N"
    } else {
        // Leak the upper-cased first letter so it too is a `&'static str`.
        let upper: String = name[..1].to_uppercase();
        Box::leak(upper.into_boxed_str())
    };
    let name_static: &'static str = Box::leak(name.to_string().into_boxed_str());
    let id = next_id.get();
    next_id.set(id + 1);
    let cat_id = id * 1000;
    let chan_id = id * 1000 + 1;
    let chan = Box::leak(Box::new(TextChannel {
        id: chan_id,
        name: "general",
        topic: "",
        // A freshly created channel has no slowmode yet — `None` keeps the
        // header's `SlowmodeIndicator` from showing on it.
        slowmode_secs: None,
        messages: &[],
        unread: 0,
        mentions: 0,
    }));
    let category = Box::leak(Box::new(Category {
        id: cat_id,
        title: "TEXT CHANNELS",
        text: std::slice::from_ref(chan),
        voice: &[],
    }));
    let member = Box::leak(Box::new(Member {
        name: USER.name,
        color: USER.color,
        status: Status::Online,
        // The creator owns the server they just made.
        role: "Owner",
    }));
    let server = Box::leak(Box::new(Server {
        id,
        name: name_static,
        initial,
        accent: BUTTON,
        categories: std::slice::from_ref(category),
        members: std::slice::from_ref(member),
    }));
    dynamic_servers.borrow_mut().insert(id, server);
    id
}

// --- mock login screen -------------------------------------------------------

/// The login screen, shown while `authed` is false. A username + password
/// form with two modes toggled by `mode`: sign in (the default) and sign up.
/// The password field is `secure` (masks input like a real login); empty
/// fields show an inline error. On success — a real backend login or signup —
/// `authed` flips and this screen unmounts. "Continue as guest" skips the
/// fields entirely. State is local to this function — it unmounts the moment
/// `authed` flips, so no session survives a reload.
fn login_screen(authed: Signal<bool>, busy: Signal<bool>) -> Element {
    let _ = &busy; // threaded for the submit button + handlers' guard (below).
    // "you" is the only identity in this mock build; authing in just flips
    // the shared signal the app tree is keyed on.
    // `mode` flips the form between sign-in (false, the default) and sign-up
    // (true); both reuse the same username + password fields. `error` holds
    // the inline message for a failed submit in either mode.
    let username: Signal<String> = signal(String::new());
    let password: Signal<String> = signal(String::new());
    let error: Signal<String> = signal(String::new());
    let mode: Signal<bool> = signal(false);
    // If a persisted session was just dropped as stale (the app shell cleared
    // the note before flipping us back here), surface it immediately instead
    // of a blank form. One-shot: it's `None` on a real logout or a first-time
    // visit, so those render cleanly.
    if let Some(note) = SESSION_NOTE
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .take()
    {
        error.set(note);
    }

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
        ..borderless_input()
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
        let busy = busy.clone();
        move || {
            let em = username.get();
            let pw = password.get();
            if em.trim().is_empty() || pw.trim().is_empty() {
                error.set("Username and password are required.".to_string());
                return;
            }
            // Double-submit guard: a login is already in flight (covers the
            // Enter-key path, which bypasses the button's own press blocking).
            if busy.get() {
                return;
            }
            // Real login: spawn the backend `#[server]` login call and resolve
            // the session before flipping `authed`. On success `auth::login`
            // records the numeric id and the signed session token
            // process-wide; the client threads the token back as
            // `Authorization: Bearer *** on backend calls. On failure the
            // server's message is shown inline.
            error.set(String::new());
            // In-flight flag: disables the submit button (and flips its label)
            // until the login call lands.
            busy.set(true);
            runtime_core::driver::spawn_async(async move {
                match crate::auth::login(&em, &pw).await {
                    Ok(_) => {
                        authed.set(true);
                        busy.set(false);
                    }
                    Err(message) => {
                        error.set(message);
                        busy.set(false);
                    }
                }
            });
        }
    };
    // The shared sign-up action: same validation, but calls `auth::signup`,
    // which creates the account and logs it straight in (so the session
    // token is set too), then flips `authed`. A duplicate username surfaces
    // inline the same way a failed login does.
    let do_signup = {
        let authed = authed.clone();
        let busy = busy.clone();
        move || {
            let em = username.get();
            let pw = password.get();
            if em.trim().is_empty() || pw.trim().is_empty() {
                error.set("Username and password are required.".to_string());
                return;
            }
            // Double-submit guard (covers the Enter-key path).
            if busy.get() {
                return;
            }
            error.set(String::new());
            // In-flight flag: disables the submit button (and flips its label)
            // until the sign-up call lands.
            busy.set(true);
            runtime_core::driver::spawn_async(async move {
                match crate::auth::signup(&em, &pw).await {
                    Ok(_) => {
                        authed.set(true);
                        busy.set(false);
                    }
                    Err(message) => {
                        error.set(message);
                        busy.set(false);
                    }
                }
            });
        }
    };
    // Submit routes by the current `mode`: sign in or sign up. Both are `Copy`
    // closures (capturing only `Copy` signals), so they're callable from a
    // `Fn` button handler and the Enter-in-password handler alike.
    let on_submit_click = {
        let mode = mode.clone();
        move || {
            if mode.get() {
                do_signup();
            } else {
                do_login();
            }
        }
    };
    let on_pw_enter = move |ev: &KeyEvent| {
        if ev.key == "Enter" && !ev.shift && !ev.ctrl && !ev.alt && !ev.meta {
            on_submit_click();
            KeyOutcome::PreventDefault
        } else {
            KeyOutcome::Default
        }
    };
    // "Continue as guest" — a mock path: skips the fields and the backend
    // entirely and just flips `authed`. Real sign-in / sign-up go through the
    // fields and `crate::auth`; guest remains a local no-account shortcut.
    let on_guest = move || authed.set(true);

    // The form. The inline error is reactive: a nested `switch` keyed on the
    // `error` signal rebuilds the form whenever a failed submit sets it (the
    // same `switch(key, render)` pattern the sidebar/message list use —
    // `ui!` doesn't re-read signal state on its own, only on rebuild).
    // `form` is keyed on `(error, mode)` so it also rebuilds when the mode
    // toggles between sign-in and sign-up.
    let username_change = username.clone();
    let pw_change = password.clone();
    let on_username_change = move |s: String| username_change.set(s);
    let on_pw_change = move |s: String| pw_change.set(s);
    // Mode toggle: flip `mode` to its opposite (sign-in <-> sign-up), so a
    // single closure serves both reciprocal links. Captures only the `Copy`
    // `mode` + `error` signals, so it's itself `Copy` and can be cloned into
    // the (also `Copy`) switch render closure. Toggling clears any inline
    // error so a failure in one mode doesn't bleed into the other.
    let on_toggle_mode = {
        let mode = mode.clone();
        move || {
            mode.set(!mode.get());
            error.set(String::new());
        }
    };
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
        // Keyed on `(error, mode, busy)` so the form rebuilds when a failed
        // submit sets `error`, when the mode toggles, AND when a submit flips
        // `busy` (which drives the submit button's `disabled` state and label
        // — the inert/dimmed button and in-flight wording must re-render as the
        // flag turns on and off).
        // Same `switch(key, render)` pattern the sidebar/message list use —
        // `ui!` doesn't re-read signal state on its own, only on rebuild.
        move || (error.get(), mode.get(), busy.get()),
        move |state: &(String, bool, bool)| {
            let err = state.0.clone();
            let is_signup = state.1;
            let in_flight = state.2;
            // The inline error, shown only while non-empty (a failed submit
            // sets it; a successful sign-in clears it and unmounts the screen).
            // Reused verbatim for both sign-in and sign-up failures.
            let err_style = StyleRules {
                margin_top: Some(px(12.0)),
                font_size: Some(px(13.0)),
                color: Some(col("#f23f43")),
                text_align: Some(TextAlign::Center),
                ..Default::default()
            };
            let err_el = if err.is_empty() {
                None
            } else {
                Some(ui! { text(style = err_style) { err } })
            };
            // The mode-scoped heading + sub. The `StyleRules` are cloned into
            // the render (whiteboard idiom); the copy changes with `mode`.
            let heading = if is_signup {
                "Create an account".to_string()
            } else {
                "Welcome back!".to_string()
            };
            let sub = if is_signup {
                "Ready to jump in? Create your account in seconds."
            } else {
                "We're so excited to see you again!"
            }
            .to_string();
            // The "Continue as guest" pill (mode-independent). Built *inside*
            // the render closure (Elements aren't `Copy` and the closure is
            // `Fn`, so a pre-built one would be moved on the first rebuild and
            // gone the second).
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
            // The reciprocal mode-toggle link: "No account? Sign up" in
            // sign-in mode, "Have an account? Sign in" in sign-up mode. Tapping
            // it flips `mode` (and clears any inline error), so the `(error,
            // mode)` key above rebuilds the form into the other mode.
            let toggle_label = if is_signup {
                "Have an account? Sign in".to_string()
            } else {
                "No account? Sign up".to_string()
            };
            let toggle_test_id = if is_signup {
                "login-signin-toggle"
            } else {
                "login-signup-toggle"
            };
            let toggle = on_toggle_mode.clone();
            let toggle_style = StyleRules {
                margin_top: Some(px(16.0)),
                font_size: Some(px(13.0)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                ..Default::default()
            };
            let toggle_text_style = StyleRules {
                color: Some(col(BUTTON)),
                font_size: Some(px(13.0)),
                font_weight: Some(FontWeight::SemiBold),
                ..Default::default()
            };
            let toggle_btn = ui! {
                view(style = toggle_style, test_id = toggle_test_id) {
                    text(style = toggle_text_style) { toggle_label }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        toggle();
                    }
                    TouchResponse::CONSUMED
                })
            };
            // The submit button label flips with the mode; the handler
            // (`on_submit_click`) routes on `mode` internally. While a submit
            // is in flight (`busy`), the label shows the in-flight wording
            // ("Signing in…" / "Creating account…") and the button is disabled
            // (inert) — so the user gets clear feedback and can't double-submit.
            let submit_label = if in_flight {
                if is_signup {
                    "Creating account…".to_string()
                } else {
                    "Signing in…".to_string()
                }
            } else if is_signup {
                "Sign Up".to_string()
            } else {
                "Sign In".to_string()
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
                    text(style = StyleRules {
                        // Centred (the heading/sub live in the form's full-width
                        // column now, so they need an explicit `text_align` to
                        // keep the card's centred look — `sub_style` already
                        // carries one, the heading didn't).
                        text_align: Some(TextAlign::Center),
                        ..heading_style.clone()
                    }) { heading }
                    text(style = sub_style.clone()) { sub }
                    text(style = label_style.clone()) { "Username".to_string() }
                    text_input(
                        value = username,
                        test_id = "login-email",
                        on_change = on_username_change,
                        placeholder = "username",
                        style = field_style.clone(),
                    )
                    // Enter in the username field submits too -- without this,
                    // only the password field (below) responded to Enter, so
                    // the natural "type username, hit Enter" flow silently did
                    // nothing until the user tabbed into the password field.
                    .on_key_down(on_pw_enter)
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
                        label = submit_label,
                        test_id = "login-submit",
                        on_click = on_submit_click,
                        // Live `disabled` state (the proven in-repo idiom, cf.
                        // rules_accept_checkbox.rs): while a submit is in flight
                        // this reads `busy.get()` inside a closure, so the prop
                        // re-evaluates reactively as `busy` flips — muting the
                        // button (inert + visually dimmed) and blocking the
                        // press against double-taps. The Enter-key path is
                        // separately guarded by the `busy.get()` early-return at
                        // the top of `do_login`/`do_signup`.
                        disabled = move || busy.get(),
                        style = submit_style.clone(),
                    )
                    toggle_btn
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
                // The mode-scoped heading + sub live inside the form now (so
                // they flip with the sign-in/sign-up mode); only the logo stays
                // static in the card here.
                form
            }
        }
    }
}

// --- the tree ----------------------------------------------------------------

/// True for this process only when `app()` re-seeded the statics from a
/// persisted (localStorage) session. Distinguishes a *stale restored* token —
/// which must sign the user back out on the first failed call — from a
/// transient blip on a *freshly issued* token (network drop, brief 500), which
/// must NOT log a working session out. Set once in `app()`, read (and consumed)
/// by the app shell's first authenticated fetch.
static RESTORED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// One-shot note handed to the next login screen when a persisted session is
/// dropped as stale. The app shell writes "Session expired, please log in
/// again" here before flipping `authed` back to false; `login_screen` consumes
/// it (taking `None`) when it mounts, so a fresh login or an explicit logout
/// shows a clean form. `Mutex` (not atomic) because it carries a `String`.
static SESSION_NOTE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

pub fn app() -> Element {
    // Point the backend `#[server]` RPC stubs at a base URL and attach the
    // signed-in user id as the `x-user-id` header on every outgoing call.
    // Runs once per process (the `app()` entry), so no "only once" guard is
    // needed.
    crate::auth::configure_server();
    // Root-level document font: publish a system-sans stack so web text does
    // NOT fall back to the browser's serif default (Times). Without this the
    // whole app renders in serif — the single biggest "off" feel vs real
    // Discord, which is 100% sans (gg sans / system sans fallback). This is
    // the one global control: it cascades to every `text` node (heading,
    // channel names, message bodies) instead of patching each `text()` style.
    // Runs once per process, exactly like `configure_server` above.
    runtime_core::set_default_text_font(Some(FontFamily::System(
        "-apple-system, BlinkMacSystemFont, \"Segoe UI\", Roboto, Helvetica, Arial, sans-serif"
            .to_string(),
    )));
    // Restore a persisted session before defaulting `authed`: a refresh wipes
    // the WASM statics, but `localStorage` still holds the token + user id a
    // successful login saved. If present, re-seed them and start signed in —
    // the login screen is skipped entirely. If the restored token turns out
    // to be stale, the first authenticated backend call fails and the app
    // shell routes the failure back here (see `live_servers` fetch), clearing
    // the saved session and showing "session expired" on the login screen.
    let restored = matches!(crate::auth::load_session(), Some((_, user_id, _, _))
        if user_id != 0);
    // On web the restored token slot is intentionally EMPTY — the httpOnly
    // session cookie the browser replays authenticates every backend call
    // (and the WS upgrade). Identity keys alone decide the restore; a dead
    // cookie means the first authed call fails and routes back here.
    if restored {
        if let Some((token, user_id, username, avatar)) = crate::auth::load_session() {
            crate::auth::CURRENT_USER_ID
                .store(user_id, std::sync::atomic::Ordering::SeqCst);
            *crate::auth::CURRENT_TOKEN
                .lock()
                .unwrap_or_else(|p| p.into_inner()) = token;
            // The username segment is absent in sessions persisted before it
            // was saved (they restore with an empty name) — skip the write
            // then so a stale empty value can't clobber anything.
            if !username.is_empty() {
                *crate::auth::CURRENT_USERNAME
                    .lock()
                    .unwrap_or_else(|p| p.into_inner()) = username.clone();
                // The avatar rides the same persisted session so the signed-in
                // user's uploaded image comes back on a refresh without waiting
                // for a roster fetch.
                crate::auth::remember_avatar(username.as_str(), avatar.clone());
            }
        }
        RESTORED.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    let authed: Signal<bool> = signal(restored);
    let authed_copy = authed; // `Signal` is `Copy`; the render closure only
                              // receives the current value, so pass the signal in.
    // The login screen's in-flight flag: `true` between a submit and the
    // backend call landing (login or signup). Root-scoped so it outlives the
    // screen; threaded in and consumed by the form's submit button (its
    // `disabled` state + in-flight label) and the handlers' double-submit
    // guard. See `login_screen`.
    let busy: Signal<bool> = signal(false);
    let busy_copy = busy;
    switch(
        move || authed.get(),
        move |authed: &bool| {
            if *authed {
                app_shell(authed_copy)
            } else {
                login_screen(authed_copy, busy_copy)
            }
        },
    )
}

// --- @-mention autocomplete helpers (pure) --------------------------------
// These are `&`-free, no-capture functions so the autocomplete closures in
// `app_shell` can be `Fn` (callable from both the composer's `on_key_down` and
// the dropdown rows' `on_touch`). They operate on explicit args, never on
// captured locals.

/// The active `@`-mention token in the input: the text after the LAST `@`
/// that is immediately followed by a non-whitespace run (e.g. `"hi @jo"` ->
/// `Some("jo")`, `"hi @ "` -> `None`). `None` means there is no active mention
/// token — the signal that a space has closed the dropdown.
fn mention_active_query(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let at = bytes.iter().rposition(|&b| b == b'@')?;
    let after = &text[at + 1..];
    if after.is_empty() {
        return Some(String::new());
    }
    for b in after.bytes() {
        if b.is_ascii_whitespace() {
            return None;
        }
    }
    Some(after.to_string())
}

/// The active live server's member roster: the distinct author labels that have
/// posted in any of its channels (from the already-buffered gateway messages).
/// No backend call — pure reuse of the data the client already holds. If no
/// channel is open yet (no frames buffered) and nothing has posted, fall back
/// to the signed-in user's own label so `@` still has a match.
fn live_member_roster(
    live_channels: &[crate::servers::ChannelSummary],
    live_messages: &HashMap<i64, Vec<LiveMsg>>,
) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for c in live_channels {
        if let Some(msgs) = live_messages.get(&c.id) {
            for (_mid, _aid, author, _body, _ts, _edited, _reply_to) in msgs {
                if !names.contains(author) {
                    names.push(author.clone());
                }
            }
        }
    }
    if names.is_empty() {
        let my = crate::auth::CURRENT_USER_ID.load(std::sync::atomic::Ordering::SeqCst);
        if my != 0 {
            names.push(format!("user {}", my));
        }
    }
    names.sort();
    names
}

/// The role a member advances to when their cycle-button is tapped, in the
/// order member → moderator → admin → owner → member. `'owner'` is included in
/// the cycle (so all four role strings are reachable) even though the backend
/// rejects assigning it to anyone who isn't already the owner — the rejection
/// is surfaced as an inline error rather than pre-validated (the backend is
/// the single source of truth for who may hold which role).
fn next_role(current: &str) -> &'static str {
    match current {
        "member" => "moderator",
        "moderator" => "admin",
        "admin" => "owner",
        // owner and anything unknown cycle back to member.
        _ => "member",
    }
}

/// Client-side permission pre-check mirroring the backend's role bars (the
/// backend stays the enforcer; this only decides which menu rows RENDER):
/// roles rank member < moderator < admin < owner; kick and channel-rename
/// need moderator+, ban / timeout / channel-delete need admin+.
fn role_rank(role: String) -> i32 {
    match role.trim().to_lowercase().as_str() {
        "owner" => 3,
        "admin" => 2,
        "moderator" => 1,
        "member" => 0,
        _ => -1,
    }
}

/// The acting (signed-in) user's own role string in the active live roster —
/// the discriminator for rendering moderation controls (Kick/Ban/Timeout/
/// Rename/Delete) before the user even taps them. A user not on the roster
/// (or a guest) resolves to "" and ranks below every bar, so no controls
/// render. Data comes from the same `list_members` fetch the panel renders.
fn acting_role(members: &Vec<crate::moderation::MemberInfo>, self_id: i64) -> String {
    members.iter()
        .find(|m| m.user_id == self_id)
        .map(|m| m.role.clone())
        .unwrap_or_default()
}

/// A small popup action menu anchored below a target element (the member-
/// row / channel-row context menu Discord opens from a row tap). Rendered by
/// a `switch` keyed on a SHARED `Signal<Option<i64>>` — the row's tap handler
/// sets it to `Some(key)`, so exactly one menu (the tapped row's) mounts,
/// anchored below that row. Backdrop is `BackdropMode::None`: the rest of the
/// screen stays interactive (a menu, not a full-screen modal). Rows are
/// (label, destructive-red?, handler); a row tap fires its handler then
/// closes the menu (the downstream confirm steps — kick/ban/timeout inline
/// two-steps, rename/delete modals — stay as they were).
fn action_menu(
    open: Signal<Option<i64>>,
    key: i64,
    anchor: Ref<ViewHandle>,
    rows: Vec<(String, bool, Rc<dyn Fn()>)>,
) -> Element {
    switch(
        move || open.get(),
        move |sel: &Option<i64>| {
            if *sel != Some(key) {
                return ui! {
                    view(style = StyleRules {
                        width: Some(px(0.0)),
                        height: Some(px(0.0)),
                        ..Default::default()
                    }) {}
                };
            }
            // The render closure is `Fn` — clone the (non-`Copy`) row list
            // into a local so the `for` below can consume it each render.
            let rows = rows.clone();
            ui! {
                anchored_overlay(
                    target = AnchorTarget::from(anchor),
                    side = ElementSide::Below,
                    align = ElementAlign::Start,
                    offset = 6.0,
                    backdrop = BackdropMode::None,
                ) {
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Column),
                        width: Some(px(150.0)),
                        gap: Some(px(2.0)),
                        background: Some(col("#111214")),
                        border_top_left_radius: Some(px(8.0)),
                        border_top_right_radius: Some(px(8.0)),
                        border_bottom_left_radius: Some(px(8.0)),
                        border_bottom_right_radius: Some(px(8.0)),
                        border_top_width: Some(border_width(1.0)),
                        border_right_width: Some(border_width(1.0)),
                        border_bottom_width: Some(border_width(1.0)),
                        border_left_width: Some(border_width(1.0)),
                        border_top_color: Some(col(BORDER)),
                        border_right_color: Some(col(BORDER)),
                        border_bottom_color: Some(col(BORDER)),
                        border_left_color: Some(col(BORDER)),
                        overflow: Some(Overflow::Hidden),
                        ..Default::default()
                    }) {
                        for (label, red, on) in rows.into_iter() {
                            view(style = StyleRules {
                                padding_top: Some(px(8.0)),
                                padding_bottom: Some(px(8.0)),
                                padding_left: Some(px(12.0)),
                                padding_right: Some(px(12.0)),
                                flex_direction: Some(FlexDirection::Row),
                                align_items: Some(AlignItems::Center),
                                ..Default::default()
                            }) {
                                text(style = StyleRules {
                                    font_size: Some(px(14.0)),
                                    // Destructive actions read as RED TEXT only
                                    // (Discord's treatment) — never a filled pill.
                                    color: Some(col(if red { BADGE } else { TEXT })),
                                    ..Default::default()
                                }) { label.clone() }
                            }
                            .on_touch(move |ev| {
                                if ev.phase == TouchPhase::Ended {
                                    on();
                                    open.set(None);
                                }
                                TouchResponse::CONSUMED
                            })
                        }
                    }
                }
            }
        },
    )
}

/// The live-server member panel: the real membership of the active backend
/// server (fetched from the `#[server]` `list_members` endpoint, owner-first)
/// rendered as a right-side column — the live counterpart of the mock
/// `member_panel`. A `MessageSearchBar` tops the panel (same position as the
/// mock panel's): `search_corpus` is the active live channel's gateway
/// message buffer as `(author, text)` rows (derived by the caller's
/// `live_search_corpus` memo), so a live server's search actually searches
/// real messages instead of the mock seed data. Each member row shows the
/// presence dot, the member's avatar chip (when the backend carries one) and
/// their username + role chip — the same content contract as the mock
/// `member_row` / the current user's row. Moderation actions (Kick / Ban /
/// Timeout) are NOT inline pills: tapping the row opens the `action_menu`
/// popup anchored below it (rendered only when the acting user's role clears
/// the backend's bar — moderator+ for Kick, admin+ for Ban/Timeout), and the
/// menu entry arms the row's existing inline two-step confirm.
fn live_member_panel(
    server_id: i64,
    members: Signal<Vec<crate::moderation::MemberInfo>>,
    error: Signal<String>,
    kick_confirm: Signal<Option<i64>>,
    ban_confirm: Signal<Option<i64>>,
    timeout_confirm: Signal<Option<i64>>,
    timeout_duration: Signal<i64>,
    presence: Rc<RefCell<HashMap<i64, String>>>,
    search_corpus: ReadSignal<Vec<(String, String)>>,
    member_menu: Signal<Option<i64>>,
) -> Element {
    // Snapshot of the current roster. The panel is rebuilt (by the caller's
    // `switch` keyed on this signal) whenever the roster changes — including
    // the re-fetch the mutation callbacks below trigger on success — so a
    // local snapshot is always current here.
    let roster = members.get();
    // Snapshot the live presence map the same way. The caller's `switch` key
    // also folds in `live_presence_rev`, so a `Presence` frame forces a
    // rebuild and this snapshot picks up the new status. A member absent from
    // the map (e.g. just after the roster loads, before any frame arrived) is
    // rendered as a grey "offline" dot below.
    let presence: HashMap<i64, String> = presence.borrow().clone();
    // The signed-in user's own numeric id — their own row is marked "(you)" and
    // the rest is left to the backend's permission checks (no client-side
    // pre-computation). `CURRENT_USER_ID` is `0` when signed out, which can't
    // match a real member id, so `(you)` simply never appears.
    let self_id: i64 = crate::auth::CURRENT_USER_ID
        .load(std::sync::atomic::Ordering::SeqCst);

    // Shared styles (built once per rebuild; `StyleRules` is not `Copy`, so the
    // per-row styles are written INLINE inside the loop, the `member_role_list`
    // idiom — nothing non-`Copy` is captured across the loop boundary).
    let header_style = StyleRules {
        padding_top: Some(px(14.0)),
        padding_bottom: Some(px(6.0)),
        padding_left: Some(px(12.0)),
        padding_right: Some(px(12.0)),
        ..section_label()
    };
    // The message-search bar, same position as the mock `member_panel`:
    // 12px-padded at the top of the panel, above the member list. The corpus
    // is the active live channel's gateway buffer (see the caller's
    // `live_search_corpus` memo).
    let search_block = ui! {
        view(style = StyleRules {
            padding_top: Some(px(12.0)),
            padding_bottom: Some(px(12.0)),
            padding_left: Some(px(12.0)),
            padding_right: Some(px(12.0)),
            ..Default::default()
        }) {
            MessageSearchBar(corpus = search_corpus)
        }
    };
    let name_style = StyleRules {
        font_size: Some(px(14.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT)),
        flex_grow: Some(grow(1.0)),
        flex_shrink: Some(grow(0.0)),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    let role_btn_style = StyleRules {
        padding_top: Some(px(2.0)),
        padding_bottom: Some(px(2.0)),
        padding_left: Some(px(8.0)),
        padding_right: Some(px(8.0)),
        background: Some(col(COMPOSER)),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        flex_shrink: Some(grow(0.0)),
        ..Default::default()
    };
    let role_label_style = StyleRules {
        font_size: Some(px(11.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT_CHANNEL)),
        ..Default::default()
    };
    // The Kick/Ban/Timeout pills that used to render here are gone: those
    // actions live in the row-tap `action_menu` popup (red TEXT labels only).
    let confirm_bg_style = StyleRules {
        background: Some(col(BG_ACTIVE)),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        padding_top: Some(px(4.0)),
        padding_bottom: Some(px(4.0)),
        padding_left: Some(px(8.0)),
        padding_right: Some(px(8.0)),
        gap: Some(px(8.0)),
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        ..Default::default()
    };
    let confirm_yes_style = StyleRules {
        background: Some(col(BADGE)),
        padding_top: Some(px(4.0)),
        padding_bottom: Some(px(4.0)),
        padding_left: Some(px(10.0)),
        padding_right: Some(px(10.0)),
        border_top_left_radius: Some(px(6.0)),
        border_top_right_radius: Some(px(6.0)),
        border_bottom_left_radius: Some(px(6.0)),
        border_bottom_right_radius: Some(px(6.0)),
        flex_shrink: Some(grow(0.0)),
        ..Default::default()
    };
    let confirm_no_style = StyleRules {
        background: Some(col(BG_HOVER)),
        padding_top: Some(px(4.0)),
        padding_bottom: Some(px(4.0)),
        padding_left: Some(px(10.0)),
        padding_right: Some(px(10.0)),
        border_top_left_radius: Some(px(6.0)),
        border_top_right_radius: Some(px(6.0)),
        border_bottom_left_radius: Some(px(6.0)),
        border_bottom_right_radius: Some(px(6.0)),
        flex_shrink: Some(grow(0.0)),
        ..Default::default()
    };
    let small_label_style = StyleRules {
        font_size: Some(px(12.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col("#ffffff")),
        ..Default::default()
    };
    let error_style = StyleRules {
        font_size: Some(px(11.0)),
        color: Some(col(BADGE)),
        ..Default::default()
    };

    // Build the rows as a local `Vec<Element>` (the mock `member_panel` shape).
    // Each row is a `move` closure: `server_id` (i64) and `member_id` (i64) are
    // `Copy`, and the three `Signal`s it captures are `Copy`, so every closure
    // owns a fresh copy and stays `Fn` across rebuilds.
    // Client-side permission bars mirror the backend's role checks (the backend
    // stays the enforcer — this only decides which menu rows RENDER): the
    // acting user's own role from the roster gates whether the Kick/Ban/Timeout
    // popup menu shows at all, and rank decides which entries it carries.
    let acting_rank = role_rank(acting_role(&roster, self_id));
    let can_kick = acting_rank >= 1;
    let can_ban = acting_rank >= 2;
    let mut rows: Vec<Element> = Vec::new();
    for member in roster.iter() {
        let member_id = member.user_id;
        let is_self = member_id == self_id;
        let role = member.role.clone();
        let confirming = kick_confirm.clone();
        let kick = kick_confirm.clone();
        let banning = ban_confirm.clone();
        let ban = ban_confirm.clone();
        let timing = timeout_confirm.clone();
        let timeout = timeout_confirm.clone();
        let dur = timeout_duration.clone();
        let error = error.clone();
        // Fresh per-iteration clones of every style used below -- the whole
        // `ui! { ... }` row block compiles to one closure per iteration, and
        // a style declared once above this loop gets moved into that
        // closure on iteration 1, leaving nothing for iteration 2+ (the
        // same "StyleRules moved twice" class of bug documented elsewhere
        // in nodes.yaml's b60 history; the fix there was the same: clone
        // before the loop body captures it, not just at each use site).
        let name_style = name_style.clone();
        let role_btn_style = role_btn_style.clone();
        let role_label_style = role_label_style.clone();
        let confirm_bg_style = confirm_bg_style.clone();
        let confirm_yes_style = confirm_yes_style.clone();
        let confirm_no_style = confirm_no_style.clone();
        let small_label_style = small_label_style.clone();
        let error_style = error_style.clone();
        // The kick-confirm / ban-confirm / idle arms below share the confirm +
        // kick styles above. They render through ONE flat `match` (in the row
        // below), not an `if / else if / else` chain: a chain lowers to nested
        // `when` closures whose `else if`/`else` arms MOVE any non-`Copy` style
        // they use out of the parent `Fn` (the ui! macro's "B5" capture bug),
        // whereas a single `match` lowers to one `switch` dispatcher that every
        // arm BORROWS via `.clone()` — so no per-branch style copies are needed.
        // name_style is used in two mutually-exclusive branches below, but
        // the ui! macro's own parser doesn't support a nested `let` inside a
        // branch (tried it -- breaks its DSL parsing), so each branch gets
        // its own pre-made clone here instead, outside the macro entirely.
        let name_style_self_branch = name_style.clone();
        let name_style_other_branch = name_style;
        // Live presence dot for this row. `presence` is the snapshot taken at
        // the top of the fn (rebuilt on every `live_presence_rev` bump); the
        // status string is whatever this member last reported over the gateway.
        // Absent (no frame yet — e.g. right after the roster first loads) and
        // any value the backend doesn't send are both rendered as the offline
        // grey. The colors mirror the mock `Status::color()` convention so the
        // live panel reads like the mock one: green online, amber away (the
        // server's idle state), grey offline. A standalone filled circle — the
        // live rows carry no avatar chip, so this is the plain dot, not the
        // ring-on-avatar variant the mock `member_row` uses.
        let presence_color = match presence.get(&member_id).map(|s| s.as_str()) {
            Some("online") => "#23a559",
            Some("away") => "#f0b232",
            Some("offline") => "#80848e",
            _ => "#80848e",
        };
        // Snapshot the member's avatar from the shared client registry before
        // the row closure: the ui! body cannot hold a `let`, and the `if
        // let Some(...)` child branch reads this plain local.
        let member_avatar: Option<String> =
            crate::auth::avatar_for(member.username.as_str());
        let dot_style = StyleRules {
            width: Some(px(10.0)),
            height: Some(px(10.0)),
            background: Some(col(presence_color)),
            border_top_left_radius: Some(px(5.0)),
            border_top_right_radius: Some(px(5.0)),
            border_bottom_left_radius: Some(px(5.0)),
            border_bottom_right_radius: Some(px(5.0)),
            flex_shrink: Some(grow(0.0)),
            ..Default::default()
        };

        // The row's anchored-popup state: the row view fills this handle at
        // mount, and the row's tap handler opens `action_menu` below it.
        let row_ref: Ref<ViewHandle> = Ref::<ViewHandle>::new();
        // The row's menu entries, gated by the ACTING user's role (mirrors the
        // backend bars: Kick = moderator+; Ban + Timeout = admin+). Handlers
        // only ARM the existing inline two-step confirms — permission checks
        // and confirm steps stay exactly as before.
        let on_kick = Rc::new(move || { kick.set(Some(member_id)); });
        let on_ban = Rc::new(move || { ban.set(Some(member_id)); });
        let on_timeout = Rc::new(move || { timeout.set(Some(member_id)); });
        let menu_items: Vec<(String, bool, Rc<dyn Fn()>)> = {
            let mut items: Vec<(String, bool, Rc<dyn Fn()>)> =
                Vec::<(String, bool, Rc<dyn Fn()>)>::new();
            if !is_self {
                if can_kick {
                    items.push((
                        "Kick".to_string(),
                        true,
                        on_kick,
                    ));
                }
                if can_ban {
                    items.push(("Ban".to_string(), true, on_ban));
                    items.push(("Timeout".to_string(), false, on_timeout));
                }
            }
            items
        };
        let menu = action_menu(member_menu, member_id, row_ref, menu_items);

        let row = ui! {
            view(style = StyleRules {
                padding_top: Some(px(6.0)),
                padding_bottom: Some(px(6.0)),
                padding_left: Some(px(12.0)),
                padding_right: Some(px(12.0)),
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(8.0)),
                ..Default::default()
            }) {
                // Live presence dot (green/amber/grey from the `live_presence`
                // map; grey when no `Presence` frame has landed for this member
                // yet). Placed first so it leads the row.
                view(style = dot_style) {}
                // The member's avatar when the shared client registry has one
                // (fed by `list_members` / login / gateway frames): a 22px
                // round-cropped image. Members without an uploaded avatar
                // keep the plain dot-led row (the pre-existing look).
                if let Some(avatar) = &member_avatar {
                    image(
                        src = crate::auth::media_url(avatar.clone()),
                        alt = format!("{} avatar", member.username),
                        style = StyleRules {
                            width: Some(px(22.0)),
                            height: Some(px(22.0)),
                            flex_shrink: Some(grow(0.0)),
                            object_fit: Some(ObjectFit::Cover),
                            border_top_left_radius: Some(px(11.0)),
                            border_top_right_radius: Some(px(11.0)),
                            border_bottom_left_radius: Some(px(11.0)),
                            border_bottom_right_radius: Some(px(11.0)),
                            ..Default::default()
                        },
                    )
                }
                // Name (+ a "(you)" tag on the signed-in user's own row).
                // flex_shrink 0 + min content: the wrapper must NEVER collapse
                // to zero width (the bug this fixes: shrinking the name's
                // wrapper to fit alongside the row's inline action pills
                // rendered non-self rows with a role badge and NO username).
                view(style = StyleRules {
                    flex_shrink: Some(grow(0.0)),
                    overflow: Some(Overflow::Hidden),
                    ..Default::default()
                }) {
                    if is_self {
                        text(style = name_style_self_branch) {
                            format!("{} (you)", member.username)
                        }
                    } else {
                        text(style = name_style_other_branch) { member.username.clone() }
                    }
                }

                // Role cycle-button: shows the current role; a tap advances it
                // (member→moderator→admin→owner→member) and calls
                // `set_member_role`, refreshing the roster on success. The
                // backend rejects invalid changes (e.g. assigning `owner`),
                // which surface via the inline error line below.
                if !is_self {
                    view(style = role_btn_style.clone()) {
                        text(style = role_label_style.clone()) {
                            format!("{role} ▸")
                        }
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            let sid = server_id;
                            let uid = member_id;
                            let new_role = next_role(&role);
                            let err = error;
                            let kick = kick;
                            let members = members;
                            runtime_core::driver::spawn_async(async move {
                                match crate::moderation::set_member_role(sid, uid, new_role).await {
                                    Ok(()) => {
                                        kick.set(None);
                                        if let Ok(fresh) = crate::moderation::list_members(sid).await {
                                            members.set(fresh);
                                        }
                                    }
                                    Err(msg) => err.set(msg),
                                }
                            });
                        }
                        TouchResponse::CONSUMED
                    })
                }

                // Kick: a two-step inline confirm. First tap shows Confirm/Cancel;
                // a second tap ("Confirm") calls `remove_member` and refreshes;
                // "Cancel" (or tapping the row again) dismisses. The confirm
                // branch replaces the role+kick controls for this row only.
                // Kick / Ban: a two-step inline confirm each. The row's three
                // mutually-exclusive action states (kick confirm pending, ban
                // confirm pending, idle buttons) are one flat `match` over the
                // the confirm signals, not an `if / else if / else` chain — see
                // the "B5" capture note above. The timeout confirm is the
                // third mutually-exclusive pending state (kick / ban / timeout).
                match (confirming.get(), banning.get(), timing.get()) {
                    (Some(uid), _, _) if *uid == member_id => {
                    view(style = confirm_bg_style.clone()) {
                        text(style = StyleRules {
                            font_size: Some(px(12.0)),
                            color: Some(col(TEXT)),
                            flex_shrink: Some(grow(1.0)),
                            ..Default::default()
                        }) { "Kick?" }
                        view(style = confirm_yes_style.clone()) {
                            text(style = small_label_style.clone()) { "Yes" }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                let sid = server_id;
                                let uid = member_id;
                                let err = error;
                                let members = members;
                                runtime_core::driver::spawn_async(async move {
                                    match crate::moderation::kick_member(sid, uid).await {
                                        Ok(()) => {
                                            kick.set(None);
                                            if let Ok(fresh) = crate::moderation::list_members(sid).await {
                                                members.set(fresh);
                                            }
                                        }
                                        Err(msg) => {
                                            kick.set(None);
                                            err.set(msg);
                                        }
                                    }
                                });
                            }
                            TouchResponse::CONSUMED
                        })
                        view(style = confirm_no_style.clone()) {
                            text(style = small_label_style.clone()) { "No" }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                kick.set(None);
                            }
                            TouchResponse::CONSUMED
                        })
                    }
                } (_, Some(uid), _) if *uid == member_id && !is_self => {
                    // Ban: same two-step shape as the kick confirm, but calls
                    // `ban_member` (remove AND bar rejoining) and clears the ban
                    // confirm on resolve. Takes priority over the kick confirm
                    // when both are somehow pending for a row (they're set from
                    // different buttons, so in practice never both at once).
                    view(style = confirm_bg_style.clone()) {
                        text(style = StyleRules {
                            font_size: Some(px(12.0)),
                            color: Some(col(TEXT)),
                            flex_shrink: Some(grow(1.0)),
                            ..Default::default()
                        }) { "Ban?" }
                        view(style = confirm_yes_style.clone()) {
                            text(style = small_label_style.clone()) { "Yes" }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                let sid = server_id;
                                let uid = member_id;
                                let err = error;
                                let members = members;
                                let ban = ban;
                                runtime_core::driver::spawn_async(async move {
                                    match crate::moderation::ban_member(sid, uid, None).await {
                                        Ok(()) => {
                                            ban.set(None);
                                            if let Ok(fresh) = crate::moderation::list_members(sid).await {
                                                members.set(fresh);
                                            }
                                        }
                                        Err(msg) => {
                                            ban.set(None);
                                            err.set(msg);
                                        }
                                    }
                                });
                            }
                            TouchResponse::CONSUMED
                        })
                        view(style = confirm_no_style.clone()) {
                            text(style = small_label_style.clone()) { "No" }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                ban.set(None);
                            }
                            TouchResponse::CONSUMED
                        })
                    }
                } (_, _, Some(uid)) if *uid == member_id && !is_self => {
                    // Timeout: the mildest of the three severities — the member
                    // stays on the roster but is silenced from posting for the
                    // chosen duration. The row shows the duration picker (taps
                    // write the shared `timeout_duration` signal) plus Yes/No;
                    // Yes calls `timeout_member` with the duration read at
                    // click time, No (or a re-tap) dismisses.
                    // Stacks vertically (unlike the kick/ban confirm, whose
                    // one-word label + Yes/No fit in the 260px panel as a row):
                    // the inline duration picker is wide, so a flex Row pushes
                    // Yes/No past the panel's right edge (clipped, unreachable).
                    // A flex Column keeps every control inside the panel width.
                    view(style = StyleRules {
                        background: Some(col(BG_ACTIVE)),
                        border_top_left_radius: Some(px(8.0)),
                        border_top_right_radius: Some(px(8.0)),
                        border_bottom_left_radius: Some(px(8.0)),
                        border_bottom_right_radius: Some(px(8.0)),
                        padding_top: Some(px(4.0)),
                        padding_bottom: Some(px(4.0)),
                        padding_left: Some(px(8.0)),
                        padding_right: Some(px(8.0)),
                        gap: Some(px(8.0)),
                        flex_direction: Some(FlexDirection::Column),
                        align_items: Some(AlignItems::Center),
                        ..Default::default()
                    }) {
                        text(style = StyleRules {
                            font_size: Some(px(12.0)),
                            color: Some(col(TEXT)),
                            ..Default::default()
                        }) { "Time out?" }
                        TimeoutDurationPicker(duration = dur.clone())
                        view(style = confirm_yes_style.clone()) {
                            text(style = small_label_style.clone()) { "Yes" }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                let sid = server_id;
                                let uid = member_id;
                                let err = error;
                                let secs = dur.get();
                                let timeout = timeout;
                                runtime_core::driver::spawn_async(async move {
                                    match crate::moderation::timeout_member(sid, uid, secs).await {
                                        Ok(()) => {
                                            timeout.set(None);
                                        }
                                        Err(msg) => {
                                            timeout.set(None);
                                            err.set(msg);
                                        }
                                    }
                                });
                            }
                            TouchResponse::CONSUMED
                        })
                        view(style = confirm_no_style.clone()) {
                            text(style = small_label_style.clone()) { "No" }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                timeout.set(None);
                            }
                            TouchResponse::CONSUMED
                        })
                    }
                }
                // Idle rows carry NO inline action buttons: the moderation
                // actions (Kick / Ban / Timeout) live in the popup `action_menu`
                // anchored below the row, opened by a tap on the row itself
                // (below, in `.on_touch`). Only the confirm states render here.
                _ => {}
            }

            // Inline backend error for this row (the most recent one). Shown
            // only when a kick/role-change was rejected — the backend's own
            // message, no client-side permission logic.
            if !error.get().is_empty() {
                text(style = error_style.clone()) { move || error.get().clone() }
            }
            // The row's tap popup menu, anchored below this row's own box
            // (the row is Position::Relative and fills its handle ref). Only
            // this row's menu mounts — `action_menu`'s switch key matches on
            // the shared `member_menu` signal's `Some(member_id)`.
            { menu }
        }
        .bind(row_ref)
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended && !is_self {
                // Tapping a member row: a pending confirm for THIS row is
                // dismissed (the old re-tap-cancel idiom — the two-step stays);
                // otherwise the row's action menu opens (permitted users only;
                // `menu` is empty — a zero-size view — for everyone else).
                if confirming.get().is_some()
                    || banning.get().is_some()
                    || timing.get().is_some()
                {
                    confirming.set(None);
                    banning.set(None);
                    timing.set(None);
                } else if can_kick || can_ban {
                    member_menu.set(Some(member_id));
                }
            }
            TouchResponse::CONSUMED
        })
        };
        rows.push(row);
    }

    ui! {
        view(style = StyleRules {
            width: Some(px(260.0)),
            background: Some(col(BG_SIDEBAR)),
            border_left_width: Some(border_width(1.0)),
            border_left_color: Some(col(BORDER)),
            flex_direction: Some(FlexDirection::Column),
            overflow: Some(Overflow::Hidden),
            ..Default::default()
        }) {
            text(style = header_style) {
                format!("MEMBERS — {}", roster.len())
            }
            search_block
            scroll_view(style = StyleRules {
                flex_grow: Some(grow(1.0)),
                ..Default::default()
            }) { rows }
        }
    }
}

/// Case-insensitive prefix filter on the roster for the autocomplete dropdown.
fn filter_members(q: &str, members: Vec<String>) -> Vec<String> {
    let q = q.to_lowercase();
    members
        .into_iter()
        .filter(|m| m.to_lowercase().starts_with(&q))
        .collect()
}

/// A thin vertical drag handle that resizes a fixed-width column by feeding
/// horizontal pans back into its `width` signal. It sits in the top-level
/// row flush against the column's inner edge, between that column and the
/// growing main view. `sign` is `+1` when dragging right should *grow* the
/// column (the channel sidebar, whose handle rides its right edge) and `-1`
/// for the member list (handle on its left edge, so dragging right shrinks
/// it). `id` is the robot `test_id` anchor (kept distinct per instance so the
/// two handles don't collide in the automation registry).
///
/// The pan callback is `Fn` (not `FnMut`), so the "width at drag start" is
/// stashed in a `Cell` — that gives the callback interior mutability without
/// requiring `FnMut`. `PanEvent::Moved.delta` is *cumulative* from `Began`
/// (window space, not view-local), so it's applied directly as an offset from
/// the stashed start rather than accumulated event-to-event (accumulating
/// onto an already-moving value would oscillate — see the recognizer docs).
fn resize_handle(width: Signal<f32>, sign: f32, id: &'static str) -> Element {
    let hovered = signal(false);
    let start = Cell::new(0.0f32);
    // Transparent at rest (invisible, but still hit-testable — the default
    // `PointerEvents::Auto` doesn't skip empty-area hit-testing), a subtle
    // fill once hovered so the affordance is discoverable. `ColResize` gives
    // the east-west resize cursor over the strip.
    let handle_style = move || StyleRules {
        width: Some(px(6.0)),
        background: Some(col(if hovered.get() { BG_HOVER } else { "transparent" })),
        cursor: Some(Cursor::ColResize),
        // A fixed grab strip: never shrinks below 6px in the flex row, so it
        // stays reliably draggable even when the window is narrow.
        flex_shrink: Some(grow(0.0)),
        flex_direction: Some(FlexDirection::Column),
        ..Default::default()
    };
    ui! {
        view(style = handle_style, test_id = id) {}
            .on_touch({
                let h = pan(
                    PanRecognizer::default(),
                    move |ev: &PanEvent| {
                        match ev {
                            PanEvent::Began { .. } => {
                                start.set(width.get());
                            }
                            PanEvent::Moved { delta, .. } => {
                                let next = (start.get() + sign * delta.x).clamp(180.0, 400.0);
                                width.set(next);
                            }
                            // Width stays wherever it was left on release.
                            PanEvent::Ended { .. } | PanEvent::Cancelled => {}
                        }
                    },
                );
                move |ev: &TouchEvent| h(ev)
            })
            .on_hover(move |on| hovered.set(on))
    }
}

// --- responsive sidebar widths (window-resize driven) ----------------------
// The two side columns keep the user's manual drag choice in `sidebar_width` /
// `member_panel_width` (the previous task's drag handles write straight to
// those signals). But when the *window* itself narrows — a live
// `viewport_size()` change, not a drag — the fixed columns stop fitting and
// the layout used to just clip. These helpers fold the viewport into the
// *rendered* width without ever writing back to the user's signal, so
// widening the window back up past the collapse threshold automatically
// restores the manual width (we only read it here — we never overwrite it, so
// a user's dragged choice is never fought).
//
// Calling these inside the reactive style closures (`sidebar_style`,
// `member_panel`'s `panel_style`) is what subscribes the layout to the
// viewport: each closure re-runs on a real resize and re-resolves its `width`.
// Same idiom whiteboard's `chrome.rs` popover uses for a viewport-driven
// height — no hand-rolled resize detection, just the framework's
// `viewport_size()` signal.
//
// Thresholds (CSS px of total window width). The main message column is the
// star and gets a hard floor of `MAIN_MIN`; the two panels surrender space
// first, in Discord's own order — the member list collapses, then the channel
// sidebar shrinks toward its 180px drag-handle minimum. `RAIL` is the fixed
// server-rail width; `HANDLE` is each 6px drag handle.
const RAIL: f32 = 72.0;
const HANDLE: f32 = 6.0;
/// The minimum width the main message column is guaranteed. The two side
/// panels are what gets squeezed below this (the message column is the star).
const MAIN_MIN: f32 = 520.0;
/// The channel sidebar's hard floor — the same 180 the drag handle clamps to,
/// so a narrow window never shrinks it past what a manual drag can reach.
const SIDEBAR_MIN: f32 = 180.0;

/// `(rendered sidebar width, rendered member width)` for the current viewport.
/// Keeps the main message column at a hard floor of `MAIN_MIN` and squeezes
/// the two side panels to make room, in Discord's own order: the member list
/// sheds its space first (all the way down to `0` = fully collapsed) before the
/// channel sidebar is ever shrunk (toward the 180px handle floor). When the
/// viewport is wide enough to hold both panels at their chosen sizes, both are
/// returned exactly as the user set them — so widening the window back up
/// past the squeeze automatically restores a manual drag choice for free. The
/// manual signals are only ever *read* here, never written, so a drag choice is
/// never fought. Reading `viewport_size()` inside the reactive style closures
/// that call this is what makes the layout re-fire on every real resize (the
/// framework's own signal — no hand-rolled resize detection).
/// `(rendered sidebar width, rendered member width)` for the given viewport
/// width and the user's manual widths. Keeps the main message column at a hard
/// floor of `MAIN_MIN` and squeezes the two side panels to make room, in
/// Discord's own order: the member list sheds its space first (all the way
/// down to `0` = fully collapsed) before the channel sidebar is ever shrunk
/// (toward the 180px handle floor). When the viewport is wide enough to hold
/// both panels at their chosen sizes, both are returned exactly as the user set
/// them — so widening the window back up past the squeeze automatically
/// restores a manual drag choice for free. `sidebar`/`member` are the user's
/// manual widths; they are only ever *read*, never written, so a drag choice is
/// never fought.
///
/// Pure (no signal reads) so it can be unit-tested against exact pixel
/// budgets; `vw` is the value `viewport_size()` reports. The caller (the
/// reactive style closures) supplies that live value.
fn effective_panel_widths_for(vw: f32, sidebar: f32, member: f32) -> (f32, f32) {
    let s = sidebar.max(SIDEBAR_MIN);
    let m = member.max(0.0);
    // Before the backend reports a real size (vw == 0) there is nothing to be
    // responsive about — keep the manual widths (the normal desktop sizes) so
    // the first paint is the same as the pre-resize behaviour, no collapse
    // flash.
    let budget = (vw - RAIL - HANDLE * 2.0 - MAIN_MIN).max(0.0);
    if vw <= 0.0 || budget >= s + m {
        return (s, m);
    }
    // Not enough room for both at their chosen sizes: shed the excess, member
    // first. The member panel absorbs the overflow down to 0; only what it
    // can't absorb then pulls the sidebar down, but never below its floor.
    let shed = (s + m) - budget;
    let m_out = m - m.min(shed);
    let s_shed = (shed - m.min(shed)).min((s - SIDEBAR_MIN).max(0.0));
    let s_out = (s - s_shed).max(SIDEBAR_MIN);
    (s_out, m_out)
}

/// Thin signal-reading wrapper over [`effective_panel_widths_for`]: reads the
/// live `viewport_size()` and the user's manual `sidebar_width` /
/// `member_panel_width` signals, then folds the viewport into the rendered
/// widths. Called *inside* the reactive style closures (see `sidebar_style`
/// and `member_panel`'s `panel_style`), which is what subscribes the layout to
/// the framework's `viewport_size()` signal — every real window resize re-runs
/// the closure and re-resolves the width (no hand-rolled resize detection).
fn effective_panel_widths(sidebar: Signal<f32>, member: Signal<f32>) -> (f32, f32) {
    let vw = viewport_size().get().width;
    effective_panel_widths_for(vw, sidebar.get(), member.get())
}

#[cfg(test)]
mod grouping_tests {
    use super::*;

    /// Discord convention: same author within the 7-minute window groups.
    #[test]
    fn same_author_within_window_groups() {
        assert!(same_group("alice", 1_000_000, "alice", 1_000_000 + 419));
        assert!(same_group("alice", 1_000_000, "alice", 1_000_000));
    }

    /// Same author past the window → fresh header.
    #[test]
    fn same_author_past_window_breaks() {
        assert!(!same_group("alice", 1_000_000, "alice", 1_000_000 + 420));
    }

    /// Different author → always breaks, regardless of timing.
    #[test]
    fn different_author_breaks() {
        assert!(!same_group("alice", 1_000_000, "bob", 1_000_001));
    }

    /// A message older than the previous one (out of order) never groups.
    #[test]
    fn out_of_order_never_groups() {
        assert!(!same_group("alice", 1_000_000, "alice", 999_999));
    }
}

#[cfg(test)]
mod responsive_width_tests {
    use super::*;

    /// Both panels at their manual 240px fit comfortably in a 1400px window
    /// → returned unchanged (the restore case: widening back up past the
    /// squeeze keeps the user's manual choice).
    #[test]
    fn wide_viewport_keeps_manual_widths() {
        assert_eq!(effective_panel_widths_for(1400.0, 240.0, 240.0), (240.0, 240.0));
        // A user who manually dragged the panels wider is also left alone.
        assert_eq!(effective_panel_widths_for(1900.0, 380.0, 380.0), (380.0, 380.0));
    }

    /// At 1000px the 480px of panels + 72 rail + 12 handles + 520 main floor =
    /// 1084 > 1000, so the excess (84) is shed from the member panel first:
    /// member 240 -> 156, sidebar stays 240.
    #[test]
    fn mid_viewport_shrinks_member_first() {
        let (s, m) = effective_panel_widths_for(1000.0, 240.0, 240.0);
        assert_eq!(s, 240.0, "channel sidebar untouched while member can absorb");
        assert_eq!(m, 156.0, "member panel sheds the 84px overflow first");
    }

    /// At 820px the budget for panels is 820 - 72 - 12 - 520 = 216. The 480px
    /// of panels overflows by 264: the member panel collapses entirely
    /// (240 -> 0) absorbing 240 of it, and the sidebar sheds the remaining 24,
    /// down to 216 — never below its 180 floor. (At 860 the overflow is only
    /// 224, so the member panel alone absorbs it down to 16 and the sidebar is
    /// untouched — the collapse boundary is right around here.)
    #[test]
    fn narrow_viewport_collapses_member_then_shrinks_sidebar() {
        let (s, m) = effective_panel_widths_for(820.0, 240.0, 240.0);
        assert_eq!(m, 0.0, "member panel fully collapsed first");
        assert_eq!(s, 216.0, "sidebar sheds only what the member couldn't absorb");
    }

    /// Below the point where even sidebar=180 fits, the sidebar pins at its
    /// 180px drag-handle floor and the member panel stays fully collapsed —
    /// it is never pushed below 0, and the sidebar never below 180.
    #[test]
    fn very_narrow_viewport_floors_sidebar_and_collapses_member() {
        let (s, m) = effective_panel_widths_for(760.0, 240.0, 240.0);
        assert_eq!(s, 180.0, "sidebar held at the 180px drag-handle floor");
        assert_eq!(m, 0.0, "member panel collapsed to 0, not negative");
    }

    /// Before any backend reports a viewport size (width 0) the helper keeps
    /// the manual widths — no first-paint collapse flash.
    #[test]
    fn zero_viewport_keeps_manual_widths() {
        assert_eq!(effective_panel_widths_for(0.0, 240.0, 240.0), (240.0, 240.0));
    }
}

fn app_shell(authed: Signal<bool>) -> Element {
    // `app_shell` is only reached once `authed` is true, which a "Continue as
    // guest" also flips — so `authed` alone can't tell a real account from a
    // guest. The real-account sentinel is a non-zero signed-in user id
    // (`CURRENT_USER_ID`, set on login/signup and on a restored session, and
    // never set for a guest). This is read once, here in `app_shell` scope:
    // `app_shell` is re-invoked on every `authed` flip (see the `switch` in
    // `app`), so the value is always the signed-in one at build time (the
    // `user_panel` idiom). NOTE: this used to key off `current_username()`
    // instead, but a restored session (page refresh) only re-seeds
    // `CURRENT_TOKEN`/`CURRENT_USER_ID` from localStorage — not the username —
    // so that check was always false after a refresh, incorrectly treating a
    // real, still-logged-in account as a guest and showing the mock demo
    // servers again.
    let is_real_user =
        crate::auth::CURRENT_USER_ID.load(std::sync::atomic::Ordering::SeqCst) != 0;
    // [Feature 16] `active_server`/`active_channel` are the *mock* selection.
    // They keep their `SERVERS[0]` default (a demo server) but that default is
    // only ever rendered for guests: a real account either lands on its own
    // live server (the auto-enter effect below sets the live-mode signals) or
    // sees the onboarding empty state, and the panes' mock fallback branches
    // are suppressed for real users (see each pane's `is_real_user` gate).
    // `app_shell` is only reached once `authed` is true, which a "Continue as
    // guest" also flips — so `authed` alone can't tell a real account from a
    // guest; the real-account sentinel is a non-empty signed-in username
    // (`current_username` records it on login/signup and clears on logout).
    // Read once here in `app_shell` scope: `app_shell` is re-invoked on every
    // `authed` flip (see the `switch` in `app`), so this is the signed-in value
    // at build time (the `user_panel` idiom).
    let active_server: Signal<u32> = signal(SERVERS[0].id);
    let active_channel: Signal<u32> = signal(SERVERS[0].first_text_channel().id);
    // Resizable column widths (CSS pixels), driven by the thin drag handles
    // between the channel sidebar / main view and the main view / member list.
    // Both start at the 240px the columns used to be hard-coded to; the handles
    // clamp to 180..=400. `Signal` is `Copy`, so the drag closures and the
    // reactive style closures below each capture their own handle.
    let sidebar_width: Signal<f32> = signal(240.0);
    let member_panel_width: Signal<f32> = signal(240.0);
    let composer: Signal<String> = signal(String::new());
    // Emoji-picker popover: `true` = the floating emoji grid is open above the
    // smile button. Pure local UI state (no backend), toggled by the smile
    // icon and closed by picking an emoji or tapping elsewhere.
    let emoji_open: Signal<bool> = signal(false);
    // Pending message-image attachments staged by the composer's attach
    // button: each entry is `(staging_key, filename, mime, data_url)` where
    // `data_url` is the encoded `data:image/…;base64,` URL, already filled by
    // the file-picker callback BEFORE the entry is staged (staging happens
    // when the async pick completes), so the string never mutates in place.
    // The thumbnail strip above the composer reads this signal (keyed
    // `for …, key =` loop); an 'x' on a thumbnail removes that entry; Send
    // consumes the list alongside the composer text. Keys are unique via
    // `attach_next_key`.
    let pending_attachment: Signal<Vec<PendingImage>> = signal(Vec::new());
    let attach_next_key: Signal<i64> = signal(0);
    // The attach flow's in-flight flag and visible rejection line (same
    // triple idiom as the profile screen's avatar upload).
    let attach_busy: Signal<bool> = signal(false);
    let attach_error: Signal<String> = signal(String::new());
    // Persisted/live attachments per wire message id (`message_id` ->
    // attachments carried on the `WsServerMsg::Message` frames — history
    // pages and live fan-out alike). Plain buffer (not a signal): the
    // message-list `switch` already rebuilds on `live_revision`, which the
    // gateway effect bumps whenever a message (and its attachments) land.
    let live_attachments: Rc<RefCell<HashMap<i64, Vec<discord_server::server_settings::MessageAttachment>>>> =
        Rc::new(RefCell::new(HashMap::new()));
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
    // A send failed: `true` = the most recent live send was rejected by the
    // backend (the gateway replies with an `Error` frame, handled below in
    // `gateway_incoming_effect`). The "Failed to send - Retry" banner above the
    // composer reads this; clicking it clears the signal to dismiss the banner
    // (the user resends to retry). Pure UI state — the gateway `send` itself is
    // fire-and-forget, so this is the app's signal for a failed send.
    let send_failed: Signal<bool> = signal(false);
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

    // --- live/real message state (composer image attachments ride this) ----
    // Separate from the mock `sent`/`revision` above: when `active_live_channel`
    // is `Some(id)` the message list reads from here instead of the mock path.
    // channel_id -> Vec<LiveMsg>, oldest-first (the gateway's History replies
    // arrive oldest-first, and new frames append in order). Mutating the Vec
    // does NOT trigger reactivity, so `live_revision` is bumped on every frame
    // the buffer absorbs and folded into the list `switch`'s key — exactly the
    // mock `revision` idiom.
    let live_messages: Rc<RefCell<HashMap<i64, Vec<LiveMsg>>>> =
        Rc::new(RefCell::new(HashMap::new()));
    let live_revision: Signal<u32> = signal(0);

    // --- message reactions (shared by the MOCK and the LIVE message lists) --
    // Per-message reaction state: `reactions` is the pill map (see the `ReactionMap`
    // doc for the keying), `picker` names the message whose add-reaction overlay
    // is open (`None` while closed), and `reaction_rev` is the rebuild counter the
    // message-list `switch` folds into its key so a pill change re-renders the
    // list. MOCK pills are session-local (seed demo); LIVE pills are persisted
    // server-side via the `add_reaction` / `remove_reaction` RPCs and restored
    // on cold loads from `WsServerMsg::Message.reactions` (history frames) plus
    // updated live by the `ReactionAdd` / `ReactionRemove` broadcast frames.
    // Declared HERE (before `gateway_incoming_effect`, whose `move` closure
    // captures these handles — a `switch` effect cannot see a binding defined
    // after its own call) and read by both paths.
    let reactions: Rc<RefCell<ReactionMap>> = Rc::new(RefCell::new(HashMap::new()));
    let reaction_rev: Signal<u32> = signal(0);
    let picker: Signal<Option<MessageKey>> = signal(None);
    // Scroll-back state for the live message list:
    // - `live_at_bottom` is `true` while the scroll view is at (or near) the
    //   content bottom — the "follow the live tail" mode the list starts in.
    //   Flipped to `false` the moment the user scrolls up away from the bottom
    //   (the `on_scroll` max-offset heuristic) and back to `true` on
    //   `on_end_reached`. Drives the floating "jump to bottom" button.
    // - `live_scroll_handle` holds the `ScrollViewHandle` the scroll view hands
    //   us in its `on_handle` callback, so the "load earlier" and "jump to
    //   bottom" buttons can scroll the pane programmatically (and, when
    //   prepending, restore the user's reading position).
    let live_at_bottom: Signal<bool> = signal(true);
    let live_scroll_handle: Rc<RefCell<Option<ScrollViewHandle>>> =
        Rc::new(RefCell::new(None));
    // The maximum scroll offset the live pane has reported, kept so the
    // `on_scroll` max-offset heuristic can tell a scroll UP (away from the
    // bottom) from a scroll DOWN (back to the live tail). An offset-only
    // signal — `on_scroll` only reports the absolute `scrollTop`, never the
    // content height (that is `on_end_reached`'s job), so "scrolled up from
    // the bottom" is detected as `y < max_seen - threshold`, not as
    // `y > content - viewport`.
    let live_max_offset: Rc<RefCell<f32>> = Rc::new(RefCell::new(0.0));
    // Per-message edit mode: the wire `message_id` whose row currently shows
    // the inline edit input (None = none). One at a time — switching rows
    // just overwrites it, and the list rebuild (keyed on `live_revision` and
    // this signal's value) tears down the old input and mounts the new one.
    // It must be a top-level `Signal` (not a `Rc<RefCell>`) because the
    // `text_input value = …` prop needs a reactive `Signal<String>`.
    let live_editing: Signal<Option<i64>> = signal(None);
    // The draft text for the row being edited. A top-level `Signal` because the
    // inline `text_input value = …` prop needs a reactive handle; it is seeded
    // with the row's content the moment a row enters edit mode (below) and read
    // back on confirm. Reset to `""` after a confirmed edit so the next edit
    // starts from the row's (now-updated) content, not a stale draft.
    let live_edit_text: Signal<String> = signal(String::new());
    // Per-message delete confirm: the wire `message_id` whose row is showing
    // "are you sure?" (None = none). One step, mirroring the composer's
    // "simple confirm" ask — no two-step kick-style confirm.
    let live_delete_confirm: Signal<Option<i64>> = signal(None);
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
        match crate::servers::list_servers().await {
            Ok(servers) => {
                live_servers.set(servers);
            }
            // The first authenticated backend call after a refresh. If the
            // restored token is no longer valid, this fails: clear the saved
            // session and sign the user back to the login screen with a "session
            // expired" note. Guarded by `RESTORED` so a transient blip on a
            // freshly issued token (network drop, brief 500) does NOT log a
            // working session out.
            Err(_) => {
                if crate::app::RESTORED
                    .compare_exchange(
                        true,
                        false,
                        std::sync::atomic::Ordering::SeqCst,
                        std::sync::atomic::Ordering::SeqCst,
                    )
                    .is_ok()
                {
                    crate::auth::clear_session();
                    *SESSION_NOTE
                        .lock()
                        .unwrap_or_else(|p| p.into_inner()) =
                        Some("Session expired, please log in again.".to_string());
                    authed.set(false);
                }
            }
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
    // Scrollback paging for the live message list. `load_busy` is `true` while a
    // `History { before_id }` request is in flight (gates the "Load earlier"
    // pill's tap so a double-tap can't fire a second page; the pill's label
    // goes muted while `true`). The gateway effect clears it on the
    // `WsServerMsg::History` marker frame (which arrives before the page's
    // `Message { new: false }` frames). `has_more_earlier` is set by the same
    // marker frame from the server's `has_more` field (a full page = more
    // older messages exist). Both signals are declared HERE — before the
    // gateway effect below captures them — because Rust requires a value in
    // scope at the point a closure captures it (same pattern as `notif_level`
    // etc. above).
    let load_busy: Signal<bool> = signal(false);
    let has_more_earlier: Signal<bool> = signal(true);
    // Live @mention indicator state, driven by the real backend data (the
    // mock path's `c.mentions`/`total_mentions` is its own, untouched copy).
    // `live_mentioned` maps a live channel id -> count of UNREAD messages that
    // mention the signed-in user. It is bumped by the gateway's inbound
    // `Message { mentioned_user_ids, new: true }` frames and zeroed when the
    // user opens the channel (the local "read" reset — the backend's own
    // per-channel unread-mentions is exposed via `unread_counts(server_id)`,
    // but a live client that is actively watching only needs the local count).
    // Mutating the HashMap does not trigger reactivity, so `live_mention_rev`
    // is bumped on every change and folded into the sidebar `switch` key —
    // exactly the `live_revision` idiom used for the message buffer.
    let live_mentioned: Rc<RefCell<HashMap<i64, i64>>> = Rc::new(RefCell::new(HashMap::new()));
    let live_mention_rev: Signal<u32> = signal(0);
    // Live per-channel UNREAD COUNTS from the backend, fetched alongside the
    // channel list whenever a live server is selected (`unread_counts` RPC).
    // `count` = messages not yet read; `unread_mentions` = the subset that
    // @mention the signed-in user (0..=count). This is the REAL data source
    // for the live sidebar's badges — independent of `live_mentioned` above,
    // which stays the gateway-incrementing local indicator for the currently
    // open session. Mutating the HashMap does not trigger reactivity, so
    // `live_unread_rev` is bumped on every change and folded into the
    // sidebar `switch` key — exactly the `live_mention_rev` idiom.
    let live_unread: Rc<RefCell<HashMap<i64, (i64, i64)>>> = Rc::new(RefCell::new(HashMap::new()));
    let live_unread_rev: Signal<u32> = signal(0);
    // A second `Rc` handle for the rail's live-section fetch closure: both
    // that `move` closure and the sidebar `switch`'s `move` render closure
    // need the map, and `Rc` is not `Copy`, so each captures its own clone
    // (the clones share the same `RefCell` — that's the point).
    let rail_unread: Rc<RefCell<HashMap<i64, (i64, i64)>>> = live_unread.clone();

    // Live typing-indicator state, keyed per live channel: the user most
    // recently seen typing in that channel, as `(user_id, display_label, gen)`.
    // `gen` is bumped on every inbound `Typing` frame for the channel; it is the
    // guard that lets a 5-second auto-clear `after_ms_detached` task
    // self-cancel — it fires only if the captured `gen` still matches the map's
    // current entry for that channel, so a newer frame (which bumps `gen`)
    // turns the older, still-pending clear into a no-op instead of wiping a
    // fresh indicator. Only one user per channel is tracked — the latest typer
    // — which matches a single "… is typing…" line (the server fans out every
    // member's typing to the whole channel audience). Mutating the `Rc` does
    // not trigger reactivity, so `live_typing_rev` is bumped on every change
    // and folded into the typing row's `switch` key (the `live_revision` idiom).
    let live_typing: Rc<RefCell<HashMap<i64, (i64, String, i64)>>> =
        Rc::new(RefCell::new(HashMap::new()));
    let live_typing_rev: Signal<u32> = signal(0);

    // Live member-roster state for the moderation panel (LIVE mode only). The
    // roster is fetched via the new backend `list_members` endpoint when a live
    // server is selected (see the rail on_touch below) and re-fetched after a
    // successful kick/role-change. `live_kick_confirm` is the two-step inline
    // confirm for a kick: None = no pending kick, Some(user_id) = "are you
    // sure?" is shown for that member's row. `live_member_error` surfaces a
    // backend authorization error (e.g. "you need moderator privileges") as
    // inline text rather than a modal.
    let live_members: Signal<Vec<crate::moderation::MemberInfo>> = signal(Vec::new());
    let live_kick_confirm: Signal<Option<i64>> = signal(None);
    // The two-step inline confirm for a ban (the ban is the more severe
    // action, so it gets its own confirm rather than sharing the kick one —
    // a mis-tap that confirms a ban is harder to undo than a mis-tap kick).
    let live_ban_confirm: Signal<Option<i64>> = signal(None);
    // The two-step inline confirm for a timeout: None = no pending timeout,
    // Some(user_id) = "Timeout? [Yes/No]" is shown for that member's row. A
    // timeout silences a member from posting for `live_timeout_duration`
    // without removing their membership (the milder of the three severities).
    let live_timeout_confirm: Signal<Option<i64>> = signal(None);
    // The chosen timeout duration in seconds, shared with the duration picker
    // the confirm branch renders. Defaults to 60s (the picker's first option)
    // so a bare Timeout tap still has a sensible duration. The picker writes
    // to this signal as the user taps an option; the confirm's "Yes" reads it.
    let live_timeout_duration: Signal<i64> = signal(60);
    let live_member_error: Signal<String> = signal(String::new());
    // Which member row's Kick/Ban/Timeout popup menu is currently open
    // (`Some(user_id)` = that member's menu; `None` = closed). A member-row
    // tap sets it; a menu entry arms the per-action confirm signal above and
    // clears this. Part of the member panel's `switch` key so the portal
    // mounts/unmounts on tap.
    let live_member_menu: Signal<Option<i64>> = signal(None);
    // Which channel row's management menu (Rename / Delete) is currently open
    // (`Some(channel_id)`; `None` = closed). A gear tap on a hovered channel
    // row sets it; the gear itself renders ONLY while the row is hovered and
    // the acting user is moderator+ (the per-row idiom `message_action_bar`
    // uses for message rows — a per-row `hovered` signal fed by `on_hover`).
    let live_chan_menu: Signal<Option<i64>> = signal(None);
    // Live presence: `user_id -> status string`. The backend broadcasts
    // `WsServerMsg::Presence` ('online' on socket auth + activity, 'away' after
    // `PRESENCE_IDLE_SECS` of no inbound frames, 'offline' on the user's last
    // socket disconnect) over the gateway, so the member panel can show a
    // per-row status dot. Mutating the `Rc` does not trigger reactivity, so
    // `live_presence_rev` is bumped on every frame and folded into the member
    // panel's `switch` key (the `live_revision` / `live_typing_rev` idiom). A
    // member with no frame yet (just after the roster loads) reads as absent
    // and is rendered as a grey "offline" dot.
    let live_presence: Rc<RefCell<HashMap<i64, String>>> =
        Rc::new(RefCell::new(HashMap::new()));
    let live_presence_rev: Signal<u32> = signal(0);

    // Friends panel (live): the panel snapshot of `list_friends`, flat + tagged
    // (`0 = incoming / 1 = outgoing / 2 = accepted`) so the panel's `switch`
    // can key on the whole vector. The ACTOR half of the server's sync path is
    // in `friends_view` (each mutating RPC re-fetches and `set()`s this); the
    // OTHER user's half is the `FriendChanged` gateway branch below, which
    // re-fetches into the same signal — the snapshot IS the rebuild key. The
    // tab selector (0 = Online, 1 = All, 2 = Pending, 3 = Add Friend) and the
    // Add Friend draft + inline server error are owned here so a panel rebuild
    // never drops the typed username.
    let friends_snap: Signal<Vec<crate::friends_view::FriendRow>> = signal(Vec::new());
    let friends_tab: Signal<u8> = signal(1);
    let friends_add_username: Signal<String> = signal(String::new());
    let friends_add_error: Signal<String> = signal(String::new());
    // Whether the Friends panel is showing in the live-DM main area (the
    // "Friends" entry at the top of the live DM column toggles it; while
    // false the main area shows the selected DM conversation as before).
    let friends_open: Signal<bool> = signal(false);

    // Realtime gateway: the signed session token authenticates the connect
    // (the backend verifies it). It rides the connect URL's `?args=` query —
    // a browser websocket cannot set request headers, so the token can't go on
    // an Authorization header here the way the REST calls do. Non-async hook;
    // call it once here. An empty token (guest, before a successful login)
    // simply gets no working socket, so the status label below shows
    // Closed/Error. Declared BEFORE the auto-enter effect below because that
    // effect sends the saved channel's initial `History` frame over the
    // gateway (same as a manual channel-row tap), and `UseSocket` is `Copy`
    // so the effect's `async move` block just captures it.
    let gateway = discord_server::server_settings::message_gateway(
        crate::auth::current_token(),
    );

    // [Feature 16] A real account lands on its own live server, not the mock
    // demo. When the list resolves and the user has at least one live server,
    // auto-enter one of them (select it + pull its channels / roster /
    // unread), mirroring the rail live-tile enter handler. Guests have no
    // backend identity, so `list_servers` returns empty and this is a no-op —
    // they keep the mock default. A real account with zero servers stays on
    // the onboarding empty state (no live server to enter).
    //
    // Which server/channel: prefer the exact spot `crate::auth::save_last_view_*`
    // recorded the last time this account entered a live server / selected a
    // live channel (or opened a live DM) — so a refresh lands back where the
    // user left off, not wherever `list_servers` happens to put first. The key
    // is per user id, so two accounts in one browser each restore THEIR OWN
    // last view. Falls back to the first server's first text channel when
    // there's nothing saved, or the saved ids are stale (the server/channel is
    // gone or the channel isn't a text channel in this server), or the saved
    // value is unparseable.
    {
        let ls = live_servers.clone();
        let als = active_live_server.clone();
        let alc = active_live_channel.clone();
        let lch = live_channels.clone();
        let lmbrs = live_members.clone();
        let lmbr_err = live_member_error.clone();
        let lkick = live_kick_confirm.clone();
        let lbcm = live_ban_confirm.clone();
        let ltcm = live_timeout_confirm.clone();
        let lunread = rail_unread.clone();
        let lunread_rev = live_unread_rev.clone();
        // The live list's scroll state re-arms for the restored channel exactly
        // as the channel-row select handler does (the pane starts "following
        // the live tail" at the bottom, and the fresh pane's `on_handle` gets
        // to capture its own scroll handle).
        let latbot = live_at_bottom.clone();
        let lhandle = live_scroll_handle.clone();
        let is_real = is_real_user;
        runtime_core::driver::spawn_async(async move {
            if !is_real {
                return;
            }
            // Fetch directly rather than reading `ls.get()`: `live_servers`
            // is populated by a SEPARATE spawn_async'd fetch above with no
            // ordering guarantee relative to this one, so reading the signal
            // here raced it and (near-)always saw the pre-fetch empty `Vec`,
            // silently no-op'ing this whole effect. Awaiting our own fetch
            // makes the decision authoritative regardless of scheduling.
            let Ok(servers) = crate::servers::list_servers().await else {
                return;
            };
            ls.set(servers.clone());
            // The saved last view (kind 0 = server+channel, kind 1 = open DM).
            // A DM-form save carries no server preference: the channel half of
            // the restore then follows the plain first-server fallback.
            let saved = crate::auth::load_last_view();
            let saved_sid = match saved {
                Some((0, sid, _)) => Some(sid),
                _ => None
            };
            let Some(sid) = saved_sid
                .filter(|id| servers.iter().any(|s| s.id == *id))
                .or_else(|| servers.first().map(|s| s.id))
            else {
                return; // no live servers: nothing to auto-enter
            };
            // Already entered (e.g. a rail tap beat this) — don't clobber it.
            if als.get().is_some() {
                return;
            }
            lkick.set(None);
            lbcm.set(None);
            ltcm.set(None);
            lmbr_err.set(String::new());
            lmbrs.set(Vec::new());
            if let Ok(channels) = crate::servers::list_channels(sid).await {
                let first = channels
                    .iter()
                    .find(|c| c.kind == "text")
                    .map(|c| c.id);
                // Prefer the SAVED channel over the first text channel — but
                // only when it belongs to the server we actually entered and
                // is a text channel there (a deleted/renamed channel or a
                // server change makes the saved half stale; stale halves fall
                // back independently).
                let saved_cid = match saved {
                    Some((0, s, c))
                        if s == sid && channels.iter().any(|ch| ch.kind == "text" && ch.id == c) =>
                        Some(c),
                    _ => None
                };
                lch.set(channels);
                let chosen = saved_cid.or_else(|| first);
                if let Some(id) = chosen {
                    alc.set(Some(id));
                    // Re-arm the live list's scroll state for the restored
                    // channel — identical to the channel-row select handler
                    // (start "following the live tail" at the bottom, and
                    // drop the old pane's captured scroll handle so the
                    // fresh pane's `on_handle` callback can capture its own).
                    latbot.set(true);
                    *lhandle.borrow_mut() = None;
                    gateway.send(
                        discord_server::server_settings::WsClientMsg::History {
                            channel_id: id,
                            before_id: None,
                        },
                    );
                }
            }
            if let Ok(members) = crate::moderation::list_members(sid).await {
                lmbrs.set(members);
            }
            if let Ok(counts) = crate::servers::unread_counts(sid).await {
                let buf = lunread.clone();
                buf.borrow_mut()
                    .extend(counts.into_iter().map(|c| (c.channel_id, (c.count, c.unread_mentions))));
                lunread_rev.set(lunread_rev.get() + 1);
            }
            als.set(Some(sid));
        });
    }

    // Realtime gateway: the signed session token authenticates the connect
    // (the backend verifies it). Declared above (before the auto-enter
    // effect) so that effect can send the restored channel's initial
    // `History` frame — see the note there.
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

    // Connection-lost banner for the message column. Keyed on the same reactive
    // status string as `gateway_status` above, but rendered as a full-width red
    // bar at the top of the message pane (NOT the rail dot). It shows only when
    // the socket is `Closed` or `Error`; while `Open` / `Connecting` it renders
    // a 0-height view (nothing), so the layout is unchanged in the healthy
    // state. The socket auto-reconnects (`message_gateway` owns the loop), so
    // this is a passive "you're disconnected" indicator, not a retry button.
    let conn_lost_banner = switch(
        move || format!("{:?}", gateway.status()),
        move |status: &String| {
            let lost = status == "Closed" || status == "Error";
            if !lost {
                // Healthy: render an inert 0-height view (the no-op-render
                // idiom) so the switch's value is a valid `Element` and the
                // layout is unchanged.
                return ui! {
                    view(style = StyleRules {
                        height: Some(px(0.0)),
                        ..Default::default()
                    }) {}
                };
            }
            ui! {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    gap: Some(px(8.0)),
                    padding_top: Some(px(8.0)),
                    padding_bottom: Some(px(8.0)),
                    background: Some(col("#f23f43")),
                    ..Default::default()
                }) {
                    text(style = StyleRules {
                        font_size: Some(px(13.0)),
                        font_weight: Some(FontWeight::SemiBold),
                        color: Some(col("#ffffff")),
                        ..Default::default()
                    }) { "Connection lost — reconnecting…".to_string() }
                }
            }
        },
    );

    // Reactive "watch every frame" effect: `switch` re-runs its arm whenever
    // frame. This one runs a SIDE EFFECT (absorb the message into `live_messages`
    // and bump `live_revision`), not a render — so it returns an inert 0-height
    // `view` (the no-op-render idiom; the element itself is never seen, mounted
    // in the rail below). Only `Message` frames are buffered; `History` replies
    // surface as a run of `Message { new: false }` frames, which land here too.
    // `created_at` arrives as epoch MILLISECONDS from the server; the client's
    // `format_when`/`now_secs` work in SECONDS, so divide by 1000 on ingest.
    let live_buf = live_messages.clone();
    let live_rev = live_revision.clone();
    let mentioned_buf = live_mentioned.clone();
    let mention_rev = live_mention_rev.clone();
    let open_chan = active_live_channel.clone();
    let unread_live = live_unread.clone();
    let unread_live_rev = live_unread_rev.clone();
    let my_id = crate::auth::CURRENT_USER_ID.load(std::sync::atomic::Ordering::SeqCst);
    let typing_buf = live_typing.clone();
    let typing_rev = live_typing_rev.clone();
    // A send failure: an `Error` frame from the backend (the server answers a
    // failed `Send` / `EditMessage` / `DeleteMessage` on this socket) sets
    // `send_failed` so the "Failed to send - Retry" banner above the composer
    // appears. A successful send is echoed as a `Message` frame instead, which
    // is handled above — so only a genuine failure flips this.
    let send_failed_sig = send_failed.clone();
    // Scrollback markers: cleared/set by the `WsServerMsg::History` marker frame
    // the server sends before each page (see the `History` arm below). Cloned so
    // the `move` closure owns them (same idiom as `send_failed_sig`).
    let load_busy_sig = load_busy.clone();
    let has_more_sig = has_more_earlier.clone();
    let presence_buf = live_presence.clone();
    let presence_rev = live_presence_rev.clone();
    // Reaction handles cloned so the gateway effect's `move` closure owns them:
    // history `Message` frames restore persisted pills, and the `ReactionAdd`
    // / `ReactionRemove` broadcast frames update them live for every viewer.
    let reactions_map = reactions.clone();
    let reaction_rev_sig = reaction_rev.clone();
    // Persisted/live attachments cloned so the gateway effect can store each
    // message frame's `attachments` list for the row renderer below.
    let attach_buf = live_attachments.clone();
    // [Feature 14] A `MemberChanged` frame (another account joined or was
    // kicked/banned) re-fetches the active server's roster — the same
    // `list_members` call the kick/ban/role-change callbacks use — so the
    // live member panel's rows AND its "MEMBERS — N" header update for a
    // remote change (the pre-fix gap: those only ever re-fetched on the
    // local user's own action). Captured as clones so the `move` render
    // closure owns them, mirroring `presence_buf` / `presence_rev` above.
    let member_server = active_live_server.clone();
    let member_roster = live_members.clone();
    // [Feature 10] Notification wiring. `notif_level` (all/mentions/nothing) is
    // the single source of truth shared with the `UserSettings` Notifications
    // tab — seeded from localStorage (falling back to 0 = all) so a user who
    // last picked "Nothing" isn't flooded before re-confirming their choice.
    // The three transient signals drive the top-right in-app toast, read
    // reactively by `notification_toast::notification_toast` (built once at the
    // root). Declared HERE — before the gateway effect below captures them —
    // because Rust requires a value in scope at the point a closure captures
    // it (the effect reads them inside its `move |frame|` arm).
    let notif_level: Signal<u32> =
        signal(crate::notify::load_mode().unwrap_or(crate::notify::ALL_MESSAGES));
    let notif_toast_visible: Signal<bool> = signal(false);
    let notif_toast_title: Signal<String> = signal(String::new());
    let notif_toast_body: Signal<String> = signal(String::new());
    let notif_level_sig = notif_level.clone();
    let notif_toast_sig = notif_toast_visible.clone();
    let notif_toast_title_sig = notif_toast_title.clone();
    let notif_toast_body_sig = notif_toast_body.clone();
    let gateway_incoming_effect = switch(
        move || gateway.incoming().get(),
        move |frame: &Option<discord_server::server_settings::WsServerMsg>| {
            if let Some(discord_server::server_settings::WsServerMsg::Message {
                message_id,
                channel_id,
                author_id,
                author_username,
                author_avatar,
                content,
                created_at,
                edited_at,
                new,
                mentioned_user_ids,
                reply_to_message_id,
                reactions,
                attachments,
            }) = frame
            {
                // The server resolved the author's username for the frame
                // (`WsServerMsg::Message.author_username`), so the row shows a
                // real name instead of an id. `author_id` still rides along for
                // the "@me / self" checks below. The author's avatar rides the
                // same frame and lands in the shared registry, so the row's
                // avatar chip and the member list can draw it.
                let label = author_username.clone();
                crate::auth::remember_avatar(label.as_str(), author_avatar.clone());
                // Push the full 6-tuple: the wire `message_id` + the optional
                // `edited_at` (epoch millis -> secs, like `created_at`) ride
                // along so the row's edit affordance and "(edited)" marker can
                // find the message later. History frames (`new: false`) may
                // carry a non-None `edited_at` for messages already edited
                // before this session, so store it as-is.
                //
                // Sorted-insert (not a blind `.push`): the buffer is kept
                // (created_at_secs, message_id) ascending so a scrollback page —
                // strictly older than the loaded tail — stacks in at the front,
                // while a live frame (the newest) sorts to the end, all through
                // the same write path. Duplicate `message_id`s are skipped.
                let mut changed = false;
                {
                    let mut buf = live_buf.borrow_mut();
                    let v = buf.entry(*channel_id).or_default();
                    changed = insert_live_msg(
                        v,
                        (
                            *message_id,
                            *author_id,
                            label,
                            content.clone(),
                            *created_at / 1000,
                            edited_at.map(|ms| ms / 1000),
                            *reply_to_message_id,
                        ),
                    );
                }
                // Bump the revision only when a message actually landed — a pure
                // dedup (re-delivery) is a no-op that must not force a rebuild.
                if changed {
                    live_rev.set(live_rev.get() + 1);
                }
                // Persist the frame's attachments for the row renderer (the
                // same frames — history pages AND live fan-out — carry the
                // persisted `MessageAttachment` lists from the server).
                if !attachments.is_empty() {
                    attach_buf.borrow_mut().insert(*message_id, attachments.clone());
                }
                // Live @mention indicator: a freshly posted (`new: true`) message
                // that names the signed-in user, on a channel that is NOT the one
                // currently open, bumps that channel's unread-mention count.
                // History frames (`new: false`) never count as unread.
                if *new
                    && my_id != 0
                    && mentioned_user_ids.contains(&my_id)
                    && open_chan.get() != Some(*channel_id)
                {
                    *mentioned_buf.borrow_mut().entry(*channel_id).or_insert(0) += 1;
                    mention_rev.set(mention_rev.get() + 1);
                }
                // Real unread counts: a freshly posted (`new: true`) message on a
                // channel that is NOT currently open adds one unread there (and
                // one unread mention when it names the signed-in user — the
                // `unread_mentions` invariant is `0..=count`, so both counters
                // move together). History frames (`new: false`) are the
                // initial-load backlog, already reflected by the `unread_counts`
                // fetch — never double-counted here. Opening the channel resets
                // the entry to `(0, 0)` (the effect below), so a frame landing
                // after the reset just re-adds it.
                if *new && open_chan.get() != Some(*channel_id) {
                    let mut map = unread_live.borrow_mut();
                    let entry = map.entry(*channel_id).or_insert((0, 0));
                    entry.0 += 1;
                    if my_id != 0 && mentioned_user_ids.contains(&my_id) {
                        entry.1 += 1;
                    }
                    drop(map);
                    unread_live_rev.set(unread_live_rev.get() + 1);
                }
                // [Feature 10] Notification: a freshly posted (`new: true`)
                // message that (a) matches the notification level (always for
                // `ALL_MESSAGES`, only when it names me for `MENTIONS_ONLY`,
                // never for `NOTHING`) and (b) lands on a channel that is NOT
                // currently open fires an in-app toast (always, so the user
                // sees it in the app) plus a browser `Notification` when the
                // Notification API is granted. History frames (`new: false`)
                // are the initial-load backlog and never notify. The toast
                // carries the message content + author; the browser
                // notification carries a short line. The channel name is not
                // readily available in this scope (it lives in a separate
                // map), so the toast uses the message content itself, which
                // is the most useful part.
                let is_mention = my_id != 0 && mentioned_user_ids.contains(&my_id);
                let level = notif_level_sig.get();
                let matches = match level {
                    crate::notify::NOTHING => false,
                    crate::notify::MENTIONS_ONLY => is_mention,
                    _ => true, // ALL_MESSAGES (0) or any future value
                };
                if *new && matches && open_chan.get() != Some(*channel_id) {
                    crate::notify::notify_message(
                        author_username,
                        content,
                        is_mention,
                        level,
                        notif_toast_sig,
                        notif_toast_title_sig,
                        notif_toast_body_sig,
                    );
                }
                // Cold-load persistence: a history frame (`new: false`) carries
                // the message's persisted reactions, so restore them into the
                // live pill map keyed by the wire `message_id` (`reacted_by_me`
                // came relative to the socket's own user, resolved server-side
                // from the verified token). Fresh-post echoes (`new: true`) are
                // empty — their later reaction changes ride the frames below.
                if !*new && !reactions.is_empty() {
                    let key = MessageKey::Live { id: *message_id };
                    let map_h = reactions_map.clone();
                    let mut m = map_h.borrow_mut();
                    let inner = m.entry(key).or_default();
                    for (emoji, count, reacted) in reactions {
                        if let Some(p) = inner.iter().position(|(e, _, _)| e == emoji) {
                            inner[p] = (emoji.clone(), *count, *reacted);
                        } else {
                            inner.push((emoji.clone(), *count, *reacted));
                        }
                    }
                    if m.get(&key).map_or(false, |v| v.is_empty()) {
                        m.remove(&key);
                    }
                    drop(m);
                    reaction_rev_sig.set(reaction_rev_sig.get() + 1);
                }
            }
            // A member — including the caller via its own echo — added or
            // removed an emoji reaction. The frame carries the message's
            // post-operation total for the emoji, so apply it authoritatively
            // (SET, not increment): duplicate frames and a cold-load race
            // converge. `count == 0` drops the pill. The highlight flips only
            // on a frame naming THIS client's user; other members' frames keep
            // whatever highlight the pill already has. Bumping `reaction_rev`
            // rebuilds the live list, so every connected client sees it.
            if let Some(discord_server::server_settings::WsServerMsg::ReactionAdd {
                message_id,
                channel_id,
                user_id,
                emoji,
                count,
            }) = frame
            {
                let _ = channel_id;
                let key = MessageKey::Live { id: *message_id };
                let reacted_hint = if *user_id == my_id { Some(true) } else { None };
                if apply_reaction(reactions_map.clone(), key, emoji.clone(), *count, reacted_hint) {
                    reaction_rev_sig.set(reaction_rev_sig.get() + 1);
                }
            }
            if let Some(discord_server::server_settings::WsServerMsg::ReactionRemove {
                message_id,
                channel_id,
                user_id,
                emoji,
                count,
            }) = frame
            {
                let _ = channel_id;
                let key = MessageKey::Live { id: *message_id };
                let reacted_hint = if *user_id == my_id { Some(false) } else { None };
                if apply_reaction(reactions_map.clone(), key, emoji.clone(), *count, reacted_hint) {
                    reaction_rev_sig.set(reaction_rev_sig.get() + 1);
                }
            }
            // A channel message the signed-in user edited: replace the stored
            // content + `edited_at` for `message_id` so the row shows the new
            // text and its "(edited)" marker (the marker reads `edited_at` is
            // `Some`). Mutating the buffer alone would not rebuild the list, so
            // bump `live_revision` (the list `switch`'s key) to re-render.
            if let Some(discord_server::server_settings::WsServerMsg::MessageEdited {
                message_id,
                channel_id,
                content,
                edited_at,
            }) = frame
            {
                let channel_id = *channel_id;
                let message_id = *message_id;
                let edited_secs = *edited_at / 1000;
                let changed = live_buf
                    .borrow_mut()
                    .get_mut(&channel_id)
                    .map(|msgs| {
                        msgs.iter_mut()
                            .find(|m| m.0 == message_id)
                            .map(|m| {
                                m.3 = content.clone();
                                m.5 = Some(edited_secs);
                            })
                            .is_some()
                    })
                    .unwrap_or(false);
                // Also clear a pending edit-mode/confirm for this message so the
                // row drops back to its normal (edited) rendering rather than
                // staying in an input that no longer matches the server.
                if changed {
                    if live_editing.get() == Some(message_id) {
                        live_editing.set(None);
                        live_edit_text.set(String::new());
                    }
                    if live_delete_confirm.get() == Some(message_id) {
                        live_delete_confirm.set(None);
                    }
                    live_rev.set(live_rev.get() + 1);
                }
            }
            // A channel message was deleted: drop its stored entry so it
            // disappears from the list. The `MessageDeleted` frame is the server's
            // authoritative confirmation, so no client-side optimistic removal is
            // needed — the row vanishes once this lands.
            if let Some(discord_server::server_settings::WsServerMsg::MessageDeleted {
                message_id,
                channel_id,
            }) = frame
            {
                let channel_id = *channel_id;
                let message_id = *message_id;
                let changed = live_buf
                    .borrow_mut()
                    .get_mut(&channel_id)
                    .map(|msgs| {
                        let before = msgs.len();
                        msgs.retain(|m| m.0 != message_id);
                        msgs.len() != before
                    })
                    .unwrap_or(false);
                // Clear any pending edit/delete state for the now-gone message so
                // a stale input/confirm can't outlive it.
                if changed {
                    if live_editing.get() == Some(message_id) {
                        live_editing.set(None);
                        live_edit_text.set(String::new());
                    }
                    if live_delete_confirm.get() == Some(message_id) {
                        live_delete_confirm.set(None);
                    }
                    live_rev.set(live_rev.get() + 1);
                }
            }
            // A presence change for a user on a shared server: record
            // `user_id -> status` in `live_presence` and bump `live_presence_rev`
            // so the live member panel's `switch` re-renders (the row's dot
            // updates). The backend sends 'online' on socket auth + activity,
            // 'away' after the idle timeout, and 'offline' on the user's last
            // socket disconnect — so every status we can receive is a real
            // server value (no client-invented states). `frame` is a `&WsServerMsg`
            // borrowed for this one call, so copy the fields out to owned values.
            if let Some(discord_server::server_settings::WsServerMsg::Presence {
                user_id,
                status,
            }) = frame
            {
                let user_id = *user_id;
                let status = status.clone();
                presence_buf.borrow_mut().insert(user_id, status);
                presence_rev.set(presence_rev.get() + 1);
            }
            // Typing frame from another member of the channel: record who is
            // typing in `live_typing` and bump `live_typing_rev` so the
            // typing_row `switch` re-renders. The auto-clear task self-cancels
            // on a newer frame (generation guard) — a fresh frame restarts the
            // 5-second window without clearing the still-visible indicator.
            if let Some(discord_server::server_settings::WsServerMsg::Typing {
                channel_id,
                user_id,
            }) = frame
            {
                // `frame` is a `&WsServerMsg` borrowed for this one call, so
                // copy the fields out to owned `i64`s — the auto-clear closure
                // below outlives `frame` and cannot borrow into it.
                let channel_id = *channel_id;
                let user_id = *user_id;
                let label = format!("user {}", user_id);
                let new_gen = {
                    let mut map = typing_buf.borrow_mut();
                    let entry = map
                        .entry(channel_id)
                        .or_insert((user_id, label.clone(), 0));
                    entry.0 = user_id;
                    entry.1 = label;
                    entry.2 += 1;
                    entry.2
                };
                typing_rev.set(typing_rev.get() + 1);
                // Schedule a one-shot auto-clear. The closure captures CLONES
                // of the shared handles (not the outer `Fn` closure's own
                // `typing_buf`/`typing_rev`, which it must keep using on every
                // frame). After 5 s it clears the entry only if no newer Typing
                // frame has arrived (the stored `gen` still equals `new_gen`).
                let clear_buf = typing_buf.clone();
                let clear_rev = typing_rev.clone();
                after_ms_detached(5_000, move || {
                    let should_clear = {
                        let map = clear_buf.borrow();
                        map.get(&channel_id).map(|e| e.2 == new_gen).unwrap_or(false)
                    };
                    if should_clear {
                        clear_buf.borrow_mut().remove(&channel_id);
                        clear_rev.set(clear_rev.get() + 1);
                    }
                });
            }
            // A member joined or left the server (another account redeemed an
            // invite, or was kicked/banned by someone else). Re-fetch the
            // active server's roster when the frame names it — the SAME
            // `list_members` call the kick/ban/role-change callbacks use — so
            // the live member panel's rows AND its "MEMBERS — N" header
            // reflect the change (the pre-fix gap: the roster only ever
            // re-fetched on the local user's own action). `user_id` /
            // `joined` are carried for completeness (a future
            // "just joined" / "just left" transition could use them), but the
            // re-fetch is the single source of truth, so it's the whole of the
            // handling here. `frame` is a `&WsServerMsg`, so copy the id out.
            if let Some(discord_server::server_settings::WsServerMsg::MemberChanged {
                server_id,
                ..
            }) = frame
            {
                let server_id = *server_id;
                if member_server.get() == Some(server_id) {
                    let sid = server_id;
                    let roster = member_roster.clone();
                    runtime_core::driver::spawn_async(async move {
                        if let Ok(fresh) = crate::moderation::list_members(sid).await {
                            roster.set(fresh);
                        }
                    });
                }
            }
            // A friends-list change involving the peer of one of THIS user's
            // relationships (the backend pushes `FriendChanged { friend_id,
            // action }` to the *other* user on every send/accept/decline/
            // cancel/remove): re-fetch `list_friends` and set the panel
            // snapshot — the same single-source-of-truth re-fetch idiom as the
            // `MemberChanged` branch above. The snapshot IS the panel's `switch`
            // key, so the rows rebuild with no extra revision counter. The
            // acting user re-syncs from their RPC return instead (in
            // `friends_view`), so they never wait on this frame.
            if let Some(discord_server::server_settings::WsServerMsg::FriendChanged { .. }) = frame {
                let snap_sig = friends_snap.clone();
                runtime_core::driver::spawn_async(
                    async move {
                        if let Ok(list) = crate::friends::list_friends().await {
                            snap_sig.set(crate::friends_view::snapshot(list));
                        }
                    },
                );
            }
            // Scrollback marker: the server sends a `History { channel_id,
            // has_more }` frame BEFORE the `Message { new: false }` frames of a
            // page. `has_more` is `true` when the page was a full `limit` (50),
            // meaning more older messages exist. Use it to clear the in-flight
            // guard and update the "Load earlier" pill's visibility. This arm
            // must run before the `Message` frames land (it arrives first on the
            // wire), so it sets state before the list rebuilds.
            if let Some(discord_server::server_settings::WsServerMsg::History {
                channel_id,
                has_more,
            }) = frame
            {
                let cid = *channel_id;
                let active = open_chan.get();
                // Only update scrollback state for the currently open channel —
                // a stale page for a previously open channel must not clobber
                // the new channel's state.
                if active == Some(cid) {
                    load_busy_sig.set(false);
                    has_more_sig.set(*has_more);
                }
            }
            // A backend error (a rejected `Send` / `EditMessage` / `DeleteMessage`
            // — see `message_gateway`, which answers failures on this socket):
            // set `send_failed` so the "Failed to send - Retry" banner above the
            // composer shows. A successful send is echoed as a `Message` frame
            // (handled above), so only a real failure flips this.
            if let Some(discord_server::server_settings::WsServerMsg::Error { .. }) = frame {
                send_failed_sig.set(true);
            }
            ui! {
                view(style = StyleRules {
                    height: Some(px(0.0)),
                    ..Default::default()
                }) {}
            }
        },
    );

    // Reset a channel's unread-mention count the moment it's opened, so the
    // red badge disappears as soon as the user reads it. This lives here as a
    // side-effect `switch` keyed on the active live channel (rather than in the
    // per-row click handler) because that handler can't mutate the shared `Rc`
    // mention buffer out of the sidebar's `FnMut` builder. Bumping
    // `live_mention_rev` forces the sidebar `switch` to re-read the now-zero
    // count and drop the badge.
    let mention_reset_buf = live_mentioned.clone();
    let mention_reset_rev = live_mention_rev.clone();
    let unread_reset_buf = live_unread.clone();
    let unread_reset_rev = live_unread_rev.clone();
    let msgs_for_mark_read = live_messages.clone();
    let mention_reset_effect = switch(
        move || active_live_channel.get(),
        move |chan: &Option<i64>| {
            if let Some(cid) = *chan {
                if *mention_reset_buf.borrow().get(&cid).unwrap_or(&0) > 0 {
                    mention_reset_buf.borrow_mut().insert(cid, 0);
                    mention_reset_rev.set(mention_reset_rev.get() + 1);
                }
                // Real backend read-state: mark this channel read up to its
                // most recent message (the newest buffered wire id, which is
                // the latest loaded history entry or live frame so far — the
                // `live_messages` buffer is oldest-first). The RPC is
                // fire-and-forget (a rejection is a backend auth issue, not a
                // UI state to surface here), and the local unread count is
                // cleared OPTIMISTICALLY right away: opening a channel has
                // read its buffered messages by definition, so the badge must
                // drop immediately rather than waiting for the ack.
                if *unread_reset_buf.borrow().get(&cid).unwrap_or(&(0, 0)) != (0, 0) {
                    unread_reset_buf
                        .borrow_mut()
                        .insert(cid, (0, 0));
                    unread_reset_rev.set(unread_reset_rev.get() + 1);
                }
                if let Some(last_id) = msgs_for_mark_read
                    .borrow()
                    .get(&cid)
                    .and_then(|msgs| msgs.last())
                    .map(|m| m.0)
                {
                    runtime_core::driver::spawn_async(async move {
                        let _ = crate::servers::mark_read(cid, last_id).await;
                    });
                }
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

    // Selected member: tapping a member row in the right-hand list sets this
    // to that member's name and opens the full-screen `member_profile_panel`
    // overlay (built once, shown/hidden by this signal — the same toggle
    // idiom as `profile_mode`). `None` while no profile is open.
    let selected_member: Signal<Option<&'static str>> = signal(None);

    // Notification Settings screen: the user panel's settings gear opens it as
    // a full-screen overlay, the same build-once/toggle idiom as `profile_mode`.
    let notif_mode: Signal<bool> = signal(false);

    // Keyboard Shortcuts overlay: the user panel's '?' icon opens it as a
    // full-screen overlay, the same build-once/toggle idiom as `profile_mode`.
    let shortcuts_mode: Signal<bool> = signal(false);

    // Direct Messages (mock): a "DMs" rail circle toggles the full-screen
    // `dm_view::dm_screen` overlay, the same build-once/toggle idiom as
    // `profile_mode` / `notif_mode`. `dm_active` selects the open conversation
    // (seeded to the most recent mock DM — `DMS` is ordered by recency, so the
    // first entry), `dm_sent` is this session's sent-DM log, and `dm_revision`
    // is the rebuild counter its message list's `switch` dedups on. These are
    // created fresh here (not threaded from elsewhere) — the DM view is
    // self-contained over `crate::data`.
    let dm_mode: Signal<bool> = signal(false);
    let dm_active: Signal<u32> = signal(DMS.first().map(|d| d.id).unwrap_or(0));
    let dm_sent: Rc<RefCell<Vec<(u32, &'static str, String, i64)>>> = Rc::new(RefCell::new(Vec::new()));
    let dm_revision: Signal<u32> = signal(0);
    // Mock DM unread badges: the per-DM count map + the rebuild counter its
    // list `switch` dedups on. Seeded from the static `mock_unread` counts and
    // owned here (next to `dm_sent`/`dm_revision`) so a read state survives a
    // close/re-open — `dm_screen` re-mounts its list on each open. `dm_view`'s
    // reset `switch` zeroes a DM's count the moment `dm_active` opens it.
    let dm_unread: Rc<RefCell<HashMap<u32, u32>>> = Rc::new(RefCell::new(
        DMS.iter().map(|d| (d.id, crate::dm_view::mock_unread(d.id))).collect(),
    ));
    let dm_unread_rev: Signal<u32> = signal(0);

    // Reaction state (`reactions` / `reaction_rev` / `picker`) is declared up
    // with the live state before `gateway_incoming_effect` — see that block.

    // Reply context for the channel composer: `Some((author, snippet))` while a
    // message row's hover "Reply" button is composing a reply (the
    // `ReplyPreviewBar` strip reads it); `None` while not. `PinConfirmToast`'s
    // `visible` flag, flashed by the "Pin message" overflow-menu entry.
    let reply_ctx: Signal<Option<(String, String)>> = signal(None);
    // The wire id of the message the composer is replying to. Set by a row's
    // Reply action (from its `MessageKey`) and read by `do_send` (below) to
    // attach it to the outbound `Send` frame; cleared after a successful send
    // and by the preview bar's "×". Kept separate from `reply_ctx` (the display
    // author + snippet) so the server needs only the id, not the display text.
    let reply_target: Signal<Option<i64>> = signal(None);
    let pin_visible: Signal<bool> = signal(false);
    // Pinned-messages panel state (LIVE / backend). `pinned_panel_open` is the
    // full-screen overlay's show flag; `pinned_messages` the fetched list;
    // `pinned_count` the header badge value; `pinned_revision` the rebuild
    // trigger (bumped after every list/count mutation); `pinned_error` a
    // rejection surfaced in the panel body. The header pin icon opens the
    // panel and fetches the list; each row's Unpin button updates the count
    // and list (and bumps `pinned_revision`) after a backend round trip.
    let pinned_panel_open: Signal<bool> = signal(false);
    let pinned_messages: Signal<Vec<crate::pins::PinnedMessage>> = signal(Vec::new());
    let pinned_count: Signal<i64> = signal(0);
    let pinned_revision: Signal<u32> = signal(0);
    let pinned_error: Signal<String> = signal(String::new());
    // The per-message "Translate message" overflow-menu entry reveals a
    // `TranslateToggle` on ONE row at a time (a deliberate single instance, not
    // a per-row widget). This shared signal names the active row's
    // `(channel, list_index)` key — the picker idiom — so opening another
    // row's menu or re-tapping dismisses it. Owned here next to the other mock
    // per-app signals; threaded into the mock message list only.
    let translate_key: Signal<Option<MessageKey>> = signal(None);

    // --- live (backend) DM state -------------------------------------------
    // Parallel to (and independent of) the mock `dm_*` state above, and to the
    // `live_servers`/`live_channels` live-mode idiom. A separate, new entry
    // point: `live_dm_mode` toggles its own full-screen overlay region, and
    // the mock DM screen is left untouched.
    // `live_dms` holds the fetched conversation list (populated on first open
    // via `spawn_async` calling `list_dms`); `live_dm_active` the selected DM
    // channel id (None = none); `live_dm_messages`/`live_dm_revision` the
    // fetched history buffer + its rebuild counter (the `revision` bump is the
    // rebuild trigger, the same idiom as `live_messages`/`live_revision`).
    let live_dm_mode: Signal<bool> = signal(false);
    let live_dms: Signal<Vec<crate::dms::Dm>> = signal(Vec::new());
    let live_dm_active: Signal<Option<i64>> = signal(None);
    let live_dm_messages: Rc<RefCell<Vec<(String, String, i64, Option<i64>)>>> =
        Rc::new(RefCell::new(Vec::new()));
    let live_dm_revision: Signal<u32> = signal(0);
    // The "start a new DM" draft (a numeric user id), owned here so the typed
    // value survives a list rebuild (when `start_dm` lands, `live_dms` is
    // updated and the list `switch` re-creates its rows — an in-row signal
    // would drop the draft at exactly that moment).
    // --- friends panel (live) ----------------------------------------------
    // The "Friends" entry at the TOP of the live DM list column (see the
    // `friends_entry` splice in `live_dm_view::live_dm_list`) + the panel
    // itself (built once in app_shell scope; see `friends_panel` below its
    // render site) + the `on_message` callback above. `friends_open` toggles
    // which of the two the DM screen's main area shows. Both the entry and
    // the panel are `Rc<dyn Fn() -> Element>` factories (`Element` is not
    // `Clone`; the live DM list's rows rebuild on every `live_dms` change, so
    // the entry must be re-buildable inside that `Fn` render). The signals the
    // factories touch are cloned into locals first (a `move` factory must own
    // them; the clones share the same backing cells, so a `.set` from inside
    // reaches the panes exactly as on the canonical names).
    let friends_open_entry = friends_open.clone();
    let friends_add_username_entry = friends_add_username.clone();
    let friends_add_error_entry = friends_add_error.clone();
    let friends_snap_entry = friends_snap.clone();
    let friends_entry = Rc::new(
        move || ui! {
            view(
                style = StyleRules {
                    height: Some(px(40.0)),
                    margin_left: Some(px(8.0)),
                    margin_right: Some(px(8.0)),
                    margin_bottom: Some(px(6.0)),
                    padding_left: Some(px(10.0)),
                    padding_right: Some(px(10.0)),
                    align_items: Some(AlignItems::Center),
                    flex_direction: Some(FlexDirection::Row),
                    gap: Some(px(8.0)),
                    background: Some(col(BG_SIDEBAR)),
                    border_top_left_radius: Some(px(8.0)),
                    border_top_right_radius: Some(px(8.0)),
                    border_bottom_left_radius: Some(px(8.0)),
                    border_bottom_right_radius: Some(px(8.0)),
                    ..Default::default()
                },
                test_id = "friends-entry",
            ) {
                icon(USERS).color(|| Color::from(TEXT_MUTED)).size(18.0)
                text(
                    style = StyleRules {
                        font_size: Some(px(14.0)),
                        font_weight: Some(FontWeight::SemiBold),
                        color: Some(col(TEXT)),
                        ..Default::default()
                    },
                ) { "Friends".to_string() }
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    let now_open = !friends_open_entry.get();
                    friends_open_entry.set(now_open);
                    if now_open {
                        // On open: reset the Add Friend draft + error (the same
                        // per-open reset the join/create modals apply to their
                        // fields) and re-fetch the relationship list so the
                        // panel reflects the backend's current state
                        // immediately.
                        friends_add_username_entry.set(String::new());
                        friends_add_error_entry.set(String::new());
                        let snap = friends_snap_entry.clone();
                        runtime_core::driver::spawn_async(
                            async move {
                                if let Ok(list) = crate::friends::list_friends().await {
                                    snap.set(crate::friends_view::snapshot(list));
                                }
                            },
                        );
                    }
                }
                TouchResponse::CONSUMED
            })
        },
    );
    let live_dm_new_id: Signal<String> = signal(String::new());
    // Friends panel's "Message" action: open/create the live DM with `uid`
    // through the EXISTING DM functions (mirrors `live_dm_new_row`'s start:
    // `start_dm` is idempotent, the returned channel id is selected, history
    // is fetched + the revision bumped so the conversation pane renders, and
    // `live_dm_mode` flips on so the panel is on screen). Owned here (app_shell
    // scope) and handed to `friends_screen` via `on_message`; a `Rc<dyn Fn>` so
    // each row's closure captures only a `Copy` uid + this shared handle.
    let live_dm_msgs_rc = live_dm_messages.clone();
    let friends_open_panel = friends_open.clone();
    let on_dm_message: Rc<dyn Fn(i64)> = Rc::new(
        move |uid: i64| {
            let dms_sig = live_dms.clone();
            let act = live_dm_active.clone();
            let buf = live_dm_msgs_rc.clone();
            let rev = live_dm_revision.clone();
            let mode = live_dm_mode.clone();
            runtime_core::driver::spawn_async(
                async move {
                    let input = discord_server::dms::StartDmInput { user_id: uid };
                    match discord_server::dms::start_dm(input).await {
                        Ok(dm) => {
                            mode.set(true);
                            // Message closes the Friends panel so the DM
                            // conversation the panel opened becomes visible
                            // (the panel shares the live-DM screen's main
                            // area; leaving it open would bury the DM).
                            friends_open_panel.set(false);
                            let dm_id = dm.channel_id;
                            if !dms_sig.get().iter().any(|d| d.channel_id == dm_id) {
                                let mut list = dms_sig.get();
                                list.insert(0, dm);
                                dms_sig.set(list);
                            }
                            act.set(Some(dm_id));
                            crate::auth::save_last_view_dm(dm_id);
                            if let Ok(history) = discord_server::dms::list_dm_messages(
                                discord_server::dms::ListDmMessagesInput {
                                    channel_id: dm_id,
                                    limit: None,
                                },
                            ).await {
                                buf.borrow_mut().clear();
                                buf.borrow_mut()
                                    .extend(history.into_iter().map(|msg| (
                                        msg.author_username,
                                        msg.content,
                                        msg.created_at,
                                        msg.edited_at,
                                    )));
                                rev.set(rev.get() + 1);
                            }
                        }
                        Err(_) => {
                            // A failed start leaves the selection untouched.
                        }
                    }
                },
            );
        },
    );
    // Refresh restore: when the persisted per-user last view is the DM form,
    // re-open the DM panel on THAT conversation — the same state a live-DM
    // row tap sets (`live_dm_active` + fetched history + revision bump), so
    // the user lands with the conversation's messages visible and no click.
    // Validate against the freshly fetched list first: a deleted conversation
    // or one the user lost access to is ignored (falls back to the closed
    // panel + the server fallback the auto-enter effect already applied).
    {
        let saved = crate::auth::load_last_view();
        let is_real = is_real_user;
        let mode = live_dm_mode.clone();
        let list = live_dms.clone();
        let act = live_dm_active.clone();
        let buf = live_dm_messages.clone();
        let rev = live_dm_revision.clone();
        runtime_core::driver::spawn_async(async move {
            if !is_real {
                return;
            }
            let Some((kind, dm_id, _)) = saved.filter(|(k, _, _)| *k == 1) else { return; };
            if kind != 1 {
                return;
            }
            let Ok(dms_list) = crate::dms::list_dms().await else { return; };
            if !dms_list.iter().any(|d| d.channel_id == dm_id) {
                return; // stale saved DM: keep the default (closed panel)
            }
            list.set(dms_list);
            act.set(Some(dm_id));
            mode.set(true);
            // History: identical to the row's `live_dm_row` select handler
            // (clear + refill the one-conversation buffer, bump the rebuild
            // counter) — the fetch is the click path's fetch.
            if let Ok(history) = crate::dms::list_dm_messages(dm_id, None).await {
                buf.borrow_mut().clear();
                buf.borrow_mut().extend(history.into_iter().map(|m| (
                    m.author_username,
                    m.content,
                    m.created_at,
                    m.edited_at,
                )));
                rev.set(rev.get() + 1);
            }
        });
    }

    // Server Moderation panel: a gavel icon in the channel header opens it as
    // a full-screen overlay (the same toggle idiom as `profile_mode` — built
    // once, shown/hidden by this signal, a "Done" pill flips it back off).
    // Mock-only: renders the eight unwired moderation components with hardcoded
    // rows; no backend call.
    let moderation_mode: Signal<bool> = signal(false);
    // "Bug reports" thread: the `ThreadListRow` in the channel sidebar opens
    // this as a full-screen overlay (the same toggle idiom as `moderation_mode`).
    // Mock-only: a handful of hardcoded placeholder reports, no backend call.
    let bug_reports_mode: Signal<bool> = signal(false);

    // Live "Invite People" flow (real backend, live mode only): the `USER_PLUS`
    // action in the live channel header opens this overlay, which mints a real
    // invite via `invite::create_invite` against `active_live_server` and shows
    // the resulting code in `InviteLinkDisplay` (whose Copy button now writes
    // the link to the system clipboard). `live_invite_code` holds the freshly
    // minted code (None until created); `live_invite_error` carries a backend
    // rejection message. Built once here, painted last (same idiom as
    // `moderation_region`), inert outside live mode (no header action to open
    // it, and `active_live_server` is None when a mock server is active).
    let live_invite_mode: Signal<bool> = signal(false);
    let live_invite_code: Signal<Option<String>> = signal(None);
    let live_invite_error: Signal<String> = signal(String::new());

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

    // Reactive list of the mock servers shown in the rail. Seeded with the
    // static `SERVERS` ids; the "Add a server" (+) flow appends new ids, and
    // the rail `switch` below rebuilds on that signal so new rows appear.
    let server_rail: Signal<Vec<u32>> = signal(SERVERS.iter().map(|s| s.id).collect());

    // [Feature 16] The mock demo servers are guest-only rail content. A real
    // account sees only its own live servers (the live section below) plus the
    // create / join affordances — the demo `SERVERS` are not theirs, so the
    // mock rows are gated off for real users (empty here; the onboarding
    // state is driven by the create affordance in the rail). Guests keep the
    // full demo rail.
    let server_rows: Vec<Element> = if is_real_user {
        Vec::new()
    } else {
        SERVERS.iter().map(|s| {
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
            // The row keeps its squircle background on selection (Discord does
            // not invert the icon to white); the active-server pill on the far
            // left edge is the sole selection signal. Background is static
            // `BG_SIDEBAR` — no reactive white-invert.
            background: Some(col(BG_SIDEBAR)),
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
            // The label keeps its own color on selection (no white-invert);
            // the left-edge pill alone signals the active server.
            color: Some(col(TEXT_CHANNEL)),
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
    }).collect()
    };

    // --- "Create a Server" reactive plumbing -------------------------------
    // `dynamic_servers` holds the servers the user creates this session
    // (leaked `&'static` behind an id — the same heap-leak trick the static
    // seed in `data.rs` uses). `next_id` mints fresh ids above the static max.
    // `server_rail` (seeded with the static ids above) is the rail's re-render
    // trigger: appending a new id makes the `switch` below rebuild and add the
    // row. Both the `Cell` and the `HashMap` live behind an `Rc` so the
    // `on_create` closure (a `Fn`) can mutate them through interior
    // mutability.
    let dynamic_servers: Rc<RefCell<HashMap<u32, &'static Server>>> =
        Rc::new(RefCell::new(HashMap::new()));
    let next_id: Rc<Cell<u32>> = Rc::new(Cell::new(
        SERVERS.iter().map(|s| s.id).max().map(|m| m + 1).unwrap_or(1),
    ));
    let create_open: Signal<bool> = signal(false);
    let create_name: Signal<String> = signal(String::new());
    // The create modal's inline error (the backend's own rejection wording)
    // and in-flight busy guard. Both owned by the app (like the join modal's
    // `join_error` / `join_busy`) and reset on re-open below.
    let create_error: Signal<String> = signal(String::new());
    let create_busy: Signal<bool> = signal(false);

    // Dynamically created servers — a separate section below the mock rows.
    // Rebuilt whenever `server_rail` changes (a new id appended). Each row
    // mirrors the mock row: squircle + initial + selection.
    let dyn_servers = dynamic_servers.clone();
    let create_server_section = switch(
        move || server_rail.get(),
        move |ids: &Vec<u32>| {
            let rows: Vec<Element> = ids.iter().filter_map(|id| {
                let s = *dyn_servers.borrow().get(id)?;
                let active = active_server.clone();
                let channel = active_channel.clone();
                let voice = joined_voice.clone();
                let live_server = active_live_server.clone();
                let live_channel = active_live_channel.clone();
                let row_style = move || StyleRules {
                    width: Some(px(48.0)),
                    height: Some(px(48.0)),
                    // Positioning ancestor for the active-server pill.
                    position: Some(Position::Relative),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    // No white-invert on selection — the left-edge pill alone
                    // signals the active created server (matches the mock rows).
                    background: Some(col(BG_SIDEBAR)),
                    // Discord's squircle is a 16px radius on a 48px icon.
                    border_top_left_radius: Some(px(16.0)),
                    border_top_right_radius: Some(px(16.0)),
                    border_bottom_left_radius: Some(px(16.0)),
                    border_bottom_right_radius: Some(px(16.0)),
                    ..Default::default()
                };
                let label_style = move || StyleRules {
                    font_size: Some(px(18.0)),
                    font_weight: Some(FontWeight::Bold),
                    color: Some(col(TEXT_CHANNEL)),
                    ..Default::default()
                };
                Some(ui! {
                    view(style = row_style) {
                        // The active-server pill on the far-left rail edge — the
                        // sole selection signal (the row is `Position::Relative`,
                        // 48px centered in the 72px rail, so left: -12 is flush).
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
                            // Selecting a created server exits live mode (same
                            // as a mock row) and defaults to its first text
                            // channel.
                            live_server.set(None);
                            live_channel.set(None);
                            active.set(s.id);
                            channel.set(s.first_text_channel().id);
                            voice.set(None);
                        }
                        TouchResponse::CONSUMED
                    })
                })
            }).collect();
            ui! {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Column),
                    align_items: Some(AlignItems::Center),
                    gap: Some(px(10.0)),
                    ..Default::default()
                }) {
                    rows
                }
            }
        },
    );

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
            // Small "LIVE" badge under the thin divider: 9px uppercase,
            // semi-bold, muted text on a BG_SIDEBAR pill (radius 8). The bare
            // unstyled text version read as a floating label with no chrome;
            // the pill gives it badge styling and still marks the section's
            // purpose — backend-connected servers, as opposed to the mock rows
            // above the divider.
            let label_style = StyleRules {
                font_size: Some(px(9.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(TEXT_MUTED)),
                letter_spacing: Some(Tokenized::Literal(0.5)),
                text_transform: Some(TextTransform::Uppercase),
                background: Some(col(BG_SIDEBAR)),
                padding_top: Some(px(3.0)),
                padding_bottom: Some(px(3.0)),
                padding_left: Some(px(7.0)),
                padding_right: Some(px(7.0)),
                border_top_left_radius: Some(px(8.0)),
                border_top_right_radius: Some(px(8.0)),
                border_bottom_left_radius: Some(px(8.0)),
                border_bottom_right_radius: Some(px(8.0)),
                ..Default::default()
            };
            let row_style = StyleRules {
                width: Some(px(48.0)),
                height: Some(px(48.0)),
                // Positioning ancestor for the active-server pill (the mock rows
                // do the same).
                position: Some(Position::Relative),
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
                let unread_buf = rail_unread.clone();
                let unread_rev = live_unread_rev.clone();
                let live_mbrs = live_members.clone();
                let live_mbr_err = live_member_error.clone();
                let live_kick = live_kick_confirm.clone();
                let live_ban = live_ban_confirm.clone();
                let live_timeout = live_timeout_confirm.clone();
                let base = row_style.clone();
                ui! {
                    view(style = base.clone()) {
                        // The active-server pill on the far-left rail edge — the
                        // sole selection signal (the row is `Position::Relative`,
                        // 48px centered in the 72px rail, so left: -12 is flush).
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
                            opacity: Some(Tokenized::Literal(if active_live.get() == Some(sid) {
                                1.0
                            } else {
                                0.0
                            })),
                            ..Default::default()
                        }) {}
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
                            // Clear any stale moderation state from a previous
                            // live server (kick + ban + timeout confirm and error text).
                            live_kick.set(None);
                            live_ban.set(None);
                            live_timeout.set(None);
                            live_mbr_err.set(String::new());
                            live_mbrs.set(Vec::new());
                            // Clone the `Rc` before handing it to the `async move`
                            // block: the block outlives the `on_touch` `Fn`
                            // closure, so moving the captured `unread_buf`
                            // straight into it would be a move out of the
                            // closure (E0507). The clone shares the same
                            // `RefCell`, so the fetch writes the shared map.
                            let unread_buf = unread_buf.clone();
                            runtime_core::driver::spawn_async(async move {
                                // Channels, the member roster, and the
                                // per-channel unread counts are independent
                                // `#[server]` RPCs; fetch all three in one
                                // spawn so the live view is populated together.
                                if let Ok(channels) =
                                    crate::servers::list_channels(sid).await
                                {
                                    // Auto-select the first text channel so the
                                    // main panes enter live mode: the header,
                                    // message list, and composer all key on
                                    // `active_live_channel`, which stays `None`
                                    // here until a channel row is tapped. The
                                    // mock tile handler does the same via
                                    // `first_text_channel()` — without it,
                                    // clicking a live tile updates only the
                                    // sidebar (keyed on `active_live_server`)
                                    // and the header / messages keep showing the
                                    // mock server. Resolved before `channels`
                                    // is moved into `live_ch`.
                                    let first = channels
                                        .iter()
                                        .find(|c| c.kind == "text")
                                        .map(|c| c.id);
                                    live_ch.set(channels);
                                    if let Some(id) = first {
                                        live_ch_sel.set(Some(id));
                                        // Persist the exact landing spot (this
                                        // server + this first channel) keyed by
                                        // user id, so a refresh re-enters HERE
                                        // — not the account's first server.
                                        crate::auth::save_last_view_channel(sid, id);
                                        // Auto-selecting only sets which channel
                                        // is "active" — it does NOT populate
                                        // `live_messages` for it. A manual row
                                        // tap also sends History (see the
                                        // channel row's `on_touch` below); do
                                        // the same here so the auto-selected
                                        // channel isn't stuck showing stale/no
                                        // messages until the user re-taps it.
                                        gateway.send(
                                            discord_server::server_settings::WsClientMsg::History {
                                                channel_id: id,
                                                before_id: None,
                                            },
                                        );
                                    }
                                }
                                if let Ok(members) =
                                    crate::moderation::list_members(sid).await
                                {
                                    live_mbrs.set(members);
                                }
                                if let Ok(counts) =
                                    crate::servers::unread_counts(sid).await
                                {
                                    // Clone the `Rc`: this `async move` block
                                    // outlives the `on_touch` `Fn` closure that
                                    // captured it, so a plain capture would be
                                    // a move out of the closure (E0507).
                                    let buf = unread_buf.clone();
                                    let rev = unread_rev;
                                    buf.borrow_mut()
                                        .extend(counts.into_iter().map(|c| {
                                            (c.channel_id, (c.count, c.unread_mentions))
                                        }));
                                    rev.set(rev.get() + 1);
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

    // --- "Join a Server" (redeem an invite code) ---------------------------
    // Entry point is the rail's green PLUS circle (Discord's Add-a-Server
    // affordance). The modal's state lives here in `app_shell` scope so it
    // survives rail rebuilds; `join_open` gates the overlay, `join_code` is the
    // typed code, `join_result`/`join_error`/`join_busy` are the three state
    // signals the modal switches on (the `invite_modal.rs` ownership idiom).
    // On a successful redeem the backend membership is persisted, so we
    // re-fetch `live_servers` (the same RPC the rail's live section already
    // uses) to make the newly-joined server appear without a reload.
    let join_open: Signal<bool> = signal(false);
    let join_code: Signal<String> = signal(String::new());
    let join_result: Signal<Option<discord_server::invites::RedeemResult>> = signal(None);
    let join_error: Signal<String> = signal(String::new());
    let join_busy: Signal<bool> = signal(false);
    // The `#[component]` `name = expr` call form is only parsed inside `ui!`,
    // so the modal is wrapped in a `view` (the wrapper is a positioned
    // ancestor for the full-screen overlay, like the Profile/notif regions).
    let join_el = ui! {
        view {
            JoinServerModal(
                open = join_open,
                code = join_code,
                result = join_result,
                error = join_error,
                busy = join_busy,
                servers = live_servers,
            )
        }
    };

    // --- "Create a Server" modal ------------------------------------------
    // Trigger is the rail's COMPASS circle (the green PLUS is taken by the
    // Join flow). `on_create` mints a REAL backend server via the
    // `crate::servers::create_server` RPC (the backend seeds the owner's
    // membership row plus a default `#general` text channel, so the fresh
    // server is immediately usable), then re-fetches `live_servers` so the new
    // server appears in the rail's live section (the same refresh the Join
    // modal uses) and auto-enters it (selects it + pulls its channels, the
    // enter-live pattern below). `Signal` is `Copy`, so the captured `set`
    // stays `Fn`.
    let create_on_create: Rc<dyn Fn(String)> = Rc::new({
        let live_servers = live_servers.clone();
        let active_live_server = active_live_server.clone();
        let active_live_channel = active_live_channel.clone();
        let live_channels = live_channels.clone();
        let live_members = live_members.clone();
        let live_member_error = live_member_error.clone();
        let live_kick_confirm = live_kick_confirm.clone();
        let live_unread = live_unread.clone();
        let live_unread_rev = live_unread_rev.clone();
        let create_open = create_open;
        let create_error = create_error;
        let create_busy = create_busy;
        move |name: String| {
            let name_owned = name.clone();
            // Clone the `Rc` in the `Fn` body (not inside the `async move`):
            // the `async move` block would otherwise try to move the captured
            // `Rc` out of the `Fn` closure (E0507) — the enter-live handler
            // uses the same `let unread_buf = unread_buf.clone()` idiom.
            let buf = live_unread.clone();
            let c_err = create_error;
            let c_busy = create_busy;
            runtime_core::driver::spawn_async(async move {
                // Clear any prior rejection and mark in-flight; the modal's
                // Create button shows "Creating…" while this runs (the busy
                // guard also stops a fast double-tap from minting two).
                c_err.set(String::new());
                c_busy.set(true);
                match crate::servers::create_server(&name_owned).await {
                    Ok(summary) => {
                        // The backend persisted the server + default channel;
                        // re-fetch the live server list (the same RPC the rail's
                        // live section uses) so the new server appears without a
                        // reload.
                        if let Ok(list) = crate::servers::list_servers().await {
                            live_servers.set(list);
                        }
                        // Auto-enter the new server, mirroring the enter-live
                        // handler: select it, reset any stale channel/moderation
                        // state, and pull its channels + roster + unread counts.
                        active_live_server.set(Some(summary.id));
                        active_live_channel.set(None);
                        live_kick_confirm.set(None);
                        live_member_error.set(String::new());
                        live_members.set(Vec::new());
                        let sid = summary.id;
                        if let Ok(channels) = crate::servers::list_channels(sid).await {
                            // Auto-select the first text channel (same as the
                            // enter-live tile handler): the main panes key on
                            // `active_live_channel`, so a freshly created server
                            // would otherwise sit on a stale / mock main pane
                            // until a channel row is tapped.
                            let first = channels
                                .iter()
                                .find(|c| c.kind == "text")
                                .map(|c| c.id);
                            live_channels.set(channels);
                            if let Some(id) = first {
                                active_live_channel.set(Some(id));
                                // Persist the landing spot for this account so
                                // a refresh re-enters the just-created server's
                                // first channel.
                                crate::auth::save_last_view_channel(summary.id, id);
                                // See the enter-live tile handler's identical
                                // fix: auto-selecting doesn't fetch history on
                                // its own, so send it explicitly or the fresh
                                // server sits on an empty pane until a channel
                                // row is manually re-tapped.
                                gateway.send(
                                    discord_server::server_settings::WsClientMsg::History {
                                        channel_id: id,
                                        before_id: None,
                                    },
                                );
                            }
                        }
                        if let Ok(members) = crate::moderation::list_members(sid).await {
                            live_members.set(members);
                        }
                        if let Ok(counts) = crate::servers::unread_counts(sid).await {
                            let rev = live_unread_rev;
                            buf.borrow_mut()
                                .extend(counts.into_iter().map(|c| {
                                    (c.channel_id, (c.count, c.unread_mentions))
                                }));
                            rev.set(rev.get() + 1);
                        }
                        // Success: the server exists and we're auto-entering
                        // it, so close the modal. (On the error path below the
                        // modal stays open so the inline message is visible and
                        // the name field can be corrected and resubmitted.)
                        create_open.set(false);
                    }
                    Err(msg) => {
                        // Surface the backend's own rejection inline in the
                        // modal (e.g. an empty name -> "server name must not
                        // be empty"). The modal stays open; the name field is
                        // still editable so the user can correct it and hit
                        // Create again. We do NOT close here — closing on
                        // failure would hide the very message we just set.
                        eprintln!("create_server failed: {msg}");
                        c_err.set(msg);
                    }
                }
                c_busy.set(false);
            });
        }
    });
    let create_modal_el: Element =
        create_server_modal(create_open, create_name, create_error, create_busy, create_on_create);
    let create_el = ui! {
        view {
            { create_modal_el }
        }
    };

    // --- "Discover Servers" overlay ----------------------------------------
    // Trigger is a third rail circle (GLOBE) below the PLUS and COMPASS
    // circles — both existing icons are taken (Join / Create). `on_join`
    // mints the suggested server the same way Create does: `create_server` +
    // append to `server_rail` (the rail's re-render trigger), then close the
    // overlay. The mock list itself is static; no join backend.
    let discover_open: Signal<bool> = signal(false);
    let discover_on_join: Rc<dyn Fn(String)> = Rc::new({
        let next_id = next_id.clone();
        let dynamic_servers = dynamic_servers.clone();
        let server_rail = server_rail.clone();
        let discover_open = discover_open;
        move |name: String| {
            let new_id = create_server(&name, &*next_id, &dynamic_servers);
            let mut ids = server_rail.get();
            ids.push(new_id);
            server_rail.set(ids);
            discover_open.set(false);
        }
    });
    let discover_modal_el: Element = discover_servers_modal(discover_open, discover_on_join);
    let discover_el = ui! {
        view {
            { discover_modal_el }
        }
    };

    // --- Live channel management (create / rename / delete) -----------------
    // Only active in live mode: the `server_id` for every call is read from
    // `active_live_server` at submit time (the modals are built once and live
    // across re-opens, so a `None` server is a no-op). The three modals reuse
    // the build-once/toggle backdrop idiom of `join_modal.rs`; the live channel
    // list they refresh is the same `live_channels` the live sidebar renders,
    // so a successful op re-fetches `list_channels` and the sidebar's `switch`
    // (keyed on the channel fingerprint) rebuilds with the new rows. There is
    // no client-side permission logic — the backend decides and its message is
    // surfaced inline by the modal on failure.
    let cc_open: Signal<bool> = signal(false);
    let cc_name: Signal<String> = signal(String::new());
    let cc_kind: Signal<String> = signal("text".to_string());
    let cc_busy: Signal<bool> = signal(false);
    let cc_err: Signal<String> = signal(String::new());
    let cc_el = ui! {
        view {
            CreateChannelModal(
                open = cc_open,
                server_id = active_live_server,
                name = cc_name,
                kind = cc_kind,
                busy = cc_busy,
                error = cc_err,
                channels = live_channels,
            )
        }
    };
    let rc_open: Signal<bool> = signal(false);
    let rc_cid: Signal<Option<i64>> = signal(None);
    let rc_name: Signal<String> = signal(String::new());
    let rc_busy: Signal<bool> = signal(false);
    let rc_err: Signal<String> = signal(String::new());
    let rc_el = ui! {
        view {
            RenameChannelModal(
                open = rc_open,
                server_id = active_live_server,
                channel_id = rc_cid,
                name = rc_name,
                busy = rc_busy,
                error = rc_err,
                channels = live_channels,
            )
        }
    };
    let dc_open: Signal<bool> = signal(false);
    let dc_cid: Signal<Option<i64>> = signal(None);
    let dc_name: Signal<String> = signal(String::new());
    let dc_busy: Signal<bool> = signal(false);
    let dc_err: Signal<String> = signal(String::new());
    let dc_el = ui! {
        view {
            DeleteChannelModal(
                open = dc_open,
                server_id = active_live_server,
                channel_id = dc_cid,
                channel_name = dc_name,
                busy = dc_busy,
                error = dc_err,
                channels = live_channels,
            )
        }
    };
    // --- "Invite People" (mint a server invite) ------------------------------
    // LIVE mode only: the USERS icon in the live sidebar header opens this
    // full-screen overlay. `server_id` is `active_live_server` (read at submit
    // time, `None` outside live mode = a no-op, the same inert-outside-live
    // idiom as the channel-management modals above). `expiry` is the selected
    // option (0 = 30 min, 1 = 1 day, 2 = never), `created` / `created_err` are
    // the RPC outcome; on a successful mint the modal switches to the
    // copy-the-link view. All four state signals are owned here so the modal
    // survives sidebar rebuilds, and reset on re-open (see the header trigger
    // below).
    let invite_mode: Signal<bool> = signal(false);
    let invite_expiry: Signal<usize> = signal(0);
    let invite_created: Signal<Option<discord_server::invites::Invite>> = signal(None);
    let invite_err: Signal<String> = signal(String::new());
    let invite_el = ui! {
        view {
            InviteModal(
                open = invite_mode,
                server_id = active_live_server,
                expiry = invite_expiry,
                created = invite_created,
                created_err = invite_err,
            )
        }
    };
    // --- Live Server Settings (rename / icon / add member / transfer) --------
    // LIVE mode only: the gear in the live sidebar's header opens this full-screen
    // overlay. `server_id` is `active_live_server` (read at submit time, `None`
    // outside live mode = a no-op, the same inert-outside-live idiom as the
    // channel-management modals above). `name`/`icon`/`new_owner_id` are the
    // inputs, seeded from the active live server on open; `add_member_user_id` /
    // `add_member_role` seed the add-member section (numeric id + a role that
    // starts as `'member'` and cycles via the panel's chip); `busy`/`error` are
    // the RPC state; `servers` is re-fetched by the panel on any successful
    // op so the rail's live section shows the new name/icon. There is no
    // client-side permission logic beyond the panel's owner-only gate on the
    // transfer section — the backend decides and its message is surfaced verbatim.
    let ss_open: Signal<bool> = signal(false);
    let ss_name: Signal<String> = signal(String::new());
    let ss_icon: Signal<String> = signal(String::new());
    let ss_owner: Signal<String> = signal(String::new());
    // Add-member section inputs: the numeric user id and the role to grant
    // (starts at `'member'`; the panel's chip cycles it member→moderator→admin
    // →owner→member). Seeded on open, below, like `ss_owner`.
    let ss_add_user_id: Signal<String> = signal(String::new());
    let ss_add_role: Signal<String> = signal("member".to_string());
    let ss_busy: Signal<bool> = signal(false);
    let ss_err: Signal<String> = signal(String::new());
    let ss_el = ui! {
        view {
            ServerSettingsPanel(
                open = ss_open,
                server_id = active_live_server,
                name = ss_name,
                icon = ss_icon,
                new_owner_id = ss_owner,
                busy = ss_busy,
                error = ss_err,
                servers = live_servers,
                members = live_members,
                add_member_user_id = ss_add_user_id,
                add_member_role = ss_add_role,
            )
        }
    };
    // (Live channel action cluster + new-channel "+" builder live inside the
    // live arm of the sidebar `switch` render closure — see there. They need
    // the render closure's owned copies of the modal `Signal`s, which a
    // function-scope non-`move` closure could not borrow across the `move`
    // capture boundary.)
    // Bottom rail actions, grouped Discord-style: the server cluster (Add-a-
    // Server (+), Explore (compass), plus the clone's Discover (globe) and
    // Live-DM (speech-bubble) affordances, which power real functionality and
    // so are kept reachable above the divider) sits with the server list, and
    // the Direct Messages + Settings circles sit BELOW it. Real Discord's
    // bottom rail row is DM + Settings only — that is exactly what the bottom
    // holds here now.
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
        let divider_style = StyleRules {
            width: Some(px(32.0)),
            height: Some(px(1.0)),
            background: Some(col(BORDER)),
            ..Default::default()
        };
        let plus_open = join_open.clone();
        let plus_code = join_code.clone();
        let plus_result = join_result.clone();
        let plus_error = join_error.clone();
        let plus_busy = join_busy.clone();
        let create_open = create_open.clone();
        let create_name = create_name.clone();
        let create_error = create_error.clone();
        let create_busy = create_busy.clone();
        let discover_open = discover_open.clone();
        let dm_open = dm_mode.clone();
        let settings_open = notif_mode.clone();
        ui! {
            // --- Top cluster: server-related actions (above the divider) ----
            // Add-a-Server (+) + Explore (compass) sit with the server list;
            // the clone's Discover (globe) and Live-DM (speech-bubble)
            // affordances live here too — they power real functionality, so
            // they're kept reachable above the divider rather than in the
            // bottom DM+Settings row (which must stay DM + Settings only).
            view(style = circle_style.clone(), test_id = "rail-join-server") {
                icon(PLUS).color(|| Color::from("#23a55a")).size(20.0)
            }
            // Add-a-Server: open the join modal. Reset the per-open state so a
            // re-open always starts fresh (the same reset the caller owns).
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    plus_code.set(String::new());
                    plus_result.set(None);
                    plus_error.set(String::new());
                    plus_busy.set(false);
                    plus_open.set(true);
                }
                TouchResponse::CONSUMED
            })
            view(style = circle_style.clone(), test_id = "rail-explore-servers") {
                icon(COMPASS).color(|| Color::from(TEXT_MUTED)).size(20.0)
            }
            // Explore/Create-a-Server: reset the name field and open the create
            // modal (the same reset-on-open `create_server_modal`'s docs
            // describe). The compass is Discord's Explore affordance. Also
            // clear the prior attempt's inline error and busy state so a
            // re-open always starts fresh (the same per-open reset the join
            // modal applies to its `join_error` / `join_busy`).
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    create_name.set(String::new());
                    create_error.set(String::new());
                    create_busy.set(false);
                    create_open.set(true);
                }
                TouchResponse::CONSUMED
            })
            // Discover-a-Server: open the discover overlay (no per-open state
            // to reset — the suggested list is static). Moved up from the old
            // bottom slot so it stays reachable without cluttering the DM row.
            view(style = circle_style.clone(), test_id = "rail-discover-servers") {
                icon(GLOBE).color(|| Color::from(TEXT_MUTED)).size(20.0)
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    discover_open.set(true);
                }
                TouchResponse::CONSUMED
            })
            // Direct Messages (LIVE / backend): a separate, independent entry
            // point from the mock DM button below. Toggling it open fetches
            // the signed-in user's real DMs via `list_dms` (the same
            // `spawn_async` + `#[server]` RPC idiom as the `live_servers`
            // fetch) the first time it's opened; subsequent opens re-fetch so
            // the list stays current. Per-open state (`live_dm_active`,
            // `live_dm_messages`, `live_dm_revision`) is owned in `app_shell`
            // scope, so a re-open keeps the selected conversation and its
            // fetched history (the same persistence as the mock DM screen).
            // Moved up from the old bottom slot (which must be DM + Settings
            // only) so the live-DM feature stays reachable.
            view(style = circle_style.clone(), test_id = "rail-live-direct-messages") {
                icon(MESSAGE_CIRCLE).color(|| Color::from(TEXT_MUTED)).size(20.0)
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    let now_open = !live_dm_mode.get();
                    live_dm_mode.set(now_open);
                    if now_open {
                        // Persist the open conversation as this user's last
                        // view (DM form) so a refresh re-opens the DM panel on
                        // their own last conversation. None (nothing selected
                        // yet this visit) clears nothing; the DM row's select
                        // handler keeps the value current on every tap.
                        if let Some(dm_id) = live_dm_active.get() {
                            crate::auth::save_last_view_dm(dm_id);
                        }
                        // Re-fetch on every open so the list reflects the backend's
                        // current state (mirrors how a live channel is re-fetched
                        // when its server is re-selected).
                        let list = live_dms.clone();
                        runtime_core::driver::spawn_async(async move {
                            if let Ok(dms) = crate::dms::list_dms().await {
                                list.set(dms);
                            }
                        });
                    }
                }
                TouchResponse::CONSUMED
            })
            // The divider between the top (server-related) cluster and the
            // bottom (DM + Settings) row — Discord's rail separator.
            view(style = divider_style) {}
            // --- Bottom row: Direct Messages (mock) + Settings -------------
            // Real Discord's bottom rail row is DM + Settings only.
            view(style = circle_style.clone(), test_id = "rail-direct-messages") {
                icon(MAIL).color(|| Color::from(TEXT_MUTED)).size(20.0)
            }
            // Direct Messages (mock): toggle the full-screen `dm_screen` overlay.
            // No per-open state to reset — `dm_active` / `dm_sent` / `dm_revision`
            // are owned in `app_shell` scope and persist across toggles, so a
            // re-open keeps the previously-selected conversation and any DMs
            // sent this session (the same persistence as the channel `sent` log).
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    dm_open.set(!dm_open.get());
                }
                TouchResponse::CONSUMED
            })
            view(style = circle_style.clone(), test_id = "rail-settings") {
                icon(SETTINGS).color(|| Color::from(TEXT_MUTED)).size(20.0)
            }
            // Settings: opens the full-screen Notification Settings overlay
            // (the same toggle idiom the user-panel's settings gear uses —
            // `notif_mode` is the same signal that one controls).
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    settings_open.set(true);
                }
                TouchResponse::CONSUMED
            })
            // (server_folder_group/server_template_card/TemplateGalleryGrid/
            // server_tag_chips/FeaturedServerBanner used to be spliced directly
            // into this 72px icon rail -- a real usability regression, it
            // pushed the real rail controls (add-server/DM/settings) below the
            // fold. Moved into the Discover Servers overlay instead, alongside
            // the other discovery cards that were already re-homed there for
            // the same reason -- see fa4bf410 and discover_servers_modal.rs.)
        }
    };

    // --- channel sidebar -------------------------------------------------
    // Width is driven by the `sidebar_width` signal (a thin drag handle on the
    // sidebar's right edge resizes it). A rules *closure* so the width
    // re-resolves as the signal changes — the same reactive idiom the channel
    // rows use. Captures only a `Copy` `Signal`, so the closure itself is
    // `Clone`, which the `switch` render below relies on (`sidebar_style.clone()`).
    let sidebar_style = move || {
        let (s, _m) = effective_panel_widths(sidebar_width, member_panel_width);
        StyleRules {
            width: Some(px(s)),
            // flex_shrink:0 so the flex row never shrinks the sidebar below
            // the viewport-responsive width computed above — the main column
            // (flex_grow:1) absorbs any residual slack instead.
            flex_shrink: Some(grow(0.0)),
            background: Some(col(BG_SIDEBAR)),
            border_right_width: Some(border_width(1.0)),
            border_right_color: Some(col(BORDER)),
            flex_direction: Some(FlexDirection::Column),
            overflow: Some(Overflow::Hidden),
            ..Default::default()
        }
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
        ..section_label()
    };

    // Profile panel toggle, cloned so the `move` switch closure below can
    // own this copy while the overlay (built later) keeps the original.
    let user_profile_mode = profile_mode.clone();
    // Notification panel toggle, cloned the same way.
    let user_notif_mode = notif_mode.clone();
    // Keyboard shortcuts toggle, cloned the same way.
    let user_shortcuts_mode = shortcuts_mode.clone();
    // The `collapsed` map is `Rc<RefCell<..>>` (not `Copy`, since `RefCell`
    // isn't `Copy`). The key and render closures are both `move`, so each
    // captures its own clone; the shared `RefCell` inside keeps them in sync.
    let collapsed_key = collapsed.clone();
    let collapsed_render = collapsed.clone();
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
            // Collapse fingerprint: (category id, collapsed) pairs, ordered by
            // id so it's stable. Read from the shared `collapsed` map — when a
            // CategoryHeader flips its signal, this changes and the row loop
            // re-runs to re-gate each category's rows on the new flag.
            let cat_collapsed: Vec<(u32, bool)> = {
                let mut v: Vec<(u32, bool)> = collapsed_key
                    .borrow()
                    .iter()
                    .map(|(id, sig)| (*id, sig.get()))
                    .collect();
                v.sort_by_key(|(id, _)| *id);
                v
            };
            (
                active_server.get(),
                active_channel.get(),
                joined_voice.get(),
                active_live_server.get(),
                live_fp,
                live_mention_rev.get(),
                live_unread_rev.get(),
                // Per-category collapse flags, so the row loop re-runs (and
                // re-gates each category's rows) when a CategoryHeader toggles.
                cat_collapsed,
                // Which channel row's management popup is open — in the key so
                // a gear tap rebuilds the rows and mounts/unmounts the portal.
                live_chan_menu.get(),
            )
        },
        move |(server_id, _channel_id, _voice, _live_server, _live_fp, _mention_rev, _unread_rev, _cat_collapsed, _chan_menu): &(
            u32,
            u32,
            Option<(u32, u32)>,
            Option<i64>,
            Vec<(i64, String, String)>,
            u32,
            u32,
            Vec<(u32, bool)>,
            Option<i64>,
        )| {
            // `switch`'s render is `Fn` — clone the captured styles so the
            // closure borrows them rather than moving them out (whiteboard idiom).
            let sidebar_style = sidebar_style.clone();
            // [Feature 16] A real account with no live channel selected must
            // never fall through to the mock render: its default `active_server`
            // / `active_channel` point at the demo `SERVERS[0]`, and the mock
            // branch below would render those demo identities (the "Nova"
            // confusion). During the login -> server-fetch race (and for a real
            // account with zero servers) there is no live channel, so show an
            // onboarding hint instead — the live auto-enter effect settles the
            // steady state once the first server resolves.
            if is_real_user && _live_server.is_none() {
                // The profile bar still shows here — a real (authed) user's
                // bottom-left sidebar bar is present for the login -> fetch
                // race and the zero-server state, not just once a live server
                // is entered. Without it this onboarding hint is the only thing
                // in the sidebar, leaving the same "missing profile bar" the
                // live branch used to have. `user_panel` renders the presence
                // status dot ON the avatar, coexisting with the gateway badge.
                let user = user_panel(
                    user_status,
                    joined_voice,
                    authed,
                    user_profile_mode,
                    user_notif_mode,
                    user_shortcuts_mode,
                    muted,
                    deafened,
                );
                return ui! {
                    view(style = sidebar_style.clone()) {
                        text(style = cat_style.clone()) {
                            "No server selected — create or join one to get started."
                                .to_string()
                        }
                        user
                    }
                };
            }
            let sidebar_header_style = sidebar_header_style.clone();
            let sidebar_title_style = sidebar_title_style.clone();
            let chevron_style = chevron_style.clone();
            let cat_style = cat_style.clone();
            let bug_reports_open = bug_reports_mode.clone();
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
                    // Positioning ancestor for the absolutely-positioned mention
                    // badge on the row's right edge (the mock rows do the same).
                    position: Some(Position::Relative),
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
                    // The live server id — read by the channel-row select
                    // handler to key the persisted last view (server+channel).
                    let live_sid = active_live_server.clone();
                    let gw = gateway.clone();
                    let live_unread_map = live_unread.clone();
                    // Scroll-back state for the live list, captured for the
                    // channel-select handler: selecting a channel re-arms the
                    // "follow the live tail" mode (the list starts at the
                    // bottom) and clears the scroll handle the old pane had
                    // captured, so the fresh pane's `on_handle` callback can
                    // store its own.
                    let live_at_bottom_row = live_at_bottom.clone();
                    let live_scroll_handle_row = live_scroll_handle.clone();
                    // The channel-row management-menu state (which hovered row's
                    // Rename/Delete popup is open), captured for `chan_actions`
                    // and the per-row hover handlers.
                    let chan_menu = live_chan_menu.clone();
                    // The roster (already fetched when a live server entered) —
                    // read here ONLY to pick which menu rows RENDER (the
                    // backend re-checks on commit). Plain read of the acting
                    // id is fine at this build site: the sidebar rebuilds on
                    // live-server/channel changes and on login (`authed`).
                    let live_mbrs = live_members.clone();
                    let chan_self_id =
                        crate::auth::CURRENT_USER_ID.load(std::sync::atomic::Ordering::SeqCst);
                    // Shared style for the live channel row action icon buttons
                    // (hover gear) and the new-channel "+" header action. A
                    // rules *closure* (like `live_row_style` below) so the
                    // gear's nested `move` render closures can capture it and
                    // re-build it on every re-render (StyleRules is not `Copy`)
                    // without the E0507 "move out of Fn" the per-use `.clone()`
                    // hits when the style itself is a plain local.
                    let chan_btn_style = move || StyleRules {
                        width: Some(px(22.0)),
                        height: Some(px(22.0)),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        border_top_left_radius: Some(px(6.0)),
                        border_top_right_radius: Some(px(6.0)),
                        border_bottom_left_radius: Some(px(6.0)),
                        border_bottom_right_radius: Some(px(6.0)),
                        flex_shrink: Some(runtime_core::Tokenized::Literal(0.0)),
                        ..Default::default()
                    };
                    // The per-row name is shared into both `move` tap handlers via
                    // an `Rc<String>` (`Clone`), so neither handler moves a `String`
                    // out of this `Fn` closure (E0507) — the established idiom for
                    // non-`Copy` data in `'static` handlers here.
                    let chan_actions = |cid: i64, nm: Rc<String>, u_count: i64, u_mentions: i64, hovered: Signal<bool>, row_ref: Ref<ViewHandle>| -> Element {
                        // `nm` is an `Rc` (`Clone`, not `Copy`): each `move` tap
                        // handler captures its own clone, and reads the name via
                        // `(*nm)` without ever moving the `String` out of this
                        // `Fn` render closure (E0507).
                        let nm_rename = nm.clone();
                        let nm_delete = nm.clone();
                        // The unread badge, fed by the REAL per-channel
                        // `unread_counts` data: a red count pill whenever
                        // `u_count > 0` (the same pill shape the mock rows and
                        // this row used for mentions). When the channel also
                        // has an unread @mention, the pill shows the
                        // mention count — the exact distinction the mock
                        // sidebar makes for "has an unread @mention".
                        // Mirror the mock sidebar's badge treatment exactly
                        // (see the mock `total_mentions`/`total_unread` branch
                        // above): a red numbered pill for an unread @mention
                        // (showing the mention count), a small plain white dot
                        // for unread with no mention, and nothing for a read
                        // channel. The live row's data is the real
                        // `unread_counts` pair, not the mock `mentions` field.
                        let badge = if u_mentions > 0 {
                            Some(ui! {
                                view(style = StyleRules {
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
                                    flex_shrink: Some(runtime_core::Tokenized::Literal(0.0)),
                                    ..Default::default()
                                }) {
                                    text(style = StyleRules {
                                        font_size: Some(px(11.0)),
                                        font_weight: Some(FontWeight::Bold),
                                        color: Some(col(TEXT)),
                                        ..Default::default()
                                    }) { u_mentions.to_string() }
                                }
                            })
                        } else if u_count > 0 {
                            // Plain dot: unread but no mention — Discord shows a
                            // small white indicator instead of the red badge.
                            Some(ui! {
                                view(style = StyleRules {
                                    width: Some(px(10.0)),
                                    height: Some(px(10.0)),
                                    background: Some(col(TEXT)),
                                    border_top_left_radius: Some(px(5.0)),
                                    border_top_right_radius: Some(px(5.0)),
                                    border_bottom_left_radius: Some(px(5.0)),
                                    border_bottom_right_radius: Some(px(5.0)),
                                    flex_shrink: Some(runtime_core::Tokenized::Literal(0.0)),
                                    ..Default::default()
                                }) {}
                            })
                        } else {
                            None
                        };
                        // The ALWAYS-VISIBLE pencil + trash icons are gone: a
                        // single settings GEAR renders only while the row is
                        // hovered (per-row `hovered`, the `message_action_bar`
                        // idiom), and only for users whose own roster role
                        // clears the backend bars — the menu below still arms
                        // the existing rename/delete modals (permission checks
                        // and their confirm/commit steps stay the backend's).
                        let my_role = live_mbrs
                            .get()
                            .iter()
                            .find(|m| m.user_id == chan_self_id)
                            .map(|m| role_rank(m.role.clone()))
                            .unwrap_or(-1);
                        let menu_items: Vec<(String, bool, Rc<dyn Fn()>)> = {
                            let mut items: Vec<(String, bool, Rc<dyn Fn()>)> =
                                Vec::<(String, bool, Rc<dyn Fn()>)>::new();
                            if my_role >= 1 {
                                items.push((
                                    "Rename".to_string(),
                                    false,
                                    Rc::new(move || {
                                        rc_cid.set(Some(cid));
                                        rc_name.set((*nm_rename).clone());
                                        rc_busy.set(false);
                                        rc_err.set(String::new());
                                        rc_open.set(true);
                                    }),
                                ));
                            }
                            if my_role >= 2 {
                                items.push((
                                    "Delete".to_string(),
                                    true,
                                    Rc::new(move || {
                                        dc_cid.set(Some(cid));
                                        dc_name.set((*nm_delete).clone());
                                        dc_busy.set(false);
                                        dc_err.set(String::new());
                                        dc_open.set(true);
                                    }),
                                ));
                            }
                            items
                        };
                        let has_menu = !menu_items.is_empty();
                        let menu = action_menu(chan_menu, cid, row_ref, menu_items);
                        // The gear: mounted only while hovered (and only when
                        // this row has any menu entry at all — `has_menu` is
                        // the Copy snapshot computed BEFORE `menu_items` is
                        // moved into `action_menu` above, so the gear's `Fn`
                        // render closure borrows a bool, never the Vec), and a
                        // tap opens this row's Rename/Delete menu.
                        let gear = ui! {
                            switch(
                                move || hovered.get() && has_menu,
                                move |on: &bool| {
                                    if !*on {
                                        ui! {
                                            view(style = StyleRules {
                                                width: Some(px(0.0)),
                                                height: Some(px(0.0)),
                                                ..Default::default()
                                            }) {}
                                        }
                                    } else {
                                        ui! {
                                            view(style = chan_btn_style(), test_id = "live-channel-gear") {
                                                icon(SETTINGS).color(|| Color::from(TEXT_MUTED)).size(14.0)
                                            }
                                            .on_touch(move |ev| {
                                                if ev.phase == TouchPhase::Ended {
                                                    chan_menu.set(Some(cid));
                                                }
                                                TouchResponse::CONSUMED
                                            })
                                        }
                                    }
                                },
                            )
                        };
                        ui! {
                            view(style = StyleRules {
                                position: Some(Position::Absolute),
                                top: Some(px(6.0)),
                                right: Some(px(8.0)),
                                flex_direction: Some(FlexDirection::Row),
                                align_items: Some(AlignItems::Center),
                                gap: Some(px(2.0)),
                                ..Default::default()
                            }) {
                                if let Some(b) = badge {
                                    { b }
                                }
                                { gear }
                                { menu }
                            }
                        }
                    };
                    let live_rows: Vec<Element> = live
                        .iter()
                        .map(|c| {
                            // '#' for text channels, a speaker icon for voice —
                            // the same icons the mock rows use below.
                            let name = c.name.clone();
                            let is_text = c.kind == "text";
                            let cid = c.id;
                            // `live_scroll_handle_row` is an `Rc` (not `Copy`); the
                            // row's `on_touch(move …)` closure moves it, so each
                            // row needs its own clone (the `nm: Rc<String>` idiom
                            // a few lines up) — the first row would otherwise
                            // move the single shared binding out of this `FnMut`
                            // and the second row hits E0507.
                            let live_scroll_handle_row = live_scroll_handle_row.clone();
                            // The REAL per-channel unread state from the backend
                            // `unread_counts` fetch (fetched when the live server
                            // was selected, bumped per new inbound frame, zeroed
                            // when the channel is opened). Drives the row's
                            // badge: red count pill when `count > 0` with an
                            // unread @mention, a plain dot when `count > 0`
                            // without one, nothing when fully read — the exact
                            // visual contract the mock rows use for
                            // `c.mentions` / `c.unread`.
                            let (u_count, u_mentions) = *live_unread_map
                                .borrow()
                                .get(&cid)
                                .unwrap_or(&(0, 0));
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
                                // A channel with real unread messages brightens
                                // its name (the mock rows do the same for
                                // `c.unread > 0`).
                                color: Some(col(if live_sel.get() == Some(cid) || u_count > 0 {
                                    TEXT
                                } else {
                                    TEXT_CHANNEL
                                })),
                                ..live_label_style()
                            };
                            // The right-side action cluster for this row (mention
                            // pill + hover-gated gear + the gear's Rename/Delete
                            // menu), built once per row. `chan_actions` is
                            // `Copy` (all `Copy` captures), so calling it here
                            // does not move it out of the `switch` render
                            // closure. `hovered` is minted PER ROW here (not
                            // threaded through the shared row builder — the
                            // per-row idiom `message_action_bar` uses) so each
                            // row's gear tracks its own hover, and `row_ref`
                            // anchors the menu portal to this row.
                            let hovered = signal(false);
                            let row_ref: Ref<ViewHandle> = Ref::<ViewHandle>::new();
                            let actions =
                                chan_actions(cid, Rc::new(name.clone()), u_count, u_mentions, hovered, row_ref);
                            let row = if is_text {
                                // Voice rows stay non-clickable for now — a later
                                // task handles voice selection. Text rows select
                                // the channel and request its history over the
                                // gateway (the echo/History frames land in
                                // `gateway.incoming()`). The right-side action
                                // cluster (mention pill + rename/delete buttons)
                                // is `chan_actions`, built per row above.
                                ui! {
                                    view(style = row_style) {
                                        icon(HASH)
                                            .color(|| Color::from(TEXT_MUTED))
                                            .size(14.0)
                                        text(style = label_style) { name }
                                        { actions }
                                    }
                                    .on_touch(move |ev| {
                                        if ev.phase == TouchPhase::Ended {
                                            live_sel.set(Some(cid));
                                            // Persist THIS channel (and its
                                            // server) as the account's last
                                            // view: the tap path is the
                                            // canonical "selection", so the
                                            // saved value always tracks where
                                            // the user last clicked — a
                                            // non-first server AND a non-first
                                            // channel.
                                            if let Some(sid) = live_sid.get() {
                                                crate::auth::save_last_view_channel(sid, cid);
                                            }
                                            // Re-arm the live list's scroll state for the
                                            // freshly opened channel: start "following the
                                            // live tail" (at the bottom) and drop the old
                                            // pane's scroll handle so the new pane's
                                            // `on_handle` callback can capture its own.
                                            live_at_bottom_row.set(true);
                                            *live_scroll_handle_row.borrow_mut() = None;
                                            gw.send(
                                                discord_server::server_settings::WsClientMsg::History {
                                                    // Initial load: the latest page (no
                                                    // scrollback cursor yet).
                                                    channel_id: cid,
                                                    // `before_id: None` = fetch the
                                                    // latest page (no before-cursor) —
                                                    // the initial load on channel select.
                                                    before_id: None,
                                                },
                                            );
                                        }
                                        TouchResponse::CONSUMED
                                    })
                                    .bind(row_ref)
                                    .on_hover(move |on| hovered.set(on))
                                }
                            } else {
                                ui! {
                                    view(style = row_style) {
                                        icon(VOLUME_2)
                                            .color(|| Color::from(TEXT_MUTED))
                                            .size(14.0)
                                        text(style = label_style) { name }
                                        { actions }
                                    }
                                    .bind(row_ref)
                                    .on_hover(move |on| hovered.set(on))
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
                    // The "New Channel" "+" button on the live header's right
                    // edge: opens the create-channel modal, resetting its
                    // per-open state so a re-open starts fresh (the same
                    // reset-on-open idiom the rail's Join flow uses).
                    let add_btn = ui! {
                        view(style = chan_btn_style.clone(), test_id = "live-channel-add") {
                            icon(PLUS).color(|| Color::from(TEXT_MUTED)).size(14.0)
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                cc_name.set(String::new());
                                cc_kind.set("text".to_string());
                                cc_busy.set(false);
                                cc_err.set(String::new());
                                cc_open.set(true);
                            }
                            TouchResponse::CONSUMED
                        })
                    };
                    // The "Invite People" (USERS) icon on the live header's
                    // right edge (next to the gear / + buttons): opens the
                    // LIVE invite overlay. On open it resets the expiry picker
                    // to the default (30 min) and clears any previous mint /
                    // error, so a re-open starts fresh — the same
                    // reset-on-open idiom as the create-channel "+" above.
                    let invite_btn = ui! {
                        view(style = chan_btn_style.clone(), test_id = "live-invite") {
                            icon(USERS).color(|| Color::from(TEXT_MUTED)).size(14.0)
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                invite_expiry.set(0);
                                invite_created.set(None);
                                invite_err.set(String::new());
                                invite_mode.set(true);
                            }
                            TouchResponse::CONSUMED
                        })
                    };
                    // The "Server Settings" gear on the live header's right edge
                    // (next to the "+" button): opens the LIVE server-settings
                    // overlay (rename / icon / transfer ownership). On open it
                    // seeds the name + icon inputs from the active live server
                    // (looked up in `live_servers`) and resets the transfer /
                    // error / busy state, so a re-open starts fresh — the same
                    // reset-on-open idiom as the create-channel "+" above.
                    let gear_btn = ui! {
                        view(style = chan_btn_style.clone(), test_id = "live-server-settings") {
                            icon(SETTINGS).color(|| Color::from(TEXT_MUTED)).size(14.0)
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                if let Some(active_id) = active_live_server.get() {
                                    let cur = live_servers.get().into_iter().find(|s| s.id == active_id);
                                    if let Some(s) = cur {
                                        ss_name.set(s.name.clone());
                                        ss_icon.set(s.icon_url.clone().unwrap_or_default());
                                    }
                                }
                                ss_owner.set(String::new());
                                // Reset the add-member inputs too, so a re-open
                                // starts with an empty id and the default role.
                                ss_add_user_id.set(String::new());
                                ss_add_role.set("member".to_string());
                                ss_busy.set(false);
                                ss_err.set(String::new());
                                ss_open.set(true);
                            }
                            TouchResponse::CONSUMED
                        })
                    };
                    // The user profile bar is the bottom-left of the sidebar in
                    // BOTH branches (Discord convention). The mock branch builds
                    // it as `user` below; the live branch must do the same. Real
                    // logged-in users always take the live branch (they have a
                    // backend server), so omitting it here is the "missing
                    // bottom-left profile bar" bug: the header shows a live
                    // channel but the sidebar never renders the profile.
                    // `user_panel` renders the presence status dot ON the avatar
                    // (like real Discord), so it coexists with the gateway
                    // "Open/Closed" badge in the 72px rail — no separate text
                    // badge is needed in the sidebar slot.
                    let user = user_panel(
                        user_status,
                        joined_voice,
                        authed,
                        user_profile_mode,
                        user_notif_mode,
                        user_shortcuts_mode,
                        muted,
                        deafened,
                    );
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
                                view(style = StyleRules {
                                    flex_grow: Some(runtime_core::Tokenized::Literal(1.0)),
                                    ..Default::default()
                                }) {}
                                { invite_btn }
                                { gear_btn }
                                { add_btn }
                            }
                            // Channel list fills the available vertical space
                            // (flex_grow:1), pinning the user profile bar to
                            // the bottom of the sidebar — Discord convention,
                            // and the same layout the mock branch below uses
                            // (its `view(flex_grow){ scroll_view }` wraps `rows`
                            // before `user`). Without the grow, a short live
                            // channel list leaves the profile bar floating
                            // mid-sidebar instead of bottom-left.
                            view(style = StyleRules {
                                flex_grow: Some(runtime_core::Tokenized::Literal(1.0)),
                                overflow: Some(runtime_core::Overflow::Hidden),
                                ..Default::default()
                            }) {
                                live_content
                            }
                            user
                        }
                    }
                } else {
                    let server = SERVERS.iter().find(|x| x.id == *server_id).unwrap();
                    let mut rows: Vec<Element> = Vec::new();
            // Category sections, in data order: each section is a small
            // uppercase header, its text-channel rows, then its voice rows.
            for cat in server.categories {
                let cat_label = cat.title.to_string();
                // The category's per-category collapse signal (created up front
                // in `app_shell` scope, see the `collapsed` map). The header
                // toggles it; the row loop below re-runs when it flips (the
                // switch key fingerprints it) and re-gates the rows.
                let cat_collapsed_signal = collapsed_render
                    .borrow()
                    .get(&cat.id)
                    .cloned()
                    .unwrap_or_else(|| signal(false));
                rows.push(ui! {
                    CategoryHeader(
                        title = cat_label,
                        collapsed = cat_collapsed_signal,
                    )
                });
                if cat_collapsed_signal.get() {
                    continue; // rows for this category are collapsed
                }
                for c in cat.text {
                    let active = channel;
                    // Discord's channel rows are compact (~34px) with a
                    // rounded pill (inset 8px each side) that fills with the
                    // hover colour, and the active colour when selected — the
                    // row itself stays transparent otherwise.
                    let hovered = signal(false);
                    // Per-channel mute state, owned by the row (the same
                    // per-row `Signal` ownership as `hovered`). Starts unmuted;
                    // a press on the hover-revealed icon below flips it. It
                    // survives a sidebar rebuild within a selection (the
                    // `switch` key doesn't include it), which is the desired
                    // "my mute choice persists" behavior.
                    let muted = signal(false);
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
                            // Hover-revealed per-channel mute control. A `when`
                            // gated on `hovered || muted`: the icon appears only
                            // while the row is hovered, OR while the channel is
                            // already muted (the persistent red glyph is the
                            // mute affordance — Discord keeps a muted channel's
                            // marker visible, not just on hover). A press on it
                            // flips `muted`; the icon's own `on_touch` CONSUMEs
                            // so the row's channel-select `on_touch` (below) is
                            // not also fired. Deliberate per-row OPTION, not a
                            // persistent widget on every row (the graveyard
                            // anti-pattern).
                            if hovered.get() || muted.get() {
                                crate::mute_channel_toggle::MuteChannelToggle(muted)
                            }
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
            // Discord shows active threads as a persistent list near the bottom
            // of the sidebar (above the user bar), not interleaved with the
            // scrollable channel rows. Kept fixed (non-scrolling) so it stays
            // put regardless of channel-list scroll position.
            let thread_list_style = StyleRules {
                flex_direction: Some(FlexDirection::Column),
                padding_top: Some(px(8.0)),
                padding_left: Some(px(8.0)),
                padding_right: Some(px(8.0)),
                ..Default::default()
            };
            // The user profile is the bottom-left of the sidebar (Discord
            // convention). The member list is a separate right-side panel —
            // see `members_switch` further down — so it's not built here.
            let user = user_panel(user_status, joined_voice, authed, user_profile_mode, user_notif_mode, user_shortcuts_mode, muted, deafened);
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
                    // Mock thread list near the channel sidebar: a `ThreadListRow`
                    // above the user panel. Local mock data, no backend. Clicking
                    // it opens the "Bug reports" full-screen overlay (flips
                    // `bug_reports_mode`; the overlay's "Done" pill closes it).
                    view(style = thread_list_style) {
                        ThreadListRow()
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            bug_reports_open.set(!bug_reports_open.get());
                        }
                        TouchResponse::CONSUMED
                    })
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
    // Small muted-grey icon button (the gavel) that opens the Server
    // Moderation panel. A `Copy` handle on `moderation_open` is captured by
    // its `.on_touch` closure below.
    let moderation_style = StyleRules {
        width: Some(px(24.0)),
        height: Some(px(24.0)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        background: Some(col(COMPOSER)),
        border_top_left_radius: Some(px(12.0)),
        border_top_right_radius: Some(px(12.0)),
        border_bottom_left_radius: Some(px(12.0)),
        border_bottom_right_radius: Some(px(12.0)),
        ..Default::default()
    };

    // Live-mode header: a live channel's name has no mock counterpart, so it's
    // looked up from `live_channels` (fetched when the live server was
    // selected) and the topic is the static "No topic set yet" (backend
    // channels have no topic column). `live_channels` is a `Copy` `Signal`, so
    // capturing it here is cheap; the arm re-runs whenever
    // `active_live_channel` (in the key) changes. `None` falls through to the
    // mock header below, byte-for-byte unchanged.
    // `app_shell` owns `moderation_mode` and renders the overlay when it flips
    // on; the gavel button in the header below sets this copy.
    let moderation_open = moderation_mode.clone();
    // Live-mode invite entry: the `USER_PLUS` action in the live header flips
    // this (a clone of `live_invite_mode`, declared above). The live header arm
    // is the only place a real backend `server_id` (`active_live_server`) is
    // in scope, so the invite flow lives here — the mock moderation panel has
    // no real server to mint against.
    let live_invite_open = live_invite_mode.clone();
    let header = switch(
        move || {
            (
                active_server.get(),
                active_channel.get(),
                active_live_channel.get(),
                pinned_count.get(),
            )
        },
        move |(server_id, channel_id, live_cid, pinned_cnt): &(u32, u32, Option<i64>, i64)| {
            let header_style = header_style.clone();
            let hash_style = hash_style.clone();
            let title_style = title_style.clone();
            let topic_style = topic_style.clone();
            let moderation_style = moderation_style.clone();
            // [Feature 16] Real account with no live channel: don't fall
            // through to the mock header (it would show the demo `SERVERS[0]`
            // channel). Show a neutral onboarding title during the login ->
            // server-fetch race / for a zero-server account.
            if is_real_user && live_cid.is_none() {
                return ui! {
                    view(style = header_style) {
                        text(style = StyleRules {
                            color: Some(col(TEXT_MUTED)),
                            ..hash_style
                        }) { "#".to_string() }
                        text(style = title_style) { "No channel selected".to_string() }
                    }
                };
            }
            if let Some(live_id) = *live_cid {
                let title = live_channels
                    .get()
                    .iter()
                    .find(|c| c.id == live_id)
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                let topic = "No topic set yet".to_string();
                // Hoisted before `ui!` (the macro has no `let`-child form):
                // `pin_live` feeds the pin icon's open handler; `pin_count_label`
                // is the badge's count text (read from the `switch` key's tuple).
                let pin_live = live_id;
                let pin_count_label = pinned_cnt.to_string();
                // The live header early-returns here (before the mock server
                // lookup at the bottom of this `switch` render, which would
                // `unwrap()` a non-mock id and panic in live mode).
                return ui! {
                    view(style = header_style) {
                        text(style = hash_style) { "#".to_string() }
                        text(style = title_style) { title }
                        text(style = topic_style) { topic }
                        // Spacer: eats the row's remaining width so the pin /
                        // invite actions sit at the far right.
                        view(style = StyleRules { flex_grow: Some(grow(1.0)), ..Default::default() }) {}
                        // Pin: opens the "Pinned messages" panel and fetches this
                        // channel's REAL pinned list (the panel's Unpin buttons
                        // and the badge below both read the same signals the
                        // backend round trips update). `pinned_count > 0` is
                        // folded into the `switch` key so the badge count
                        // re-renders when an Unpin lands.
                        view(style = moderation_style.clone(), test_id = "pin-live") {
                            icon(PIN).color(|| Color::from(TEXT_MUTED)).size(15.0)
                        }
                        .on_touch(move |_ev| {
                            let open_p = pinned_panel_open;
                            let msgs_p = pinned_messages;
                            let err_p = pinned_error;
                            let rev_p = pinned_revision;
                            let cnt_p = pinned_count;
                            open_p.set(true);
                            err_p.set(String::new());
                            runtime_core::driver::spawn_async(async move {
                                match crate::pins::list_pinned(pin_live).await {
                                    Ok(list) => {
                                        msgs_p.set(list);
                                        rev_p.set(rev_p.get() + 1);
                                    }
                                    Err(e) => {
                                        err_p.set(e);
                                        rev_p.set(rev_p.get() + 1);
                                    }
                                }
                                // Refresh the badge count in the same round trip
                                // (the backend also returns it on Unpin, but on
                                // open we pull it once so the badge is fresh).
                                if let Ok(c) = crate::pins::pinned_count(pin_live).await {
                                    cnt_p.set(c);
                                }
                            });
                            TouchResponse::CONSUMED
                        })
                        // Real pin count badge (replaces the static mock `3`).
                        // `pinned_cnt` comes from the `switch` key above; a
                        // zero count mounts no node (the runtime `if` lowers to
                        // the `when` primitive). Same pill style as the mock
                        // `PinCountBadge`.
                        if *pinned_cnt > 0 {
                            view(style = StyleRules {
                                flex_direction: Some(FlexDirection::Row),
                                align_items: Some(AlignItems::Center),
                                gap: Some(px(4.0)),
                                padding_left: Some(px(7.0)),
                                padding_right: Some(px(7.0)),
                                padding_top: Some(px(3.0)),
                                padding_bottom: Some(px(3.0)),
                                background: Some(col("#3a3c41")),
                                border_top_left_radius: Some(px(8.0)),
                                border_top_right_radius: Some(px(8.0)),
                                border_bottom_left_radius: Some(px(8.0)),
                                border_bottom_right_radius: Some(px(8.0)),
                                ..Default::default()
                            }) {
                                text(style = StyleRules {
                                    font_size: Some(px(13.0)),
                                    color: Some(col("#f2f3f5")),
                                    ..Default::default()
                                }) { "📌".to_string() }
                                text(style = StyleRules {
                                    font_size: Some(px(12.0)),
                                    font_weight: Some(FontWeight::SemiBold),
                                    color: Some(col("#f2f3f5")),
                                    ..Default::default()
                                }) { pin_count_label }
                            }
                        }
                        // Invite: opens the live "Invite People" overlay, which
                        // mints a real invite for this live server.
                        view(style = moderation_style, test_id = "invite-live") {
                            icon(USER_PLUS).color(|| Color::from(TEXT_MUTED)).size(15.0)
                        }
                        .on_touch(move |_ev| {
                            live_invite_open.set(true);
                            TouchResponse::CONSUMED
                        })
                    }
                };
            }
            let server = SERVERS.iter().find(|x| x.id == *server_id).unwrap();
            let channel = server.find_text_channel(*channel_id).unwrap();
            let title = channel.name.to_string();
            let topic = channel.topic.to_string();
            ui! {
                view(style = header_style) {
                    text(style = hash_style) { "#".to_string() }
                    text(style = title_style) { title }
                    text(style = topic_style) { topic }
                    // Spacer: eats the row's remaining width so the pin badge
                    // and search field sit at the far right.
                    view(style = StyleRules { flex_grow: Some(grow(1.0)), ..Default::default() }) {}
                    PinCountBadge()
                    // Gavel: opens the Server Moderation panel (a full-screen
                    // overlay, same idiom as the Profile Settings overlay).
                    view(style = moderation_style, test_id = "server-moderation") {
                        icon(GAVEL).color(|| Color::from(TEXT_MUTED)).size(15.0)
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            moderation_open.set(true);
                        }
                        TouchResponse::CONSUMED
                    })
                }
            }
        },
    );

    // Slowmode indicator for the active channel: a channel-keyed `switch` that
    // resolves the current channel's `slowmode_secs` (`Option<u32>`) and feeds
    // it to `SlowmodeIndicator(secs)`. When the active channel has no slowmode
    // (`None`) — or when on a live channel (backend channels have no slowmode
    // column yet) — it passes `0`, and the component renders *nothing* (no
    // pill, no empty node). Only the couple of mock channels with
    // `slowmode_secs: Some(_)` show the pill. Gated per-channel rather than
    // unconditional, so the "every channel shows a timer" anti-pattern (the one
    // that spliced a bare `SlowmodeIndicator()` into the composer column) is
    // gone. `active_server`/`active_channel` are `Copy` signals, so capturing
    // them is cheap; the arm re-runs only on a real channel/server change.
    let slowmode_el = switch(
        move || (active_server.get(), active_channel.get(), active_live_channel.get()),
        move |(server_id, channel_id, live_cid): &(u32, u32, Option<i64>)| {
            // Live channels and channels without slowmode resolve to `0` ->
            // the indicator renders nothing (the component's own static
            // `if secs > 0` branch mounts no node for `0`).
            let secs: u32 = if live_cid.is_some() {
                0
            } else {
                SERVERS
                    .iter()
                    .find(|x| x.id == *server_id)
                    .and_then(|s| s.find_text_channel(*channel_id))
                    .and_then(|c| c.slowmode_secs)
                    .unwrap_or(0)
            };
            ui! {
                SlowmodeIndicator(secs = secs)
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
    // Attachments per wire `message_id` cloned for the row renderer below —
    // the gateway effect stored each frame's list; a row looks its id up.
    let attach_render = live_attachments.clone();
    // Handles for the per-row edit/delete mode (see `live_edit_row`). Cloned so
    // the `sent_messages` closure moves its own copies (the closures below all
    // `move`-capture their handles rather than sharing one non-`Copy` value).
    let live_editing_k = live_editing.clone();
    let live_delete_k = live_delete_confirm.clone();
    // The mock switch's `move` closure below consumes `reactions`; keep a clone
    // alive for the paint-last `reaction_picker_overlay` call (a separate site).
    let reactions_overlay = reactions.clone();
    // Shared reply/pin handles cloned so the `sent_messages` closure can move
    // its own copies (the action-bar wiring in `message_row_with_actions`).
    let reply_ctx_k = reply_ctx.clone();
    let reply_target_k = reply_target.clone();
    let pin_visible_k = pin_visible.clone();
    let translate_key_k = translate_key.clone();
    // LIVE pin state cloned so the `sent_messages` closure can move its own
    // copies into each live row's real "Pin message" handler (the mock rows
    // instead flash the `pin_visible_k` toast).
    let pinned_msgs_k = pinned_messages.clone();
    let pinned_count_k = pinned_count.clone();
    let pinned_rev_k = pinned_revision.clone();
    let pinned_err_k = pinned_error.clone();
    let sent_messages = switch(
        move || (
            active_server.get(),
            active_channel.get(),
            revision.get(),
            active_live_channel.get(),
            live_revision.get(),
            live_editing_k.get(),
            live_delete_k.get(),
            reaction_rev.get(),
            load_busy.get(),
            live_at_bottom.get(),
            has_more_earlier.get(),
        ),
        move |(server_id, channel_id, _revision, live_cid, _live_rev, _editing, _confirm, _react_rev, _load_busy, _at_bottom, _has_more): &(u32, u32, u32, Option<i64>, u32, Option<i64>, Option<i64>, u32, bool, bool, bool)| {
            // [Feature 16] Real account with no live channel: don't fall
            // through to the mock message list (it would render the demo
            // `SERVERS[0]` seed messages). Show a neutral onboarding hint
            // during the login -> server-fetch race / for a zero-server account.
            if is_real_user && live_cid.is_none() {
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
                        }) { "Create or join a server to start messaging.".to_string() }
                    }
                };
            }
            // Live/real channel active: render the gateway-fed buffer instead of
            // the mock path. `None` falls through to the mock render unchanged.
            if let Some(live_id) = *live_cid {
                let now = crate::data::now_secs();
                let mine: Vec<LiveMsg> = live_read
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
                let self_id = crate::auth::CURRENT_USER_ID.load(std::sync::atomic::Ordering::SeqCst);
                let editing = live_editing;
                let delete_c = live_delete_confirm;
                let edit_text = live_edit_text;
                let gw = gateway;
                let picker_l = picker;
                let reply_l = reply_ctx_k;
                let trans_l = translate_key_k;
                let mut rows: Vec<Element> = Vec::with_capacity(mine.len() * 2);
                // Top-of-list scrollback affordance: a "Load earlier messages"
                // pill while more older history exists (`has_more_earlier`), or
                // a "beginning of channel" anchor once the reader has reached
                // the start (nothing older left to load). Both sit in-flow
                // above the oldest row — the reader scrolls up to find them
                // (NOT a floating pill, so no `live_at_bottom` gating). The
                // channel name resolves from `live_channels` (the same lookup
                // the header title uses). `mine` is oldest-first, so `mine[0]`
                // is the scrollback cursor for the next `History` page.
                let oldest_id = mine.first().map(|m| m.0);
                let chan_name: String = live_channels
                    .get()
                    .iter()
                    .find(|c| c.id == live_id)
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                if has_more_earlier.get() {
                    // `on_load` sets `load_busy` (gates the pill's tap + mutes
                    // its label) and fires the scrollback page. `load_busy` is
                    // cleared by the gateway effect on the `History` marker
                    // frame, which arrives before the page's `Message` frames.
                    let on_load = Rc::new({
                        let busy = load_busy.clone();
                        let cid2 = live_id;
                        let oldest = oldest_id;
                        move || {
                            busy.set(true);
                            if let Some(before) = oldest {
                                gw.send(discord_server::server_settings::WsClientMsg::History {
                                    channel_id: cid2,
                                    before_id: Some(before),
                                });
                            }
                        }
                    });
                    rows.push(load_earlier_button(load_busy.clone(), on_load));
                } else {
                    rows.push(channel_anchor(&chan_name));
                }
                // Message-grouping state carried across iterations (the
                // buffer is oldest-first): the previous row's author label +
                // timestamp, for the `same_group` test below.
                let mut prev: Option<(String, i64)> = None;
                for m in mine.iter() {
                    let (mid, aid, author, body, ts, edited_at, reply_to) = m;
                    let when = format_when(*ts, now);
                    let full = format_full_when(*ts);
                    let is_mine = self_id != 0 && *aid == self_id;
                    let lkey = MessageKey::Live { id: *mid };
                    // Grouped with the row above? (same author, within the
                    // window). Edited messages keep their own header: an
                    // `(edited)` marker on a headerless row would be
                    // orphaned, so `edited_at.is_some()` breaks the group.
                    let grouped = prev
                        .as_ref()
                        .map(|(pa, pts)| same_group(pa, *pts, author.as_str(), *ts))
                        .unwrap_or(false)
                        && edited_at.is_none();
                    // Feature 5: a row with a `reply_to_message_id` gets the
                    // quoted reference strip rendered above it. The original's
                    // author + body are resolved from the already-loaded buffer
                    // (`mine`); if it isn't there yet (not loaded / deleted)
                    // there is nothing to quote, so no strip.
                    if let Some(orig_id) = *reply_to {
                        if let Some(orig) = mine.iter().find(|o| o.0 == orig_id) {
                            rows.push(crate::reply_reference_strip::reply_reference_strip(
                                orig.2.clone(),
                                crate::reply_reference_strip::reply_snippet(&orig.3),
                            ));
                        }
                    }
                    // Real "Pin message": persist via the backend `pin_message`
                    // RPC (moderator+), then update the header badge from the
                    // result's fresh `pinned_count` and refresh the panel's
                    // list (the result carries `channel_id`, so no extra id is
                    // needed). A rejection (e.g. a plain member) lands in
                    // `pinned_error` and bumps `pinned_revision` so the panel
                    // body surfaces it. `mid2` + the `Copy` pin signals are
                    // captured by the `move` handler so it owns them.
                    let mid2 = *mid;
                    let pin_msgs = pinned_msgs_k;
                    let pin_cnt = pinned_count_k;
                    let pin_rev = pinned_rev_k;
                    let pin_err = pinned_err_k;
                    let pin_c = Rc::new(move || {
                        runtime_core::driver::spawn_async(async move {
                            match crate::pins::pin_message(mid2).await {
                                Ok(res) => {
                                    pin_cnt.set(res.pinned_count);
                                    if let Ok(list) = crate::pins::list_pinned(res.channel_id).await {
                                        pin_msgs.set(list);
                                    }
                                    pin_rev.set(pin_rev.get() + 1);
                                }
                                Err(e) => {
                                    pin_err.set(e);
                                    pin_rev.set(pin_rev.get() + 1);
                                }
                            }
                        });
                    });
                    rows.push(live_edit_row(
                        *mid,
                        author.as_str(),
                        BUTTON,
                        body.as_str(),
                        when,
                        full,
                        edited_at.is_some(),
                        is_mine,
                        grouped,
                        editing,
                        edit_text,
                        delete_c,
                        gw,
                        lkey,
                        picker_l,
                        reply_l,
                        reply_target_k,
                        pin_visible_k,
                        pin_c,
                        trans_l,
                    ));
                    // Image attachments render UNDER the body: the persisted
                    // list the gateway effect stored for this wire id (empty
                    // rows push nothing, so every other row is unchanged).
                    let atts = attach_render.borrow().get(&*mid).cloned().unwrap_or_default();
                    if !atts.is_empty() {
                        rows.push(attachment_row(&atts));
                    }
                    // Reaction strip under each live row, keyed by the real wire
                    // `message_id` — the same session-local pill store the mock
                    // rows use, so live reactions behave exactly like mock ones.
                    rows.push(reaction_area(
                        lkey,
                        reactions.clone(),
                        reaction_rev,
                        picker_l,
                    ));
                    prev = Some((author.clone(), *ts));
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
            // Message-grouping state carried across both loops (seed then
            // this session's sends — oldest-first): the previous row's author
            // + timestamp for the `same_group` test.
            let mut prev: Option<(&str, i64)> = None;
            for (i, m) in channel.messages.iter().enumerate() {
                let ts = m.ts_secs(now);
                let when = format_when(ts, now);
                // Mock seed messages carry no edit state: pass `false` so they
                // don't show a fake "(edited)" tag (the live path uses
                // `edited_at.is_some()`).
                let grouped = prev
                    .map(|(pa, pts)| same_group(pa, pts, m.author, ts))
                    .unwrap_or(false);
                // MOCK pin: flash the "Message pinned!" toast and auto-dismiss
                // it after a beat (the pre-backend idiom, now caller-supplied
                // since `message_row_with_actions` takes `on_pin` as a `Rc`).
                let pv = pin_visible_k;
                let on_pin = Rc::new(move || {
                    pv.set(true);
                    let pv_d = pv;
                    after_ms_detached(2_500, move || pv_d.set(false));
                });
                rows.push(message_row_with_actions(
                    m.author,
                    m.color,
                    m.body,
                    when,
                    format_full_when(ts),
                    false,
                    grouped,
                    MessageKey::Mock { cid, idx: i },
                    picker,
                    reply_ctx_k,
                    reply_target_k,
                    pin_visible_k,
                    translate_key_k,
                    on_pin,
                    None,
                    None,
                ));
                // Reaction strip under each mock row, keyed by this row's
                // rendered-list position (seed messages first, so index `i`).
                rows.push(reaction_area(
                    MessageKey::Mock { cid, idx: i },
                    reactions.clone(),
                    reaction_rev,
                    picker,
                ));
                prev = Some((m.author, ts));
            }
            let mine: Vec<(u32, &'static str, String, i64)> = sent_read.borrow().clone();
            let mine: Vec<(u32, &'static str, String, i64)> = mine
                .into_iter()
                .filter(|(mcid, _, _, _)| *mcid == cid)
                .collect();
            for (i, (_mcid, author, body, ts)) in mine.iter().enumerate() {
                let when = format_when(*ts, now);
                // Own prior sends get the same hover bar as the seed rows. `false`
                // for edited -- these are freshly composed, never-edited sends.
                let grouped = prev
                    .map(|(pa, pts)| same_group(pa, pts, author, *ts))
                    .unwrap_or(false);
                // MOCK pin: flash the toast + auto-dismiss (see the seed loop
                // above — the same caller-supplied `on_pin` idiom).
                let pv = pin_visible_k;
                let on_pin = Rc::new(move || {
                    pv.set(true);
                    let pv_d = pv;
                    after_ms_detached(2_500, move || pv_d.set(false));
                });
                rows.push(message_row_with_actions(
                    author,
                    BUTTON,
                    body.as_str(),
                    when,
                    format_full_when(*ts),
                    false,
                    grouped,
                    MessageKey::Mock { cid, idx: channel.messages.len() + i },
                    picker,
                    reply_ctx_k,
                    reply_target_k,
                    pin_visible_k,
                    translate_key_k,
                    on_pin,
                    None,
                    None,
                ));
                // Sent rows continue the list position after the seed rows, so
                // their key is `seed_len + i` (see `ReactionMap` doc).
                rows.push(reaction_area(
                    MessageKey::Mock { cid, idx: channel.messages.len() + i },
                    reactions.clone(),
                    reaction_rev,
                    picker,
                ));
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

    // Typing indicator row — sits directly above the composer. In live mode
    // (a real backend channel is open) it renders the latest typer in
    // `live_typing`, keyed on `(active_live_channel, live_typing_rev)`. The
    // mock fallback (server 1, channel 101 = #general, reads "dyno is
    // typing...") is kept for the mock path. Both branches render a row with
    // the same line-box height (4/4/16/16 paddings + 12px font) so the
    // composer's y-offset never jumps on channel switch.
    let typing_live_chan = active_live_channel.clone();
    let typing_rev_for_key = live_typing_rev.clone();
    let typing_map_for_key = live_typing.clone();
    let typing_row = switch(
        move || {
            let live = typing_live_chan.get();
            let (server, channel) = (active_server.get(), active_channel.get());
            // The key includes the live revision so the row re-renders when a
            // Typing frame arrives or the auto-clear fires. The mock key is
            // `(server, channel)` (unchanged from the previous shape) — for
            // live channels both are irrelevant (the arm below keys on the
            // live channel + rev).
            (server, channel, live, typing_rev_for_key.get())
        },
        move |(server_id, channel_id, live_chan, _rev): &(u32, u32, Option<i64>, u32)| {
            // [Feature 16] Real account with no live channel: render an empty
            // spacer (no mock typing indicator) during the login ->
            // server-fetch race / for a zero-server account.
            if is_real_user && live_chan.is_none() {
                return ui! {
                    view(style = StyleRules { height: Some(px(0.0)), ..Default::default() }) {}
                };
            }
            // Live mode: show the typer from the gateway, or the empty
            // spacer if nobody is typing right now.
            if let Some(live_id) = *live_chan {
                let label = {
                    let map = typing_map_for_key.borrow();
                    map.get(&live_id).map(|e| e.1.clone())
                };
                if let Some(label) = label {
                    ui! { TypingIndicator(name = label.clone()) }
                } else {
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
            } else if *server_id == 1 && *channel_id == 101 {
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
        ..borderless_input()
    };
    // Circular muted-grey icon buttons flanking the composer input — the '+'
    // attachment affordance on the left (stages/unstages a mock attachment),
    // the smile emoji affordance on the right (toggles the emoji popover).
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

    // One send action, fired by the Enter key on the composer (Discord has no
    // visible Send button — Enter is the only submit affordance). Every captured
    // signal/Rc is cloned behind an `Rc` so the handler can be `Fn` (callable
    // from the `text_input on_key_down` closure).
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
        let attached = pending_attachment.get();
        // A message must carry a body OR at least one staged image — the
        // same "not both empty" gate the backend enforces on the `Send`
        // frame, mirrored client-side so a no-op send never round-trips.
        if body.trim().is_empty() && attached.is_empty() {
            return;
        }
        // Live/real mode: stage the pending images as the wire frame's
        // `AttachmentInput` list — the data URLs were already validated +
        // encoded client-side (`stage_attachment`), and the backend
        // re-validates and persists them with the message row.
        let inputs: Vec<discord_server::server_settings::AttachmentInput> = attached
            .iter()
            .map(|(_, filename, mime, url)| discord_server::server_settings::AttachmentInput {
                filename: filename.clone(),
                mime: mime.clone(),
                data: url.clone(),
            })
            .collect();
        // Live/real mode: send over the gateway socket to the live channel.
        // The server echoes it back as a `Message` frame, which the incoming
        // effect buffers and bumps `live_revision`, rebuilding the list.
        if let Some(live_cid) = live_chan.get() {
            gateway_send.send(discord_server::server_settings::WsClientMsg::Send {
                channel_id: live_cid,
                content: body.clone(),
                reply_to_message_id: reply_target.get(),
                attachments: inputs,
            });
            send_composer.set(String::new());
            pending_attachment.set(Vec::new());
            reply_ctx.set(None);
            reply_target.set(None);
            return;
        }
        // Mock path: the per-session log carries only a body string, so
        // staged images ride it as `[attachment: <name>]` lines (the
        // pre-backend affordance; live mode sends real data URLs).
        let out_body = if attached.is_empty() {
            body
        } else {
            let names: Vec<String> = attached.iter().map(|(_, filename, _, _)| filename.clone()).collect();
            format!("{}\n[attachment: {}]", body, names.join(", "))
        };
        let cid = active_channel.get();
        sent_write.borrow_mut().push((cid, "Nova", out_body, crate::data::now_secs()));
        send_composer.set(String::new());
        pending_attachment.set(Vec::new());
        // Bump the revision so the message-list `switch` rebuilds (its key
        // dedups on value, so re-setting the same channel alone would not).
        revision_bump.set(revision_bump.get() + 1);
    };
    let on_send = Rc::new(do_send);
    // Robot test hook (`send.now`): Enter on the composer is a raw keyboard
    // event the robot bridge cannot synthesize, so an E2E test drives the
    // exact action Enter invokes — the same `on_send` `Rc` the `on_key_down`
    // handler calls — body + staged attachments included.
    runtime_vocabulary::robot_only! {
        register_send_now_hook(on_send.clone());
    };
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

    // --- Composer placeholder: "Message #<channel>" (Discord's real format,
    // channel-suffixed)
    // Derives the active channel's display name (same source of truth as the
    // header: the live backend's channel list when on a live channel, else the
    // mock server's text-channel table) and prefixes it with "Message #".
    // A `memo` keeps it reactive: switching channels re-derives the string
    // without the composer's `text_input` needing to rebuild its own node.
    // The `placeholder_reactive` prop wants `Reactive<Option<String>>`, so the
    // memo yields `Some("Message #<name>")` (a `None` placeholder would be
    // pointless — there is always an active channel).
    let ph_server = active_server.clone();
    let ph_channel = active_channel.clone();
    let ph_live = active_live_channel.clone();
    let ph_channels = live_channels.clone();
    let composer_placeholder = memo(move || {
        let name = if let Some(live_id) = ph_live.get() {
            ph_channels
                .get()
                .iter()
                .find(|c| c.id == live_id)
                .map(|c| c.name.to_string())
                .unwrap_or_default()
        } else {
            let server = SERVERS.iter().find(|x| x.id == ph_server.get()).unwrap();
            server
                .find_text_channel(ph_channel.get())
                .map(|c| c.name.to_string())
                .unwrap_or_default()
        };
        Some(format!("Message #{name}"))
    });

    // --- @-mention autocomplete (live mode) ------------------------------
    // Pure client-side: typing `@<prefix>` in the composer opens a dropdown
    // of the active server's members whose name starts with the prefix
    // (case-insensitive). The roster is derived from the already-buffered
    // gateway messages (every distinct author label that has posted) — no
    // backend call. `ac` mirrors the active query + highlight so both the key
    // handler and the dropdown's click handler can share it (the closures
    // must all be `Fn`).
    let ac: Rc<RefCell<(String, i64)>> = Rc::new(RefCell::new((String::new(), 0)));
    let suggestions: Signal<Vec<String>> = signal(Vec::new());
    let ac_hi: Signal<i64> = signal(0);
    let ac_sugg = suggestions.clone();
    let ac_hi_sig = ac_hi.clone();
    let ac_state = ac.clone();
    let composer_sig = composer.clone();
    let live_channels_sig = live_channels.clone();
    let live_messages_sig = live_messages.clone();
    // Recompute the filtered suggestion list from the current composer text +
    // active server roster, and reset the highlight to 0. Held in an `Rc` so
    // `update_on_type` can share it (closures are not `Clone`, `Rc` is `Copy`).
    let recompute = Rc::new(move || {
        let text = composer_sig.get();
        let q = mention_active_query(&text);
        let (members, list) = match &q {
            None => (String::new(), Vec::new()),
            Some(q) => {
                let members =
                    live_member_roster(&live_channels_sig.get(), &live_messages_sig.borrow());
                let list = filter_members(q, members);
                (q.clone(), list)
            }
        };
        *ac_state.borrow_mut() = (members.clone(), 0);
        ac_hi_sig.set(0);
        ac_sugg.set(list);
    });
    let recompute2 = recompute.clone();
    // Throttle for the outbound Typing frame: channel_id -> epoch-millis of the
    // last Typing frame sent while typing in that channel. At most one per ~3 s
    // per channel, so a fast typer doesn't flood the gateway. Plain
    // `Rc<RefCell<HashMap>>` — no reactivity needed (it only gates the side
    // effect below, never renders anything).
    let typing_last_sent_h: Rc<RefCell<HashMap<i64, i64>>> =
        Rc::new(RefCell::new(HashMap::new()));
    let typing_chan_h = active_live_channel.clone();
    let typing_gateway_h = gateway; // `UseSocket` is `Copy`
    // Update on every keystroke: recompute from the (already-updated) value,
    // and — in live mode with actual text — emit a throttled Typing frame so
    // other members' clients see "… is typing…". The server fans that out to
    // the channel audience (minus the sender), so this is the client half of
    // the typing indicator; the inbound half is the `Typing` arm in the
    // gateway effect above.
    let update_on_type = move |s: String| {
        // Throttled outbound Typing frame: at most one per channel per ~3 s,
        // and only while there is actual text. Evaluated BEFORE
        // `composer_sig.set(s)` because `set` moves `s` — the check only borrows
        // it via `trim()`. The server fans the frame out to the channel
        // audience (minus the sender), so this is the client half; the inbound
        // half is the `Typing` arm in the gateway effect above.
        let open_live = typing_chan_h.get();
        if !s.trim().is_empty() {
            if let Some(cid) = open_live {
                let now_ms = runtime_shared::time::epoch_millis();
                let too_soon = typing_last_sent_h
                    .borrow()
                    .get(&cid)
                    .map(|last| now_ms - *last < 3_000)
                    .unwrap_or(false);
                if !too_soon {
                    typing_gateway_h
                        .send(discord_server::server_settings::WsClientMsg::Typing {
                            channel_id: cid,
                        });
                    typing_last_sent_h.borrow_mut().insert(cid, now_ms);
                }
            }
        }
        composer_sig.set(s);
        (recompute2)();
    };
    // Select a suggestion: replace the partial `@partial` with
    // `@fullusername ` (truncating at the active `@`) and close the dropdown.
    // Held in an `Rc` so both the key handler and the row click handler can
    // share it. Rc is Clone but NOT Copy, so each `move` closure that needs
    // its own handle gets a fresh `.clone()` from the original `ac`/`apply`
    // right before capture -- the earlier `ac_state` binding above was
    // already consumed by `recompute`'s closure.
    let ac_state = ac.clone();
    let apply = Rc::new(move |username: &str| {
        let mut text = composer_sig.get();
        if let Some(at) = text.rfind('@') {
            text.truncate(at);
        }
        text.push('@');
        text.push_str(username);
        text.push(' ');
        composer_sig.set(text);
        *ac_state.borrow_mut() = (String::new(), 0);
        ac_hi_sig.set(0);
        ac_sugg.set(Vec::new());
    });
    // Layered key handler: while the dropdown is open, Enter/Tab select the
    // highlighted suggestion (swallowing the key so it does NOT send the
    // message), arrows move the highlight, and Escape closes; everything else
    // falls through to the platform default (which still routes Enter to send
    // when the dropdown is closed).
    let ac_state = ac.clone();
    let apply_for_key = apply.clone();
    let ac_key = move |ev: &KeyEvent| {
        let (q, hi) = ac_state.borrow().clone();
        let open = !q.is_empty() || !ac_sugg.get().is_empty();
        if !open {
            return KeyOutcome::Default;
        }
        match ev.key.as_str() {
            "Enter" | "Tab" => {
                let list = ac_sugg.get().clone();
                match list.get(hi as usize) {
                    Some(u) => {
                        (apply_for_key)(u.as_str());
                        KeyOutcome::PreventDefault
                    }
                    None => {
                        // Highlight out of range: close the dropdown and let
                        // Enter send (Tab: swallow).
                        *ac_state.borrow_mut() = (String::new(), 0);
                        ac_hi_sig.set(0);
                        ac_sugg.set(Vec::new());
                        if ev.key == "Tab" {
                            KeyOutcome::PreventDefault
                        } else {
                            KeyOutcome::Default
                        }
                    }
                }
            }
            "ArrowDown" | "ArrowUp" => {
                let count = ac_sugg.get().len() as i64;
                if count > 0 {
                    let n = if ev.key == "ArrowDown" {
                        (hi + 1) % count
                    } else {
                        (hi - 1 + count) % count
                    };
                    ac_state.borrow_mut().1 = n;
                    ac_hi_sig.set(n);
                }
                KeyOutcome::PreventDefault
            }
            "Escape" => {
                *ac_state.borrow_mut() = (String::new(), 0);
                ac_hi_sig.set(0);
                ac_sugg.set(Vec::new());
                KeyOutcome::PreventDefault
            }
            _ => KeyOutcome::Default,
        }
    };
    // The dropdown element: absolutely positioned above the composer input,
    // showing only when there is an active query with matching members.
    let ac_render = ac.clone();
    let ac_hi_r = ac_hi.clone();
    let apply_touch = apply.clone();
    let ac_el = switch(
        move || suggestions.get(),
        move |list: &Vec<String>| {
            let q = ac_render.borrow().0.clone();
            let hi = ac_hi_r.get();
            if q.is_empty() {
                return ui! {
                    view(style = StyleRules { height: Some(px(0.0)), ..Default::default() }) {}
                };
            }
            let rows: Vec<Element> = list
                .iter()
                .enumerate()
                .map(|(i, name)| {
                    let on = (i as i64) == hi;
                    let n = name.clone();
                    // Fresh per-row clone: `apply_touch` is an `Rc`, not `Copy`,
                    // and this `.map()` runs once per suggestion row, so each
                    // row's `move` closure below needs its own handle (the
                    // same rule documented in components/connection_quality_
                    // indicator.rs for its per-bar StyleRules).
                    let apply_touch = apply_touch.clone();
                    ui! {
                        view(style = StyleRules {
                            height: Some(px(34.0)),
                            padding_left: Some(px(14.0)),
                            padding_right: Some(px(14.0)),
                            background: Some(col(if on { BG_ACTIVE } else { "transparent" })),
                            flex_direction: Some(FlexDirection::Row),
                            align_items: Some(AlignItems::Center),
                            ..Default::default()
                        }) {
                            text(style = StyleRules {
                                font_size: Some(px(14.0)),
                                color: Some(col(if on { TEXT } else { TEXT_CHANNEL })),
                                ..Default::default()
                            }) { format!("@{}", name) }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                (apply_touch)(n.as_str());
                            }
                            TouchResponse::CONSUMED
                        })
                    }
                })
                .collect();
            ui! {
                view(style = StyleRules {
                    position: Some(Position::Absolute),
                    left: Some(px(16.0)),
                    right: Some(px(16.0)),
                    bottom: Some(px(60.0)),
                    max_height: Some(px(190.0)),
                    background: Some(col(BG_SIDEBAR)),
                    border_top_left_radius: Some(px(8.0)),
                    border_top_right_radius: Some(px(8.0)),
                    border_bottom_left_radius: Some(px(8.0)),
                    border_bottom_right_radius: Some(px(8.0)),
                    overflow: Some(Overflow::Hidden),
                    ..Default::default()
                }) {
                    scroll_view(style = StyleRules {
                        flex_grow: Some(grow(1.0)),
                        ..Default::default()
                    }) { rows }
                }
            }
        },
    );

    // --- staged-attachment chip + emoji popover --------------------------
    // Both are pure-local-UI affordances for the two composer icon buttons
    // (`+` = stage/unstage a mock attachment, smile = toggle the emoji grid).
    // They are positioned siblings of the composer inside the same
    // `Position::Relative` ancestor that already holds `ac_el`, so both anchor
    // correctly: the chip is a normal (in-flow) row just above the input and
    // collapses to zero height when nothing is staged, while the emoji popover
    // is `Position::Absolute` floating just above the composer.

    // Staged-attachment strip: the composer's pending images, each a small
    // thumbnail cell with an 'x' that removes just that entry (the send
    // consumes the whole list). `switch` on the staged `Vec<PendingImage>`
    // rebuilds on every stage/removal/send; empty renders zero height.
    // `strip_pending` is a `Copy` handle on `pending_attachment`; the nested
    // `on_touch` `move` closure gets a fresh per-render copy (`rm`) so it
    // never moves the outer closure's handle (E0507 pattern from `chip_clear`).

    // Attach flow: open the file picker (multi-image) and stage every picked
    // file through `stage_attachment` — the SAME validate + encode the
    // avatar-upload picker callback runs (`profile_settings`), against the
    // message-attachment limits. Rejections (bad type / oversize / over
    // count) land in the visible `attach_error` line below the composer.
    // Same busy/error/picker triple idiom as the profile screen's avatar
    // upload; the '+' button calls it from its `on_touch`.
    let do_attach = move || {
        if attach_busy.get() {
            return;
        }
        let busy = attach_busy;
        let err = attach_error;
        let pending = pending_attachment;
        let nextk = attach_next_key;
        busy.set(true);
        err.set(String::new());
        runtime_core::driver::spawn_async(async move {
            let outcome = match FilePicker::new()
                .pick(PickRequest::media(MediaKind::Images).multiple())
                .await
            {
                Ok(outcome) => outcome,
                Err(e) => {
                    err.set(format!("file picker: {e}"));
                    busy.set(false);
                    return;
                }
            };
            match outcome {
                PickOutcome::Picked(files) => {
                    for file in files.iter() {
                        match file.read_all().await {
                            Ok(bytes) => stage_attachment(
                                pending.clone(),
                                nextk.clone(),
                                err.clone(),
                                file.name().to_string(),
                                file.mime().to_string(),
                                &bytes,
                            ),
                            Err(e) => err.set(format!("could not read the file: {e}")),
                        }
                    }
                }
                // Cancelled (dismissed picker): back to idle, not an error.
                _ => {}
            }
            busy.set(false);
        });
    };

    // Register the robot test hook so an MCP E2E can stage a composer
    // attachment through the same `stage_attachment` validate → encode path
    // the file-picker callback above runs — without driving the native file
    // dialog (which no in-page tool can tap). Rebuilt per screen open; a
    // no-op outside robot builds.
    runtime_vocabulary::robot_only! {
        register_attach_stage_hook(pending_attachment.clone(), attach_next_key.clone(), attach_error.clone());
    };

    let strip_pending = pending_attachment;
    let attach_chip = switch(
        move || pending_attachment.get(),
        move |list: &Vec<PendingImage>| {
            if list.is_empty() {
                return ui! {
                    view(style = StyleRules { height: Some(px(0.0)), ..Default::default() }) {}
                };
            }
            let cells: Vec<Element> = list.iter().map(|(key, filename, _mime, url)| {
                let k = *key;
                let rm = strip_pending;
                ui! {
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        gap: Some(px(6.0)),
                        ..Default::default()
                    }) {
                        crate::profile_settings::attachment_thumbnail(signal(crate::auth::media_url(url.clone())).into(), filename.clone(), 56.0)
                        view(style = StyleRules {
                            width: Some(px(16.0)),
                            height: Some(px(16.0)),
                            align_items: Some(AlignItems::Center),
                            justify_content: Some(JustifyContent::Center),
                            border_top_left_radius: Some(px(8.0)),
                            border_top_right_radius: Some(px(8.0)),
                            border_bottom_left_radius: Some(px(8.0)),
                            border_bottom_right_radius: Some(px(8.0)),
                            ..Default::default()
                        }) {
                            text(style = StyleRules {
                                font_size: Some(px(12.0)),
                                color: Some(col(TEXT_MUTED)),
                                ..Default::default()
                            }) { "✕".to_string() }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                let mut staged = rm.get();
                                staged.retain(|(entry_key, _, _, _)| *entry_key != k);
                                rm.set(staged);
                            }
                            TouchResponse::CONSUMED
                        })
                    }
                }
            }).collect();
            ui! {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Column),
                    margin_top: Some(px(8.0)),
                    margin_bottom: Some(px(8.0)),
                    margin_left: Some(px(16.0)),
                    margin_right: Some(px(16.0)),
                    gap: Some(px(8.0)),
                    ..Default::default()
                }) { cells }
            }
        },
    );

    // Emoji popover: `switch` on `emoji_open`. Closed = a zero-height spacer
    // (so it never occupies layout); open = a dark rounded panel floating just
    // above the composer, holding a wrapped grid of clickable glyph cells.
    // Clicking a glyph appends it to the composer text and closes the popover.
    let emoji_open_sig = emoji_open;
    let composer_e = composer;
    let emoji_popover = switch(
        move || emoji_open.get(),
        move |open: &bool| {
            if !open {
                return ui! {
                    view(style = StyleRules { height: Some(px(0.0)), ..Default::default() }) {}
                };
            }
            let c = composer_e;
            let eo = emoji_open_sig;
            let pick_cb: Rc<dyn Fn(String)> = Rc::new(move |glyph: String| {
                let mut t = c.get();
                t.push_str(&glyph);
                c.set(t);
                eo.set(false);
            });
            ui! {
                view(style = StyleRules {
                    position: Some(Position::Absolute),
                    right: Some(px(16.0)),
                    bottom: Some(px(76.0)),
                    ..Default::default()
                }) {
                    EmojiPicker(on_pick = Some(pick_cb))
                }
            }
        },
    );

    // Layer the autocomplete key handler in front of the send (`on_enter`)
    // handler: autocomplete runs first and returns `Default` whenever the
    // dropdown is closed, so the send path still receives the key otherwise.
    let ac_key2 = ac_key.clone();
    let send_enter = on_enter.clone();
    let composer_key = move |ev: &KeyEvent| {
        let r = (ac_key2)(ev);
        if r != KeyOutcome::Default {
            return r;
        }
        (send_enter)(ev)
    };

    // --- member list (right side) ----------------------------------------
    // Rebuilds when the active server changes (each server has its own
    // roster) AND when the signed-in user's live status changes — the user
    // regroups to their new status bucket, and the member count follows.
    // Rendered as the right-most top-level column.
    // The right-side member column. Mirrors the middle view's mock/live split:
    // when a *live* server is active (`active_live_server` is `Some`, set the
    // moment a live channel is opened) render the real `live_member_panel`
    // backed by the gateway-fed `live_members` roster + `live_presence` map;
    // otherwise (a mock server is selected) render the static `member_panel`.
    // The key folds in every signal the live panel reads — roster, error,
    // kick-confirm, and the presence revision — so any of those changes (e.g.
    // a `Presence` frame arriving) rebuilds the panel and the dots update.
    // [Feature 16] A real account with no live server yet renders an empty
    // roster (not the mock `USER`/roster, which would leak demo identities
    // like "Nova" into a real account's onboarding state) — the key folds in
    // `is_real_user` so that transition re-renders the panel too. Guests
    // (`is_real_user` false) always fall back to the mock panel.
    let live_members_panel = live_members.clone();
    let live_member_error_panel = live_member_error.clone();
    let live_kick_panel = live_kick_confirm.clone();
    let live_ban_panel = live_ban_confirm.clone();
    let live_timeout_panel = live_timeout_confirm.clone();
    let live_timeout_dur_panel = live_timeout_duration.clone();
    let live_presence_panel = live_presence.clone();
    // The member row's tap-popup state (which member's Kick/Ban/Timeout menu
    // is open, `None` = closed). In the `members_switch` key so opening the
    // menu from a row tap rebuilds the panel and mounts the portal.
    let live_member_menu_panel = live_member_menu.clone();
    // The message-search corpus the mock member panel's `MessageSearchBar`
    // filters over: the active channel's seed messages plus this session's
    // sends, as `(author, text)` rows. A `memo` keyed on the same signals the
    // message list uses (`active_server`/`active_channel` + the `revision`
    // send-counter), so switching channels or sending a message rebuilds the
    // corpus; the bar then live-filters it client-side.
    let search_corpus = memo(move || -> Vec<(String, String)> {
        let (server_id, channel_id) = (active_server.get(), active_channel.get());
        // Read `revision` (the per-send counter) so the memo subscribes to it:
        // `sent` is an `Rc<RefCell<…>>`, invisible to the reactive graph, so
        // without this read a send would not rebuild the corpus — the same
        // counter the message list's `switch` is keyed on.
        let _rev = revision.get();
        let sent_snapshot = sent.borrow().clone();
        let server = match SERVERS.iter().find(|x| x.id == server_id) {
            Some(s) => s,
            None => return Vec::new(),
        };
        let channel = match server.find_text_channel(channel_id) {
            Some(c) => c,
            None => return Vec::new(),
        };
        let mut corpus: Vec<(String, String)> = channel
            .messages
            .iter()
            .map(|m| (m.author.to_string(), m.body.to_string()))
            .collect();
        for (_cid, author, body, _ts) in sent_snapshot.iter().filter(|(cid, _, _, _)| *cid == channel_id) {
            corpus.push((author.to_string(), body.clone()));
        }
        corpus
    });
    // The LIVE counterpart: the same `(author, text)` shape, but sourced from
    // the gateway-fed `live_messages` buffer of the active live channel — the
    // same data the live message list renders, so search hits what the user
    // can see. `live_messages` is an `Rc<RefCell<…>>` (invisible to the
    // reactive graph), so `live_revision` is read to subscribe: any gateway
    // message or scrollback page bumps it (the message list's `switch` is
    // keyed on the same pair). Switching live channels changes
    // `active_live_channel` and re-derives over the new buffer.
    let live_search_corpus = memo(move || -> Vec<(String, String)> {
        let live_cid = active_live_channel.get();
        let _rev = live_revision.get();
        let cid = match live_cid {
            Some(c) => c,
            None => return Vec::new(),
        };
        live_messages
            .borrow()
            .get(&cid)
            .map(|msgs| msgs.iter().map(|m| (m.2.clone(), m.3.clone())).collect())
            .unwrap_or_default()
    });
    // Dedicated clone for the `members_switch` arm closure below (it moves its
    // own copy; the `memo` itself is `Copy`).
    let live_search_corpus_panel = live_search_corpus.clone();
    let members_switch = switch(
        move || {
            (
                active_server.get(),
                user_status.get(),
                active_live_server.get(),
                live_members.get(),
                live_member_error.get(),
                live_kick_confirm.get(),
                live_ban_confirm.get(),
                live_timeout_confirm.get(),
                live_timeout_duration.get(),
                live_presence_rev.get(),
                live_member_menu.get(),
                is_real_user,
            )
        },
        move |(_server_id, user_status, live_active, _live_members, _live_err, _live_kick, _live_ban, _live_timeout, _live_timeout_dur, _presence_rev, _live_menu, real_user): &(
            u32,
            Status,
            Option<i64>,
            Vec<crate::moderation::MemberInfo>,
            String,
            Option<i64>,
            Option<i64>,
            Option<i64>,
            i64,
            u32,
            Option<i64>,
            bool,
        )| {
            if let Some(ls) = *live_active {
                // Live server: real roster + live presence dots. The panel owns
                // its width internally (fixed 260px, same as the mock default
                // before a user drags the handle).
                return live_member_panel(
                    ls,
                    live_members_panel.clone(),
                    live_member_error_panel.clone(),
                    live_kick_panel.clone(),
                    live_ban_panel.clone(),
                    live_timeout_panel.clone(),
                    live_timeout_dur_panel.clone(),
                    // `Rc` isn't `Copy`, and this is an `Fn` (re-entrant) closure —
                    // clone the handle each render.
                    live_presence_panel.clone(),
                    live_search_corpus_panel.read_only(),
                    live_member_menu_panel.clone(),
                );
            }
            if *real_user {
                // Real account, no live server yet -> an empty member panel
                // (not the mock `USER`/roster, which would leak demo identities
                // like "Nova" into a real account's onboarding state).
                return ui! {
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
                        }) {
                            text(style = StyleRules {
                                padding_top: Some(px(14.0)),
                                padding_left: Some(px(12.0)),
                                padding_right: Some(px(12.0)),
                                font_size: Some(px(13.0)),
                                color: Some(col(TEXT_MUTED)),
                                ..Default::default()
                            }) { "No members yet — join a server to see its roster.".to_string() }
                        }
                    }
                };
            }
            // Mock server (guests): the static grouped panel.
            let server = SERVERS.iter().find(|x| x.id == *_server_id).unwrap();
            member_panel(server, &USER.as_member(*user_status), selected_member.clone(), member_panel_width, sidebar_width, search_corpus.read_only())
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
        background: Some(col("rgba(0, 0, 0, 0.55)")),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    let profile_region = ui! {
        view(style = profile_wrap) {
            profile_header
            view(style = StyleRules { flex_grow: Some(grow(1.0)), background: Some(col(BG_MAIN)), overflow: Some(Overflow::Hidden), ..Default::default() }) {
                scroll_view(style = StyleRules { flex_grow: Some(grow(1.0)), ..Default::default() }) {
                    profile_el
                    NicknameEditField()
                    CustomStatusInput()
                    CharCounter()
                    TwoFactorToggle()
                    DeleteAccountWarning()
                    IconChangeConfirm()
                    SettingsTabBar()
                    SetupChecklistDemo()
                    EmojiUploadSlot()
                    rules_accept_checkbox()
                }
            }
        }
    };

    // User Settings screen (built once, same shape as the Profile overlay
    // above): collapses to zero size when `notif_mode` is off, fills the
    // window when on, with a "Done" pill to close it. It is the tabbed
    // `UserSettings` suite (Notifications / Account / Appearance) — the gear's
    // target. The notification level it edits is the shared `notif_level`
    // signal (also read by the gateway effect), passed as a `Signal` prop.
    let notif_close = notif_mode.clone();
    let notif_header = ui! {
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
            }) { "User Settings".to_string() }
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
                    notif_close.set(false);
                }
                TouchResponse::CONSUMED
            })
        }
    };
    let notif_open_chk = notif_mode.clone();
    let notif_wrap = move || StyleRules {
        position: Some(Position::Absolute),
        left: Some(px(0.0)),
        top: Some(px(0.0)),
        width: Some(if notif_open_chk.get() { pct(100.0) } else { px(0.0) }),
        height: Some(if notif_open_chk.get() { pct(100.0) } else { px(0.0) }),
        background: Some(col("rgba(0, 0, 0, 0.55)")),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    // The `UserSettings` suite is a `#[component]`, so it's called by tag
    // INSIDE `ui!` (`UserSettings(notif_mode = …)`) — the macro generates the
    // `BuildElement` glue. It receives the shared `notif_level` signal (the
    // Notifications tab writes it; the gateway effect reads it). `Signal` is
    // `Copy`, so `notif_level.clone()` hands the same underlying signal.
    let notif_region = ui! {
        view(style = notif_wrap) {
            notif_header
            view(style = StyleRules { flex_grow: Some(grow(1.0)), background: Some(col(BG_MAIN)), overflow: Some(Overflow::Hidden), ..Default::default() }) {
                scroll_view(style = StyleRules { flex_grow: Some(grow(1.0)), ..Default::default() }) { UserSettings(notif_mode = notif_level.clone()) }
            }
        }
    };

    // Keyboard Shortcuts overlay (same shape as the two above): built once,
    // collapsed to zero size while `shortcuts_mode` is off, fills the window
    // when on. The panel's own ✕ flips `shortcuts_mode` back off (it receives
    // a clone of the signal as its `open` flag); the user panel's '?' icon
    // opens it.
    let shortcuts_el = keyboard_shortcuts_overlay(shortcuts_mode.clone());
    let shortcuts_close = shortcuts_mode.clone();
    let shortcuts_header = ui! {
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
            }) { "Keyboard Shortcuts".to_string() }
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
                    shortcuts_close.set(false);
                }
                TouchResponse::CONSUMED
            })
        }
    };
    let shortcuts_open_chk = shortcuts_mode.clone();
    let shortcuts_wrap = move || StyleRules {
        position: Some(Position::Absolute),
        left: Some(px(0.0)),
        top: Some(px(0.0)),
        width: Some(if shortcuts_open_chk.get() { pct(100.0) } else { px(0.0) }),
        height: Some(if shortcuts_open_chk.get() { pct(100.0) } else { px(0.0) }),
        background: Some(col("rgba(0, 0, 0, 0.55)")),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    let shortcuts_region = ui! {
        view(style = shortcuts_wrap) {
            shortcuts_header
            view(style = StyleRules { flex_grow: Some(grow(1.0)), background: Some(col(BG_MAIN)), overflow: Some(Overflow::Hidden), ..Default::default() }) {
                scroll_view(style = StyleRules { flex_grow: Some(grow(1.0)), ..Default::default() }) { shortcuts_el }
            }
        }
    };
    // Direct Messages screen (mock): same full-screen, paint-last idiom as the
    // Profile / Notification overlays above. `dm_screen` builds the list column
    // + the active conversation; its three inputs are the `dm_*` signals owned
    // in `app_shell` scope, so the open conversation and this session's sent
    // DMs survive a close/re-open. A "Done" pill in the header flips
    // `dm_mode` back off to close it.
    let dm_el = dm_screen(dm_active, dm_sent, dm_revision, dm_unread, dm_unread_rev);
    let dm_close = dm_mode.clone();
    let dm_header = ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::SpaceBetween),
            padding_top: Some(px(16.0)),
            padding_bottom: Some(px(16.0)),
            padding_left: Some(px(16.0)),
            padding_right: Some(px(16.0)),
            border_bottom_width: Some(border_width(1.0)),
            border_bottom_color: Some(col(BORDER)),
            background: Some(col(BG_MAIN)),
            ..Default::default()
        }) {
            text(style = StyleRules {
                font_size: Some(px(18.0)),
                font_weight: Some(FontWeight::Bold),
                color: Some(col(TEXT)),
                ..Default::default()
            }) { "Direct Messages".to_string() }
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
                    dm_close.set(false);
                }
                TouchResponse::CONSUMED
            })
        }
    };
    let dm_open_chk = dm_mode.clone();
    let dm_wrap = move || StyleRules {
        position: Some(Position::Absolute),
        left: Some(px(0.0)),
        top: Some(px(0.0)),
        width: Some(if dm_open_chk.get() { pct(100.0) } else { px(0.0) }),
        height: Some(if dm_open_chk.get() { pct(100.0) } else { px(0.0) }),
        background: Some(col("rgba(0, 0, 0, 0.55)")),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    let dm_region = ui! {
        view(style = dm_wrap) {
            dm_header
            view(style = StyleRules { flex_grow: Some(grow(1.0)), background: Some(col(BG_MAIN)), overflow: Some(Overflow::Hidden), ..Default::default() }) { dm_el }
        }
    };

    // Live (backend) Direct Messages screen: same full-screen, paint-last idiom
    // as the mock `dm_region` above, but a separate, independent entry point.
    // Its three data inputs (`live_dms` / `live_dm_active` / `live_dm_messages`
    // + `live_dm_revision`) are owned in `app_shell` scope, so the open
    // conversation and its fetched history survive a close/re-open. A "Done"
    // pill in the header flips `live_dm_mode` back off to close it.
    // Friends panel (live): the factory the live DM screen's main area calls
    // while `friends_open` is on (`Element` is not `Clone`, hence a factory —
    // all state is signals owned in app_shell scope, so a rebuild is cheap and
    // state-stable). Reads the app_shell live-presence map for the Online tab.
    let friends_panel = Rc::new(
        move || crate::friends_view::friends_screen(
            friends_snap.clone(),
            friends_tab.clone(),
            friends_add_username.clone(),
            friends_add_error.clone(),
            live_presence.clone(),
            live_presence_rev.clone(),
            on_dm_message.clone(),
        ),
    );
    let live_dm_el = crate::live_dm_view::live_dm_screen(
        live_dms,
        live_dm_active,
        live_dm_messages,
        live_dm_revision,
        live_dm_new_id,
        gateway.incoming(),
        friends_entry,
        friends_open,
        friends_panel,
    );
    let live_dm_close = live_dm_mode.clone();
    let live_dm_header = ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::SpaceBetween),
            padding_top: Some(px(16.0)),
            padding_bottom: Some(px(16.0)),
            padding_left: Some(px(16.0)),
            padding_right: Some(px(16.0)),
            border_bottom_width: Some(border_width(1.0)),
            border_bottom_color: Some(col(BORDER)),
            background: Some(col(BG_MAIN)),
            ..Default::default()
        }) {
            text(style = StyleRules {
                font_size: Some(px(18.0)),
                font_weight: Some(FontWeight::Bold),
                color: Some(col(TEXT)),
                ..Default::default()
            }) { "Direct Messages (Live)".to_string() }
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
                    live_dm_close.set(false);
                }
                TouchResponse::CONSUMED
            })
        }
    };
    let live_dm_open_chk = live_dm_mode.clone();
    let live_dm_wrap = move || StyleRules {
        position: Some(Position::Absolute),
        left: Some(px(0.0)),
        top: Some(px(0.0)),
        width: Some(if live_dm_open_chk.get() { pct(100.0) } else { px(0.0) }),
        height: Some(if live_dm_open_chk.get() { pct(100.0) } else { px(0.0) }),
        background: Some(col("rgba(0, 0, 0, 0.55)")),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    let live_dm_region = ui! {
        view(style = live_dm_wrap) {
            live_dm_header
            view(style = StyleRules { flex_grow: Some(grow(1.0)), background: Some(col(BG_MAIN)), overflow: Some(Overflow::Hidden), ..Default::default() }) { live_dm_el }
        }
    };


    // Server Moderation panel (mock): the same full-screen overlay idiom as the
    // Profile Settings overlay above — built once, shown/hidden by
    // `moderation_mode`, closed by the "Done" pill in its header. The content
    // stacks the eight moderation components, each with a section label; every
    // row is hardcoded mock data, no backend call.
    let moderation_close = moderation_mode.clone();
    let section_label_style = section_label();
    let moderation_header = ui! {
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
            }) { "Server Moderation".to_string() }
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
                    moderation_close.set(false);
                }
                TouchResponse::CONSUMED
            })
        }
    };
    let moderation_open_chk = moderation_mode.clone();
    let moderation_wrap = move || StyleRules {
        position: Some(Position::Absolute),
        left: Some(px(0.0)),
        top: Some(px(0.0)),
        width: Some(if moderation_open_chk.get() { pct(100.0) } else { px(0.0) }),
        height: Some(if moderation_open_chk.get() { pct(100.0) } else { px(0.0) }),
        background: Some(col("rgba(0, 0, 0, 0.55)")),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    // "Invite People" panel (mock, local only): generate a code via
    // `invite::generate_invite_code`, build its link via `invite::invite_link`,
    // render both through `InviteLinkDisplay`, with the `InviteExpirySelector`
    // alongside. Sits at the top of the Server Moderation panel. No backend.
    let invite_code = crate::invite::generate_invite_code(active_server.get());
    let invite_url = crate::invite::invite_link(&invite_code);
    let invite_expiry = signal(0usize);
    // The picker's chosen duration (seconds) — local to this graveyard demo.
    let timeout_duration = signal(60i64);
    let invite_panel = ui! {
        text(style = section_label_style.clone()) { "Invite People".to_string() }
        // `InviteLinkDisplay` formats `discord.gg/{code}` internally, so it
        // takes the raw code; the full shareable URL from `invite_link` is
        // shown on its own line below.
        InviteLinkDisplay(code = invite_code.clone())
        text(style = StyleRules {
            font_size: Some(px(13.0)),
            color: Some(col(TEXT_MUTED)),
            padding_left: Some(px(16.0)),
            padding_right: Some(px(16.0)),
            ..Default::default()
        }) { invite_url }
        InviteExpirySelector(value = invite_expiry)
    };
    let moderation_el = ui! {
        invite_panel
        text(style = section_label_style.clone()) { "Banned members".to_string() }
        BanListRow()
        BanListRow()
        BanReasonInput()
        text(style = section_label_style.clone()) { "Timeout duration".to_string() }
        TimeoutDurationPicker(duration = timeout_duration)
        text(style = section_label_style.clone()) { "Member roles".to_string() }
        MemberRoleList()
        MemberRoleList()
        text(style = section_label_style.clone()) { "Blocked users".to_string() }
        BlockedUserRow()
        BlockedUserRow()
        text(style = section_label_style.clone()) { "Kick a member".to_string() }
        KickConfirmDialog()
        text(style = section_label_style.clone()) { "Delete a message".to_string() }
        DeleteMessageConfirm()
        text(style = section_label_style.clone()) { "Transfer ownership".to_string() }
        TransferOwnershipConfirm()
    };
    let moderation_region = ui! {
        view(style = moderation_wrap) {
            moderation_header
            view(style = StyleRules { flex_grow: Some(grow(1.0)), background: Some(col(BG_MAIN)), overflow: Some(Overflow::Hidden), ..Default::default() }) {
                scroll_view(style = StyleRules { flex_grow: Some(grow(1.0)), ..Default::default() }) { moderation_el }
            }
        }
    };

    // Member profile overlay: built once, shown/hidden by `selected_member`.
    // Full-screen and painted last so it sits above the member list; collapses
    // to zero size while no member is selected (the same toggle idiom as the
    // other overlay regions above).
    let member_profile_el = member_profile_panel(selected_member.clone());

    // --- Live "Invite People" overlay (real backend) ----------------------
    // The live channel header's `USER_PLUS` action opens this. Unlike the mock
    // invite panel above (which only generates a deterministic code), this
    // mints a REAL invite via `invite::create_invite` against `active_live_server`
    // (the only context with a real backend `server_id`) and shows the resulting
    // code in `InviteLinkDisplay`, whose Copy button now writes the link to the
    // system clipboard. `live_invite_expiry` is the expiry the user picked
    // (index 0/1/2 = 30 min / 1 day / never, per `InviteExpirySelector`); the
    // Create tap converts it to an `expires_at` epoch-seconds `Option` for the
    // backend. The content `switch`es on `live_invite_code` so the view flips
    // from the create form to the link display the moment a code is minted.
    // Same full-screen, paint-last idiom as `moderation_region`; inert outside
    // live mode (no entry point + `active_live_server` is None on a mock server).
    let live_invite_close = live_invite_mode.clone();
    let live_invite_header = ui! {
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
            }) { "Invite People".to_string() }
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
                    live_invite_close.set(false);
                }
                TouchResponse::CONSUMED
            })
        }
    };
    let live_invite_open_chk = live_invite_mode.clone();
    let live_invite_wrap = move || StyleRules {
        position: Some(Position::Absolute),
        left: Some(px(0.0)),
        top: Some(px(0.0)),
        width: Some(if live_invite_open_chk.get() { pct(100.0) } else { px(0.0) }),
        height: Some(if live_invite_open_chk.get() { pct(100.0) } else { px(0.0) }),
        background: Some(col("rgba(0, 0, 0, 0.55)")),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    // Local expiry picker for this flow (separate from the mock panel's
    // `invite_expiry` signal); the Create tap reads it at submit time.
    let live_invite_expiry = signal(0usize);
    // The "Create" action: read the live server id + chosen expiry, compute the
    // `expires_at` epoch-seconds (`None` = never expires), then spawn the RPC
    // and write the resulting code back to `live_invite_code` (which flips the
    // content view). On rejection, the backend message lands in
    // `active_live_server.get()` is `Option<i64>`; `None` (no live server —
    // shouldn't happen, the entry point only exists in live mode) reports the
    // rejection rather than panicking. The closure captures only `Copy` signal
    // handles (the spawned async block clones the ones it needs to outlive the
    // `Fn`), so the closure itself is `Copy` and can be passed into `ui!` across
    // `switch` re-renders (the same idiom as `on_submit_click`).
    let live_invite_create = move || {
        let sid = match active_live_server.get() {
            Some(s) => s,
            None => {
                live_invite_error.set("No live server selected.".to_string());
                return;
            }
        };
        let exp = live_invite_expiry.get();
        let code = live_invite_code.clone();
        let err = live_invite_error.clone();
        runtime_core::driver::spawn_async(async move {
            let expires_at: Option<i64> = match exp {
                0 => Some(runtime_shared::time::epoch_millis() / 1000 + 30 * 60),
                1 => Some(runtime_shared::time::epoch_millis() / 1000 + 24 * 60 * 60),
                _ => None,
            };
            match crate::invite::create_invite(sid, expires_at, None).await {
                Ok(inv) => code.set(Some(inv.code)),
                Err(msg) => err.set(msg),
            }
        });
    };
    // The content `switch`es on `(code, error)`: with a code it shows the link;
    // otherwise it shows the create form, plus the backend rejection (if the
    // last Create failed) under it. Keying on both means the view also rebuilds
    // when an error lands (a failed submit sets `error` while `code` stays None).
    let live_invite_content = switch(
        move || (live_invite_code.get(), live_invite_error.get()),
        move |state: &(Option<String>, String)| {
            let (code, err) = (&state.0, state.1.clone());
            if let Some(c) = code {
                let code = c.clone();
                ui! {
                    text(style = section_label_style.clone()) { "Your invite link".to_string() }
                    InviteLinkDisplay(code = code)
                }
            } else {
                // The rejection, shown only while non-empty (a successful
                // Create clears it by setting `code`, which rebuilds the view).
                let err_style = StyleRules {
                    margin_top: Some(px(8.0)),
                    margin_left: Some(px(16.0)),
                    margin_right: Some(px(16.0)),
                    font_size: Some(px(13.0)),
                    color: Some(col(TEXT_MUTED)),
                    ..Default::default()
                };
                let err_el = if err.is_empty() {
                    None
                } else {
                    Some(ui! { text(style = err_style) { err } })
                };
                ui! {
                    InviteExpirySelector(value = live_invite_expiry)
                    if let Some(el) = err_el {
                        el
                    }
                    button(
                        label = "Create invite".to_string(),
                        on_click = live_invite_create,
                        style = StyleRules {
                            height: Some(px(40.0)),
                            margin_top: Some(px(16.0)),
                            margin_left: Some(px(16.0)),
                            margin_right: Some(px(16.0)),
                            background: Some(col(BUTTON)),
                            color: Some(col(TEXT)),
                            font_size: Some(px(15.0)),
                            font_weight: Some(FontWeight::SemiBold),
                            border_top_left_radius: Some(px(8.0)),
                            border_top_right_radius: Some(px(8.0)),
                            border_bottom_left_radius: Some(px(8.0)),
                            border_bottom_right_radius: Some(px(8.0)),
                            align_items: Some(AlignItems::Center),
                            justify_content: Some(JustifyContent::Center),
                            ..Default::default()
                        },
                    )
                }
            }
        },
    );
    let live_invite_region = ui! {
        view(style = live_invite_wrap) {
            live_invite_header
            view(style = StyleRules { flex_grow: Some(grow(1.0)), background: Some(col(BG_MAIN)), overflow: Some(Overflow::Hidden), ..Default::default() }) {
                scroll_view(style = StyleRules { flex_grow: Some(grow(1.0)), ..Default::default() }) { live_invite_content }
            }
        }
    };

    // --- Bug reports overlay -----------------------------------------------
    // The `ThreadListRow` "Bug reports" row in the channel sidebar flips
    // `bug_reports_mode`; this full-screen overlay lists a handful of
    // hardcoded placeholder reports (mock-only, no backend call). Same
    // paint-last / collapse-to-zero idiom as `moderation_region`.
    let bug_reports_close = bug_reports_mode.clone();
    let bug_reports_header = ui! {
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
            }) { "Bug reports".to_string() }
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
                    bug_reports_close.set(false);
                }
                TouchResponse::CONSUMED
            })
        }
    };
    // (title, reporter, status) — static placeholder reports.
    let bug_report_rows: Vec<(&str, &str, &str)> = vec![
        ("Messages fail to load on slow connections", "nova", "Open"),
        ("Voice channel drops after ~5 minutes", "kito", "Open"),
        ("Emoji picker does not close on outside tap", "mira", "In review"),
        ("Sidebar channel names truncate too early", "juno", "In review"),
        ("DM list loses scroll position on refresh", "sami", "Resolved"),
    ];
    let bug_reports_el = ui! {
        for (title, reporter, status) in &bug_report_rows {
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Column),
                margin_left: Some(px(16.0)),
                margin_right: Some(px(16.0)),
                margin_top: Some(px(8.0)),
                padding_top: Some(px(10.0)),
                padding_bottom: Some(px(10.0)),
                padding_left: Some(px(12.0)),
                padding_right: Some(px(12.0)),
                background: Some(col(BG_SIDEBAR)),
                border_top_left_radius: Some(px(6.0)),
                border_top_right_radius: Some(px(6.0)),
                border_bottom_left_radius: Some(px(6.0)),
                border_bottom_right_radius: Some(px(6.0)),
                ..Default::default()
            }) {
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { (*title).to_string() }
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Row),
                    justify_content: Some(JustifyContent::SpaceBetween),
                    margin_top: Some(px(4.0)),
                    ..Default::default()
                }) {
                    text(style = StyleRules {
                        font_size: Some(px(12.0)),
                        color: Some(col(TEXT_MUTED)),
                        ..Default::default()
                    }) { (format!("reported by {}", reporter)).to_string() }
                    text(style = StyleRules {
                        font_size: Some(px(12.0)),
                        color: Some(col(TEXT_MUTED)),
                        ..Default::default()
                    }) { (*status).to_string() }
                }
            }
        }
    };
    let bug_reports_open_chk = bug_reports_mode.clone();
    let bug_reports_wrap = move || StyleRules {
        position: Some(Position::Absolute),
        left: Some(px(0.0)),
        top: Some(px(0.0)),
        width: Some(if bug_reports_open_chk.get() { pct(100.0) } else { px(0.0) }),
        height: Some(if bug_reports_open_chk.get() { pct(100.0) } else { px(0.0) }),
        background: Some(col("rgba(0, 0, 0, 0.55)")),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    let bug_reports_region = ui! {
        view(style = bug_reports_wrap) {
            bug_reports_header
            view(style = StyleRules { flex_grow: Some(grow(1.0)), background: Some(col(BG_MAIN)), overflow: Some(Overflow::Hidden), ..Default::default() }) {
                scroll_view(style = StyleRules { flex_grow: Some(grow(1.0)), ..Default::default() }) { bug_reports_el }
            }
        }
    };

    // --- "jump to bottom" scroll tracking ---------------------------------
    // The floating "Jump to bottom" pill only appears while the reader has
    // scrolled UP away from the newest messages. The `scroll_view`'s `on_scroll`
    // callback reports the raw offset, so the two heights needed to test
    // "how far from the bottom?" are read live from bound `ViewHandle`s each
    // time (the same idiom as the website shell's scroll-spy: `absolute_frame()`
    // is a Backend-trait primitive, uniform across backends, `None` until the
    // first layout pass). `show` is a dumb `Signal<bool>` the component renders
    // against — no derived-height signals to keep in sync.
    let msg_show_jump: Signal<bool> = signal(false);
    let msg_scroll_ref: Ref<ScrollViewHandle> = Ref::<ScrollViewHandle>::new();
    let msg_viewport_ref: Ref<ViewHandle> = Ref::<ViewHandle>::new();
    let msg_content_ref: Ref<ViewHandle> = Ref::<ViewHandle>::new();
    // Recompute "away from bottom?" from the live measured sizes. `on_scroll`
    // only delivers the offset, so read the content column and the scroll
    // viewport boxes here; both are read on every scroll, which also refreshes
    // the state after a channel switch (the `when` content rebuild resets the
    // scroll offset, firing a fresh `on_scroll`). The closure is wrapped in `Rc`
    // so it can be shared by the `on_scroll` handler and the post-layout nudge
    // without being moved into both.
    let msg_recompute_jump: Rc<dyn Fn(f32)> = Rc::new({
        let show = msg_show_jump;
        let viewport_ref = msg_viewport_ref;
        let content_ref = msg_content_ref;
        move |scroll_y: f32| {
            let viewport_h = viewport_ref
                .with(|v| v.absolute_frame().map(|r| r.height))
                .flatten()
                .unwrap_or(0.0);
            let content_h = content_ref
                .with(|v| v.absolute_frame().map(|r| r.height))
                .flatten()
                .unwrap_or(0.0);
            // "At the bottom" ⇔ the reader's viewport bottom reaches (or
            // passes) the content bottom. Away ⇔ there's a visible gap of more
            // than ~48px of unread content below. Guard `content_h <= viewport_h`
            // (no scroll possible ⇒ always "at the bottom" ⇒ hide).
            let away = viewport_h > 0.0
                && content_h > viewport_h
                && scroll_y + viewport_h < content_h - 48.0;
            show.set(away);
        }
    });
    // First read happens before the column has laid out (`absolute_frame()` is
    // `None` then), so it reads "at the bottom" (content_h 0) — the correct
    // pill-hidden state. Nudge once after layout so the measured sizes settle
    // even if the reader's initial offset is non-zero (e.g. a channel that
    // restores a scroll position).
    after_animation_frame({
        let r = msg_recompute_jump.clone();
        move || r(0.0)
    });
    // The floating pill. `msg_show_jump` is driven by the scroll tracking above
    // (only `true` while the reader is away from the newest messages). `on_jump`
    // scrolls the message list back to the bottom — `ScrollViewHandle` has no
    // `scroll_to_bottom`, but the web backend's `scroll_to` clamps `scrollTop` to
    // the scrollable range, so a large `y` lands exactly on the last message.
    let msg_scroll_handle = msg_scroll_ref.clone();
    let msg_jump_el = JumpToBottomButton(
        msg_show_jump.clone(),
        Rc::new(move || {
            msg_scroll_handle.with(|h| h.scroll_to(0.0, 1_000_000.0));
        }),
    );

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
            view(style = rail_style) { server_rows create_server_section live_section rail_actions gateway_status gateway_incoming_effect mention_reset_effect }
            sidebar
            // Drag handle on the channel sidebar's right edge: dragging it
            // left/right resizes `sidebar_width` (right = grow, so sign `+1`).
            resize_handle(sidebar_width, 1.0, "resize-sidebar")
            view(style = main_style) {
                header
                // `Position::Relative` wrapper around the scroll surface: it
                // tightly wraps the `scroll_view` (which fills it via
                // `flex_grow`), so its measured box is the visible scroll
                // viewport — the "bottom" the pill measures against. The floating
                // pill is anchored here (a sibling of the scroll surface, so it
                // does NOT scroll with content).
                view(style = StyleRules {
                    position: Some(Position::Relative),
                    flex_grow: Some(grow(1.0)),
                    overflow: Some(Overflow::Hidden),
                    ..Default::default()
                }) {
                    // Content column: a plain (non-`flex_grow`) column that sizes
                    // to its content, so its measured height is the total
                    // scrollable height. The `sent_messages` switch is the only
                    // real child (the graveyard WelcomeBanner is gone).
                    scroll_view(style = StyleRules { flex_grow: Some(grow(1.0)), ..Default::default() }) {
                        view(style = StyleRules {
                            flex_direction: Some(FlexDirection::Column),
                            ..Default::default()
                        }) {
                            sent_messages
                        }.bind(msg_content_ref)
                    }
                        .bind(msg_scroll_ref)
                        .on_scroll({
                            let recompute = msg_recompute_jump.clone();
                            move |_x, y| recompute(y)
                        })
                    // Floating "Jump to bottom" pill (anchored to this wrapper);
                    // shown only while the reader is away from the newest
                    // messages.
                    msg_jump_el
                }.bind(msg_viewport_ref)
                typing_row
                // Positioning ancestor for the @-mention dropdown and the
                // emoji popover (both anchor above the input via
                // `Position::Absolute`). A Column so the in-flow
                // staged-attachment chip stacks directly above the composer row.
                view(style = StyleRules {
                    position: Some(Position::Relative),
                    flex_direction: Some(FlexDirection::Column),
                    ..Default::default()
                }) {
                    // "Failed to send - Retry" banner: UI chrome directly above
                    // the composer (NOT in the message list). Reads the
                    // `send_failed` signal — set when the backend answers a
                    // send with an `Error` frame — and clears it on tap (the
                    // user resends to retry). While unset it renders nothing.
                    SendFailedIndicator(failed = send_failed.clone())
                    // "Replying to <author>: <snippet>" strip: shows while a
                    // message's Reply action is active (the per-row action bar
                    // sets `reply_ctx`), directly above the composer. Its "×"
                    // clears the same signal the bar reads. Zero-size (nothing
                    // rendered) while no reply is active.
                    reply_preview(reply_ctx, reply_target)
                    attach_chip
                    emoji_popover
                    ac_el
                    // Per-channel slowmode indicator (gated): the channel-keyed
                    // `slowmode_el` above resolves the active channel's
                    // `slowmode_secs` and renders the pill only for
                    // slowmode-enabled channels — nothing for the rest. Same
                    // home b60 dropped the bare, unconditional
                    // `SlowmodeIndicator()` into; now it reads real state.
                    slowmode_el
                    // Slash-command autocomplete: shown only while the composer
                    // text actually starts with "/" (a real slash command is
                    // being typed), not on every render. `composer` is the
                    // live text signal.
                    if composer.get().starts_with('/') {
                        SlashCommandDropdown()
                    }
                    view(style = composer_style) {
                        view(style = icon_button_style.clone()) {
                            icon(PLUS).color(|| Color::from(TEXT_MUTED)).size(16.0)
                        }
                        // `+` = open the file picker and stage a real image
                        // attachment (up to the shared per-message cap). The
                        // strip above the composer mirrors the staged list.
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                do_attach();
                            }
                            TouchResponse::CONSUMED
                        })
                        text_input(value = composer, on_change = update_on_type, placeholder_reactive = composer_placeholder.read_only(), style = input_style, test_id = "composer").on_key_down(composer_key)
                        view(style = icon_button_style) {
                            icon(SMILE).color(|| Color::from(TEXT_MUTED)).size(16.0)
                        }
                        // Smile = open/close the emoji popover (a `Copy` handle
                        // on `emoji_open`, mirroring the attachment button).
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                emoji_open.set(!emoji_open.get());
                            }
                            TouchResponse::CONSUMED
                        })
                    }
                    // Visible rejection line for the attach flow: a bad type
                    // / oversize / over-count pick (or a picker error) shows
                    // here in the app's error red; empty = nothing rendered.
                    if !attach_error.get().is_empty() {
                        text(style = StyleRules {
                            font_size: Some(px(12.0)),
                            color: Some(col(BADGE)),
                            margin_left: Some(px(16.0)),
                            ..Default::default()
                        }) { move || attach_error.get().clone() }
                    }
                }
            }
            // Drag handle on the member panel's left edge: dragging it left
            // resizes `member_panel_width` (left = grow, so sign `-1`).
            resize_handle(member_panel_width, -1.0, "resize-members")
            members_switch
            // Member profile overlay: full-screen, painted after the member
            // list so it covers the whole window when a row is tapped
            // (collapses to zero size while no member is selected).
            member_profile_el
            // Painted last so it sits on top of everything when open.
            profile_region
            notif_region
            shortcuts_region
            // Direct Messages (mock): full-screen, same paint-last idiom. Sits
            // above the server/channel view but below the modal overlays
            // (join / create / discover), so a modal opened from elsewhere still
            // wins on top.
            dm_region
            // Direct Messages (LIVE / backend): full-screen, same paint-last
            // idiom as the mock `dm_region` above; an independent entry point
            // so the mock DM screen is left untouched.
            live_dm_region
            // "Join a Server" overlay: full-screen, painted last so it sits on
            // top when open (collapses to zero size when closed).
            join_el
            // "Create a Server" overlay: same full-screen, paint-last idiom.
            create_el
            // "Discover Servers" overlay: same full-screen, paint-last idiom.
            discover_el
            // Live channel management overlays (create / rename / delete): same
            // full-screen, paint-last idiom. Each collapses to zero size when
            // closed; `server_id` is read from `active_live_server` at submit
            // time, so they're inert outside live mode.
            cc_el
            rc_el
            dc_el
            moderation_region
            ss_el
            invite_el
            // Reaction picker (mock, session-local): full-window emoji grid,
            // painted last so it covers everything while open (collapses to
            // zero size while `picker` is `None`).
            reaction_picker_overlay(picker, reactions_overlay, reaction_rev)
            // "Message pinned!" toast: floats top-right (a `Position::Absolute`
            // child of the root `Relative` ancestor) while `pin_visible` is
            // true. Set by a message's "Pin message" overflow entry; auto-
            // dismisses after a beat, or manually via its "x". Collapses to
            // nothing while hidden.
            crate::pin_confirm_toast::pin_confirm_toast(pin_visible)
            // "Invite People" (LIVE / backend): full-screen, same paint-last
            // idiom as `moderation_region`. Reached only from the live channel
            // header's invite action; inert outside live mode (no entry point +
            // `active_live_server` is None on a mock server).
            live_invite_region
            // "Pinned messages" panel (LIVE / backend): full-screen, paint-last
            // (same idiom as `moderation_region` / `live_invite_region`).
            // `pinned_panel_open` is set by the live header's pin icon; the
            // panel lists the channel's real pinned rows (each with an Unpin
            // button) and collapses to zero size while closed.
            crate::pinned_messages_panel::pinned_messages_panel(
                pinned_panel_open,
                pinned_messages,
                pinned_error,
                pinned_count,
                pinned_revision,
            )
            // "Bug reports" overlay: same full-screen, paint-last idiom as
            // `moderation_region` above.
            bug_reports_region
            // [Feature 10] New-message notification toast: floats top-right
            // (a `Position::Absolute` child of the root `Relative` ancestor),
            // exactly like the pin toast above. The gateway effect sets
            // `notif_toast_visible` (and the title / body) when a matching
            // message lands on a non-open channel; `notification_toast` reads
            // all three reactively, so its copy updates in place on each new
            // message and the banner auto-dismisses ~4.5 s later (or via its
            // "x"). Collapses to nothing while hidden.
            crate::notification_toast::notification_toast(
                notif_toast_visible,
                notif_toast_title,
                notif_toast_body,
            )
        }
    }
}
