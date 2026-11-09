//! The signed-in user's notification settings per server and channel
//! (fetched from the server, changed with [`set`]), and whether a message
//! notifies: a muted channel or server never does; otherwise the channel's
//! level applies, else its server's, else the default from User Settings.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use discord_server::notification_settings::{Level, NotificationSetting, Scope, is_muted};
use runtime_core::Signal;

thread_local! {
    static ROWS: RefCell<HashMap<(Scope, i64), NotificationSetting>> = RefCell::new(HashMap::new());
    static REV: Cell<Option<Signal<u32>>> = const { Cell::new(None) };
}

/// Set up the change signal (once, from `app_shell`).
pub(crate) fn install() {
    REV.with(|r| r.set(Some(runtime_core::signal(0))));
}

fn bump() {
    if let Some(rev) = REV.with(|r| r.get()) {
        rev.set(rev.get() + 1);
    }
}

/// Read the change signal, so a reactive closure that calls this redraws
/// when settings change.
pub(crate) fn revision() -> u32 {
    REV.with(|r| r.get()).map(|rev| rev.get()).unwrap_or(0)
}

fn now_ms() -> i64 {
    runtime_shared::time::epoch_millis()
}

/// Fetch all of the user's settings.
pub(crate) fn load() {
    // The RPC runs in the future; the row table and the revision bump land
    // in the callback, anchored on the app shell that owns REV.
    runtime_core::spawn_then_in(
        &crate::app::shell_alive(),
        crate::backend::notification_settings(None),
        move |result| {
            if let Ok(rows) = result {
                ROWS.with(|r| *r.borrow_mut() = rows.into_iter().map(|s| ((s.scope, s.scope_id), s)).collect());
                bump();
            }
        },
    );
}

/// The setting for one server or channel (inherit / not muted when unset).
pub(crate) fn get(scope: Scope, id: i64) -> NotificationSetting {
    revision();
    ROWS.with(|r| r.borrow().get(&(scope, id)).copied())
        .unwrap_or(NotificationSetting { scope, scope_id: id, level: None, muted_until: None })
}

/// Whether a server or channel is muted right now.
pub(crate) fn muted(scope: Scope, id: i64) -> bool {
    is_muted(get(scope, id).muted_until, now_ms())
}

/// Save a setting: shown at once, then sent; a refusal reloads them all.
pub(crate) fn set(s: NotificationSetting) {
    ROWS.with(|r| {
        let mut r = r.borrow_mut();
        if s.level.is_none() && s.muted_until.is_none() {
            r.remove(&(s.scope, s.scope_id));
        } else {
            r.insert((s.scope, s.scope_id), s);
        }
    });
    bump();
    // Detached on purpose: the save RPC's result is never put on screen
    // (the row was already shown optimistically above); it only reloads
    // everything on a refusal, and that reload re-anchors through
    // `load()` itself, so nothing here must run inside this caller's scope.
    runtime_core::driver::spawn_detached(async move {
        if crate::backend::set_notification_setting(s).await.is_err() {
            load();
        }
    });
}

/// The notification mode (`notify::ALL_MESSAGES` / `MENTIONS_ONLY` /
/// `NOTHING`) for a message in `channel_id` of `server_id`, given the
/// User Settings default.
pub(crate) fn mode_for(server_id: i64, channel_id: i64, default_mode: u32) -> u32 {
    if muted(Scope::Server, server_id) || muted(Scope::Channel, channel_id) {
        return crate::notify::NOTHING;
    }
    let level = get(Scope::Channel, channel_id).level.or(get(Scope::Server, server_id).level);
    match level {
        Some(Level::All) => crate::notify::ALL_MESSAGES,
        Some(Level::Mentions) => crate::notify::MENTIONS_ONLY,
        Some(Level::Nothing) => crate::notify::NOTHING,
        None => default_mode,
    }
}
