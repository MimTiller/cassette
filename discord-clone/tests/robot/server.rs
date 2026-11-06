//! Server smoke test: create a server, create a text channel, send a message
//! and see it appear; edit and delete it. All data is fresh (tagged names).
//!
//! The Python suite hovered a message row and clicked the '...' pill with
//! viewport boxes to stay on one row; the robot hovers the message's own row
//! (`msg-row`) and presses the same pill by `test_id`.
//!
//! Known app bug (see e2e/ISSUES.md): the create path never fetches the new
//! server's permission snapshot, so its dropdown hides the permission rows
//! until a page reload boots the app again. The check stays a `known_bug`
//! and `cassette.rearm_perms` (robot_hooks.rs) re-arms them for the rest of
//! the flow — the robot has no page reload of its own.
//!
//! Not ported: the Python "no uncaught page exceptions" check. A JS page
//! error never reaches the robot's log (Rust's eprintln does, and nothing
//! logs on a clean run), so there is no way to read page errors — a robot
//! gap, recorded here instead.

use robot_test::{json, robot_test, App};

use crate::support::{Account, fresh_name, known_bug, sign_in_as, sign_up, signed_out};
use crate::support::WAIT;

#[robot_test]
fn create_a_server_shows_it_in_the_sidebar(app: &mut App) {
    signed_out(app);
    let user = fresh_name("srvu");
    sign_up(app, &user, &format!("pass-{user}"));
    app.text(&user).assert_visible();

    let server = fresh_name("Smoke Server ");
    create_server(app, &server);
    // The name lands in the sidebar header once the server enters.
    app.test_id("server-header").assert_visible();
    app.text(&server).assert_visible();
    let shot = app.screenshot();
    assert!(shot.ends_with(".png"), "screenshot was not saved: {shot}");
    eprintln!("[shot] create server: {shot}");
}

#[robot_test]
fn the_server_rail_stays_beside_the_dms_and_switches_back(app: &mut App) {
    signed_out(app);
    let account = Account::new("railsw");
    sign_in_as(app, &account);
    let server = fresh_name("Rail Server ");
    create_server(app, &server);
    app.test_id("server-header").assert_visible();

    app.test_id("rail-direct-messages").click();
    app.test_id("dm-done-pill").assert_visible();
    // The rail is still there beside the DM screen, as in Discord.
    app.test_id("rail-server").assert_visible();
    let shot = app.screenshot();
    eprintln!("[shot] rail beside dms: {shot}");

    // Its server tile takes you straight back to the server.
    app.test_id("rail-server").click();
    app.set_timeout(WAIT);
    assert!(
        crate::support::eventually(10, || !app.test_id("dm-done-pill").is_visible()),
        "the DM screen stays open after picking a server from the rail"
    );
    app.test_id("server-header").assert_visible();
    app.text(&server).assert_visible();
    let shot = app.screenshot();
    eprintln!("[shot] back on the server: {shot}");
}

#[robot_test]
fn the_owners_menu_offers_create_channel(app: &mut App) {
    signed_out(app);
    let account = Account::new("srvperm");
    sign_in_as(app, &account);
    let server = fresh_name("Arming Server ");
    create_server(app, &server);

    app.set_timeout(WAIT);
    app.test_id("server-header").click();
    let armed = app.test_id("menu-create-channel").is_visible();
    known_bug(
        "the fresh server arms its permissions right away",
        armed,
        "the dropdown lists no permission rows until a reload",
    );
    if !armed {
        app.call_function("cassette.rearm_perms", json!({}));
    }
    app.test_id("menu-create-channel").assert_visible();
    app.test_id("menu-create-channel").click();
    app.text("New Channel").assert_visible();
    let shot = app.screenshot();
    assert!(shot.ends_with(".png"), "screenshot was not saved: {shot}");
    eprintln!("[shot] create channel modal: {shot}");
}

#[robot_test]
fn create_a_channel_shows_it_in_the_sidebar(app: &mut App) {
    signed_out(app);
    let account = Account::new("srvchan");
    sign_in_as(app, &account);
    let server = fresh_name("Channel Server ");
    create_server(app, &server);
    open_menu_with_perms(app);

    let channel = fresh_name("smokechan");
    app.test_id("create-channel-name-input").fill(&channel);
    app.test_id("create-channel-create").click();
    // The modal closes on a successful create, so check the sidebar row
    // rather than the input's value.
    app.text(&channel).assert_visible();
    let shot = app.screenshot();
    assert!(shot.ends_with(".png"), "screenshot was not saved: {shot}");
    eprintln!("[shot] channel in sidebar: {shot}");
}

