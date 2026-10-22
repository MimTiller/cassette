//! `notify` — the notification engine behind the "Notification Settings" screen.
//!
//! It is the glue between the settings picker's level (0 = all, 1 = mentions,
//! 2 = off) and the two delivery channels the app actually has:
//!
//! 1. the **in-app toast** (`crate::notification_toast`), shown by setting the
//!    caller-supplied `toast_visible` signal and auto-dismissing a beat later;
//! 2. **browser notifications** via the W3C `Notification` API (wasm/web only),
//!    gated on the `Notification.permission` grant. The app requests permission
//!    the first time a level-0/1 choice is made (see [`request_permission`]),
//!    so a granted permission makes real browser toasts appear alongside the
//!    in-app one.
//!
//! The picker's level is persisted to `localStorage` (web) / ignored (native)
//! so it survives a reload — the same "local preference" pattern the app uses
//! for other client-only state.
//!
//! Idioms: the wasm half is gated `#[cfg(target_arch = "wasm32")]` so the native
//! build never touches `web-sys`; the async permission path uses
//! `wasm_bindgen_futures::JsFuture` (the codebase's standard way to await a JS
//! `Promise`).

/// The three notification levels the picker exposes, matching the `Signal<u32>`
/// the settings screen holds (`0` = all messages, `1` = mentions only, `2` = off).
pub const ALL_MESSAGES: u32 = 0;
pub const MENTIONS_ONLY: u32 = 1;
pub const NOTHING: u32 = 2;

/// `localStorage` keys (namespaced so they can't collide with the backend).
const KEY_MODE: &str = "idealyst.discord.notifMode";
const KEY_ACCENT: &str = "idealyst.discord.accent";

// ── browser notification (wasm/web only) ─────────────────────────────────────

#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::JsFuture;
#[cfg(target_arch = "wasm32")]
use web_sys::{Notification, NotificationOptions};

/// Fire a notification for an inbound message, per the current level.
///
/// `is_mention` is true when the frame's `mentioned_user_ids` contains the
/// signed-in user. The in-app toast (driven by the caller's `visible` /
/// `title` / `body` signals) is shown for **any** match at level
/// `ALL_MESSAGES` or `MENTIONS_ONLY` (a mention always matches, since it is
/// also a message); level `NOTHING` shows nothing. Browser notifications are a
/// bonus that only fire when `Notification.permission` is granted.
///
/// Synchronous: the in-app toast flips three signals + arms a timer, and the
/// W3C `Notification` constructor is itself synchronous — so the gateway effect
/// (a sync `switch` arm) can call this directly without an async hop.
/// Permission is a separate, async, opt-in step (see [`request_permission`]).
pub fn notify_message(
    author: &str,
    content: &str,
    is_mention: bool,
    mode: u32,
    visible: runtime_core::Signal<bool>,
    title: runtime_core::Signal<String>,
    body: runtime_core::Signal<String>,
) {
    if mode == NOTHING {
        return;
    }
    if mode == MENTIONS_ONLY && !is_mention {
        return;
    }

    // The toast copy: the title names the author (+ a "@you" badge on a
    // mention — the one thing that distinguishes a mention from a plain
    // message at a glance); the body is the message, truncated to 80 chars.
    let title_text = if is_mention {
        format!("{author} @you")
    } else {
        author.to_string()
    };
    let body_text: String = {
        let mut b = content.to_string();
        if b.chars().count() > 80 {
            b.truncate(80);
        }
        b
    };

    // In-app toast: always available (no permission needed), auto-dismisses.
    show_toast(&title_text, &body_text, visible, title, body);

    // Browser notification: only if the user granted permission (web). The
    // constructor is synchronous; `permission()` is a cheap sync read.
    #[cfg(target_arch = "wasm32")]
    {
        let perm = Notification::permission();
        if perm == web_sys::NotificationPermission::Granted {
            let mut opts = NotificationOptions::new();
            opts.body(&body_text);
            let _ = Notification::new_with_options(&title_text, &opts);
        }
    }
}

/// Show the in-app toast for ~4.5 s, then clear it. Sets the title / body copy
/// (so the toast shows THIS message), flips `visible` on, and arms a detached
/// auto-dismiss timer (the `pin_confirm_toast` idiom). The toast's own "x" is
/// the manual dismiss.
pub fn show_toast(
    title: &str,
    body: &str,
    visible: runtime_core::Signal<bool>,
    title_sig: runtime_core::Signal<String>,
    body_sig: runtime_core::Signal<String>,
) {
    title_sig.set(title.to_string());
    body_sig.set(body.to_string());
    visible.set(true);
    let d = visible;
    runtime_core::after_ms_detached(4_500, move || d.set(false));
}

/// Request the browser `Notification` permission (no-op on native). Called when
/// the user picks a notifying level, so browser toasts start working without a
/// separate "enable" step. Returns nothing — the permission is read lazily by
/// [`notify_message`] on each fire.
pub async fn request_permission() {
    #[cfg(target_arch = "wasm32")]
    {
        let perm = Notification::permission();
        if perm == web_sys::NotificationPermission::Default {
            if let Ok(promise) = Notification::request_permission() {
                let _ = JsFuture::from(promise).await;
            }
        }
    }
}

// ── persistence (storage SDK: localStorage on web, native store elsewhere)
// ─────────────────────────────────────────────────────────────────────────────

/// Read the stored notification level (0..=2). `None` when there is no stored
/// value (first run) or the storage is unavailable. Rides the shared
/// `discord-clone` storage-SDK store (see `crate::auth::STORE_NS`), not raw
/// web_sys localStorage — so the preference persists on native targets too.
pub fn load_mode() -> Option<u32> {
    crate::auth::store()
        .get_now(KEY_MODE)
        .ok()
        .flatten()
        .and_then(|v| v.trim().parse::<u32>().ok())
}

/// Read the stored accent-color hex (e.g. `"#5865f2"`), or `None` if unset.
pub fn load_accent() -> Option<String> {
    crate::auth::store().get_now(KEY_ACCENT).ok().flatten()
}

/// Store the notification level. Best-effort: a full/absent store is ignored.
pub fn save_mode(mode: u32) {
    let _ = crate::auth::store().set_now(KEY_MODE, &mode.to_string());
}

/// Store the accent-color hex. Best-effort, same as [`save_mode`].
pub fn save_accent(hex: &str) {
    let _ = crate::auth::store().set_now(KEY_ACCENT, hex);
}
