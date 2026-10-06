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
    after_ms_detached, AlignItems, AlignSelf, Color, Cursor, Element, FlexDirection, FlexWrap,
    FontFamily, FontWeight, JustifyContent, KeyEvent, KeyOutcome, Overflow, PanEvent, PanRecognizer,
    Position, Signal, StyleRules, TextAlign, TextTransform, Tokenized, TouchEvent,
    TouchPhase, TouchResponse, memo, pan, signal, switch, ui, viewport_size,
};
use runtime_core::icon;
use icons_lucide::{
    CHEVRON_DOWN, CIRCLE_HELP, COMPASS, GAVEL, GLOBE, HASH, HEADPHONES, MAIL, MESSAGE_CIRCLE, MIC,
    MIC_OFF, PENCIL, PLUS, SETTINGS, SMILE, SMILE_PLUS, TRASH_2, VOLUME_2, USERS,
};
use crate::idle_timer_badge::idle_timer_badge;
use crate::server_settings_panel::ServerSettingsPanel;

use runtime_shared::time::local_offset_minutes;

use crate::data::{DMS, SERVERS, Category, Member, Server, Status, TextChannel, USER, member_profile};
use crate::message_search_bar::MessageSearchBar;
use crate::pin_count_badge::PinCountBadge;
use crate::ban_list_row::BanListRow;
use crate::timeout_duration_picker::TimeoutDurationPicker;
use crate::permission_override_row::PermissionOverrideRow;
use crate::member_role_list::MemberRoleList;
use crate::verified_badge::VerifiedBadge;
use crate::icon_change_confirm::IconChangeConfirm;
use crate::featured_server_banner::FeaturedServerBanner;
use crate::ban_reason_input::BanReasonInput;
use crate::attachment_preview_card::AttachmentPreviewCard;
use crate::delete_account_warning::DeleteAccountWarning;

use crate::server_boost_badge::ServerBoostBadge;
use crate::language_picker::LanguagePicker;
use crate::user_bio_block::user_bio_block;
use crate::dm_request_card::DmRequestCard;
use crate::blocked_user_row::BlockedUserRow;
use crate::server_folder_group::server_folder_group;
use crate::server_template_card::server_template_card;
use crate::template_gallery_grid::TemplateGalleryGrid;
use crate::server_tag_chips::server_tag_chips;
use crate::server_region_selector::ServerRegionSelector;
use crate::server_icon_upload_placeholder::ServerIconUploadPlaceholder;
use crate::category_header::CategoryHeader;
use crate::mic_selector::MicSelector;
use crate::friend_request_card::FriendRequestCard;
use crate::server_icon_fallback::ServerIconFallback;

use crate::welcome_banner::WelcomeBanner;
use crate::activity_card::ActivityCard;
use crate::screen_share_indicator::ScreenShareIndicator;
use crate::two_factor_toggle::TwoFactorToggle;

use crate::send_failed_indicator::SendFailedIndicator;
use crate::slowmode_indicator::SlowmodeIndicator;
use crate::latency_badge::LatencyBadge;
use crate::jump_to_bottom_button::JumpToBottomButton;
use crate::afk_timer_display::AfkTimerDisplay;
use crate::connection_quality_indicator::ConnectionQualityIndicator;
use crate::voice_join_button::VoiceJoinButton;
use crate::voice_participant_list::VoiceParticipantList;
use crate::volume_slider::VolumeSlider;
use crate::soundboard_button::SoundboardButton;
use crate::edited_label::edited_label;
use crate::notification_settings::NotificationSettings;
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
use crate::poll_widget::PollWidget;
use crate::thread_list_row::ThreadListRow;
use crate::spoiler_image_placeholder::SpoilerImagePlaceholder;
use crate::forward_message_dialog::ForwardMessageDialog;
use crate::slash_command_dropdown::SlashCommandDropdown;
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