#[robot_test]
fn send_a_message_and_see_it_appear(app: &mut App) {
    signed_out(app);
    let account = Account::new("srvsend");
    sign_in_as(app, &account);
    enter_fresh_server(app, "smokechan");

    let msg = fresh_name("hello ");
    app.test_id("composer").fill(&msg);
    app.test_id("composer").press_key("Enter");
    app.text(&msg).assert_visible();
    let shot = app.screenshot();
    assert!(shot.ends_with(".png"), "screenshot was not saved: {shot}");
    eprintln!("[shot] sent message: {shot}");
    // The composer clears after sending.
    app.test_id("composer").assert_value("");
}

#[robot_test]
fn edit_shows_the_new_text_with_the_edited_marker(app: &mut App) {
    signed_out(app);
    let account = Account::new("srvedit");
    sign_in_as(app, &account);
    enter_fresh_server(app, "smokechan");

    let msg = fresh_name("hello ");
    let edited = fresh_name("edited ");
    app.test_id("composer").fill(&msg);
    app.test_id("composer").press_key("Enter");
    app.text(&msg).assert_visible();

    // Hover the row so its action bar mounts, press the "more" pill, then
    // take the menu's Edit row (the owner-only entry for this message).
    hover_more_pill(app, &msg);
    app.test_id("menu-edit-message").click();

    app.test_id("edit-input").fill(&edited);
    app.test_id("edit-input").press_key("Enter");
    app.text(&edited).assert_visible();
    app.text("(edited)").assert_visible();
    let shot = app.screenshot();
    assert!(shot.ends_with(".png"), "screenshot was not saved: {shot}");
    eprintln!("[shot] edited message: {shot}");
}

#[robot_test]
fn delete_removes_the_message(app: &mut App) {
    signed_out(app);
    let account = Account::new("srvdel");
    sign_in_as(app, &account);
    enter_fresh_server(app, "smokechan");

    let msg = fresh_name("hello ");
    app.test_id("composer").fill(&msg);
    app.test_id("composer").press_key("Enter");
    app.text(&msg).assert_visible();

    hover_more_pill(app, &msg);
    app.test_id("menu-delete-message").click();

    // The confirm strip: "Delete this message?" then the Confirm button.
    app.text("Delete this message?").assert_visible();
    let shot = app.screenshot();
    assert!(shot.ends_with(".png"), "screenshot was not saved: {shot}");
    eprintln!("[shot] confirm strip: {shot}");
    app.test_id("confirm-delete").click();
    app.text(&msg).assert_hidden();
    let shot = app.screenshot();
    assert!(shot.ends_with(".png"), "screenshot was not saved: {shot}");
    eprintln!("[shot] message gone: {shot}");
}

// ---- shared steps for this module -----------------------------------------

/// Open the rail's create-server modal, name it, press Create, and wait for
/// the sidebar header to show the new server.
fn create_server(app: &mut App, name: &str) {
    app.test_id("rail-create-server").click();
    app.text("Add a Server").assert_visible();
    app.test_id("create-server-name-input").fill(name);
    app.test_id("create-server-create").click();
    app.test_id("server-header").assert_visible();
    app.text(name).assert_visible();
}

/// Open the fresh server's dropdown, re-arming permissions through the robot
/// hook if the create path left them unfetched (the known bug), and leave the
/// New Channel modal up.
fn open_menu_with_perms(app: &mut App) {
    app.set_timeout(WAIT);
    app.test_id("server-header").click();
    if !app.test_id("menu-create-channel").is_visible() {
        app.call_function("cassette.rearm_perms", json!({}));
    }
    app.test_id("menu-create-channel").assert_visible();
    app.test_id("menu-create-channel").click();
    app.text("New Channel").assert_visible();
    app.set_timeout(WAIT);
}

/// A fresh server with a fresh text channel of `prefix`, ready for message
/// tests: the create flow up to the New Channel modal, name it, Create, then
/// select the channel row so the message pane opens on it.
fn enter_fresh_server(app: &mut App, prefix: &str) {
    let server = fresh_name("Msg Server ");
    create_server(app, &server);
    open_menu_with_perms(app);

    let channel = fresh_name(prefix);
    app.test_id("create-channel-name-input").fill(&channel);
    app.test_id("create-channel-create").click();
    app.text(&channel).assert_visible();
    // Select the channel in the sidebar so the pane is live on it: the tap
    // on the name text bubbles to the row's handler.
    app.text(&channel).click();
}

/// Open the message's overflow menu the way the app does for a user: hover
/// the body (the row's `on_hover` raises the action bar) and press the
/// "more" pill. The test's channel holds one message, so the ids are
/// unscoped.
fn hover_more_pill(app: &mut App, _msg: &str) {
    app.test_id("msg-row").hover_native();
    app.test_id("msg-more").click();
    app.text("Copy text").assert_visible();
}
