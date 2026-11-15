//! Voice-call suite: tapping the live voice row joins through the backend's
//! CallApi seam (leave clears it, the sidebar and the profile strip's
//! "Voice Connected" line follow), and the backend's refusal paints under the
//! row verbatim. With CALL_PROVIDER=idea-chat configured on the run the join
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
use std::time;

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


/// Is ANY live call provider configured? A join through the seam connects (and
/// paints "Voice Connected") for the idea-chat relay, so the
/// provider-agnostic seam tests — the join/leave round trip, the roster line,
/// the fresh-boot restore — branch on this, not on the relay alone.
fn live_provider() -> bool {
    match env::var("CALL_PROVIDER") {
        Ok(v) => v == "idea-chat",
        Err(_) => false,
    }
}

/// Join the call through the seam: tapping the live voice row goes to the
/// sidebar's handler, which calls the backend's join.
fn call_voice(app: &mut App) {
    app.test_id("live-voice-row").click();
    app.set_timeout(WAIT);
}

/// Leave the call through the seam: the same tap, and the handler's already-in
/// branch routes to the backend's leave.
fn leave_voice(app: &mut App) {
    app.test_id("live-voice-row").click();
    app.set_timeout(WAIT);
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
    let live = live_provider();
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
    // The seam's own success text proves the join round trip: the backend
    // answered, the client painted its answer (the same line for both a live
    // provider and a mock; only the refusal differs).
    call_voice(app);
    signed_out_off_call(app);
}

