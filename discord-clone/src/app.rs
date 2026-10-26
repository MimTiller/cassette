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
    CHEVRON_DOWN, CHEVRON_RIGHT, CIRCLE_HELP, COMPASS, GAVEL, GLOBE, HASH, HEADPHONES, MAIL,
    MIC, MIC_OFF, PIN, PLUS, SETTINGS, SMILE, SMILE_PLUS,
    USER_PLUS, USERS, VOLUME_2,
};
use crate::idle_timer_badge::IdleTimerBadge;
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

use crate::blocked_user_row::BlockedUserRow;
use crate::category_header::CategoryHeader;
use crate::mic_selector::MicSelector;

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
use crate::edited_label::EditedLabel;
use crate::settings_suite::UserSettings;
use crate::keyboard_shortcuts_overlay::keyboard_shortcuts_overlay;
use crate::profile_settings::ProfileSettings;
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
use crate::rules_accept_checkbox::RulesAcceptCheckbox;
use crate::nickname_edit_field::NicknameEditField;
use crate::custom_status_input::CustomStatusInput;
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
mod shell_state;
use shell_state::ShellState;
mod gateway_effects;
mod rail;
mod modals;
mod user_popups;
mod sidebar;
mod messages;
mod composer;
mod member_list;
mod overlays;

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

