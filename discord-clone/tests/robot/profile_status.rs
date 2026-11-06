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

use crate::support::{Account, eventually, fresh_name, sign_in_as, sign_out, signed_out};
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
    // Every member row carries `member-row-name`, so take A by the new display
    // name it now shows (a bare test_id would pick the last row mounted).
    other.app.text(&disp).click();
    other.app.text("Member since").assert_visible();
    other.app.text(&disp).assert_visible();
    // The card shows A's about-me text.
    other.app.text(&about).assert_visible();
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
    // The avatar popup is the status menu, with a row per status.
    app.test_id("status-row-dnd").assert_visible();
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
fn cat_ears_reach_the_other_browser_and_come_off_again(app: &mut App) {
    let crew = crew("ears");

    signed_out(app);
    sign_in_as(app, &crew.a);
    app.text(&crew.server).assert_visible();
    app.text("general").click();
    app.set_timeout(WAIT);

    // B is already connected and watching A's member row.
    let mut other = open_browser(&fresh_name("earswatch"));
    sign_in_as(&mut other.app, &crew.b);
    other.app.text(&crew.server).assert_visible();
    other.app.set_timeout(WAIT);
    assert!(!other.app.test_id("avatar-decoration").is_visible(), "nobody starts with ears");

    // A picks cat ears: the preview wears them, and so does A's row on B's rail.
    open_user_settings(app);
    app.test_id("settings-nav-profile-effects").click();
    app.test_id("profile-decoration-cat-ears").click();
    app.test_id("avatar-decoration").assert_visible();
    shot(app, "ears1-preview");
    other.app.test_id("avatar-decoration").assert_visible();
    shot(&mut other.app, "ears2-b-sees-them");
    app.press_key("Escape");

    // The ears take no presses: the avatar under them still opens its menu.
    app.test_id("user-panel-avatar").click();
    app.test_id("status-menu").assert_visible();
    shot(app, "ears3-status-menu");
    app.press_key("Escape");

    // A takes them off again, and they leave B's rail without a reload.
    open_user_settings(app);
    app.test_id("settings-nav-profile-effects").click();
    app.test_id("profile-decoration-none").click();
    let gone = eventually(10, || !other.app.test_id("avatar-decoration").is_visible());
    assert!(gone, "B still sees the ears A took off");
    app.press_key("Escape");
    sign_out(app);
}

#[robot_test]
fn a_profile_effect_shows_on_the_card_and_comes_off_again(app: &mut App) {
    let crew = crew("fx");
    signed_out(app);
    sign_in_as(app, &crew.a);
    app.text(&crew.server).assert_visible();
    app.text("general").click();
    app.set_timeout(WAIT);

    // A picks an effect: the preview draws it straight away, and every other
    // decoration in the catalog can go on beside it.
    open_user_settings(app);
    app.test_id("settings-nav-profile-effects").click();
    assert!(!app.test_id("profile-effect-preview").is_visible(), "nobody starts with an effect");
    app.test_id("profile-effect-starfield").click();
    app.test_id("profile-effect-preview").assert_visible();
    for tile in ["cassette-reels", "halo", "flower-crown", "sparkles"] {
        app.test_id(&format!("profile-decoration-{tile}")).click();
        app.test_id("avatar-decoration").assert_visible();
    }
    shot(app, "fx1-preview");
    app.press_key("Escape");

    // B opens A's card and finds the saved effect on it. Every member row
    // carries `member-row-name`, so take A by name.
    let mut other = open_browser(&fresh_name("fxwatch"));
    sign_in_as(&mut other.app, &crew.b);
    other.app.text(&crew.server).assert_visible();
    other.app.set_timeout(WAIT);
    other.app.text(&crew.a.name).click();
    other.app.text("Member since").assert_visible();
    other.app.test_id("profile-effect").assert_visible();
    shot(&mut other.app, "fx2-b-sees-the-card");
    other.app.press_key("Escape");

    // A takes it off again, and the next look at the card has none.
    open_user_settings(app);
    app.test_id("settings-nav-profile-effects").click();
    app.test_id("profile-effect-none").click();
    let gone = eventually(10, || !app.test_id("profile-effect-preview").is_visible());
    assert!(gone, "the preview still shows the effect A took off");
    app.test_id("profile-decoration-none").click();
    app.press_key("Escape");
    other.app.text(&crew.a.name).click();
    other.app.text("Member since").assert_visible();
    assert!(!other.app.test_id("profile-effect").is_visible(), "B still sees the effect A took off");
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
    // The Appearance section offers a card per cassette icon.
    app.test_id("icon-card-miami").assert_visible();
    app.test_id("icon-card-miami").click();

    let mut after = String::new();
    let changed = crate::support::eventually(10, || {
        after = robot_string(app.call_function("cassette.app_icon", json!({})));
        after == "miami"
    });
    assert!(changed && before != "miami", "picking a cassette should change the app icon (before {before}, after {after})");
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
