//! Switching the Appearance picker to each new theme and reading it back
//! through the `cassette.theme` hook: Sunset, Miami, Memphis, Bubblegum,
//! Game Boy, Windows 95, VHS, and Blockbuster.

use robot_test::{json, robot_test, App, Value};

use crate::support::{fresh_name, sign_up, signed_out};
use crate::support::WAIT;

#[robot_test]
fn picking_sunset_recolours_the_app(app: &mut App) {
    signed_out(app);
    let user = fresh_name("sunsettheme");
    sign_up(app, &user, &format!("pass-{user}"));
    app.text("Online").assert_visible();

    app.test_id("user-settings").click();
    app.text("USER SETTINGS").assert_visible();
    app.set_timeout(WAIT);
    app.test_id("settings-nav-appearance").click();
    app.set_timeout(WAIT);
    shot(app, "themes1-appearance");

    app.test_id("theme-card-sunset").click();
    app.set_timeout(WAIT);
    let mut got = String::new();
    let ok = crate::support::eventually(10, || {
        got = robot_string(app.call_function("cassette.theme", json!({})));
        got == "sunset"
    });
    assert!(ok, "cassette.theme should report sunset (got {got})");
    shot(app, "themes2-sunset");
    for _ in 0..3 { app.press_key("Escape"); }
}

#[robot_test]
fn picking_miami_recolours_the_app(app: &mut App) {
    signed_out(app);
    let user = fresh_name("miamitheme");
    sign_up(app, &user, &format!("pass-{user}"));
    app.text("Online").assert_visible();

    app.test_id("user-settings").click();
    app.text("USER SETTINGS").assert_visible();
    app.set_timeout(WAIT);
    app.test_id("settings-nav-appearance").click();
    app.set_timeout(WAIT);

    app.test_id("theme-card-miami").click();
    app.set_timeout(WAIT);
    let mut got = String::new();
    let ok = crate::support::eventually(10, || {
        got = robot_string(app.call_function("cassette.theme", json!({})));
        got == "miami"
    });
    assert!(ok, "cassette.theme should report miami (got {got})");
    shot(app, "themes3-miami");
    for _ in 0..3 { app.press_key("Escape"); }
}

#[robot_test]
fn picking_memphis_recolours_the_app(app: &mut App) {
    signed_out(app);
    let user = fresh_name("memphistheme");
    sign_up(app, &user, &format!("pass-{user}"));
    app.text("Online").assert_visible();

    app.test_id("user-settings").click();
    app.text("USER SETTINGS").assert_visible();
    app.set_timeout(WAIT);
    app.test_id("settings-nav-appearance").click();
    app.set_timeout(WAIT);

    app.test_id("theme-card-memphis").click();
    app.set_timeout(WAIT);
    let mut got = String::new();
    let ok = crate::support::eventually(10, || {
        got = robot_string(app.call_function("cassette.theme", json!({})));
        got == "memphis"
    });
    assert!(ok, "cassette.theme should report memphis (got {got})");
    shot(app, "themes4-memphis");
    for _ in 0..3 { app.press_key("Escape"); }
}

#[robot_test]
fn picking_bubblegum_recolours_the_app(app: &mut App) {
    signed_out(app);
    let user = fresh_name("bubblegumtheme");
    sign_up(app, &user, &format!("pass-{user}"));
    app.text("Online").assert_visible();

    app.test_id("user-settings").click();
    app.text("USER SETTINGS").assert_visible();
    app.set_timeout(WAIT);
    app.test_id("settings-nav-appearance").click();
    app.set_timeout(WAIT);

    app.test_id("theme-card-bubblegum").click();
    app.set_timeout(WAIT);
    let mut got = String::new();
    let ok = crate::support::eventually(10, || {
        got = robot_string(app.call_function("cassette.theme", json!({})));
        got == "bubblegum"
    });
    assert!(ok, "cassette.theme should report bubblegum (got {got})");
    shot(app, "themes5-bubblegum");
    for _ in 0..3 { app.press_key("Escape"); }
}