// Robot test hook (`attach.stage`): stage one composer image attachment
// from base64 bytes — the file-picker callback's validate → encode path
// (`stage_attachment` above, which the picker's async callback runs),
// callable through the robot bridge so an E2E test can attach a real
// image without driving the native file-open dialog (which no in-page
// tool can tap). Compiled only into robot builds.
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
    let _live_max_offset: Rc<RefCell<f32>> = Rc::new(RefCell::new(0.0));
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
    // The live sidebar's collapsed categories (channel ids). Plain data in one
    // app_shell-owned signal: a per-category signal created inside the
    // sidebar's render would be freed on the next rebuild.
    let live_collapsed: Signal<Vec<i64>> = signal(Vec::new());
    // The open profile card / user context menu and the profile it shows
    // (see `user_popups`); the async profile fetch writes here.
    let user_popup: Signal<Option<user_popups::UserPopup>> = signal(None);
    let user_profile: Signal<Option<discord_server::profiles::UserProfile>> = signal(None);
    // The signed-in user's blocked list, and the blocked messages they chose
    // to reveal this session (see `crate::blocks`).
    let blocked_users: Signal<Vec<crate::blocks::BlockedUser>> = signal(Vec::new());
    let revealed_blocked: Signal<Vec<i64>> = signal(Vec::new());
    // The live sidebar header's server dropdown.
    let server_menu_open: Signal<bool> = signal(false);
    if is_real_user {
        crate::blocks::load(blocked_users, live_revision);
    }
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
    let rc_open: Signal<bool> = signal(false);
    let rc_cid: Signal<Option<i64>> = signal(None);
    let rc_name: Signal<String> = signal(String::new());
    let rc_busy: Signal<bool> = signal(false);
    let rc_err: Signal<String> = signal(String::new());
    let dc_open: Signal<bool> = signal(false);
    let dc_cid: Signal<Option<i64>> = signal(None);
    let dc_name: Signal<String> = signal(String::new());
    let dc_busy: Signal<bool> = signal(false);
    let dc_err: Signal<String> = signal(String::new());
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

    // Shared by the rail's buttons and the modal dialogs they open.
    // Reactive list of the mock servers shown in the rail. Seeded with the
    // static `SERVERS` ids; the "Add a server" (+) flow appends new ids, and
    // the rail `switch` below rebuilds on that signal so new rows appear.
    let server_rail: Signal<Vec<u32>> = signal(SERVERS.iter().map(|s| s.id).collect());

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

    // --- "Discover Servers" overlay ----------------------------------------
    // Trigger is a third rail circle (GLOBE) below the PLUS and COMPASS
    // circles — both existing icons are taken (Join / Create). `on_join`
    // mints the suggested server the same way Create does: `create_server` +
    // append to `server_rail` (the rail's re-render trigger), then close the
    // overlay. The mock list itself is static; no join backend.
    let discover_open: Signal<bool> = signal(false);

    // Everything the section builders below read, gathered once.
    let s = ShellState {
        user_popup: user_popup.clone(),
        blocked_users: blocked_users.clone(),
        server_menu_open: server_menu_open.clone(),
        revealed_blocked: revealed_blocked.clone(),
        user_profile: user_profile.clone(),
        live_collapsed: live_collapsed.clone(),
        server_rail: server_rail.clone(),
        dynamic_servers: dynamic_servers.clone(),
        next_id: next_id.clone(),
        create_open: create_open.clone(),
        create_name: create_name.clone(),
        create_error: create_error.clone(),
        create_busy: create_busy.clone(),
        join_open: join_open.clone(),
        join_code: join_code.clone(),
        join_result: join_result.clone(),
        join_error: join_error.clone(),
        join_busy: join_busy.clone(),
        discover_open: discover_open.clone(),
        active_channel: active_channel.clone(),
        active_live_channel: active_live_channel.clone(),
        active_live_server: active_live_server.clone(),
        active_server: active_server.clone(),
        attach_busy: attach_busy.clone(),
        attach_error: attach_error.clone(),
        attach_next_key: attach_next_key.clone(),
        authed: authed.clone(),
        bug_reports_mode: bug_reports_mode.clone(),
        cc_busy: cc_busy.clone(),
        cc_err: cc_err.clone(),
        cc_kind: cc_kind.clone(),
        cc_name: cc_name.clone(),
        cc_open: cc_open.clone(),
        collapsed: collapsed.clone(),
        composer: composer.clone(),
        dc_busy: dc_busy.clone(),
        dc_cid: dc_cid.clone(),
        dc_err: dc_err.clone(),
        dc_name: dc_name.clone(),
        dc_open: dc_open.clone(),
        deafened: deafened.clone(),
        dm_active: dm_active.clone(),
        dm_mode: dm_mode.clone(),
        dm_revision: dm_revision.clone(),
        dm_sent: dm_sent.clone(),
        dm_unread: dm_unread.clone(),
        dm_unread_rev: dm_unread_rev.clone(),
        emoji_open: emoji_open.clone(),
        friends_add_error: friends_add_error.clone(),
        friends_add_username: friends_add_username.clone(),
        friends_entry: friends_entry.clone(),
        friends_open: friends_open.clone(),
        friends_snap: friends_snap.clone(),
        friends_tab: friends_tab.clone(),
        gateway: gateway.clone(),
        has_more_earlier: has_more_earlier.clone(),
        invite_created: invite_created.clone(),
        invite_err: invite_err.clone(),
        invite_expiry: invite_expiry.clone(),
        invite_mode: invite_mode.clone(),
        is_real_user: is_real_user.clone(),
        joined_voice: joined_voice.clone(),
        live_at_bottom: live_at_bottom.clone(),
        live_attachments: live_attachments.clone(),
        live_ban_confirm: live_ban_confirm.clone(),
        live_chan_menu: live_chan_menu.clone(),
        live_channels: live_channels.clone(),
        live_delete_confirm: live_delete_confirm.clone(),
        live_dm_active: live_dm_active.clone(),
        live_dm_messages: live_dm_messages.clone(),
        live_dm_mode: live_dm_mode.clone(),
        live_dm_new_id: live_dm_new_id.clone(),
        live_dm_revision: live_dm_revision.clone(),
        live_dms: live_dms.clone(),
        live_edit_text: live_edit_text.clone(),
        live_editing: live_editing.clone(),
        live_invite_code: live_invite_code.clone(),
        live_invite_error: live_invite_error.clone(),
        live_invite_mode: live_invite_mode.clone(),
        live_kick_confirm: live_kick_confirm.clone(),
        live_member_error: live_member_error.clone(),
        live_member_menu: live_member_menu.clone(),
        live_members: live_members.clone(),
        live_mention_rev: live_mention_rev.clone(),
        live_mentioned: live_mentioned.clone(),
        live_messages: live_messages.clone(),
        live_presence: live_presence.clone(),
        live_presence_rev: live_presence_rev.clone(),
        live_revision: live_revision.clone(),
        live_scroll_handle: live_scroll_handle.clone(),
        live_servers: live_servers.clone(),
        live_timeout_confirm: live_timeout_confirm.clone(),
        live_timeout_duration: live_timeout_duration.clone(),
        live_typing: live_typing.clone(),
        live_typing_rev: live_typing_rev.clone(),
        live_unread: live_unread.clone(),
        live_unread_rev: live_unread_rev.clone(),
        load_busy: load_busy.clone(),
        member_panel_width: member_panel_width.clone(),
        moderation_mode: moderation_mode.clone(),
        muted: muted.clone(),
        notif_level: notif_level.clone(),
        notif_mode: notif_mode.clone(),
        notif_toast_body: notif_toast_body.clone(),
        notif_toast_title: notif_toast_title.clone(),
        notif_toast_visible: notif_toast_visible.clone(),
        on_dm_message: on_dm_message.clone(),
        pending_attachment: pending_attachment.clone(),
        picker: picker.clone(),
        pin_visible: pin_visible.clone(),
        pinned_count: pinned_count.clone(),
        pinned_error: pinned_error.clone(),
        pinned_messages: pinned_messages.clone(),
        pinned_panel_open: pinned_panel_open.clone(),
        pinned_revision: pinned_revision.clone(),
        profile_mode: profile_mode.clone(),
        rail_unread: rail_unread.clone(),
        rc_busy: rc_busy.clone(),
        rc_cid: rc_cid.clone(),
        rc_err: rc_err.clone(),
        rc_name: rc_name.clone(),
        rc_open: rc_open.clone(),
        reaction_rev: reaction_rev.clone(),
        reactions: reactions.clone(),
        reply_ctx: reply_ctx.clone(),
        reply_target: reply_target.clone(),
        revision: revision.clone(),
        selected_member: selected_member.clone(),
        send_failed: send_failed.clone(),
        sent: sent.clone(),
        shortcuts_mode: shortcuts_mode.clone(),
        sidebar_width: sidebar_width.clone(),
        ss_add_role: ss_add_role.clone(),
        ss_add_user_id: ss_add_user_id.clone(),
        ss_busy: ss_busy.clone(),
        ss_err: ss_err.clone(),
        ss_icon: ss_icon.clone(),
        ss_name: ss_name.clone(),
        ss_open: ss_open.clone(),
        ss_owner: ss_owner.clone(),
        translate_key: translate_key.clone(),
        user_status: user_status.clone(),
    };

    let gateway_effects::GatewayEffectsParts { gateway_status, gateway_incoming_effect, mention_reset_effect } = gateway_effects::build(&s);
    let rail::RailParts { rail_style, server_rows, create_server_section, live_section, rail_actions } = rail::build(&s);
    let modals::ModalsParts { join_el, create_el, discover_el, cc_el, rc_el, dc_el, invite_el, ss_el } = modals::build(&s);
    let user_popups::UserPopupsParts { user_popup_el, chan_menu_catcher_el } = user_popups::build(&s);
    let sidebar::SidebarParts { sidebar } = sidebar::build(&s);
    let messages::MessagesParts { header, slowmode_el, sent_messages, typing_row } = messages::build(&s);
    let composer::ComposerParts { composer_area } = composer::build(&s, slowmode_el);
    let member_list::MemberListParts { members_switch } = member_list::build(&s);

    let main_style = StyleRules {
        flex_grow: Some(grow(1.0)),
        background: Some(col(BG_MAIN)),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    let overlays::OverlaysParts { profile_region, notif_region, shortcuts_region, dm_region, live_dm_region, moderation_region, member_profile_el, live_invite_region, bug_reports_region } = overlays::build(&s);

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
                composer_area
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
            reaction_picker_overlay(picker, reactions.clone(), reaction_rev)
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
            chan_menu_catcher_el
            // Profile card / user context menu: last, so it floats above
            // everything else.
            user_popup_el
        }
    }
}
