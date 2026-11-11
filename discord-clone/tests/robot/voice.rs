//! Voice-call suite: tapping the live voice row joins through the backend's
//! CallApi seam (leave clears it, the sidebar and the profile strip's
//! "Voice Connected" line follow), and the backend's refusal paints under the
//! row verbatim. With CALL_PROVIDER=livekit configured on the run the join
//! issues a ticket: the VoiceRoster frames draw the connected names under the
//! row, leaving drops them, and a fresh browser restores the in-progress call
//! via voice_state's re-issued ticket without a new click. Without a provider
//! every join is refused, so the same seams must paint NO connection line
//! anywhere, including after a fresh boot.
//!
//! The robot cannot drive a real provider socket, so the ticket is only
//! observed through the client's opaque handling: roster names and the
//! connection line are the observables, not audio.

use std::env;

use robot_test::{json, robot_test, App, Value};

use crate::support::{Account, eventually, fresh_name, sign_in_as, signed_out};
use crate::support::WAIT;
use crate::users::open_browser;

/// Sign the caller out through the login screen; when they are still in a
/// voice call their panel reads "Voice Connected" instead of "Online", so
/// leaving the call first is what lets `signed_out` find the settings gear.
pub fn signed_out_off_call(app: &mut App) {
    if app.text("Voice Connected").count() > 0 {
        app.test_id("live-voice-row").click();
        app.set_timeout(WAIT);
    }
    signed_out(app);
}

/// Is this run's dev server configured for livekit calls? The wrapper's
/// `--env` reaches the test binary and the server sidecar alike, so the
/// suite branches instead of pretending a provider that is not there — the
/// same env-branch the GIF picker test makes on GIPHY_API_KEY.
fn livekit_configured() -> bool {
    match env::var("CALL_PROVIDER") {
        Ok(v) => v == "livekit",
        Err(_) => false,
    }
}

/// The rail's create-server modal path from `coverage.rs`, kept local so the
/// module stays independent.
fn create_and_enter(app: &mut App, name: String) {
    app.test_id("rail-create-server").click();
    app.text("Add a Server").assert_visible();
    app.test_id("create-server-name-input").fill(name.as_str());
    app.test_id("create-server-create").click();
    app.test_id("server-header").assert_visible();
    app.text(name.as_str()).assert_visible();
    app.call_function("cassette.rearm_perms", json!({}));
    app.set_timeout(WAIT);
}

/// The live server id for `name` (list_servers rows: `[{id, name, icon_url}]`).
fn server_of(account: &Account, name: String) -> i64 {
    let rows = account.call("list_servers", json!(null));
    let items = rows.get("Ok").unwrap_or(&rows);
    if let Value::Array(items) = items {
        for row in items {
            if row.get("name").and_then(|v| v.as_str()).unwrap_or("") == name {
                return row.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
            }
        }
    }
    panic!("no live server named {name} for this account: {rows:?}")
}

/// Seed a fresh account plus a fresh server it owns with one voice channel
/// named `meet`; sign the account in and enter the server through the rail
/// modal. Returns the account.
fn voice_crew(app: &mut App, prefix: &str) -> Account {
    let a = Account::new(prefix);
    sign_in_as(app, &a);
    let server = fresh_name("VCrew ");
    create_and_enter(app, server.clone());
    let sid = server_of(&a, server);
    a.call("create_channel", json!({ "server_id": sid, "name": "meet", "kind": "voice" }));
    app.text("meet").assert_visible();
    a
}

#[robot_test]
fn joining_the_voice_row_goes_through_the_backend_seam(app: &mut App) {
    signed_out_off_call(app);
    let live = livekit_configured();
    voice_crew(app, "voicetap");

    // The row exists and joining it goes through the backend seam.
    app.test_id("live-voice-row").assert_visible();
    app.test_id("live-voice-row").click();
    app.set_timeout(WAIT);

    if live {
        app.text("Voice Connected").assert_visible();
        app.text("voice is not enabled on this server").assert_hidden();
    } else {
        // No provider: the backend's own sentence paints under the row
        // verbatim, and no phantom join appears anywhere.
        app.text("voice is not enabled on this server").assert_visible();
        app.text("Voice Connected").assert_hidden();
    }
    signed_out_off_call(app);
}

#[robot_test]
fn leaving_a_live_call_clears_the_roster_and_the_line(app: &mut App) {
    signed_out_off_call(app);
    let live = livekit_configured();
    let a = voice_crew(app, "voiceleave");
    let name = a.name.clone();

    app.test_id("live-voice-row").click();
    app.set_timeout(WAIT);

    if live {
        app.text("Voice Connected").assert_visible();
        assert!(
            eventually(20, || app.text(name.as_str()).count() > 0),
            "the roster frame never painted the caller: {name}"
        );
        // Tapping the row again leaves: the line clears.
        app.test_id("live-voice-row").click();
        app.set_timeout(WAIT);
        app.text("Voice Connected").assert_hidden();
    } else {
        // Refused both times: a second tap stays refused, never "connected".
        app.test_id("live-voice-row").click();
        app.set_timeout(WAIT);
        app.text("Voice Connected").assert_hidden();
    }
    signed_out_off_call(app);
}

