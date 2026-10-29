//! The signed-in user's notification settings per server and channel
//! (fetched from the server, changed with [`set`]), and whether a message
//! notifies: a muted channel or server never does; otherwise the channel's
//! level applies, else its server's, else the default from User Settings.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use discord_server::notification_settings::{self as ns, Level, NotificationSetting, Scope};
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
    runtime_core::driver::spawn_async(async move {
        let input = ns::NotificationSettingsInput { server_id: None };
        if let Ok(rows) = ns::notification_settings(input).await {
            ROWS.with(|r| *r.borrow_mut() = rows.into_iter().map(|s| ((s.scope, s.scope_id), s)).collect());
            bump();
        }
    });
}

/// The setting for one server or channel (inherit / not muted when unset).
pub(crate) fn get(scope: Scope, id: i64) -> NotificationSetting {
    revision();
    ROWS.with(|r| r.borrow().get(&(scope, id)).copied())
        .unwrap_or(NotificationSetting { scope, scope_id: id, level: None, muted_until: None })
}

/// Whether a server or channel is muted right now.
pub(crate) fn muted(scope: Scope, id: i64) -> bool {
    ns::is_muted(get(scope, id).muted_until, now_ms())
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
    runtime_core::driver::spawn_async(async move {
        if ns::set_notification_setting(s).await.is_err() {
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