#[robot_test]
fn picking_gameboy_recolours_the_app(app: &mut App) {
    signed_out(app);
    let user = fresh_name("gameboytheme");
    sign_up(app, &user, &format!("pass-{user}"));
    app.text("Online").assert_visible();

    app.test_id("user-settings").click();
    app.text("USER SETTINGS").assert_visible();
    app.set_timeout(WAIT);
    app.test_id("settings-nav-appearance").click();
    app.set_timeout(WAIT);

    app.test_id("theme-card-gameboy").click();
    app.set_timeout(WAIT);
    let mut got = String::new();
    let ok = crate::support::eventually(10, || {
        got = robot_string(app.call_function("cassette.theme", json!({})));
        got == "gameboy"
    });
    assert!(ok, "cassette.theme should report gameboy (got {got})");
    shot(app, "themes6-gameboy");
    for _ in 0..3 { app.press_key("Escape"); }
}

#[robot_test]
fn picking_win95_recolours_the_app(app: &mut App) {
    signed_out(app);
    let user = fresh_name("win95theme");
    sign_up(app, &user, &format!("pass-{user}"));
    app.text("Online").assert_visible();

    app.test_id("user-settings").click();
    app.text("USER SETTINGS").assert_visible();
    app.set_timeout(WAIT);
    app.test_id("settings-nav-appearance").click();
    app.set_timeout(WAIT);

    app.test_id("theme-card-win95").click();
    app.set_timeout(WAIT);
    let mut got = String::new();
    let ok = crate::support::eventually(10, || {
        got = robot_string(app.call_function("cassette.theme", json!({})));
        got == "win95"
    });
    assert!(ok, "cassette.theme should report win95 (got {got})");
    shot(app, "themes7-win95");
    for _ in 0..3 { app.press_key("Escape"); }
}

#[robot_test]
fn picking_vhs_recolours_the_app(app: &mut App) {
    signed_out(app);
    let user = fresh_name("vhstheme");
    sign_up(app, &user, &format!("pass-{user}"));
    app.text("Online").assert_visible();

    app.test_id("user-settings").click();
    app.text("USER SETTINGS").assert_visible();
    app.set_timeout(WAIT);
    app.test_id("settings-nav-appearance").click();
    app.set_timeout(WAIT);

    app.test_id("theme-card-vhs").click();
    app.set_timeout(WAIT);
    let mut got = String::new();
    let ok = crate::support::eventually(10, || {
        got = robot_string(app.call_function("cassette.theme", json!({})));
        got == "vhs"
    });
    assert!(ok, "cassette.theme should report vhs (got {got})");
    shot(app, "themes8-vhs");
    for _ in 0..3 { app.press_key("Escape"); }
}

#[robot_test]
fn picking_blockbuster_recolours_the_app(app: &mut App) {
    signed_out(app);
    let user = fresh_name("blockbustertheme");
    sign_up(app, &user, &format!("pass-{user}"));
    app.text("Online").assert_visible();

    app.test_id("user-settings").click();
    app.text("USER SETTINGS").assert_visible();
    app.set_timeout(WAIT);
    app.test_id("settings-nav-appearance").click();
    app.set_timeout(WAIT);

    app.test_id("theme-card-blockbuster").click();
    app.set_timeout(WAIT);
    let mut got = String::new();
    let ok = crate::support::eventually(10, || {
        got = robot_string(app.call_function("cassette.theme", json!({})));
        got == "blockbuster"
    });
    assert!(ok, "cassette.theme should report blockbuster (got {got})");
    shot(app, "themes9-blockbuster");
    for _ in 0..3 { app.press_key("Escape"); }
}

#[robot_test]
fn picking_miami_offers_the_matching_cassette(app: &mut App) {
    signed_out(app);
    let user = fresh_name("miamicassette");
    sign_up(app, &user, &format!("pass-{user}"));
    app.text("Online").assert_visible();

    app.test_id("user-settings").click();
    app.text("USER SETTINGS").assert_visible();
    app.set_timeout(WAIT);
    app.test_id("settings-nav-appearance").click();
    app.set_timeout(WAIT);

    // The default icon is Sunset, so picking Miami arms the cassette offer.
    app.test_id("theme-card-miami").click();
    app.set_timeout(WAIT);
    app.test_id("pair-offer-icon").assert_visible();
    app.test_id("pair-offer-icon-text").assert_text_contains("matching Miami cassette");
    shot(app, "themes10-offer-icon");

    app.test_id("pair-switch-icon").click();
    app.set_timeout(WAIT);
    let mut got = String::new();
    let ok = crate::support::eventually(10, || {
        got = robot_string(app.call_function("cassette.app_icon", json!({})));
        got == "miami"
    });
    assert!(ok, "Switch icon should set the app icon (got {got})");
    app.test_id("pair-offer-icon").assert_hidden();
    shot(app, "themes11-miami-icon-set");
    for _ in 0..3 { app.press_key("Escape"); }
}