/// A fresh account plus a fresh server it owns with one voice channel named
/// `meet`; like `voice_crew`, but returns the server id too so a second
/// browser can be invited onto the same server.
fn voice_crew_with_sid(app: &mut App, prefix: &str) -> (Account, i64) {
    let a = Account::new(prefix);
    sign_in_as(app, &a);
    let server = fresh_name("VCrewSid ");
    create_and_enter(app, server.clone());
    let sid = server_of(&a, server);
    a.call("create_channel", json!({ "server_id": sid, "name": "meet", "kind": "voice" }));
    app.text("meet").assert_visible();
    (a, sid)
}

/// Redeem an invite code through the rail's join modal (the coverage.rs
/// steps, kept local so this module stays independent).
fn redeem_invite(app: &mut App, code: &str) {
    app.test_id("rail-create-server").click();
    app.text("Add a Server").assert_visible();
    app.test_id("create-server-join-link").click();
    app.text("Join a Server").assert_visible();
    app.test_id("join-server-code-input").fill(code);
    app.test_id("join-server-join").click();
    app.set_timeout(WAIT);
}

/// Fake-media camera test: A turns their camera on through the user panel's
/// Video button (the seam broadcasts the roster frame), and B — in the same
/// server and channel from its own browser — paints a video tile carrying
/// A's name. The robot cannot drive a real provider socket, so the tile is
/// the observable, not actual pixels.
#[robot_test]
fn a_camera_on_shows_a_video_tile_to_the_other_browser(app: &mut App) {
    signed_out_off_call(app);
    assert!(livekit_configured(), "camera tiles only render with a live provider");
    let (a, sid) = voice_crew_with_sid(app, "camerab");
    let name_a = a.name.clone();

    // A joins the voice channel.
    app.test_id("live-voice-row").click();
    app.set_timeout(WAIT);
    app.text("Voice Connected").assert_visible();

    // B joins the same server through a minted invite. @everyone gets
    // Connect + Speak (bit 0 view + 16 connect + 17 speak) before B's
    // join arms its permission snapshot, so B can join the voice row too.
    let inv = a.call("create_invite", json!({ "server_id": sid }));
    let granted = a.call("set_everyone_permissions", json!({ "server_id": sid, "permissions": 196609 }));
    assert!(granted.as_i64().unwrap_or(0) >= 196609, "granted no perms: {granted:?}");
    let code = inv.get("code").and_then(|v| v.as_str()).unwrap_or("").to_string();
    assert!(!code.is_empty(), "minted no invite: {inv:?}");
    let mut second = open_browser("cam");
    second.app.set_timeout(WAIT);
    let b = Account::new("cameraw");
    sign_in_as(&mut second.app, &b);
    redeem_invite(&mut second.app, &code);
    second.app.text("Joined!").assert_visible();
    second.app.test_id("join-server-done").click();
    second.app.set_timeout(WAIT);
    second.app.text("meet").assert_visible();

    // A turns the camera on through the user panel's Video button.
    app.test_id("user-camera").click();
    app.set_timeout(WAIT);

    // B joins too, and the roster frame shows A: a video tile with A's name.
    second.app.test_id("live-voice-row").click();
    second.app.set_timeout(WAIT);
    assert!(
        eventually(20, || second.app.test_id("voice-tile").count() > 0),
        "B never got a video tile"
    );
    assert!(
        eventually(20, || second.app.text(name_a.as_str()).count() > 0),
        "the tile never carried A's name: {name_a}"
    );
    let shot = second.app.screenshot();
    eprintln!("[shot] cam-tile-b: {shot}");

    // Leave the call so A's panel reads "Online" again and the suite's
    // signed-out cleanup applies.
    app.test_id("live-voice-row").click();
    app.set_timeout(WAIT);
    signed_out_off_call(app);
}

#[robot_test]
fn a_fresh_browser_restores_only_a_real_in_progress_call(app: &mut App) {
    signed_out_off_call(app);
    let live = livekit_configured();
    let a = voice_crew(app, "voiceboot");

    app.test_id("live-voice-row").click();
    app.set_timeout(WAIT);
    if live {
        app.text("Voice Connected").assert_visible();
    }

    // A fresh boot (login screen) re-runs the restore effects: voice_state
    // answers with a ticket ONLY when the caller is connected, so with a live
    // join the line repaints without a new click, and after a refusal a fresh
    // boot must NOT paint a phantom connection.
    let mut second = open_browser("boot");
    second.app.set_timeout(WAIT);
    sign_in_as(&mut second.app, &a);
    second.app.set_timeout(WAIT);
    second.app.text("meet").assert_visible();
    if live {
        second.app.text("Voice Connected").assert_visible();
    } else {
        second.app.text("Voice Connected").assert_hidden();
    }
    signed_out_off_call(&mut second.app);
    signed_out_off_call(app);
}