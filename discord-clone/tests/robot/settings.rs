//! Server Settings smoke: Overview/Roles/Members sections, create a role in
//! a colour, give it to yourself from your profile card, and your name takes
//! the role colour; then User Settings and switch theme.
//!
//! The Python suite read computed CSS colours; the robot cannot, so the swatch
//! pick is a `role-swatch-pink` test_id, the role's listed colour and your
//! name's colour are read through the `cassette.role_named` /
//! `cassette.name_color` hooks (src/robot_hooks.rs), and the theme through
//! `cassette.theme` — a colour no element shows, exposed as app state.
//! Settings sidebar rows carry `settings-nav-<section>` test_ids and the card
//! "+ role" chip a `card-role-give` test_id.
//!
//! Not ported: the Python "no uncaught page exceptions" check — a JS page
//! error never reaches the robot's log (same gap as `server.rs`).

use robot_test::{json, robot_test, App, Value};

use crate::support::{fresh_name, known_bug, sign_up, signed_out};
use crate::support::WAIT;

/// The suite screenshots the moment it names; matches the Python file names.
fn shot(app: &mut App, label: &str) {
    let path = app.screenshot();
    assert!(path.ends_with(".png"), "screenshot was not saved: {path}");
    eprintln!("[shot] {label}: {path}");
}

#[robot_test]
fn the_server_settings_sections_switch(app: &mut App) {
    signed_out(app);
    let user = fresh_name("setsec");
    sign_up(app, &user, &format!("pass-{user}"));
    app.text(&user).assert_visible();

    create_server(app, fresh_name("Settings Crew "));
    select_general(app);
    shot(app, "settings1-shell");

    open_server_settings(app);

    // Overview is the section the panel opens on: the rename and icon rows.
    // (Their labels duplicate a hidden mock's texts, so the rows are checked
    // through their buttons' test_ids.)
    app.test_id("server-rename-save").assert_visible();
    app.test_id("server-icon-save").assert_visible();
    shot(app, "settings2-overview");

    app.test_id("settings-nav-roles").click();
    app.test_id("role-create-name-input").assert_visible();
    shot(app, "settings3-roles");

    app.test_id("settings-nav-members").click();
    app.text("Add a member").assert_visible();
    shot(app, "settings4-members");
}

#[robot_test]
fn create_a_role_and_wear_it_from_the_card(app: &mut App) {
    signed_out(app);
    let user = fresh_name("setrole");
    sign_up(app, &user, &format!("pass-{user}"));

    create_server(app, fresh_name("Role Crew "));
    select_general(app);
    let msg = fresh_name("settings crew hello ");
    app.test_id("composer").fill(&msg);
    app.test_id("composer").press_key("Enter");
    app.text(&msg).assert_visible();
    shot(app, "settings1-shell");

    open_server_settings(app);
    app.test_id("settings-nav-roles").click();

    let role = fresh_name("NightWatch");
    app.test_id("role-create-name-input").fill(&role);
    app.test_id("role-swatch-pink").click();
    app.test_id("create-role").click();
    app.text(&role).assert_visible();

    // The role keeps the colour of the swatch that was picked (#e91e63).
    let listed = robot_string(app.call_function("cassette.role_named", json!({ "name": role })));
    assert_eq!(listed, "#e91e63", "the new role is listed in its colour (got {listed})");
    shot(app, "settings5-role-created");

    // Give it to yourself from your own profile card, opened from your
    // message-row name (the robot has no right-click: the card comes from a
    // plain click on the author text).
    app.press_key("Escape");
    app.test_id("msg-author-name").click();
    app.text("Member since").assert_visible();
    app.test_id("card-role-give").click();
    shot(app, "settings6-card-role");
    app.press_key("Escape");

    // Your message-row name now draws in the role colour.
    let name_col = robot_string(app.call_function("cassette.name_color", json!({ "username": user })));
    assert_eq!(name_col, "#e91e63", "my name takes the role colour (got {name_col})");
}

#[robot_test]
fn switching_the_theme_recolours_the_app(app: &mut App) {
    signed_out(app);
    let user = fresh_name("settheme");
    sign_up(app, &user, &format!("pass-{user}"));

    open_user_settings(app);
    shot(app, "settings7-user-myaccount");
    let before = robot_string(app.call_function("cassette.theme", json!({})));

    app.test_id("settings-nav-appearance").click();
    shot(app, "settings8-appearance");

    app.test_id("theme-card-light").click();
    let mut after = String::new();
    let changed = crate::support::eventually(10, || {
        after = robot_string(app.call_function("cassette.theme", json!({})));
        after != before
    });
    assert!(changed, "picking a theme should change the theme (before {before}, after {after})");
    shot(app, "settings9-theme-switched");
}

// ---- shared steps for this module -----------------------------------------

/// Open the fresh server's dropdown, re-arming permissions through the robot
/// hook when the create path left them unfetched (the known bug, same as
/// `server.rs`: the create path never fetches the permission snapshot), then
/// press the Server Settings row, leaving the panel up on Overview.
fn open_server_settings(app: &mut App) {
    app.set_timeout(WAIT);
    // A phone may sit in the Chat pane (select_general opened it); the
    // sidebar header with the dropdown lives in the List pane, so close it.
    if app.test_id("mobile-back").is_visible() {
        app.test_id("mobile-back").click();
    }
    app.test_id("server-header").click();
    if !app.test_id("menu-server-settings").is_visible() {
        known_bug(
            "the fresh server arms its permissions right away",
            false,
            "the dropdown lists no permission rows until a reload",
        );
        app.call_function("cassette.rearm_perms", json!({}));
    }
    app.test_id("menu-server-settings").assert_visible();
    app.test_id("menu-server-settings").click();
    // The panel opens on Overview: its sidebar row is live.
    app.test_id("settings-nav-overview").assert_visible();
    app.set_timeout(WAIT);
}

/// Open the rail's create-server modal, name it, press Create, and wait for
/// the sidebar header to show the new server.
fn create_server(app: &mut App, name: String) {
    app.test_id("rail-create-server").click();
    app.text("Add a Server").assert_visible();
    app.test_id("create-server-name-input").fill(&name);
    app.test_id("create-server-create").click();
    app.test_id("server-header").assert_visible();
    app.text(&name).assert_visible();
}

/// Select the fresh server's default `general` channel (the create path seeds
/// it) so the message pane and member rail are live on it.
fn select_general(app: &mut App) {
    app.text("general").click();
    app.set_timeout(WAIT);
}

/// Open the full-screen User Settings from the user panel's gear button.
fn open_user_settings(app: &mut App) {
    app.test_id("user-settings").click();
    app.text("USER SETTINGS").assert_visible();
    app.set_timeout(WAIT);
}

/// A robot function's string reply: the bare string or an `Ok`-wrapped one.
fn robot_string(v: Value) -> String {
    if let Some(s) = v.as_str() {
        return s.to_string();
    }
    v.get("Ok").and_then(|v| v.as_str()).map(str::to_string).unwrap_or_default()
}
