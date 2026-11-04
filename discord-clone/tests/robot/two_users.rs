//! Two-user smoke: A creates a server and mints an invite; B (a second real
//! browser, its own fresh account) joins with the code; A sends "@B hello" and
//! B's view shows the mention highlighted; B replies and A sees it live; both
//! member panels show both online.
//!
//! The Python suite drove both users over CDP in one shared browser with two
//! tabs; here B gets its own real browser through `open_browser`. The invite
//! code is read straight off the card's `invite-code-field` input, and the
//! overlay's Create/Done buttons and the join modal's code input carry
//! `live-invite-create` / `invite-done` / `join-server-code-input` test_ids,
//! so nothing is clicked by viewport coordinates.
//!
//! The overlay opens from the channel header's invite action (`invite-live`),
//! which needs the server's permission snapshot; a fresh create path leaves it
//! unfetched (the gap server.rs documents), so `cassette.rearm_perms` re-arms
//! it — the robot has no page reload of its own. The Python "Invite People"
//! label is duplicated by a hidden mock (components/invite_modal.rs), so the
//! open live overlay is asserted through its header test_id `live-invite-open`
//! instead of the label.
//!
//! Not ported: the Python "B reloads keeps cookies" check. The robot has no
//! page-reload verb, and a third browser signing B back in re-runs the same
//! restore effects `open_browser` already exercises in each test above (fresh
//! boot, roster seed, socket re-register), so it would only repeat existing
//! coverage — a robot gap, recorded here like server.rs records its gaps.

use robot_test::{json, robot_test, App};

use crate::support::{Account, fresh_name, sign_in_as, signed_out};
use crate::support::WAIT;
use crate::users::open_browser;

/// A creates a server and mints an invite code; B, in its own browser with its
/// own fresh account, joins through the join modal with that code and lands on
/// the new server open, with A already online in its member list.
#[robot_test]
fn b_joins_a_server_through_an_invite(app: &mut App) {
    signed_out(app);
    let a = Account::new("twoA");
    sign_in_as(app, &a);
    app.text(&a.name).assert_visible();

    let server = fresh_name("Duo Server ");
    create_server(app, &server);

    // The fresh create path never fetches the server's permission snapshot
    // (the gap server.rs documents), so the header's invite action stays
    // collapsed until the perms are re-armed through the robot hook. rearm_perms
    // fetches them asynchronously, so wait until the header actually shows
    // before tapping it — a click on a zero-width header is a no-op.
    app.call_function("cassette.rearm_perms", json!({}));
    app.test_id("invite-live").assert_visible();
    app.test_id("invite-live").click();
    // The hidden mock invite_modal duplicates the "Invite People" text, so
    // the open live overlay is asserted through its header test_id.
    app.test_id("live-invite-open").assert_visible();

    app.test_id("live-invite-create").click();
    app.test_id("invite-code-field").assert_visible();
    let state = app.test_id("invite-code-field").state();
    let code = state.value.as_ref().and_then(|v| v.as_str()).unwrap_or("").to_string();
    assert!(!code.is_empty(), "A minted an empty invite code");
    let shot = app.screenshot();
    eprintln!("[shot] duo1-invite-code: {shot}");
    app.test_id("invite-done").click();
    app.test_id("composer").assert_visible();

    // B: its own browser, its own fresh account, joining with A's code.
    let b = Account::new("twoB");
    let mut second = open_browser("b");
    sign_in_as(&mut second.app, &b);
    second.app.test_id("rail-create-server").click();
    second.app.text("Add a Server").assert_visible();
    second.app.test_id("create-server-join-link").click();
    second.app.text("Join a Server").assert_visible();
    second.app.test_id("join-server-code-input").fill(&code);
    second.app.test_id("join-server-join").click();
    second.app.text("Joined!").assert_visible();
    let shot = second.app.screenshot();
    eprintln!("[shot] duo2-b-joined: {shot}");

    // A successful join auto-enters the new server, so B lands on it open.
    second.app.test_id("server-header").assert_visible();
    second.app.text(&server).assert_visible();
    // B's browser came online while A sat in the same channel, so the two
    // Presence frames settle both rows online in B's member list.
    second.app.text("ONLINE — 2").assert_visible();
    let shot = second.app.screenshot();
    eprintln!("[shot] duo3-b-panel-both-online: {shot}");
}

