//! Robot functions the E2E suite (`tests/robot.rs`) reads app state through,
//! for state no element shows: the colour theme, the message a search result
//! jumped to, and the chosen app icon. They are read-only, and like all robot
//! functions they do nothing outside an `idealyst dev` / `idealyst test` build.

/// The active colour theme's key (`"dark"`, `"ash"`, ...).
#[runtime_core::robot_fn(name = "cassette.theme")]
fn theme_key() -> Result<String, String> {
    Ok(crate::theme::current().key().to_string())
}

/// The id of the message a jump (search result, reply, pin) is highlighting.
#[runtime_core::robot_fn(name = "cassette.jump_target")]
fn jump_target() -> Result<Option<i64>, String> {
    Ok(crate::app::jump_target())
}

/// The chosen app icon's key.
#[runtime_core::robot_fn(name = "cassette.app_icon")]
fn app_icon() -> Result<String, String> {
    Ok(crate::app_icon::current().key().to_string())
}

/// Re-enter the active server the way a page reload does: fetch its roles and
/// the signed-in user's permissions, arming the dropdown's permission rows.
#[runtime_core::robot_fn(name = "cassette.rearm_perms")]
fn rearm_perms() -> Result<(), String> {
    match crate::mentions::server_id() {
        Some(sid) => {
            crate::mentions::load_roles(sid);
            Ok(())
        }
        None => Err("no active server".to_string()),
    }
}

/// Register the functions; called once from `app()`.
pub(crate) fn register() {
    runtime_vocabulary::robot_fn::register(theme_key::robot());
    runtime_vocabulary::robot_fn::register(jump_target::robot());
    runtime_vocabulary::robot_fn::register(app_icon::robot());
    runtime_vocabulary::robot_fn::register(rearm_perms::robot());
}
