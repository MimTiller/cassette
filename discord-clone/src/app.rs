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

use crate::theme;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use runtime_core::{
    after_ms_detached, AlignItems, AlignSelf, Cursor, Element,
    FlexDirection, FlexWrap, FontFamily, FontWeight, JustifyContent, KeyEvent, KeyOutcome, Overflow,
    PanEvent, PanRecognizer, Position, Ref, ScrollViewHandle, Signal, StyleRules,
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
    CHEVRON_DOWN, CHEVRON_RIGHT, HASH, HEADPHONES,
    MIC, MIC_OFF, PIN, PLUS, SETTINGS, SMILE, SMILE_PLUS,
    USER_PLUS, USERS, VOLUME_2, MESSAGE_CIRCLE, MENU,
};
use crate::server_settings_panel::ServerSettingsPanel;

use runtime_shared::time::local_offset_minutes;

use crate::data::{SERVERS, Status, USER};
use crate::ban_list_row::BanListRow;
use crate::timeout_duration_picker::TimeoutDurationPicker;
use crate::member_role_list::MemberRoleList;
use crate::verified_badge::VerifiedBadge;
use crate::ban_reason_input::BanReasonInput;

use crate::blocked_user_row::BlockedUserRow;

use crate::send_failed_indicator::SendFailedIndicator;
use crate::jump_to_bottom_button::JumpToBottomButton;
use crate::load_earlier_button::load_earlier_button;
use crate::edited_label::EditedLabel;
use crate::settings_suite::UserSettings;
use crate::keyboard_shortcuts_overlay::keyboard_shortcuts_overlay;
use crate::style_helpers::{borderless_input, col, pct, px, rounded, section_label, with_radius};
use crate::join_modal::JoinServerModal;
use crate::create_server_modal::create_server_modal;
use crate::create_channel_modal::CreateChannelModal;
use crate::rename_channel_modal::RenameChannelModal;
use crate::delete_channel_modal::DeleteChannelModal;
use crate::typing_indicator::TypingIndicator;
use crate::invite_link_display::InviteLinkDisplay;
use crate::invite_expiry_selector::InviteExpirySelector;
use crate::slash_command_dropdown::SlashCommandDropdown;
use crate::emoji_picker::EmojiPicker;
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
use shell_state::{ShellState, Nav, Layout, Composer, Messages, Members, Overlays, Dm, Dialogs};
mod gateway_effects;
mod rail;
mod modals;
mod user_popups;
mod popups;
mod sidebar;
mod messages;
mod jump;
mod search;
mod composer;
mod member_list;
mod overlays;
pub(crate) mod mobile;

// Discord dark palette (Blurple-flavoured).
const BG_RAIL: &str = theme::BG_TERTIARY;
const BG_SIDEBAR: &str = theme::BG_SECONDARY;
const BG_MAIN: &str = theme::BG_PRIMARY;
const BG_ACTIVE: &str = theme::BG_SELECTED;
/// Discord's row hover colour (lighter than `#2b2d31`, darker than active).
const BG_HOVER: &str = theme::BG_HOVER;
/// Discord's elevated surface — one step above `BG_MAIN`, for the channel
/// anchor chip and other small raised elements that should sit visually above
/// the message pane background.
const BG_ELEVATED: &str = theme::BG_INPUT;
const TEXT: &str = theme::TEXT_STRONG;
const TEXT_MUTED: &str = theme::TEXT_MUTED;
const TEXT_CHANNEL: &str = theme::TEXT_NORMAL;
const BORDER: &str = theme::BG_TERTIARY;
const COMPOSER: &str = theme::BG_INPUT;
const BUTTON: &str = theme::ACCENT;
/// Active-server indicator pill on the rail (Discord's white sliver).
const PILL: &str = theme::TEXT_STRONG;
/// Discord's unread badge red (the pill on channels + the rail count).
const BADGE: &str = theme::DANGER;
/// The user-profile section at the bottom of the sidebar sits a shade darker
/// than the channel list above it.
const USER_BAR: &str = theme::BG_USER_PANEL;
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
    // The colour theme (tokens for every role), with the saved accent.
    crate::theme::install(crate::notify::load_theme(), crate::notify::load_accent());
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
        if user_id != 0)
        // A reset link opens the reset form even where someone is signed in.
        && crate::auth::reset_token_in_url().is_none();
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

thread_local! {
    /// The app shell's scope (see [`shell_alive`]).
    static SHELL_ALIVE: std::cell::RefCell<Option<runtime_core::ScopeAlive>> = const { std::cell::RefCell::new(None) };
}

