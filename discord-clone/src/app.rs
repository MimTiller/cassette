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

// Helpers split out of this file (see src/app/).
mod time_fmt;
pub(crate) use time_fmt::*;
mod message_view;
pub(crate) use message_view::*;
mod reactions_ui;
use reactions_ui::*;
mod member_panels;
use member_panels::*;
mod user_panel;
use user_panel::*;
mod login_screen;
use login_screen::*;
mod layout;
use layout::*;

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

fn grow(v: f32) -> Tokenized<f32> {
    Tokenized::Literal(v)
}

fn border_width(px: f32) -> Tokenized<f32> {
    Tokenized::Literal(px)
}

// --- timestamps --------------------------------------------------------------

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

    // A `History` request sent while the gateway is still connecting is
    // dropped (`send` returns false). After a refresh the auto-enter above
    // usually selects the restored channel before the socket opens, which
    // left that channel open with an empty pane until it was clicked again.
    // So each time the socket reaches `Open` (first connect and every
    // reconnect), ask again for the open channel's history. History frames
    // are de-duplicated by message id, so a repeat request is harmless. The
    // channel is read untracked: channel switches send their own request.
    {
        let alc = active_live_channel.clone();
        runtime_core::effect!({
            if gateway.status() == server::SocketStatus::Open {
                if let Some(id) = runtime_core::Reactive::from(alc).get_untracked() {
                    gateway.send(discord_server::server_settings::WsClientMsg::History {
                        channel_id: id,
                        before_id: None,
                    });
                }
            }
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
                // The uploaded icon fills the 48px tile with the tile's own
                // rounding; servers without one keep the initial letter.
                let face = match s.icon_url.clone().filter(|u| !u.is_empty()) {
                    Some(url) => ui! {
                        image(
                            src = crate::auth::media_url(url),
                            alt = s.name.clone(),
                            style = StyleRules {
                                width: Some(px(48.0)),
                                height: Some(px(48.0)),
                                flex_shrink: Some(grow(0.0)),
                                object_fit: Some(ObjectFit::Cover),
                                border_top_left_radius: Some(px(16.0)),
                                border_top_right_radius: Some(px(16.0)),
                                border_bottom_left_radius: Some(px(16.0)),
                                border_bottom_right_radius: Some(px(16.0)),
                                ..Default::default()
                            },
                        )
                    },
                    None => ui! { text(style = initial_style.clone()) { initial } },
                };
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
                        { face }
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