/// A sends "@B hello"; B's browser receives it live over the gateway with the
/// mention pill on B's name; B replies and A receives the reply live too.
#[robot_test]
fn a_message_and_a_reply_cross_between_two_browsers(app: &mut App) {
    signed_out(app);
    let a = Account::new("twoA");
    sign_in_as(app, &a);
    let server = fresh_name("Duo Server ");
    create_server(app, &server);
    app.call_function("cassette.rearm_perms", json!({}));

    let b = Account::new("twoB");
    let mut second = open_browser("b");
    sign_in_as(&mut second.app, &b);
    // B joins through the same invite flow, then both sit in the channel.
    join_by_code(&mut second.app, app_test_code(app));
    second.app.text(&server).assert_visible();

    // A sends the mention; A sees its own message in the pane.
    let b_name = b.name;
    let msg = format!("@{b_name} hello");
    app.test_id("composer").fill(&msg);
    app.test_id("composer").press_key("Enter");
    app.text("hello").assert_visible();
    let shot = app.screenshot();
    eprintln!("[shot] duo4-a-sent: {shot}");

    // B receives it live; the row's mention pill highlights B's name.
    second.app.test_id("mention-pill").assert_visible();
    second.app.text("hello").assert_visible();
    let shot = second.app.screenshot();
    eprintln!("[shot] duo5-b-sees-mention: {shot}");

    // B replies; B sees it and A receives it live over its open gateway.
    let reply = format!("hi from {b_name}");
    second.app.test_id("composer").fill(&reply);
    second.app.test_id("composer").press_key("Enter");
    second.app.text(&reply).assert_visible();
    app.text(&reply).assert_visible();
    let shot = app.screenshot();
    eprintln!("[shot] duo6-a-sees-reply: {shot}");
}

/// Presence both ways: B came online after A entered the channel, and Presence
/// frames only fire on changes, so each side's member list — not a fresh frame
/// — must carry the other user online.
#[robot_test]
fn both_users_are_online_in_each_member_list(app: &mut App) {
    signed_out(app);
    let a = Account::new("twoA");
    sign_in_as(app, &a);
    let server = fresh_name("Duo Server ");
    create_server(app, &server);
    app.call_function("cassette.rearm_perms", json!({}));

    let b = Account::new("twoB");
    let mut second = open_browser("b");
    sign_in_as(&mut second.app, &b);
    join_by_code(&mut second.app, app_test_code(app));
    second.app.text(&server).assert_visible();

    app.text("ONLINE — 2").assert_visible();
    second.app.text("ONLINE — 2").assert_visible();
    app.text(&b.name).assert_visible();
    second.app.text(&a.name).assert_visible();
    let shot = app.screenshot();
    eprintln!("[shot] duo7-a-panel: {shot}");
    let shot = second.app.screenshot();
    eprintln!("[shot] duo8-b-panel: {shot}");
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
    app.set_timeout(WAIT);
}

/// Mint an invite on the entered server (perms re-armed by the caller) and
/// read the code off the card's `invite-code-field`, leaving the overlay open.
fn app_test_code(app: &mut App) -> String {
    app.test_id("invite-live").click();
    app.test_id("live-invite-open").assert_visible();
    app.test_id("live-invite-create").click();
    app.test_id("invite-code-field").assert_visible();
    let state = app.test_id("invite-code-field").state();
    let code = state.value.as_ref().and_then(|v| v.as_str()).unwrap_or("").to_string();
    assert!(!code.is_empty(), "minted an empty invite code");
    app.test_id("invite-done").click();
    app.test_id("composer").assert_visible();
    code
}

/// The join modal flow on another browser: paste `code`, Join, close with
/// Done; the successful join auto-enters the new server.
fn join_by_code(app: &mut App, code: String) {
    app.test_id("rail-create-server").click();
    app.text("Add a Server").assert_visible();
    app.test_id("create-server-join-link").click();
    app.text("Join a Server").assert_visible();
    app.test_id("join-server-code-input").fill(&code);
    app.test_id("join-server-join").click();
    app.text("Joined!").assert_visible();
    let shot = app.screenshot();
    eprintln!("[shot] duo-joined-toast: {shot}");
    app.test_id("join-server-done").click();
    app.set_timeout(WAIT);
}
