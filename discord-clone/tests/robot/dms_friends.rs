//! Friends/DMs (ported from e2e/test_dms_friends.py): A sends B a friend
//! request from Add Friend and sees it in Pending; B's own browser sees A's
//! request as pending, accepts it, and A appears under All; A stays on the
//! DM screen while that lands, starts a DM with B from the "Find or start a
//! conversation" search, and they message each other both ways live; A opens
//! a group DM with three people through the picker; a server-seeded group DM
//! shows in the DM list; B blocks A from the DM's user menu and A's messages
//! collapse into a hidden row.
//!
//! The Python suite drove a second browser over CDP; here B gets one through
//! `open_browser`, and the group tests make a third account C (friends are
//! seeded through the server, like the Python `srv` setup).
//!
//! The rail's DM bubble, the DM rows and the picker all carry test_ids now
//! (`rail-direct-messages`, `live-dm-row`, `picker-friend`,
//! `picker-group-name`); the friends panel's controls already do
//! (`friends-entry`, `friends-tab-*`, `friends-add-username`,
//! `friends-add-send`, `friends-act`), so nothing is clicked by coordinates.
//! The Python right-click on the DM row's name has no robot verb: the
//! context menu opens through the `cassette.open_user_menu` hook in
//! robot_hooks.rs instead (that gap is noted there too).
//!
//! Not ported: the Python "no uncaught page exceptions" check — the same
//! no-page-error-log gap `server.rs` and `messages.rs` record.

use robot_test::{json, robot_test, App};

use crate::support::{Account, fresh_name, sign_in_as, signed_out};
use crate::support::WAIT;
use crate::users::open_browser;

/// Three fresh accounts plus the "Friends Crew" server A owns and B and C
/// join (the Python `_srv` setup): every test calls this so nothing crosses
/// tests. Friends themselves are seeded per test, the way that test drives
/// them.
fn crew(prefix: &str) -> (Account, Account, Account, String) {
    let a = Account::new(prefix);
    let b = Account::new(&format!("{prefix}b"));
    let c = Account::new(&format!("{prefix}c"));
    let server = fresh_name("Friends Crew ");
    let sid = a.call("create_server", json!({ "name": server }))["id"].as_i64().expect("created server id");
    a.call("add_member", json!({ "server_id": sid, "user_id": b.id }));
    a.call("add_member", json!({ "server_id": sid, "user_id": c.id }));
    (a, b, c, server)
}

/// Screenshot moment, named like the Python suite's files.
fn shot(app: &mut App, label: &str) {
    let path = app.screenshot();
    assert!(path.ends_with(".png"), "screenshot was not saved: {path}");
    eprintln!("[shot] {label}: {path}");
}

/// Open the live DM screen from the rail's chat-bubble icon and wait for its
/// search box — the DM surface's entry every test starts from.
fn open_dms(app: &mut App) {
    app.test_id("rail-direct-messages").click();
    app.test_id("live-dm-find").assert_visible();
}

/// Leave the DM screen as entered: the Done pill in the DM header flips the
/// overlay off (Escape never closes it), so the sidebar is clickable again.
fn close_dms(app: &mut App) {
    app.test_id("dm-done-pill").click();
    // On a phone an opened conversation switched the pane to Chat, so the
    // profile bar (and its settings gear) stays hidden until the pane closes.
    if app.test_id("mobile-back").is_visible() {
        app.test_id("mobile-back").click();
    }
    app.test_id("user-settings").assert_visible();
}