#[robot_test]
fn no_thanks_leaves_the_icon_unchanged(app: &mut App) {
    signed_out(app);
    let user = fresh_name("nothanksmiami");
    sign_up(app, &user, &format!("pass-{user}"));
    app.text("Online").assert_visible();

    app.test_id("user-settings").click();
    app.text("USER SETTINGS").assert_visible();
    app.set_timeout(WAIT);
    app.test_id("settings-nav-appearance").click();
    app.set_timeout(WAIT);

    // The suite shares one browser, so another test may already have set the
    // active icon to Miami (which suppresses this offer). Pick Sunset first so
    // the Miami cassette offer deterministically arms.
    app.test_id("icon-card-sunset").click();
    app.set_timeout(WAIT);
    app.test_id("theme-card-miami").click();
    app.set_timeout(WAIT);
    app.test_id("pair-offer-icon").assert_visible();
    let before = robot_string(app.call_function("cassette.app_icon", json!({})));

    app.test_id("pair-dismiss-icon").click();
    app.set_timeout(WAIT);
    app.test_id("pair-offer-icon").assert_hidden();
    let after = robot_string(app.call_function("cassette.app_icon", json!({})));
    assert_eq!(after, before, "No thanks must leave the icon unchanged");

    // The dismissed pair must not come back for this account.
    app.test_id("theme-card-miami").click();
    app.set_timeout(WAIT);
    app.test_id("pair-offer-icon").assert_hidden();
    shot(app, "themes12-dismissed-no-reoffer");
    for _ in 0..3 { app.press_key("Escape"); }
}

#[robot_test]
fn picking_memphis_cassette_offers_the_theme(app: &mut App) {
    signed_out(app);
    let user = fresh_name("memphisthemeoffer");
    sign_up(app, &user, &format!("pass-{user}"));
    app.text("Online").assert_visible();

    app.test_id("user-settings").click();
    app.text("USER SETTINGS").assert_visible();
    app.set_timeout(WAIT);
    app.test_id("settings-nav-appearance").click();
    app.set_timeout(WAIT);

    // The default theme is not Memphis, so picking the cassette arms it.
    app.test_id("icon-card-memphis").click();
    app.set_timeout(WAIT);
    app.test_id("pair-offer-theme").assert_visible();
    app.test_id("pair-offer-theme-text").assert_text_contains("matching Memphis theme");
    shot(app, "themes13-theme-offer");
    for _ in 0..3 { app.press_key("Escape"); }
}

#[robot_test]
fn a_theme_without_a_cassette_offers_nothing(app: &mut App) {
    signed_out(app);
    let user = fresh_name("gameboynoop");
    sign_up(app, &user, &format!("pass-{user}"));
    app.text("Online").assert_visible();

    app.test_id("user-settings").click();
    app.text("USER SETTINGS").assert_visible();
    app.set_timeout(WAIT);
    app.test_id("settings-nav-appearance").click();
    app.set_timeout(WAIT);

    app.test_id("theme-card-gameboy").click();
    app.set_timeout(WAIT);
    app.test_id("pair-offer-icon").assert_hidden();
    shot(app, "themes14-gameboy-no-offer");
    for _ in 0..3 { app.press_key("Escape"); }
}

// Shared helpers for this module.
fn shot(app: &mut App, label: &str) {
    let path = app.screenshot();
    assert!(path.ends_with(".png"), "screenshot was not saved: {path}");
    eprintln!("[shot] {label}: {path}");
}

fn robot_string(v: Value) -> String {
    if let Some(s) = v.as_str() {
        return s.to_string();
    }
    if let Some(ok) = v.get("Ok").or_else(|| v.get("ok")) {
        return robot_string(ok.clone());
    }
    v.to_string()
}
