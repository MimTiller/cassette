//! Notifications suite (ported from e2e/test_notifications.py): 12+ unread
//! messages show the "N new messages" bar in a channel opened with them —
//! both the one the login lands on and one opened once the counts are in —
//! and tapping the bar jumps to the first unread; muting a channel from its
//! right-click menu opens the mute dialog, "Until I turn it back on" stores
//! a Channel-scope setting on the server, and the muted row draws its bell
//! icon while another channel is open.
//!
//! The robot has no right-click: the channel menu comes from the visible
//! gear button on the channel row (hover-gated `live-channel-gear`), which
//! opens the same menu the secondary press does. The bar's jump half carries
//! a new `unread-bar-jump` test_id (src/app/messages.rs) so the tap is a
//! locator, and the muted row's bell icon carries `muted-bell-icon` (src/app/sidebar.rs)
//! so the mute the Python suite read off the dimmed label colour — a colour
//! the bridge cannot see — is readable as the bell's presence.
//!
//! Not ported: the Python "Only @mentions" block — mention notifications
//! are moving to the client and are a known failure — and the Python "no
//! uncaught page exceptions" check, the same robot gap `server.rs` records
//! (a JS page error never reaches the robot's log).

use robot_test::{json, robot_test, App, Value};

use crate::support::{Account, back_to_list, eventually, fresh_name, sign_in_as, signed_out};
use crate::support::WAIT;
use crate::users::open_browser;

/// A fresh pair of accounts plus a fresh server A owns and B is a member of
/// (seeded #general, plus a #lobby made through the server like the Python
/// `_srv` setup); every test calls this so no state crosses tests. Returns
/// the server's name and id too.
fn crew(prefix: &str) -> (Account, Account, String, i64) {
    let a = Account::new(prefix);
    let b = Account::new(&format!("{prefix}b"));
    let server = fresh_name("Notif Crew ");
    let sid = a.call("create_server", json!({ "name": server }))["id"].as_i64().expect("created server id");
    a.call("add_member", json!({ "server_id": sid, "user_id": b.id }));
    a.call("create_channel", json!({ "server_id": sid, "name": "lobby", "kind": "text" }));
    (a, b, server, sid)
}

/// Screenshot moment, named like the Python suite's files.
fn shot(app: &mut App, label: &str) {
    let path = app.screenshot();
    assert!(path.ends_with(".png"), "screenshot was not saved: {path}");
    eprintln!("[shot] {label}: {path}");
}

/// Fill the composer, send, and wait for the row; the server rate-limits a
/// user to ~20 sends per 10 s, so a refused send keeps its draft and gets
/// re-sent until the row mounts (same pacing the messages suite uses).
fn send(app: &mut App, body: String) {
    app.test_id("composer").fill(&body);
    app.test_id("composer").press_key("Enter");
    while !eventually(2, || app.text(&body).count() > 0) {
        app.test_id("composer").fill(&body);
        app.test_id("composer").press_key("Enter");
    }
}

#[robot_test]
fn the_new_messages_bar_shows_on_entry_and_jumps(app: &mut App) {
    let (a, b, server, _sid) = crew("unread");
    signed_out(app);

    // B posts 12 in each channel BEFORE A ever opens the server: B's own
    // fresh browser logs in and lands on #general through the login restore.
    let mut second = open_browser("unreadb");
    sign_in_as(&mut second.app, &b);
    second.app.text(&server).assert_visible();
    second.app.set_timeout(WAIT);
    second.app.test_id("composer").assert_visible();
    for i in 0..12 {
        send(&mut second.app, format!("unread{i}"));
    }
    // The sidebar's rows mount in channel order: #general first, #lobby
    // second (the create path seeds general, the crew helper adds lobby).
    second.app.test_id("live-channel-row").nth(1).click();
    for i in 0..12 {
        send(&mut second.app, format!("lobby{i}"));
    }

    // A's fresh login auto-enters its only server and lands on #general.
    // The unread counts are fetched BEFORE the landing channel is selected
    // (the order shell_state.rs pins), so the bar the login opens on shows.
    sign_in_as(app, &a);
    app.text(&server).assert_visible();
    app.set_timeout(WAIT);
    app.text("12 new messages").assert_visible();

    // Opening a channel once the counts are in: #lobby has 12 unread, so its
    // bar is live and addressable.
    app.test_id("live-channel-row").nth(1).click();
    app.test_id("unread-bar-jump").assert_visible();
    app.text("12 new messages").assert_visible();
    shot(app, "notif1-unread-bar");

    // Tapping the bar jumps to the first unread: #lobby was posted
    // lobby0..lobby11, so the pane lands with lobby0 on screen.
    app.test_id("unread-bar-jump").click();
    app.text("lobby0").assert_in_viewport();
}

#[robot_test]
fn a_channel_mutes_from_its_menu_and_shows_it(app: &mut App) {
    let (a, _b, server, sid) = crew("mute");
    signed_out(app);
    sign_in_as(app, &a);
    app.text(&server).assert_visible();
    app.set_timeout(WAIT);

    // No right-click on the robot: hovering the channel row shows its gear,
    // and tapping the gear opens the same context menu the secondary press
    // does — visible button, not a hook.
    app.test_id("live-channel-row").nth(0).hover_native();
    app.test_id("live-channel-gear").assert_visible();
    app.test_id("live-channel-gear").click();
    app.text("Mute Channel").assert_visible();
    app.text("Mute Channel").click();

    // The channel's notification dialog is up: mute it until turned back on.
    app.text("MUTE CHANNEL").assert_visible();
    shot(app, "notif2-mute-dialog");
    app.text("Until I turn it back on").click();
    app.press_key("Escape");
    app.text("MUTE CHANNEL").assert_hidden();

    // With another channel open, the muted row draws its bell icon — the
    // state the Python suite read off the label's dimmed colour. On a phone
    // opening a channel hides the rail, so close the pane first.
    app.test_id("live-channel-row").nth(1).click();
    back_to_list(app);
    app.test_id("muted-bell-icon").assert_visible();

    // The mute is stored on the server as a Channel-scope setting for A.
    let stored = eventually(
        10,
        || {
            let rows = a.call("notification_settings", json!({ "server_id": sid }));
            matches_rows(rows, "Channel")
        },
    );
    assert!(stored, "the mute is stored on the server with Channel scope");
}

/// Whether the server's notification-settings reply carries a row of `scope`
/// ("Channel" or "Server").
fn matches_rows(rows: Value, scope: &str) -> bool {
    if let Value::Array(items) = rows {
        return items.iter().any(|r| match r.get("scope").and_then(|s| s.as_str()) {
            Some(s) => s == scope,
            None => false,
        });
    }
    false
}