/// The app shell's lifetime, for server calls whose results land in shell
/// state but start from a short-lived node (a menu that closes, a row that
/// rebuilds): a `spawn_then_in(&shell_alive(), …)` callback runs while the
/// user is signed in and is dropped after logout.
pub(crate) fn shell_alive() -> runtime_core::ScopeAlive {
    SHELL_ALIVE.with(|s| s.borrow().clone()).unwrap_or_else(runtime_core::ScopeAlive::current)
}

fn app_shell(authed: Signal<bool>) -> Element {
    SHELL_ALIVE.with(|s| *s.borrow_mut() = Some(runtime_core::ScopeAlive::current()));
    mobile::install(signal(mobile::Pane::default()), signal(true));
    // --- nav: shared app-shell state (signals built in shell_state.rs) ---
    let nav = Nav::new();
    let Nav { active_server, active_live_server, active_live_channel, live_servers, live_channels, live_dm_mode, .. } = nav.clone();
    let active_channel: Signal<u32> = signal(SERVERS[0].first_text_channel().id);
    // --- layout: shared app-shell state (signals built in shell_state.rs) ---
    let layout = Layout::new();
    let Layout { sidebar_width, member_panel_width, collapsed, .. } = layout.clone();
    // --- composer: shared app-shell state (signals built in shell_state.rs) ---
    let composer_state = Composer::new();
    // --- messages: shared app-shell state (signals built in shell_state.rs) ---
    let messages = Messages::new();
    let Messages { live_revision, reactions, reaction_rev, picker, live_at_bottom, live_scroll_handle, live_unread_rev, rail_unread, pinned_panel_open, pinned_messages, pinned_count, pinned_revision, pinned_error, pin_visible, .. } = messages.clone();
    // --- overlays: shared app-shell state (signals built in shell_state.rs) ---
    let overlays = Overlays::new();
    let Overlays { notif_toast_body, notif_toast_title, notif_toast_visible, .. } = overlays.clone();
    // Bumped on every send. `switch` dedups on its scrutinee VALUE (equal
    // re-fire keeps the mounted arm), so re-setting `active_channel` to its
    // current value does NOT rebuild the message list — this counter does.
    let revision: Signal<u32> = signal(0);

    crate::mentions::install(live_revision);

    // The maximum scroll offset the live pane has reported, kept so the
    // `on_scroll` max-offset heuristic can tell a scroll UP (away from the
    // bottom) from a scroll DOWN (back to the live tail). An offset-only
    // signal — `on_scroll` only reports the absolute `scrollTop`, never the
    // content height (that is `on_end_reached`'s job), so "scrolled up from
    // the bottom" is detected as `y < max_seen - threshold`, not as
    // `y > content - viewport`.
    let _live_max_offset: Rc<RefCell<f32>> = Rc::new(RefCell::new(0.0));
    // --- dm: shared app-shell state (signals built in shell_state.rs) ---
    let dm = Dm::new();
    let Dm { live_dms, live_dm_active, live_dm_messages, live_dm_revision, friends_snap, friends_add_username, friends_add_error, friends_open, blocked_users, .. } = dm.clone();
    // --- members: shared app-shell state (signals built in shell_state.rs) ---
    let members = Members::new();
    let Members { live_members, live_member_error, live_kick_confirm, live_ban_confirm, live_timeout_confirm, members_pref, .. } = members.clone();
    crate::blocks::load(blocked_users, live_revision);
    {
        let s = active_server.get();
        if let Some(server) = SERVERS.iter().find(|x| x.id == s) {
            for cat in server.categories {
                collapsed.borrow_mut().entry(cat.id).or_insert_with(|| signal(false));
            }
        }
    }
    runtime_core::spawn_then(async move { crate::servers::list_servers().await }, move |result| {
        match result {
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
        runtime_core::driver::spawn_async(async move {
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
                crate::mentions::members_loaded(sid, &members);
                lmbrs.set(members);
            }
            crate::mentions::load_roles(sid);
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




    // Direct Messages are LIVE only (see the `live_dm_*` state below); the mock
    // DM screen and its state were removed with it.

    // Reaction state (`reactions` / `reaction_rev` / `picker`) is declared up
    // with the live state before `gateway_incoming_effect` — see that block.


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
                    ..rounded(8.0)
                },
                test_id = "friends-entry",
            ) {
                icon(USERS).color(|| theme::live(TEXT_MUTED)).size(18.0)
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
                        mobile::show(mobile::Pane::Chat);
                    }
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
        let mode = live_dm_mode.clone();
        let list = live_dms.clone();
        let act = live_dm_active.clone();
        let buf = live_dm_messages.clone();
        let rev = live_dm_revision.clone();
        runtime_core::driver::spawn_async(async move {
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



    // --- dialogs: shared app-shell state (signals built in shell_state.rs) ---
    let dialogs = Dialogs::new();



    // Everything the section builders below read, gathered once.
    let s = ShellState {
        dialogs: dialogs.clone(),
        dm: dm.clone(),
        overlays: overlays.clone(),
        members: members.clone(),
        messages: messages.clone(),
        composer_state: composer_state.clone(),
        layout: layout.clone(),
        nav: nav.clone(),
        authed: authed.clone(),
        friends_entry: friends_entry.clone(),
        gateway: gateway.clone(),
        on_dm_message: on_dm_message.clone(),
    };
    // Before any section builds its rows: they read `popups::msg_menu`.
    let popups::PopupsParts { catcher_el: popup_catcher_el, exclusive_el: popup_exclusive_el } = popups::build(&s);

    let gateway_effects::GatewayEffectsParts { gateway_incoming_effect, mention_reset_effect } = gateway_effects::build(&s);
    let rail::RailParts { rail_style, rail_top, live_section, rail_actions } = rail::build(&s);
    let modals::ModalsParts { join_el, create_el, cc_el, rc_el, dc_el, invite_el, ss_el } = modals::build(&s);
    let user_popups::UserPopupsParts { user_popup_el } = user_popups::build(&s);
    let sidebar::SidebarParts { sidebar } = sidebar::build(&s);
    let messages::MessagesParts { header, sent_messages, typing_row } = messages::build(&s);
    let composer::ComposerParts { composer_area } = composer::build(&s);
    let member_list::MemberListParts { members_switch } = member_list::build(&s);

    // On a phone the chat column gives way to the channel list (see `mobile`).
    let main_style = move || StyleRules {
        flex_grow: Some(grow(if mobile::hidden(mobile::Pane::Chat) { 0.0 } else { 1.0 })),
        width: mobile::hidden(mobile::Pane::Chat).then(|| px(0.0)),
        background: Some(col(BG_MAIN)),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    let overlays::OverlaysParts { profile_region, notif_region, shortcuts_region, live_dm_region, moderation_region, live_invite_region } = overlays::build(&s);

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
    jump::install(msg_scroll_ref, msg_content_ref);
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
    // restores a scroll position). A dropped `after_animation_frame` handle
    // cancels its callback, so this is a detached 50 ms timer.
    after_ms_detached(50, {
        let r = msg_recompute_jump.clone();
        move || r(0.0)
    });
    // The floating pill. `msg_show_jump` is driven by the scroll tracking above
    // (only `true` while the reader is away from the newest messages). `on_jump`
    // scrolls the message list back to the bottom — `ScrollViewHandle` has no
    // `scroll_to_bottom`, but the web backend's `scroll_to` clamps `scrollTop` to
    // the scrollable range, so a large `y` lands exactly on the last message.
    // Follow the newest message: after each list update, scroll to the
    // bottom when the reader was already there (the jump pill hidden), when
    // they just sent something (`request_scroll_to_latest`), or when the
    // channel changed. A reader scrolled up into history stays put.
    let follow_el = {
        let scroll = msg_scroll_ref.clone();
        let away = msg_show_jump;
        let last_channel: Rc<Cell<Option<(Option<i64>, u32)>>> = Rc::new(Cell::new(None));
        switch(
            move || (live_revision.get(), revision.get(), active_live_channel.get(), active_channel.get()),
            move |(_, _, live_cid, mock_cid): &(u32, u32, Option<i64>, u32)| {
                let channel = (*live_cid, *mock_cid);
                let switched = last_channel.replace(Some(channel)) != Some(channel);
                let forced = FOLLOW_LATEST.with(|f| f.replace(false));
                // Detached timers: a dropped `after_animation_frame` handle
                // cancels its callback. The first pass runs once the rebuilt
                // list has laid out; a channel open or a send gets a second
                // pass for rows that settle late.
                // A jump to a search result does its own scrolling.
                if jump::target().is_none() && (switched || forced || !away.get()) {
                    let passes: &[i32] = if switched || forced { &[30, 300] } else { &[30] };
                    for &ms in passes {
                        let scroll = scroll.clone();
                        runtime_core::after_ms_detached(ms, move || {
                            scroll.with(|h| h.scroll_to(0.0, 1_000_000.0));
                        });
                    }
                }
                ui! { view(style = StyleRules { width: Some(px(0.0)), height: Some(px(0.0)), ..Default::default() }) {} }
            },
        )
    };
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
            // The rail and channel list; on a phone they fill the window
            // while the list pane shows and fold away while the chat does.
            view(style = move || StyleRules {
                flex_direction: Some(FlexDirection::Row),
                flex_shrink: Some(grow(0.0)),
                flex_grow: Some(grow(if mobile::is_phone() && !mobile::hidden(mobile::Pane::List) { 1.0 } else { 0.0 })),
                width: mobile::hidden(mobile::Pane::List).then(|| px(0.0)),
                overflow: Some(Overflow::Hidden),
                ..Default::default()
            }) {
                view(style = rail_style) { rail_top live_section rail_actions gateway_incoming_effect mention_reset_effect }
                sidebar
                // Drag handle on the channel sidebar's right edge: dragging it
                // left/right resizes `sidebar_width` (right = grow, so sign `+1`).
                resize_handle(sidebar_width, 1.0, "resize-sidebar")
            }
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
                    // No visible scrollbar (Discord-style); the wheel,
                    // trackpad and keyboard still scroll it.
                    scroll_view(style = StyleRules {
                        flex_grow: Some(grow(1.0)),
                        scrollbar: Some(runtime_core::ScrollbarVisibility::Hidden),
                        ..Default::default()
                    }) {
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
                    follow_el
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
            // The member list and its drag handle, collapsed to nothing while
            // hidden (the header toggle, or automatically on narrow windows).
            // On a phone the list slides over the chat from the right, with
            // a dimmed strip to its left that closes it.
            view(style = move || {
                let shown = members_visible(members_pref);
                let phone = mobile::is_phone();
                StyleRules {
                    flex_direction: Some(FlexDirection::Row),
                    width: if !shown { Some(px(0.0)) } else if phone { Some(pct(100.0)) } else { None },
                    height: phone.then(|| pct(100.0)),
                    position: phone.then_some(Position::Absolute),
                    top: phone.then(|| px(0.0)),
                    right: phone.then(|| px(0.0)),
                    background: phone.then(|| col(theme::OVERLAY)),
                    overflow: Some(Overflow::Hidden),
                    ..Default::default()
                }
            }) {
                view(style = move || StyleRules {
                    flex_grow: Some(grow(if mobile::is_phone() { 1.0 } else { 0.0 })),
                    ..Default::default()
                }, test_id = "members-scrim") {}
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            members_pref.set(None);
                        }
                        TouchResponse::CONSUMED
                    })
                resize_handle(member_panel_width, -1.0, "resize-members")
                members_switch
            }
            // Painted last so it sits on top of everything when open.
            profile_region
            notif_region
            shortcuts_region
            // Direct Messages (LIVE / backend): full-screen, same paint-last
            // idiom; an independent entry point (the mock DM screen is gone).
            live_dm_region
            // "Join a Server" overlay: full-screen, painted last so it sits on
            // top when open (collapses to zero size when closed).
            join_el
            // "Create a Server" overlay: same full-screen, paint-last idiom.
            create_el
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
            // Click-outside layer for open dropdowns, and the one-popup-at-a-
            // time rule (see `popups`).
            popup_catcher_el
            popup_exclusive_el
            // Profile card / user context menu: last, so it floats above
            // everything else.
            user_popup_el
        }
    }
}

thread_local! {
    /// Set by a send; the message list's follow effect scrolls to the newest
    /// message on its next update even if the reader had scrolled up.
    static FOLLOW_LATEST: Cell<bool> = const { Cell::new(false) };
}

/// Scroll the message list to the newest message on its next update.
pub(crate) fn request_scroll_to_latest() {
    FOLLOW_LATEST.with(|f| f.set(true));
}

/// Below this window width the member list starts hidden.
const MEMBERS_AUTO_MIN_WIDTH: f32 = 1200.0;

/// Whether the member list shows: the header toggle's choice, or, until the
/// toggle is used, whether the window is wide enough.
pub(super) fn members_visible(pref: Signal<Option<bool>>) -> bool {
    pref.get().unwrap_or_else(|| viewport_size().get().width >= MEMBERS_AUTO_MIN_WIDTH)
}
