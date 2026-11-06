//! Switching the Appearance picker to each new theme and reading it back
//! through the `cassette.theme` hook: Sunset, Miami, Memphis, and Bubblegum.

use robot_test::{json, robot_test, App, Value};

use crate::support::{eventually, fresh_name, sign_up, signed_out};
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