#[robot_test]
fn a_friend_request_is_sent_and_accepted_across_browsers(app: &mut App) {
    let (a, b, _c, server) = crew("dms");
    signed_out(app);
    sign_in_as(app, &a);
    app.text(&server).assert_visible();
    app.set_timeout(WAIT);

    // B: its own browser, signed in before A sends, so A's request lands
    // live on B's screen through the gateway's FriendChanged frame.
    let mut second = open_browser("bfriend");
    sign_in_as(&mut second.app, &b);
    second.app.set_timeout(WAIT);

    // A: the DM screen, then the Friends panel's Add Friend tab.
    open_dms(app);
    app.test_id("friends-entry").click();
    app.test_id("friends-tab-add").click();
    app.test_id("friends-add-username").assert_visible();
    app.test_id("friends-add-username").fill(&b.name);
    app.test_id("friends-add-send").click();

    // A's Pending tab lists the outgoing request.
    app.test_id("friends-tab-pending").click();
    app.test_id("friends-act").with_label("Cancel").assert_visible();
    app.text(&b.name).assert_visible();

    // B sees A's request as pending and accepts it.
    open_dms(&mut second.app);
    second.app.test_id("friends-entry").click();
    second.app.test_id("friends-tab-pending").click();
    second.app.text(&a.name).assert_visible();
    second.app.test_id("friends-act").with_label("Accept").click();
    second.app.test_id("friends-tab-all").click();
    second.app.text(&a.name).assert_visible();
    shot(&mut second.app, "dm0-b-accepted");

    // A stays on the DM screen while the acceptance arrives: the Python
    // suite called this a known bug (the page switched back on its own),
    // but the search box is still live, so it is asserted.
    app.test_id("live-dm-find").assert_visible();

    // The acceptance is live on A too: B left Pending and joined All.
    app.test_id("friends-tab-pending").click();
    app.test_id("friends-act").with_label("Cancel").assert_hidden();
    app.test_id("friends-tab-all").click();
    app.text(&b.name).assert_visible();
    close_dms(app);
}

#[robot_test]
fn a_dm_opens_from_the_search_and_both_read_it_live(app: &mut App) {
    let (a, b, _c, _server) = crew("dmdm");
    // A and B are friends before either signs in: the search starts DMs
    // with friends, so the relationship must already be there.
    a.call("send_friend_request", json!({ "username": b.name }));
    b.call("accept_friend_request", json!({ "user_id": a.id }));

    signed_out(app);
    sign_in_as(app, &a);
    app.set_timeout(WAIT);

    let mut second = open_browser("b");
    sign_in_as(&mut second.app, &b);
    second.app.set_timeout(WAIT);

    // A types B's name into the find box and taps the result: the DM opens
    // (and the Friends panel, if open, closes over it).
    open_dms(app);
    app.test_id("live-dm-find").fill(&b.name);
    app.test_id("live-dm-find-result").assert_visible();
    app.test_id("live-dm-find-result").click();
    app.test_id("live-dm-composer").assert_visible();

    // A sends and sees its own message.
    let tag = fresh_name("d");
    let dm_msg = format!("dm from A {tag}");
    app.test_id("live-dm-composer").fill(&dm_msg);
    app.test_id("live-dm-composer").press_key("Enter");
    app.text(&dm_msg).assert_visible();

    // The brand-new DM appears in B's list live (DmChanged frame), and B
    // opens it and reads A's message.
    open_dms(&mut second.app);
    second.app.text(&a.name).assert_visible();
    second.app.test_id("live-dm-row").click();
    second.app.text(&dm_msg).assert_visible();

    // B replies, and A receives the reply live.
    let reply = format!("dm reply from B {tag}");
    second.app.test_id("live-dm-composer").fill(&reply);
    second.app.test_id("live-dm-composer").press_key("Enter");
    second.app.text(&reply).assert_visible();
    app.text(&reply).assert_visible();
    shot(app, "dm1-a-view");
    close_dms(app);
}