/// One buffered live (gateway-fed) message: the server's wire `message_id`,
/// then `author_id` + `author_label`, `content`, `created_at_secs`, and
/// `edited_at` (the `MessageEdited` timestamp in seconds, `None` until edited).
/// The display fields mirror the old 4-tuple; `message_id` + `edited_at` are
/// what the per-row edit/delete affordance and the "(edited)" marker need on
/// top of them (the mock path carries no wire id, so it renders `None`).
type LiveMsg = (i64, i64, String, String, i64, Option<i64>);

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
/// preserve its previous (always-on placeholder) rendering.
pub(crate) fn message_row(
    author: &str,
    color: &str,
    body: &str,
    when: String,
    full: String,
    edited: bool,
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
            view(style = avatar_style) {
                text(style = avatar_text_style) { initial }
            }
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

/// One entry of the "more" overflow menu (a tap row). The real Discord
/// entries: "Copy text" (a harmless no-op that just closes the menu),
/// "Pin message" (shows the `PinConfirmToast`), and "Translate message"
/// (reveals that message's `TranslateToggle`). Kept deliberately minimal.
enum MoreMenuAction {
    Pin,
    Copy,
    Translate,
}

/// The small overflow menu that the action bar's "more" button opens. Rendered
/// only while `open` is true (the `switch` primitive, same show/hide idiom as
/// `MessageActionBar`); it drops just below the bar. `on_pin` shows the pin
/// toast; `on_translate` reveals the message's translate control; every entry
/// closes the menu on tap.
fn message_overflow_menu(
    open: Signal<bool>,
    on_pin: Rc<dyn Fn()>,
    on_translate: Rc<dyn Fn()>,
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
            let menu_row = |label: &'static str, action: MoreMenuAction, pin: Rc<dyn Fn()>, translate: Rc<dyn Fn()>| -> Element {
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
                // Each action owns its own handles so the `move` handler can own
                // them (branch-local `pin`/`translate` clones + a `Copy` `open`).
                let touch = move |ev: &runtime_shared::TouchEvent| {
                    if ev.phase == TouchPhase::Ended {
                        match action {
                            MoreMenuAction::Pin => pin(),
                            MoreMenuAction::Translate => translate(),
                            MoreMenuAction::Copy => {}
                        }
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
                    { menu_row("Copy text", MoreMenuAction::Copy, pin.clone(), translate.clone()) }
                    { menu_row("Pin message", MoreMenuAction::Pin, pin.clone(), translate.clone()) }
                    { menu_row("Translate message", MoreMenuAction::Translate, pin, translate) }
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
    key: MessageKey,
    picker: Signal<Option<MessageKey>>,
    reply_ctx: Signal<Option<(String, String)>>,
    pin_visible: Signal<bool>,
    translate_key: Signal<Option<MessageKey>>,
) -> Element {
    let hovered = signal(false);
    let menu_open = signal(false);
    let base = message_row(author, color, body, when, full, edited);

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
    let on_reply = Rc::new(move || {
        reply_c.set(Some((author_r.clone(), snippet_r.clone())));
    });

    // React: point the shared picker at this row's `MessageKey` — the exact
    // entry point `reaction_area`'s add button uses. No second UI.
    let picker_c = picker;
    let on_react = Rc::new(move || {
        picker_c.set(Some(key));
    });

    // More: toggle the overflow menu.
    let menu_c = menu_open;
    let on_more = Rc::new(move || {
        menu_c.set(!menu_c.get());
    });

    // "Pin message" (from the overflow menu): flash the toast, then auto-dismiss
    // after a beat. The toast's own "x" is the manual dismiss.
    let pin_c = pin_visible;
    let on_pin = Rc::new(move || {
        pin_c.set(true);
        let pin_d = pin_c;
        after_ms_detached(2_500, move || pin_d.set(false));
    });

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

    let bar = crate::message_action_bar::message_action_bar(hovered, on_react, on_reply, on_more);
    let menu = message_overflow_menu(menu_open, on_pin, on_translate);

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
/// its "×" cancel with a callback that clears the shared `reply_ctx` signal
/// (the same signal each message's Reply action sets). `reply_ctx` is `Copy`,
/// so the `on_cancel` closure captures its own handle; the bar itself reads a
/// separate copy (both are the same underlying signal).
fn reply_preview(reply_ctx: Signal<Option<(String, String)>>) -> Element {
    let clear = reply_ctx;
    let on_cancel = Rc::new(move || {
        clear.set(None);
    });
    crate::reply_preview_bar::reply_preview_bar(reply_ctx, on_cancel)
}

// --- message reactions (session-local, mock + live) -------------------------
// Per-message reaction state shared by BOTH the MOCK and the LIVE (gateway)
// message lists. Keyed by `MessageKey`: MOCK rows key by `(channel_id,
// rendered-list-index)` (seed messages first, then this session's sends — a
// stable identity across `revision` bumps, since the list order is
// deterministic), LIVE rows key by their real wire `message_id`. Each value is
// a list of `(emoji, count, reacted_by_me)` pills. Mutating the map does not
// fire reactivity, so the message-list `switch` folds `reaction_rev` into its
// key (the `revision` idiom) and re-reads the map on every rebuild. Purely
// session-local — nothing is persisted or synced (there is no backend reaction
// store); the live rows reuse the exact same pill store as the mock rows.
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

/// The 12 common glyphs the add-reaction picker offers — the same set the
/// composer's emoji popover uses (see `emoji_popover`).
const REACT_EMOJIS: [&str; 12] = [
    "😀", "😊", "😂", "😍", "😎", "🤔", "👍", "🎉", "🚀", "❤️", "🙌", "🍕",
];

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
                // Toggle: not-yet-reacted -> count+1 and highlight;
                // already-reacted -> count-1 and un-highlight, dropping the pill
                // (and the empty map entry) once the count hits 0.
                if ev.phase == TouchPhase::Ended {
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
            let cells: Vec<Element> = REACT_EMOJIS.iter().map(|g| {
                let glyph = g.to_string();
                let glyph_close = glyph.clone();
                let r = reactions.clone();
                let rev = reaction_rev;
                let close = picker;
                let key = key;
                ui! {
                    view(style = StyleRules {
                        width: Some(px(34.0)),
                        height: Some(px(34.0)),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        border_top_left_radius: Some(px(6.0)),
                        border_top_right_radius: Some(px(6.0)),
                        border_bottom_left_radius: Some(px(6.0)),
                        border_bottom_right_radius: Some(px(6.0)),
                        ..Default::default()
                    }) {
                        text(style = StyleRules {
                            font_size: Some(px(20.0)),
                            ..Default::default()
                        }) { glyph }
                    }
                    .on_touch(move |ev| {
                        // Add (or bump) this emoji's reaction for the message,
                        // then close the picker.
                        if ev.phase == TouchPhase::Ended {
                            let mut m = r.borrow_mut();
                            let inner = m.entry(key).or_default();
                            if let Some(pos) = inner.iter().position(|(e, _, _)| e == &glyph_close) {
                                let (e, c, _) = inner[pos].clone();
                                inner[pos] = (e, c + 1, true);
                            } else {
                                inner.push((glyph_close.clone(), 1, true));
                            }
                            rev.set(rev.get() + 1);
                            close.set(None);
                        }
                        TouchResponse::CONSUMED
                    })
                }
            }).collect();
            // Opaque full-window backdrop (dismiss-on-outside-tap) with the
            // wrapping emoji grid centered in a rounded elevated card.
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
                        background: Some(col(BG_SIDEBAR)),
                        padding_top: Some(px(12.0)),
                        padding_bottom: Some(px(12.0)),
                        padding_left: Some(px(12.0)),
                        padding_right: Some(px(12.0)),
                        border_top_left_radius: Some(px(10.0)),
                        border_top_right_radius: Some(px(10.0)),
                        border_bottom_left_radius: Some(px(10.0)),
                        border_bottom_right_radius: Some(px(10.0)),
                        ..Default::default()
                    }) {
                        view(style = StyleRules {
                            flex_direction: Some(FlexDirection::Row),
                            flex_wrap: Some(FlexWrap::Wrap),
                            gap: Some(px(4.0)),
                            width: Some(px(220.0)),
                            ..Default::default()
                        }) { cells }
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
/// like a mock row), plus — for the signed-in user's own messages — a compact
/// always-visible Edit / Delete affordance and the inline edit input, which
/// coexist beneath the hover bar (Discord's own-message edit/delete home; the
/// hover bar handles the shared react/reply/pin/translate actions).
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
    pin_visible: Signal<bool>,
    translate_key: Signal<Option<MessageKey>>,
) -> Element {
    // The base row now carries the hover `MessageActionBar` + overflow menu
    // exactly like the mock path (react / reply / pin / translate). The only
    // difference is the real `(edited)` marker (gated on the message's
    // `edited_at` state) and the live `MessageKey`.
    let base = message_row_with_actions(
        author,
        color,
        body,
        when,
        full,
        edited,
        key,
        picker,
        reply_ctx,
        pin_visible,
        translate_key,
    );
    // Only one's own messages are editable/deletable — the backend enforces the
    // same, but the affordance is hidden for other authors' rows.
    if !is_mine {
        return base;
    }
    let mid = message_id;
    let editing_sig = live_editing;
    let edit_text = live_edit_text;
    let confirm_sig = live_delete_confirm;
    // --- inline edit input (reuses the composer's `text_input` idiom) -------
    // `text_input` takes a reactive `Signal<String>` for `value`, so the draft
    // binds straight to `live_edit_text`. Confirmation is the explicit Save
    // button (Enter isn't wired here to avoid surprising the composer's Enter
    // handler); the input is seeded with the current content on Save-cancel
    // paths by the list rebuild.
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
            text_input(value = live_edit_text, placeholder = "Edit message", style = input_style, test_id = "edit-input")
        }
    };
    // The current message content, captured at row-build time so the Edit
    // button can seed the draft with what the row currently shows. Defined
    // before `small_btn` so the closure can clone it (a per-call clone, not a
    // move — `small_btn` is invoked once per button).
    let edit_seed = body.to_string();
    // --- per-row control buttons ------------------------------------------
    // Compact text pills matching the moderation feature's inline control set
    // (role / kick buttons). Each button carries a `MsgAction` the shared
    // `on_touch` handler matches on — the action is owned per-button so the
    // closure captures nothing borrowed (avoids the `&str`-lifetime trap of a
    // label-matching closure).
    #[derive(Clone, Copy, PartialEq)]
    enum MsgAction {
        Edit,
        Delete,
        Save,
        Cancel,
        Confirm,
        No,
    }
    let small_btn = |action: MsgAction, label: &str, bg: &'static str, test_id: &'static str| -> Element {
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
        // Per-button copy of the seed: `touch` is `move`, so it owns the value
        // it captures. Cloning here (inside `small_btn`) rather than letting
        // `touch` move the single `edit_seed` keeps `edit_seed` borrowable
        // across the multiple `small_btn` calls. Only the Edit button reads it;
        // the clone is cheap (a short message body).
        let seed = edit_seed.clone();
        let touch = move |ev: &runtime_shared::TouchEvent| {
            if ev.phase != TouchPhase::Ended {
                return TouchResponse::CONSUMED;
            }
            match action {
                MsgAction::Edit => {
                    // Enter edit mode: seed the draft with the current content
                    // and flag this message id. The list rebuild (keyed on
                    // `live_editing`) then mounts the input for this row.
                    editing_sig.set(Some(mid));
                    edit_text.set(seed.clone());
                }
                MsgAction::Delete => confirm_sig.set(Some(mid)),
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
                }
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
    };
    // The three states are mutually exclusive (edit mode and delete-confirm
    // are gated by separate signals, and the UI never sets both for one row).
    // Render the matching control set beneath the base row — in the idle
    // case this IS the Edit/Delete affordance, in the other two it is the
    // edit input or the delete-confirm UI. This one-slot replacement is the
    // moderation-feature precedent (inline control set swapped in for that
    // row only).
    let edit_mode = editing_sig.get() == Some(mid);
    let delete_mode = confirm_sig.get() == Some(mid);
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
                        { small_btn(MsgAction::Save, "Save", BUTTON, "save-edit") }
                        { small_btn(MsgAction::Cancel, "Cancel", COMPOSER, "cancel-edit") }
                        { small_btn(MsgAction::Delete, "Delete", BADGE, "row-delete") }
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
                    { small_btn(MsgAction::Confirm, "Confirm", BADGE, "confirm-delete") }
                    { small_btn(MsgAction::No, "No", COMPOSER, "cancel-delete") }
                }
            } else {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Row),
                    gap: Some(px(6.0)),
                    ..Default::default()
                }) {
                    { small_btn(MsgAction::Edit, "Edit", COMPOSER, "row-edit") }
                    { small_btn(MsgAction::Delete, "Delete", COMPOSER, "row-delete-idle") }
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
    ui! {
        view(style = row_style) {
            view(style = avatar_style) {
                text(style = initial_style) { initial }
                view(style = dot_style) {}
            }
            text(style = name_style) { m.name.to_string() }
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
                        flex_direction: Some(FlexDirection::Column),
                        overflow: Some(Overflow::Hidden),
                        padding_top: Some(px(8.0)),
                        padding_bottom: Some(px(16.0)),
                        padding_left: Some(px(16.0)),
                        padding_right: Some(px(16.0)),
                        gap: Some(px(12.0)),
                        ..Default::default()
                    }) {
                        WelcomeBanner(name = name.clone())
                        AfkTimerDisplay()
                        ActivityCard(
                            activity = prof.activity_verb.to_string(),
                            detail = prof.activity_detail.to_string(),
                        )
                        ServerBoostBadge(boost_level = prof.boost_level as i32)
                        LanguagePicker()
                        user_bio_block(prof.bio.to_string().into())
                        FriendRequestCard(sender = prof.friend_sender.to_string())
                        DmRequestCard()
                        BlockedUserRow()
                        NicknameEditField(default_nickname = prof.nickname.to_string())
                        ServerIconFallback()
                        MicSelector()
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
                ConnectionQualityIndicator()
                ScreenShareIndicator()
                LatencyBadge()
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
        // The mock voice-participant list (three fake participants with mic
        // states) sits alongside the connected-member sub-rows while the user
        // is joined to this channel. Local/mock only — no backend call.
        if joined_voice.get() == Some((server_id, v.id)) {
            out.push(VoiceParticipantList());
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
) -> Element {
    let mut body: Vec<Element> = Vec::new();

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
fn login_screen(authed: Signal<bool>) -> Element {
    // "you" is the only identity in this mock build; authing in just flips
    // the shared signal the app tree is keyed on.
    // `mode` flips the form between sign-in (false, the default) and sign-up
    // (true); both reuse the same username + password fields. `error` holds
    // the inline message for a failed submit in either mode.
    let username: Signal<String> = signal(String::new());
    let password: Signal<String> = signal(String::new());
    let error: Signal<String> = signal(String::new());
    let mode: Signal<bool> = signal(false);

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
        move || {
            let em = username.get();
            let pw = password.get();
            if em.trim().is_empty() || pw.trim().is_empty() {
                error.set("Username and password are required.".to_string());
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
    // The shared sign-up action: same validation, but calls `auth::signup`,
    // which creates the account and logs it straight in (so the session
    // token is set too), then flips `authed`. A duplicate username surfaces
    // inline the same way a failed login does.
    let do_signup = {
        let authed = authed.clone();
        move || {
            let em = username.get();
            let pw = password.get();
            if em.trim().is_empty() || pw.trim().is_empty() {
                error.set("Username and password are required.".to_string());
                return;
            }
            error.set(String::new());
            runtime_core::driver::spawn_async(async move {
                match crate::auth::signup(&em, &pw).await {
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
        // Keyed on `(error, mode)` so the form rebuilds both when a failed
        // submit sets `error` and when the mode toggles between sign-in and
        // sign-up (the same `switch(key, render)` pattern the sidebar/message
        // list use — `ui!` doesn't re-read signal state on its own, only on
        // rebuild).
        move || (error.get(), mode.get()),
        move |state: &(String, bool)| {
            let err = state.0.clone();
            let is_signup = state.1;
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
            // (`on_submit_click`) routes on `mode` internally.
            let submit_label = if is_signup {
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
            for (_mid, _aid, author, _body, _ts, _edited) in msgs {
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

/// The live-server member panel: the real membership of the active backend
/// server (fetched from the `#[server]` `list_members` endpoint, owner-first)
/// rendered as a right-side column — the live counterpart of the mock
/// `member_panel`. Each row shows the member's name, their role (a tappable
/// cycle-button member→moderator→admin→owner→member), and a `Kick` button that
/// opens an inline two-step confirm before calling the backend `remove_member`.
/// After a successful kick or role change the roster is re-fetched, so the
/// list refreshes (a kicked row disappears; a changed role re-renders).
///
/// The backend enforces every permission check (who may kick, who may change
/// roles, that the owner can't be kicked or re-roled, that `owner` can't be
/// assigned to someone else); this panel does **no** client-side permission
/// pre-computation — on a rejection the backend's message is shown inline as a
/// simple red line beneath the row, and on success the roster reloads.
///
/// Idioms, verified against this file: a plain `fn -> Element` (like
/// `member_panel`); the row list is a local `Vec<Element>` built in a `for`
/// loop (like the mock `member_panel`'s `body`) with a `.collect()` — the
/// standard static-row idiom; the per-row controls are `view {…}.on_touch(move
/// |ev| …)` chains (the live channel rows' idiom); `Signal` is `Copy`, so each
/// `move` closure captures a copy and stays `Fn`; the inline kick-confirm and
/// the per-row error line are reactive `if` branches inside `ui!` (lowered to
/// the `when` primitive, `ban_list_row`'s idiom).
fn live_member_panel(
    server_id: i64,
    members: Signal<Vec<crate::moderation::MemberInfo>>,
    error: Signal<String>,
    kick_confirm: Signal<Option<i64>>,
) -> Element {
    // Snapshot of the current roster. The panel is rebuilt (by the caller's
    // `switch` keyed on this signal) whenever the roster changes — including
    // the re-fetch the mutation callbacks below trigger on success — so a
    // local snapshot is always current here.
    let roster = members.get();
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
    let kick_btn_style = StyleRules {
        padding_top: Some(px(3.0)),
        padding_bottom: Some(px(3.0)),
        padding_left: Some(px(8.0)),
        padding_right: Some(px(8.0)),
        background: Some(col(BADGE)),
        color: Some(col("#ffffff")),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        flex_shrink: Some(grow(0.0)),
        ..Default::default()
    };
    let kick_label_style = StyleRules {
        font_size: Some(px(11.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col("#ffffff")),
        ..Default::default()
    };
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
    let mut rows: Vec<Element> = Vec::new();
    for member in roster.iter() {
        let member_id = member.user_id;
        let is_self = member_id == self_id;
        let role = member.role.clone();
        let confirming = kick_confirm.clone();
        let kick = kick_confirm.clone();
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
        let kick_btn_style = kick_btn_style.clone();
        let kick_label_style = kick_label_style.clone();
        let error_style = error_style.clone();
        // name_style is used in two mutually-exclusive branches below, but
        // the ui! macro's own parser doesn't support a nested `let` inside a
        // branch (tried it -- breaks its DSL parsing), so each branch gets
        // its own pre-made clone here instead, outside the macro entirely.
        let name_style_self_branch = name_style.clone();
        let name_style_other_branch = name_style;

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
                // Name (+ a "(you)" tag on the signed-in user's own row).
                view(style = StyleRules {
                    flex_shrink: Some(grow(1.0)),
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
                if confirming.get() == Some(member_id) {
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
                } else if !is_self {
                    view(style = kick_btn_style.clone()) {
                        text(style = kick_label_style.clone()) { "Kick" }
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            kick.set(Some(member_id));
                        }
                        TouchResponse::CONSUMED
                    })
                }
            }

            // Inline backend error for this row (the most recent one). Shown
            // only when a kick/role-change was rejected — the backend's own
            // message, no client-side permission logic.
            if !error.get().is_empty() {
                text(style = error_style.clone()) { move || error.get().clone() }
            }
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
    // Mock attachment chip: `Some(name)` = a pending fake attachment is staged
    // in the composer strip above the input (e.g. "screenshot.png"). `None` =
    // none. No real file picker/upload — just local state the '+' toggles and
    // the 'x' removes; Send clears it alongside the composer text.
    let pending_attachment: Signal<Option<String>> = signal(None);
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

    // --- live/real message state -----------------------------------------
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
    let live_member_error: Signal<String> = signal(String::new());

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
    let gateway_incoming_effect = switch(
        move || gateway.incoming().get(),
        move |frame: &Option<discord_server::server_settings::WsServerMsg>| {
            if let Some(discord_server::server_settings::WsServerMsg::Message {
                message_id,
                channel_id,
                author_id,
                content,
                created_at,
                edited_at,
                new,
                mentioned_user_ids,
                ..
            }) = frame
            {
                let label = format!("user {}", author_id);
                // Push the full 6-tuple: the wire `message_id` + the optional
                // `edited_at` (epoch millis -> secs, like `created_at`) ride
                // along so the row's edit affordance and "(edited)" marker can
                // find the message later. History frames (`new: false`) may
                // carry a non-None `edited_at` for messages already edited
                // before this session, so store it as-is.
                live_buf
                    .borrow_mut()
                    .entry(*channel_id)
                    .or_default()
                    .push((
                        *message_id,
                        *author_id,
                        label,
                        content.clone(),
                        *created_at / 1000,
                        edited_at.map(|ms| ms / 1000),
                    ));
                live_rev.set(live_rev.get() + 1);
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

    // --- message reactions (mock, session-local) ---------------------------
    // Per-message reaction state for the MOCK message list: `reactions` is the
    // pill map (keyed by `(channel_id, list_index)` — see the `ReactionMap`
    // doc for why the rendered-list position is a stable-enough key), `picker`
    // names the message whose add-reaction overlay is open (`None` while
    // closed), and `reaction_rev` is the rebuild counter the message-list
    // `switch` folds into its key so a pill change re-renders the list. Owned
    // here next to the other mock per-app signals; the live/gateway path is
    // left untouched (this feature is mock/session-local only).
    let reactions: Rc<RefCell<ReactionMap>> = Rc::new(RefCell::new(HashMap::new()));
    let reaction_rev: Signal<u32> = signal(0);
    let picker: Signal<Option<MessageKey>> = signal(None);
    // Reply context for the channel composer: `Some((author, snippet))` while a
    // message row's hover "Reply" button is composing a reply (the
    // `ReplyPreviewBar` strip reads it); `None` while not. `PinConfirmToast`'s
    // `visible` flag, flashed by the "Pin message" overflow-menu entry.
    let reply_ctx: Signal<Option<(String, String)>> = signal(None);
    let pin_visible: Signal<bool> = signal(false);
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
    let live_dm_new_id: Signal<String> = signal(String::new());

    // Server Moderation panel: a gavel icon in the channel header opens it as
    // a full-screen overlay (the same toggle idiom as `profile_mode` — built
    // once, shown/hidden by this signal, a "Done" pill flips it back off).
    // Mock-only: renders the eight unwired moderation components with hardcoded
    // rows; no backend call.
    let moderation_mode: Signal<bool> = signal(false);

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
    }).collect();

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
                // Static base for the squircle. No white-invert on selection —
                // the left-edge pill alone signals the active live server
                // (matches the mock / created rows).
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
                            // live server (kick confirm + error text).
                            live_kick.set(None);
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
                                    live_ch.set(channels);
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
    // Join flow). `on_create` mints a fresh server via `create_server`, appends
    // its id to `server_rail` (the rail's re-render trigger — the
    // `create_server_section` `switch` above rebuilds and shows the row), and
    // closes the modal. Both the `Cell` and the `HashMap` are `Rc` so this
    // `Fn` closure can mutate them; `Signal` is `Copy`, so the `set(false)`
    // capture stays `Fn`.
    let create_on_create: Rc<dyn Fn(String)> = Rc::new({
        let next_id = next_id.clone();
        let dynamic_servers = dynamic_servers.clone();
        let server_rail = server_rail.clone();
        let create_open = create_open;
        move |name: String| {
            let new_id = create_server(&name, &*next_id, &dynamic_servers);
            let mut ids = server_rail.get();
            ids.push(new_id);
            server_rail.set(ids);
            create_open.set(false);
        }
    });
    let create_modal_el: Element =
        create_server_modal(create_open, create_name, create_on_create);
    let create_el = ui! {
        view {
            { create_modal_el }
            ServerIconUploadPlaceholder()
            ServerRegionSelector()
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
            // describe). The compass is Discord's Explore affordance.
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    create_name.set(String::new());
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
            // Explore / discovery browsing content: a folder of servers, a
            // templated server card, a template gallery grid, and the tag
            // chips that filter a discovery result set. All mock/local state.
            // (The older server_stats_card/ServerEventCard/server_discovery_card/
            // category_filter_chips cards that used to sit here were already
            // re-homed into the Discover Servers overlay -- see fa4bf410 --
            // so they are deliberately NOT re-added here.)
            server_folder_group()
            server_template_card()
            TemplateGalleryGrid()
            server_tag_chips()
            FeaturedServerBanner()
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
            )
        },
        move |(server_id, _channel_id, _voice, _live_server, _live_fp, _mention_rev, _unread_rev, _cat_collapsed): &(
            u32,
            u32,
            Option<(u32, u32)>,
            Option<i64>,
            Vec<(i64, String, String)>,
            u32,
            u32,
            Vec<(u32, bool)>,
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
                    let gw = gateway.clone();
                    let live_unread_map = live_unread.clone();
                    // Shared style for the live channel row action icon buttons
                    // (rename pencil / delete trash) and the new-channel "+"
                    // header action. Defined here, inside the live arm of the
                    // `switch` render closure, because the per-button handlers
                    // touch the modal `Signal`s (`rc_*` / `dc_*` / `cc_*`) that
                    // this `move` render closure owns copies of — a
                    // function-scope non-`move` closure could not borrow those
                    // across the `move` capture boundary (E0373).
                    let chan_btn_style = StyleRules {
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
                    let chan_actions = |cid: i64, nm: Rc<String>, u_count: i64, u_mentions: i64| -> Element {
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
                        let rename_btn = ui! {
                            view(style = chan_btn_style.clone(), test_id = "live-channel-rename") {
                                icon(PENCIL).color(|| Color::from(TEXT_MUTED)).size(14.0)
                            }
                            .on_touch(move |ev| {
                                if ev.phase == TouchPhase::Ended {
                                    rc_cid.set(Some(cid));
                                    rc_name.set((*nm_rename).clone());
                                    rc_busy.set(false);
                                    rc_err.set(String::new());
                                    rc_open.set(true);
                                }
                                TouchResponse::CONSUMED
                            })
                        };
                        let delete_btn = ui! {
                            view(style = chan_btn_style.clone(), test_id = "live-channel-delete") {
                                icon(TRASH_2).color(|| Color::from(TEXT_MUTED)).size(14.0)
                            }
                            .on_touch(move |ev| {
                                if ev.phase == TouchPhase::Ended {
                                    dc_cid.set(Some(cid));
                                    dc_name.set((*nm_delete).clone());
                                    dc_busy.set(false);
                                    dc_err.set(String::new());
                                    dc_open.set(true);
                                }
                                TouchResponse::CONSUMED
                            })
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
                                { rename_btn }
                                { delete_btn }
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
                            // pill + rename/delete buttons), built once per row.
                            // `chan_actions` is `Copy` (all `Copy` captures), so
                            // calling it here does not move it out of the
                            // `switch` render closure.
                            let actions = chan_actions(cid, Rc::new(name.clone()), u_count, u_mentions);
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
                                        { actions }
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
            // Mock thread list appended below the channel rows — local-only
            // showcase of the thread rows (no backend call).
            rows.push(ThreadListRow());
            rows.push(ThreadListRow());
            let title = server.name.to_string();
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
                    // Mock thread list near the channel sidebar: a short stack
                    // of `ThreadListRow`s above the user panel. Local mock
                    // data, no backend.
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Column),
                        align_items: Some(AlignItems::Stretch),
                        padding_top: Some(px(8.0)),
                        padding_left: Some(px(8.0)),
                        padding_right: Some(px(8.0)),
                        ..Default::default()
                    }) {
                        ThreadListRow()
                        ThreadListRow()
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
    let header = switch(
        move || (active_server.get(), active_channel.get(), active_live_channel.get()),
        move |(server_id, channel_id, live_cid): &(u32, u32, Option<i64>)| {
            let header_style = header_style.clone();
            let hash_style = hash_style.clone();
            let title_style = title_style.clone();
            let topic_style = topic_style.clone();
            let moderation_style = moderation_style.clone();
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
            ui! {
                view(style = header_style) {
                    text(style = hash_style) { "#".to_string() }
                    text(style = title_style) { title }
                    text(style = topic_style) { topic }
                    // Spacer: eats the row's remaining width so the pin badge
                    // and search field sit at the far right.
                    view(style = StyleRules { flex_grow: Some(grow(1.0)), ..Default::default() }) {}
                    PinCountBadge()
                    // Message-search field: a clean field on the right side of
                    // the channel header row (Discord's layout) — no separate
                    // label above it.
                    MessageSearchBar()
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
    let pin_visible_k = pin_visible.clone();
    let translate_key_k = translate_key.clone();
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
        ),
        move |(server_id, channel_id, _revision, live_cid, _live_rev, _editing, _confirm, _react_rev): &(u32, u32, u32, Option<i64>, u32, Option<i64>, Option<i64>, u32)| {
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
                let pin_l = pin_visible_k;
                let trans_l = translate_key_k;
                let mut rows: Vec<Element> = Vec::with_capacity(mine.len() * 2);
                for m in mine.iter() {
                    let (mid, aid, author, body, ts, edited_at) = m;
                    let when = format_when(*ts, now);
                    let full = format_full_when(*ts);
                    let is_mine = self_id != 0 && *aid == self_id;
                    let lkey = MessageKey::Live { id: *mid };
                    rows.push(live_edit_row(
                        *mid,
                        author.as_str(),
                        BUTTON,
                        body.as_str(),
                        when,
                        full,
                        edited_at.is_some(),
                        is_mine,
                        editing,
                        edit_text,
                        delete_c,
                        gw,
                        lkey,
                        picker_l,
                        reply_l,
                        pin_l,
                        trans_l,
                    ));
                    // Reaction strip under each live row, keyed by the real wire
                    // `message_id` — the same session-local pill store the mock
                    // rows use, so live reactions behave exactly like mock ones.
                    rows.push(reaction_area(
                        lkey,
                        reactions.clone(),
                        reaction_rev,
                        picker_l,
                    ));
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
            for (i, m) in channel.messages.iter().enumerate() {
                let when = format_when(m.ts_secs(now), now);
                // Mock seed rows carry no edit state; pass `true` to preserve
                // their previous always-on "(edited)" placeholder rendering.
                rows.push(message_row_with_actions(
                    m.author,
                    m.color,
                    m.body,
                    when,
                    format_full_when(m.ts_secs(now)),
                    true,
                    MessageKey::Mock { cid, idx: i },
                    picker,
                    reply_ctx_k,
                    pin_visible_k,
                    translate_key_k,
                ));
                // Reaction strip under each mock row, keyed by this row's
                // rendered-list position (seed messages first, so index `i`).
                rows.push(reaction_area(
                    MessageKey::Mock { cid, idx: i },
                    reactions.clone(),
                    reaction_rev,
                    picker,
                ));
            }
            let mine: Vec<(u32, &'static str, String, i64)> = sent_read.borrow().clone();
            let mine: Vec<(u32, &'static str, String, i64)> = mine
                .into_iter()
                .filter(|(mcid, _, _, _)| *mcid == cid)
                .collect();
            for (i, (_mcid, author, body, ts)) in mine.iter().enumerate() {
                let when = format_when(*ts, now);
                // Own prior sends get the same hover bar as the seed rows.
                rows.push(message_row_with_actions(
                    author,
                    BUTTON,
                    body.as_str(),
                    when,
                    format_full_when(*ts),
                    true,
                    MessageKey::Mock { cid, idx: channel.messages.len() + i },
                    picker,
                    reply_ctx_k,
                    pin_visible_k,
                    translate_key_k,
                ));
                // Sent rows continue the list position after the seed rows, so
                // their key is `seed_len + i` (see `ReactionMap` doc).
                rows.push(reaction_area(
                    MessageKey::Mock { cid, idx: channel.messages.len() + i },
                    reactions.clone(),
                    reaction_rev,
                    picker,
                ));
            }
            ui! {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Column),
                    ..Default::default()
                }) { rows PollWidget() SpoilerImagePlaceholder() }
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
        if body.trim().is_empty() {
            return;
        }
        // A staged mock attachment (if any) rides along with the send. It's a
        // pure UI mock (no real file), so the visible message body is what
        // carries it: we append a `[attachment: <name>]` line to it, then clear
        // the chip alongside the composer text on both the live and mock paths.
        let attached = pending_attachment.get();
        pending_attachment.set(None);
        let out_body = match attached {
            Some(name) => format!("{body}\n[attachment: {name}]"),
            None => body,
        };
        // Live/real mode: send over the gateway socket to the live channel.
        // The server echoes it back as a `Message` frame, which the incoming
        // effect buffers and bumps `live_revision`, rebuilding the list.
        if let Some(live_cid) = live_chan.get() {
            gateway_send.send(discord_server::server_settings::WsClientMsg::Send {
                channel_id: live_cid,
                content: out_body.clone(),
            });
            send_composer.set(String::new());
            return;
        }
        let cid = active_channel.get();
        sent_write.borrow_mut().push((cid, "you", out_body, crate::data::now_secs()));
        send_composer.set(String::new());
        // Bump the revision so the message-list `switch` rebuilds (its key
        // dedups on value, so re-setting the same channel alone would not).
        revision_bump.set(revision_bump.get() + 1);
    };
    let on_send = Rc::new(do_send);
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

    // The mock emoji set: 12 common glyphs as a wrapped grid. A local `let`
    // (not a module const) so the popover stays self-contained in `app()`.
    let EMOJIS: [&str; 12] = [
        "😀", "😊", "😂", "😍", "😎", "🤔", "👍", "🎉", "🚀", "❤️", "🙌", "🍕",
    ];

    // Staged-attachment chip: `switch` on `pending_attachment` so the chip
    // only renders while a mock attachment is staged. The `x` clears it.
    // `chip_clear` is a `Copy` handle on `pending_attachment`; the nested
    // `on_touch` `move` closure gets a fresh per-render copy (`cc`) so it never
    // tries to move the outer closure's captured handle (E0507).
    let chip_clear = pending_attachment;
    let attach_chip = switch(
        move || pending_attachment.get(),
        move |att: &Option<String>| {
            match att {
                None => ui! {
                    view(style = StyleRules { height: Some(px(0.0)), ..Default::default() }) {}
                },
                Some(name) => ui! {
                    // A thin row just above the composer holding the pill; the
                    // row's own margins give the pill breathing room.
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        margin_top: Some(px(8.0)),
                        margin_bottom: Some(px(8.0)),
                        margin_left: Some(px(16.0)),
                        margin_right: Some(px(16.0)),
                        ..Default::default()
                    }) {
                        // The pill itself: fully-rounded (radius = half height)
                        // — the app's standard chip/lozenge shape (see
                        // `server_tag_chips` / the rail unread pill).
                        view(style = StyleRules {
                            height: Some(px(26.0)),
                            padding_left: Some(px(12.0)),
                            padding_right: Some(px(8.0)),
                            flex_direction: Some(FlexDirection::Row),
                            align_items: Some(AlignItems::Center),
                            gap: Some(px(8.0)),
                            background: Some(col(BG_SIDEBAR)),
                            border_top_left_radius: Some(px(13.0)),
                            border_top_right_radius: Some(px(13.0)),
                            border_bottom_left_radius: Some(px(13.0)),
                            border_bottom_right_radius: Some(px(13.0)),
                            ..Default::default()
                        }) {
                            text(style = StyleRules {
                                font_size: Some(px(12.0)),
                                color: Some(col(TEXT)),
                                ..Default::default()
                            }) { format!("📎 {name}") }
                            // The 'x' to clear the staged attachment: a tiny
                            // circular hit target (a `view` + glyph, the same
                            // shape as the composer's icon buttons).
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
                                    let cc = chip_clear;
                                    cc.set(None);
                                }
                                TouchResponse::CONSUMED
                            })
                        }
                    }
                }
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
            // One clickable glyph cell per emoji. Built as a `Vec<Element>`
            // (the `ac_el` idiom) and splatted below; each cell's `on_touch`
            // gets fresh `Copy` handles (`c`/`eo`) per iteration so it never
            // moves the outer closure's captured signals.
            let cells: Vec<Element> = EMOJIS.iter().enumerate().map(|(_, g)| {
                let glyph = g.to_string();
                let glyph_close = glyph.clone();
                let c = composer_e;
                let eo = emoji_open_sig;
                ui! {
                    view(style = StyleRules {
                        width: Some(px(34.0)),
                        height: Some(px(34.0)),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        border_top_left_radius: Some(px(6.0)),
                        border_top_right_radius: Some(px(6.0)),
                        border_bottom_left_radius: Some(px(6.0)),
                        border_bottom_right_radius: Some(px(6.0)),
                        ..Default::default()
                    }) {
                        text(style = StyleRules {
                            font_size: Some(px(20.0)),
                            ..Default::default()
                        }) { glyph }
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            let mut t = c.get();
                            t.push_str(&glyph_close);
                            c.set(t);
                            eo.set(false);
                        }
                        TouchResponse::CONSUMED
                    })
                }
            }).collect();
            // The floating panel: absolutely positioned just above the composer
            // (mirroring `ac_el`'s `bottom` anchor), dark `BG_SIDEBAR` fill with
            // a wrapping row of cells inside.
            ui! {
                view(style = StyleRules {
                    position: Some(Position::Absolute),
                    left: Some(px(16.0)),
                    right: Some(px(16.0)),
                    bottom: Some(px(76.0)),
                    background: Some(col(BG_SIDEBAR)),
                    padding_top: Some(px(10.0)),
                    padding_bottom: Some(px(10.0)),
                    padding_left: Some(px(10.0)),
                    padding_right: Some(px(10.0)),
                    border_top_left_radius: Some(px(8.0)),
                    border_top_right_radius: Some(px(8.0)),
                    border_bottom_left_radius: Some(px(8.0)),
                    border_bottom_right_radius: Some(px(8.0)),
                    overflow: Some(Overflow::Hidden),
                    ..Default::default()
                }) {
                    // The wrapping grid of emoji cells.
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Row),
                        flex_wrap: Some(FlexWrap::Wrap),
                        gap: Some(px(4.0)),
                        ..Default::default()
                    }) { cells }
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
    let members_switch = switch(
        move || (active_server.get(), user_status.get()),
        move |(server_id, user_status): &(u32, Status)| {
            let server = SERVERS.iter().find(|x| x.id == *server_id).unwrap();
            member_panel(server, &USER.as_member(*user_status), selected_member.clone(), member_panel_width, sidebar_width)
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
            view(style = StyleRules { flex_grow: Some(grow(1.0)), overflow: Some(Overflow::Hidden), ..Default::default() }) {
                profile_el
                NicknameEditField()
                CustomStatusInput()
                CharCounter()
                TwoFactorToggle()
                DeleteAccountWarning()
                // Account-security setting, living with the other account
                // settings in this overlay (it is NOT a composer element).
                TwoFactorToggle()
                IconChangeConfirm()
                SettingsTabBar()
                SetupChecklistDemo()
                EmojiUploadSlot()
                rules_accept_checkbox()
            }
        }
    };

    // Notification Settings screen (built once, same shape as the Profile
    // overlay above): collapses to zero size when `notif_mode` is off, fills
    // the window when on, with a "Done" pill to close it.
    let notif_el = NotificationSettings();
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
            }) { "Notifications".to_string() }
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
        background: Some(col(BG_MAIN)),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    let notif_region = ui! {
        view(style = notif_wrap) {
            notif_header
            view(style = StyleRules { flex_grow: Some(grow(1.0)), overflow: Some(Overflow::Hidden), ..Default::default() }) { notif_el }
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
        background: Some(col(BG_MAIN)),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    let shortcuts_region = ui! {
        view(style = shortcuts_wrap) {
            shortcuts_header
            view(style = StyleRules { flex_grow: Some(grow(1.0)), overflow: Some(Overflow::Hidden), ..Default::default() }) { shortcuts_el }
        }
    };
    // Direct Messages screen (mock): same full-screen, paint-last idiom as the
    // Profile / Notification overlays above. `dm_screen` builds the list column
    // + the active conversation; its three inputs are the `dm_*` signals owned
    // in `app_shell` scope, so the open conversation and this session's sent
    // DMs survive a close/re-open. A "Done" pill in the header flips
    // `dm_mode` back off to close it.
    let dm_el = dm_screen(dm_active, dm_sent, dm_revision);
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
        background: Some(col(BG_MAIN)),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    let dm_region = ui! {
        view(style = dm_wrap) {
            dm_header
            view(style = StyleRules { flex_grow: Some(grow(1.0)), overflow: Some(Overflow::Hidden), ..Default::default() }) { dm_el }
        }
    };

    // Live (backend) Direct Messages screen: same full-screen, paint-last idiom
    // as the mock `dm_region` above, but a separate, independent entry point.
    // Its three data inputs (`live_dms` / `live_dm_active` / `live_dm_messages`
    // + `live_dm_revision`) are owned in `app_shell` scope, so the open
    // conversation and its fetched history survive a close/re-open. A "Done"
    // pill in the header flips `live_dm_mode` back off to close it.
    let live_dm_el = crate::live_dm_view::live_dm_screen(
        live_dms,
        live_dm_active,
        live_dm_messages,
        live_dm_revision,
        live_dm_new_id,
        gateway.incoming(),
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
        background: Some(col(BG_MAIN)),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    let live_dm_region = ui! {
        view(style = live_dm_wrap) {
            live_dm_header
            view(style = StyleRules { flex_grow: Some(grow(1.0)), overflow: Some(Overflow::Hidden), ..Default::default() }) { live_dm_el }
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
        background: Some(col(BG_MAIN)),
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
        TimeoutDurationPicker()
        text(style = section_label_style.clone()) { "Role permissions".to_string() }
        PermissionOverrideRow()
        PermissionOverrideRow()
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
            view(style = StyleRules { flex_grow: Some(grow(1.0)), overflow: Some(Overflow::Hidden), ..Default::default() }) {
                scroll_view(style = StyleRules { flex_grow: Some(grow(1.0)), ..Default::default() }) { moderation_el }
            }
        }
    };

    // Member profile overlay: built once, shown/hidden by `selected_member`.
    // Full-screen and painted last so it sits above the member list; collapses
    // to zero size while no member is selected (the same toggle idiom as the
    // other overlay regions above).
    let member_profile_el = member_profile_panel(selected_member.clone());


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
                NotificationPermissionPrompt()
                header
                ChannelTopicBanner()
                scroll_view(style = StyleRules { flex_grow: Some(grow(1.0)), ..Default::default() }) {
    WelcomeBanner()
    JumpToBottomButton()
    sent_messages
}
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
                    reply_preview(reply_ctx)
                    attach_chip
                    AttachmentPreviewCard()
                    emoji_popover
                    ac_el
                    // Per-channel slowmode indicator (gated): the channel-keyed
                    // `slowmode_el` above resolves the active channel's
                    // `slowmode_secs` and renders the pill only for
                    // slowmode-enabled channels — nothing for the rest. Same
                    // home b60 dropped the bare, unconditional
                    // `SlowmodeIndicator()` into; now it reads real state.
                    slowmode_el
                    // Composer-area extras (mock, local only): the slash-command
                    // autocomplete dropdown and the forward-message dialog, both
                    // stacked above the composer row. No backend.
                    SlashCommandDropdown()
                    ForwardMessageDialog()
                    view(style = composer_style) {
                        view(style = icon_button_style.clone()) {
                            icon(PLUS).color(|| Color::from(TEXT_MUTED)).size(16.0)
                        }
                        // `+` = stage/unstage the mock attachment. `Signal` is
                        // `Copy`, so each `move` closure captures its own handle;
                        // the chip above the composer mirrors this state.
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                pending_attachment.set(if pending_attachment.get().is_some() {
                                    None
                                } else {
                                    Some("mock-attachment.png".to_string())
                                });
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
        }
    }
}