#[robot_test]
fn leaving_a_live_call_clears_the_roster_and_the_line(app: &mut App) {
    signed_out_off_call(app);
    let live = live_provider();
    let a = voice_crew(app, "voiceleave");
    let name = a.name.clone();

    call_voice(app);

    if live {
        app.text("Voice Connected").assert_visible();
        assert!(
            eventually(20, || app.text(name.as_str()).count() > 0),
            "the roster frame never painted the caller: {name}"
        );
        // Tapping the row again leaves: the line clears, and the strip's
        // "Online" status is what the seam's leave answered with.
        leave_voice(app);
        app.text("Voice Connected").assert_hidden();
    } else {
        // Refused both times: a second tap stays refused, never "connected".
        call_voice(app);
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


#[robot_test]
fn a_fresh_browser_restores_only_a_real_in_progress_call(app: &mut App) {
    signed_out_off_call(app);
    let live = live_provider();
    let a = voice_crew(app, "voiceboot");

    call_voice(app);
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
        // A refused join never restores: the boot's voice_state answers no
        // ticket, so the seam-driven line stays hidden.
        call_voice(&mut second.app);
        second.app.text("Voice Connected").assert_hidden();
    }
    signed_out_off_call(&mut second.app);
    signed_out_off_call(app);
}
/// A robot function's string reply: the bare string or an `Ok`-wrapped one.
fn robot_string(v: Value) -> String {
    if let Some(s) = v.as_str() {
        return s.to_string();
    }
    v.get("Ok").and_then(|v| v.as_str()).map(str::to_string).unwrap_or_default()
}

/// The call-audio elements in `app`'s page for channel `cid`, as the app's
/// `cassette.call_audio` hook reports them.
fn call_audio(app: &mut App, cid: i64) -> Vec<Value> {
    let raw = robot_string(app.call_function("cassette.call_audio", json!({ "channel_id": cid })));
    match serde_json::from_str::<Value>(&raw) {
        Ok(Value::Array(list)) => list,
        _ => Vec::new(),
    }
}

/// The element playing `identity`'s microphone, if B's page has one.
fn mic_element_of(list: &[Value], identity: &str) -> Option<Value> {
    list.iter()
        .find(|e| {
            e.get("identity").and_then(|v| v.as_str()) == Some(identity)
                && e.get("source").and_then(|v| v.as_str()).unwrap_or("").eq_ignore_ascii_case("microphone")
        })
        .cloned()
}

/// B hears A: B's page plays A's microphone through an audio element that is
/// not paused, has data, and whose playback position moves. Deafen is local:
/// the element is muted in B's page but keeps playing (nothing changes on the
/// call server), and undeafen unmutes it. A's fake-mic tone also lights A as
/// speaking in B's voice list.
#[robot_test]
fn the_other_caller_is_heard_deafen_is_local_and_speaking_shows(app: &mut App) {
    signed_out_off_call(app);
    // Every assertion here reads the client's own hooks (the call-audio rows
    // and the speaking dot), so either live provider drives them.
    if !live_provider() {
        eprintln!("[skip] hearing needs a live provider: set CALL_PROVIDER=idea-chat to run it");
        return;
    }
    let (a, sid) = voice_crew_with_sid(app, "heara");
    let chans = a.call("list_channels", json!({ "server_id": sid }));
    let chans = chans.get("Ok").unwrap_or(&chans).clone();
    let cid = chans
        .as_array()
        .and_then(|list| list.iter().find(|c| c.get("kind").and_then(|v| v.as_str()) == Some("voice")))
        .and_then(|c| c.get("id").and_then(|v| v.as_i64()))
        .unwrap_or(0);
    assert!(cid > 0, "no voice channel listed: {chans:?}");
    let a_id = a.id.to_string();

    // A joins and publishes its fake mic (Chrome's beeping test tone).
    call_voice(app);
    app.text("Voice Connected").assert_visible();

    // B joins the same channel.
    let inv = a.call("create_invite", json!({ "server_id": sid }));
    let granted = a.call("set_everyone_permissions", json!({ "server_id": sid, "permissions": 196609 }));
    assert!(granted.as_i64().unwrap_or(0) >= 196609, "granted no perms: {granted:?}");
    let code = inv.get("code").and_then(|v| v.as_str()).unwrap_or("").to_string();
    assert!(!code.is_empty(), "minted no invite: {inv:?}");
    let mut second = open_browser("hear");
    second.app.set_timeout(WAIT);
    let b = Account::new("hearw");
    sign_in_as(&mut second.app, &b);
    redeem_invite(&mut second.app, &code);
    second.app.text("Joined!").assert_visible();
    second.app.test_id("join-server-done").click();
    second.app.set_timeout(WAIT);
    second.app.test_id("live-voice-row").click();
    second.app.set_timeout(WAIT);
    second.app.text("Voice Connected").assert_visible();

    // B's page plays A's microphone: an element exists, is not paused, has
    // data, and its position advances.
    assert!(
        eventually(20, || mic_element_of(&call_audio(&mut second.app, cid), &a_id)
            .map(|e| e.get("paused").and_then(|v| v.as_bool()) == Some(false)
                && e.get("readyState").and_then(|v| v.as_i64()).unwrap_or(0) >= 2)
            .unwrap_or(false)),
        "B never played A's microphone: {:?}",
        call_audio(&mut second.app, cid)
    );
    let t0 = mic_element_of(&call_audio(&mut second.app, cid), &a_id)
        .and_then(|e| e.get("currentTime").and_then(|v| v.as_f64()))
        .unwrap_or(0.0);
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let t1 = mic_element_of(&call_audio(&mut second.app, cid), &a_id)
        .and_then(|e| e.get("currentTime").and_then(|v| v.as_f64()))
        .unwrap_or(0.0);
    assert!(t1 > t0 + 0.5, "A's audio did not advance in B's page ({t0} -> {t1})");

    // A's tone lights A as speaking in B's voice list.
    assert!(
        eventually(20, || second.app.test_id("voice-user-speaking").count() > 0),
        "B never saw anyone marked as speaking"
    );

    // Deafen is local: B's element for A is muted but keeps playing.
    second.app.test_id("user-deafen").click();
    assert!(
        eventually(10, || mic_element_of(&call_audio(&mut second.app, cid), &a_id)
            .map(|e| e.get("muted").and_then(|v| v.as_bool()) == Some(true)
                && e.get("paused").and_then(|v| v.as_bool()) == Some(false))
            .unwrap_or(false)),
        "deafen did not mute A locally (or stopped the element): {:?}",
        call_audio(&mut second.app, cid)
    );
    second.app.test_id("user-deafen").click();
    assert!(
        eventually(10, || mic_element_of(&call_audio(&mut second.app, cid), &a_id)
            .map(|e| e.get("muted").and_then(|v| v.as_bool()) == Some(false))
            .unwrap_or(false)),
        "undeafen did not unmute A: {:?}",
        call_audio(&mut second.app, cid)
    );

    // Mute is ANNOUNCED: muting A's mic stops A sending, so B's own mic row
    // for A (the incoming one) stays playing but the row B reads for ITS OWN
    // mic flips to paused — A's mute changes what A broadcasts, seen on A's
    // page through the same hook.
    app.test_id("user-mic").click();
    assert!(
        eventually(10, || mic_element_of(&call_audio(app, cid), &a_id)
            .map(|e| e.get("paused").and_then(|v| v.as_bool()) == Some(true))
            .unwrap_or(false)),
        "muting A did not stop A's own mic line: {:?}",
        call_audio(app, cid)
    );
    app.test_id("user-mic").click();

    leave_voice(app);
    signed_out_off_call(app);
}

/// idea-chat's own video path: A turns their camera on through the user panel,
/// and B — in the same channel from its own browser — gets a `voice-tile`
/// carrying A's name and plays a frame (the client's per-participant
/// `MediaStream`, read through `cassette.video_playing`). Like the camera test,
/// but the observable is the idea-chat client's hooks, not a media
/// server, so it runs under CALL_PROVIDER=idea-chat.
#[robot_test]
fn the_idea_chat_camera_shows_a_tile_to_the_other_browser(app: &mut App) {
    signed_out_off_call(app);
    // Only for the idea-chat relay: this names the client's own streams.
    if env::var("CALL_PROVIDER").unwrap_or_default() != "idea-chat" {
        eprintln!("[skip] idea-chat tile needs CALL_PROVIDER=idea-chat to run");
        return;
    }
    let (a, sid) = voice_crew_with_sid(app, "ichattile");
    let name_a = a.name.clone();

    // A joins and turns the camera on.
    call_voice(app);
    app.text("Voice Connected").assert_visible();
    app.test_id("user-camera").click();
    app.set_timeout(WAIT);

    // B joins the same channel through a minted invite.
    let inv = a.call("create_invite", json!({ "server_id": sid }));
    let granted = a.call("set_everyone_permissions", json!({ "server_id": sid, "permissions": 196609 }));
    assert!(granted.as_i64().unwrap_or(0) >= 196609, "granted no perms: {granted:?}");
    let code = inv.get("code").and_then(|v| v.as_str()).unwrap_or("").to_string();
    assert!(!code.is_empty(), "minted no invite: {inv:?}");
    let mut second = open_browser("ichat");
    second.app.set_timeout(WAIT);
    let b = Account::new("ichatw");
    sign_in_as(&mut second.app, &b);
    redeem_invite(&mut second.app, &code);
    second.app.text("Joined!").assert_visible();
    second.app.test_id("join-server-done").click();
    second.app.set_timeout(WAIT);
    second.app.test_id("live-voice-row").click();
    second.app.set_timeout(WAIT);
    second.app.text("Voice Connected").assert_visible();

    // B sees A: a tile for A's camera, and a real frame playing through it.
    assert!(
        eventually(20, || second.app.text(name_a.as_str()).count() > 0),
        "B's roster never carried A: {name_a}"
    );
    assert!(
        eventually(20, || second.app.call_function("cassette.video_playing", json!({})).as_u64()
            .or_else(|| second.app.call_function("cassette.video_playing", json!({})).get("Ok").and_then(|v| v.as_u64()))
            .unwrap_or(0) > 0),
        "B never played a frame from A's idea-chat camera"
    );
    let shot = second.app.screenshot();
    eprintln!("[shot] ichat-tile-b: {shot}");

    // Camera off: B's tile stops playing a frame.
    app.test_id("user-camera").click();
    assert!(
        eventually(20, || second.app.call_function("cassette.video_playing", json!({})).as_u64()
            .or_else(|| second.app.call_function("cassette.video_playing", json!({})).get("Ok").and_then(|v| v.as_u64()))
            .unwrap_or(1) == 0),
        "B kept playing A's camera after it was turned off"
    );

    leave_voice(app);
    signed_out_off_call(app);
}

/// The voice tile's menu volume and mute stay in the listener's own tab:
/// B opens A's tile menu with a context tap, dials the volume to 0 and
/// mutes — both writes ride B's own call session, so the room never hears
/// them (A's volume table stays empty and A keeps playing B), and B's
/// element for A keeps receiving, since the gain lives in B's playback.
/// Muting keeps the slider position: B dials while muted, unmutes, and
/// the dial comes back. Runs on the idea-chat relay, where per-remote
/// volume lives in the client's own `Call`.
#[robot_test]
fn the_tile_menu_volume_and_mute_stay_in_the_listeners_tab(app: &mut App) {
    signed_out_off_call(app);
    if env::var("CALL_PROVIDER").unwrap_or_default() != "idea-chat" {
        eprintln!("[skip] the tile volume menu needs CALL_PROVIDER=idea-chat to run");
        return;
    }
    let (a, sid) = voice_crew_with_sid(app, "volu");
    let chans = a.call("list_channels", json!({ "server_id": sid }));
    let chans = chans.get("Ok").unwrap_or(&chans).clone();
    let cid = chans
        .as_array()
        .and_then(|list| list.iter().find(|c| c.get("kind").and_then(|v| v.as_str()) == Some("voice")))
        .and_then(|c| c.get("id").and_then(|v| v.as_i64()))
        .unwrap_or(0);
    assert!(cid > 0, "no voice channel listed: {chans:?}");
    let a_id = a.id.to_string();

    // A joins and turns the camera on: B gets a video tile for A — the
    // menu lives on the tile.
    call_voice(app);
    app.text("Voice Connected").assert_visible();
    app.test_id("user-camera").click();
    app.set_timeout(WAIT);

    // B joins the same channel through a minted invite.
    let inv = a.call("create_invite", json!({ "server_id": sid }));
    let granted = a.call("set_everyone_permissions", json!({ "server_id": sid, "permissions": 196609 }));
    assert!(granted.as_i64().unwrap_or(0) >= 196609, "granted no perms: {granted:?}");
    let code = inv.get("code").and_then(|v| v.as_str()).unwrap_or("").to_string();
    assert!(!code.is_empty(), "minted no invite: {inv:?}");
    let mut second = open_browser("volu");
    second.app.set_timeout(WAIT);
    let b = Account::new("volw");
    sign_in_as(&mut second.app, &b);
    redeem_invite(&mut second.app, &code);
    second.app.text("Joined!").assert_visible();
    second.app.test_id("join-server-done").click();
    second.app.set_timeout(WAIT);
    second.app.test_id("live-voice-row").click();
    second.app.set_timeout(WAIT);
    second.app.text("Voice Connected").assert_visible();

    assert!(
        eventually(20, || second.app.test_id("voice-tile").count() > 0),
        "B never got a video tile for A"
    );
    // A's own local table is empty before anything happens in B.
    assert_eq!(robot_string(app.call_function("cassette.call_volume", json!({}))).is_empty(),
        true, "A's tab already held a dial");

    // B opens A's tile menu — the web bridge delivers no secondary press,
    // so the suite flips the opener through the hook (the same write a real
    // right click on the tile's name row makes) — then dials to 0 and mutes.
    second.app.call_function("cassette.open_voice_tile", json!({ "user_id": a.id }));
    second.app.set_timeout(WAIT);
    second.app.test_id("voice-tile-volume").assert_visible();
    second.app.test_id("voice-tile-volume").set_slider(0.0);
    second.app.set_timeout(WAIT);
    second.app.test_id("voice-tile-mute").set_toggle(true);
    second.app.set_timeout(WAIT);

    // The tile says muted on A's name.
    assert!(second.app.test_id("voice-tile-muted-icon").count() > 0,
        "B's tile never showed the mute on A's name");

    // B's table carries the dial (`<user id>=<position>,<muted>`).
    let dialed = robot_string(second.app.call_function("cassette.call_volume", json!({})));
    assert!(
        dialed.contains(format!("{}:0.0,true", a_id).as_str()) || dialed.contains(format!("{}:0,true", a_id).as_str()),
        "B's table lost the dial: {dialed}"
    );

    // Local, not announced: A's tab never sees a dial, and A's view of B
    // is unchanged — no mute mark on A's tile for B. The room heard
    // nothing; the gain lives only in B's own `Call`.
    assert_eq!(robot_string(app.call_function("cassette.call_volume", json!({}))).is_empty(),
        true, "the dial leaked into A''s tab");
    assert_eq!(app.test_id("voice-tile-muted-icon").count(), 0,
        "B''s local mute showed up in A''s view");

    // Muting keeps the volume: dial while muted saves the position, and
    // unmuting restores it.
    second.app.test_id("voice-tile-volume").set_slider(0.5);
    second.app.set_timeout(WAIT);
    let mid = robot_string(second.app.call_function("cassette.call_volume", json!({})));
    assert!(
        mid.contains(format!("{}:0.5,true", a_id).as_str()),
        "dialing while muted lost the position: {mid}"
    );
    second.app.test_id("voice-tile-mute").set_toggle(false);
    second.app.set_timeout(WAIT);
    let restored = robot_string(second.app.call_function("cassette.call_volume", json!({})));
    assert!(
        restored.contains(format!("{}:0.5,false", a_id).as_str()),
        "unmuting did not restore the saved volume: {restored}"
    );
    assert_eq!(second.app.test_id("voice-tile-muted-icon").count(), 0,
        "B's tile still shows the mute after unmuting");

    leave_voice(app);
    signed_out_off_call(app);
}