#[robot_test]
fn a_group_dm_opens_for_three_people_from_the_picker(app: &mut App) {
    let (a, b, c, _server) = crew("grp");
    // Seed A's friendships through the server (the Python seed for the
    // group DM): requests A sends, accepted by the other side.
    a.call("send_friend_request", json!({ "username": b.name }));
    b.call("accept_friend_request", json!({ "user_id": a.id }));
    a.call("send_friend_request", json!({ "username": c.name }));
    c.call("accept_friend_request", json!({ "user_id": a.id }));

    signed_out(app);
    sign_in_as(app, &a);
    app.set_timeout(WAIT);

    // The plus beside "Direct Messages" opens the group picker (the Python
    // suite probed icons for it and called the missing picker a known bug;
    // the entry is live, so this is asserted).
    open_dms(app);
    app.test_id("new-group-dm").click();
    app.text("Select Friends").assert_visible();

    // Pick B and C from the friend rows, sorted by name so the order
    // follows the accounts' prefixes (B before C).
    app.test_id("picker-friend").nth(0).click();
    app.test_id("picker-friend").nth(1).click();
    app.test_id("picker-group-name").assert_visible();
    let grp = fresh_name("Squad ");
    app.test_id("picker-group-name").fill(&grp);
    app.text("Create Group DM").click();

    // The group opens with its name and its three members.
    app.text(&grp).assert_visible();
    app.text("3 Members").assert_visible();
    shot(app, "dm2-group");
    close_dms(app);
}

#[robot_test]
fn a_server_seeded_group_dm_shows_in_the_dm_list(app: &mut App) {
    let (a, b, c, _server) = crew("seedgrp");
    a.call("send_friend_request", json!({ "username": b.name }));
    b.call("accept_friend_request", json!({ "user_id": a.id }));
    a.call("send_friend_request", json!({ "username": c.name }));
    c.call("accept_friend_request", json!({ "user_id": a.id }));
    let grp = fresh_name("Squad ");
    a.call("create_group_dm", json!({ "user_ids": [b.id, c.id], "name": grp }));

    signed_out(app);
    sign_in_as(app, &a);
    app.set_timeout(WAIT);

    // The Python fallback branch seeded the group through the server and
    // called the list row a known bug; the row shows, so it is asserted.
    open_dms(app);
    app.text(&grp).assert_visible();
    app.test_id("live-dm-row").click();
    app.text(&grp).assert_visible();
    app.text("3 Members").assert_visible();
    close_dms(app);
}

#[robot_test]
fn blocking_in_the_dm_hides_the_blocked_messages(app: &mut App) {
    let (a, b, _c, _server) = crew("blk");
    a.call("send_friend_request", json!({ "username": b.name }));
    b.call("accept_friend_request", json!({ "user_id": a.id }));

    signed_out(app);
    sign_in_as(app, &a);
    app.set_timeout(WAIT);

    // A starts a DM with B through the search and posts into it.
    open_dms(app);
    app.test_id("live-dm-find").fill(&b.name);
    app.test_id("live-dm-find-result").click();
    app.test_id("live-dm-composer").assert_visible();
    let tag = fresh_name("d");
    let dm_msg = format!("dm from A {tag}");
    app.test_id("live-dm-composer").fill(&dm_msg);
    app.test_id("live-dm-composer").press_key("Enter");
    app.text(&dm_msg).assert_visible();

    // B opens the same DM in its browser and reads A's message.
    let mut second = open_browser("bblock");
    sign_in_as(&mut second.app, &b);
    second.app.set_timeout(WAIT);
    open_dms(&mut second.app);
    second.app.text(&a.name).assert_visible();
    second.app.test_id("live-dm-row").click();
    second.app.text(&dm_msg).assert_visible();

    // The Python suite right-clicked A's name in the DM row; the robot has
    // no secondary press, so the context menu opens through the
    // cassette.open_user_menu hook (see robot_hooks.rs). The menu's rows are
    // label-less views, so they are addressed by test_id + index:
    // 0 Profile, 1 Mention, 2 Message, 3 Remove Friend, 4 Block,
    // 5 Copy User ID (A and B are already friends, so the friend row reads
    // "Remove Friend"; no moderation rows since B does not outrank A).
    second.app.call_function("cassette.open_user_menu", json!({ "user_id": a.id }));
    second.app.text("Copy User ID").assert_visible();
    second.app.test_id("user-menu-action").assert_count(6);
    second.app.test_id("user-menu-action").nth(4).click();

    // A's messages collapse into the hidden row.
    second.app.text("Blocked Message").assert_visible();
    second.app.text(&dm_msg).assert_hidden();
    shot(&mut second.app, "dm-block-menu");
    close_dms(app);
}
