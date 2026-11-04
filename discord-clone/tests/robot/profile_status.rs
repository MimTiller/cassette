//! Profile/status smoke: A edits display name and about-me through User
//! Settings > Profile, B (a second real browser, already signed in and
//! watching) sees them on A's profile card; A sets Do Not Disturb from the
//! status menu and the presence frames turn the member dot red on both rails;
//! then A opens the Appearance picker and presses an icon, read through the
//! `cassette.app_icon` hook.
//!
//! The Python suite opened A's card in B by clicking A's username text; the
//! robot clicks the member row's `member-row-name` text (the same opener —
//! the robot has no right-click). A's own status dots (a computed colour in
//! Python) are read through the rail's `member-presence-*` test_ids: the row
//! carries the id of the status it shows.
//!
//! Not ported: the Python "no uncaught page exceptions" check — a JS page
//! error never reaches the robot's log (same gap as `server.rs`).

use robot_test::{json, robot_test, App, Value};

use crate::support::{Account, fresh_name, known_bug, sign_in_as, sign_out, signed_out};
use crate::support::WAIT;
use crate::users::open_browser;

/// The suite screenshots the moment it names; matches the Python file names.
fn shot(app: &mut App, label: &str) {
    let path = app.screenshot();
    assert!(path.ends_with(".png"), "screenshot was not saved: {path}");
    eprintln!("[shot] {label}: {path}");
}

/// A fresh pair of accounts plus the server A owns and B is a member of, made
/// through the server (the Python suite set membership up over `_srv` too).
struct Crew {
    a: Account,
    b: Account,
    server: String,
}

fn crew(prefix: &str) -> Crew {
    let (a, b) = (Account::new(prefix), Account::new(&format!("{prefix}b")));
    let server = fresh_name("Profile Crew ");
    let sid = a.call("create_server", json!({ "name": server }))["id"].as_i64().expect("created server id");
    a.call("add_member", json!({ "server_id": sid, "user_id": b.id }));
    Crew { a, b, server }
}

#[robot_test]
fn about_me_and_name_seen_from_the_others_card(app: &mut App) {
    let crew = crew("pf");

    // A signs in and opens the crew's channel.
    signed_out(app);
    sign_in_as(app, &crew.a);
    app.text(&crew.server).assert_visible();
    app.text("general").click();
    app.set_timeout(WAIT);

    // B signs in in the second real browser, on the same crew.
    let mut other = open_browser(&fresh_name("pfwatch"));
    sign_in_as(&mut other.app, &crew.b);
    other.app.text(&crew.server).assert_visible();
    shot(app, "profile0-two-browsers");

    // A edits display name and about-me through User Settings > Profile.
    let (disp, about) = (fresh_name("Display "), fresh_name("About me text "));
    open_user_settings(app);
    app.test_id("settings-nav-profile").click();
    app.test_id("profile-display-name").fill(&disp);
    app.test_id("profile-about").fill(&about);
    // Save and leave the settings overlay (Escape closes the open dialog).
    app.test_id("profile-save").click();
    app.text("Saved.").assert_visible();
    shot(app, "profile2-about-set");
    app.press_key("Escape");

    // A opens own card from its member row: it shows A by display name.
    app.test_id("member-row-name").click();
    app.text("Member since").assert_visible();
    shot(app, "profile3-own-card");
    app.press_key("Escape");

    // B (already watching) opens A's card from B's member rail: the card
    // shows the new display name and the about-me.
    other.app.test_id("member-row-name").click();
    other.app.text("Member since").assert_visible();
    other.app.text(&disp).assert_visible();
    known_bug(
        "the card shows A's about-me text",
        other.app.text(&about).is_visible(),
        "about field saved above",
    );
    shot(&mut other.app, "profile4-b-card");
    other.app.press_key("Escape");
    app.press_key("Escape");
    sign_out(app);
}

#[robot_test]
fn do_not_disturb_reaches_the_other_user_as_red(app: &mut App) {
    let crew = crew("pdnd");

    signed_out(app);
    sign_in_as(app, &crew.a);
    app.text(&crew.server).assert_visible();
    app.text("general").click();
    app.set_timeout(WAIT);

    let mut other = open_browser(&fresh_name("dndwatch"));
    sign_in_as(&mut other.app, &crew.b);

    // A opens the status menu from the user panel's avatar and picks DND.
    app.test_id("user-panel-avatar").click();
    app.test_id("status-menu").assert_visible();
    known_bug(
        "the avatar popup is the status menu",
        app.test_id("status-row-dnd").is_visible(),
        "the four status rows with dots",
    );
    app.test_id("status-row-dnd").click();

    // A's own member rail row now shows the dnd dot.
    app.test_id("member-presence-dnd").assert_visible();
    shot(app, "prof2-dnd");
    app.press_key("Escape");

    // B is already connected: the Presence frame over its live socket turns
    // A's row red on B's rail within a few seconds.
    other.app.set_timeout(WAIT);
    assert!(
        other.app.test_id("member-presence-dnd").is_visible(),
        "B sees the red Do Not Disturb dot on A's rail row"
    );
    shot(&mut other.app, "prof3-dnd-b");
    other.app.press_key("Escape");
    app.press_key("Escape");
    // Sign A out: its picked status is Do Not Disturb, so the next test's
    // `signed_out` (keyed on "Online") would otherwise leave it signed in.
    sign_out(app);
}

#[robot_test]
fn the_appearance_picker_offers_cassette_icons(app: &mut App) {
    signed_out(app);
    let account = Account::new("icon");
    sign_in_as(app, &account);

    open_user_settings(app);
    shot(app, "profile5-user-settings");
    let before = robot_string(app.call_function("cassette.app_icon", json!({})));

    app.test_id("settings-nav-appearance").click();
    known_bug(
        "the Appearance section offers a cassette icon picker",
        app.test_id("icon-card-miami").is_visible(),
        "a card per icon look in the picker",
    );
    app.test_id("icon-card-miami").click();

    let after = robot_string(app.call_function("cassette.app_icon", json!({})));
    known_bug(
        "picking a cassette changes the app icon",
        after == "miami" && before != "miami",
        &format!("app icon key before/after: {before}/{after}"),
    );
    shot(app, "prof4-icon");
}

// ---- shared steps for this module -----------------------------------------

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
    v.get("Ok")
        .and_then(|inner| inner.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default()
}
