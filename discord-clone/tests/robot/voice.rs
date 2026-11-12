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
use std::time;

use hmac::{Hmac, Mac};
use sha2::Sha256;

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
    // The seam's own success text proves the join round trip: the backend
    // answered, the client painted its answer (the same line for both a live
    // provider and a mock; only the refusal differs).
    call_voice(app);
    signed_out_off_call(app);
}

#[robot_test]
fn leaving_a_live_call_clears_the_roster_and_the_line(app: &mut App) {
    signed_out_off_call(app);
    let live = livekit_configured();
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

    // A joins the voice channel through the seam.
    call_voice(app);

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

    // B joins too through the seam, and the roster frame shows A: a video tile
    // with A's name.
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
    // The tile is real video, not the avatar stand-in: LiveKit lists A's
    // camera track, and B's page has a <video> painting frames.
    let url = env::var("LIVEKIT_URL").unwrap_or_default();
    let api_key = env::var("LIVEKIT_API_KEY").unwrap_or_default();
    let secret = env::var("LIVEKIT_API_SECRET").unwrap_or_default();
    let chans = a.call("list_channels", json!({ "server_id": sid }));
    let chans = chans.get("Ok").unwrap_or(&chans).clone();
    let cid = chans
        .as_array()
        .and_then(|list| list.iter().find(|c| c.get("kind").and_then(|v| v.as_str()) == Some("voice")))
        .and_then(|c| c.get("id").and_then(|v| v.as_i64()))
        .unwrap_or(0);
    let room = format!("rwnd-ch-{cid}");
    let me = a.id.to_string();
    assert!(
        eventually(20, || tracks_of(url.clone(), api_key.clone(), secret.clone(), room.clone(), me.clone())
            .iter()
            .any(|(kind, source)| kind == "VIDEO" && source == "CAMERA")),
        "LiveKit never listed A's camera track in {room}"
    );
    assert!(
        eventually(20, || second.app.call_function("cassette.video_playing", json!({})).as_u64()
            .or_else(|| second.app.call_function("cassette.video_playing", json!({})).get("Ok").and_then(|v| v.as_u64()))
            .unwrap_or(0) > 0),
        "B's tile never painted a video frame"
    );
    let shot = second.app.screenshot();
    eprintln!("[shot] cam-tile-b: {shot}");

    // Camera off: A's camera track leaves the room, and B's tile stops
    // playing it.
    app.test_id("user-camera").click();
    assert!(
        eventually(20, || !tracks_of(url.clone(), api_key.clone(), secret.clone(), room.clone(), me.clone())
            .iter()
            .any(|(_, source)| source == "CAMERA")),
        "A's camera track stayed in {room} after turning it off"
    );
    assert!(
        eventually(20, || second.app.call_function("cassette.video_playing", json!({})).as_u64()
            .or_else(|| second.app.call_function("cassette.video_playing", json!({})).get("Ok").and_then(|v| v.as_u64()))
            .unwrap_or(1) == 0),
        "B kept playing A's camera after it was turned off"
    );

    // Leave the call through the seam so A's panel reads "Online" again and
    // the suite's signed-out cleanup applies.
    leave_voice(app);
    signed_out_off_call(app);
}

#[robot_test]
fn a_fresh_browser_restores_only_a_real_in_progress_call(app: &mut App) {
    signed_out_off_call(app);
    let live = livekit_configured();
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

// ── provider-side check: ask the LiveKit media server directly ─────────────
//
// The client's own roster tells nothing about the wire, so a live call is
// verified against the media server itself: the Twirp `ListParticipants`
// endpoint needs a JWT whose `video` grant nests `roomAdmin` and the exact
// room (the grant shape verified against the media server, not the docs).
// The room name is what the SERVER derives from the channel id
// (`room_for(channel_id)` = `rwnd-ch-<id>`), re-derived from create_channel's
// echoed id rather than invented. The run's Chrome carries fake-media flags,
// so `getUserMedia` publishes a real synthetic audio track: a joined caller
// must show up under their identity with an audio track, and leaving must
// empty the room. A "connected but silent" join fails the first assertion;
// a socket that never drops fails the second.

/// Base64url, no padding — the JWT segments the media server expects (standard
/// base64 segments are rejected). Mirrors `discord-server/src/voice.rs`.
fn b64url(bytes: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = *chunk.first().unwrap_or(&0) as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        out.push(A[((b0 >> 2) & 63) as usize] as char);
        out.push(A[((b0 << 4 | b1 >> 4) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(A[((b1 << 2 | b2 >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(A[(b2 & 63) as usize] as char);
        }
    }
    out
}

/// The HS256 signature segment for a JWT body over `secret`: the same hmac +
/// sha2 stack the server signs the join ticket with (pinned to the same
/// crates), so the media server accepts the token the client got.
fn robot_jwt_sign(body: &[u8], secret: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("hmac accepts any key length");
    mac.update(body);
    b64url(&mac.finalize().into_bytes())
}

/// A LiveKit admin token for room `room`: `roomAdmin` nests inside the `video`
/// grant alongside the exact room name (what `EnsureAdminMiddleware` checks
/// for a `ListParticipants` call). `nbf` backdates by the TTL so a provider
/// clock skewed against this host (the LAN box runs minutes off) still
/// accepts the token.
fn admin_token(api_key: String, secret: String, room: String) -> String {
    let now = time::SystemTime::now()
        .duration_since(time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(1_000);
    let ttl = 600;
    let exp = now + ttl;
    let nbf_at = now - ttl;
    let header_seg = b64url(b"{\"alg\":\"HS256\",\"typ\":\"JWT\"}");
    let claims = json!({
        "iss": api_key,
        "sub": "robot-admin",
        "iat": now,
        "nbf": nbf_at,
        "exp": exp,
        "video": { "room": room, "roomAdmin": true },
    });
    let claims_seg = b64url(claims.to_string().as_bytes());
    let body = header_seg + "." + &claims_seg;
    let sig = robot_jwt_sign(body.as_bytes(), secret.as_bytes());
    body + "." + &sig
}

/// The media server's participants of `room`, each with the tracks it
/// published (an empty array when the room holds nobody). A ws/wss join URL
/// is the same address over http/https — the swap the admin test path makes
/// too.
fn participants(url: String, api_key: String, secret: String, room: String) -> Vec<Value> {
    let http =
        if url.starts_with("ws://") {
            url.replace("ws://", "http://")
        } else if url.starts_with("wss://") {
            url.replace("wss://", "https://")
        } else {
            url
        };
    let endpoint = format!("{http}/twirp/livekit.RoomService/ListParticipants");
    let token = admin_token(api_key, secret, room.clone());
    let reply: Value = ureq::post(&endpoint)
        .set("authorization", &format!("Bearer {token}"))
        .set("content-type", "application/json")
        .send_string(&json!({ "room": room }).to_string())
        .unwrap_or_else(|e| panic!("ListParticipants {room}: {e}"))
        .into_json()
        .unwrap_or_else(|e| panic!("ListParticipants {room}: unreadable reply: {e}"));
    match reply.get("participants") {
        Some(v) => match v.as_array() {
            Some(parts) => parts.to_vec(),
            None => Vec::new(),
        },
        None => Vec::new(),
    }
}

#[robot_test]
fn the_media_server_lists_the_joined_caller_with_published_audio(app: &mut App) {
    signed_out_off_call(app);
    assert!(livekit_configured(), "provider assertions need a live provider");
    // The SAME env the join used: the key/secret pairing, not the endpoint,
    // is the gate — a token signed with a different secret fails the media
    // server's 401 even though the join itself was accepted.
    let url = env::var("LIVEKIT_URL").unwrap_or_default();
    let api_key = env::var("LIVEKIT_API_KEY").unwrap_or_default();
    let secret = env::var("LIVEKIT_API_SECRET").unwrap_or_default();
    assert!(!url.is_empty() && !api_key.is_empty() && !secret.is_empty(), "livekit env not forwarded into the run");

    // A fresh crew whose server's voice channel fixes the room name the same
    // way the server derives it (rwnd-ch-<channel id>).
    let (a, sid) = voice_crew_with_sid(app, "provchk");
    // The voice row the join clicks is the crew's "meet" channel, so that is
    // the room to ask LiveKit about.
    let chans = a.call("list_channels", json!({ "server_id": sid }));
    let chans = chans.get("Ok").unwrap_or(&chans).clone();
    let cid = chans
        .as_array()
        .and_then(|list| list.iter().find(|c| c.get("kind").and_then(|v| v.as_str()) == Some("voice")))
        .and_then(|c| c.get("id").and_then(|v| v.as_i64()))
        .unwrap_or(0);
    assert!(cid > 0, "no voice channel listed: {chans:?}");
    // The room the server puts the channel in (`room_for(channel_id)`).
    let room = format!("rwnd-ch-{cid}");

    // Join through the seam; the page publishes its fake mic to the room.
    call_voice(app);

    let caller_id = a.id.to_string();
    assert!(
        eventually(
            20,
            || {
                let parts = participants(url.clone(), api_key.clone(), secret.clone(), room.clone());
                parts.iter().any(|p: &Value| {
                    let published: Vec<Value> = if let Some(Value::Array(t)) = p.get("tracks") {
                        t.to_vec()
                    } else {
                        Vec::new()
                    };
                    p.get("identity").and_then(|v| v.as_str()).unwrap_or("") == caller_id
                        && published.iter().any(|t: &Value| t.get("type").and_then(|v| v.as_str()).unwrap_or("") == "AUDIO")
                })
            },
        ),
        "the media server never listed the joined caller with audio: room {room}"
    );

    // Leaving drops the socket: the room must stop listing the caller
    // (an empty `{"participants":[]}` once nobody remains).
    leave_voice(app);
    assert!(
        eventually(
            20,
            || {
                let parts = participants(url.clone(), api_key.clone(), secret.clone(), room.clone());
                parts.iter().all(|p: &Value| p.get("identity").and_then(|v| v.as_str()).unwrap_or("") != caller_id)
            },
        ),
        "the caller stayed in the room after leaving"
    );

    signed_out_off_call(app);
}

/// The tracks LiveKit lists for `identity` in `room`, as (type, source) pairs
/// in LiveKit's own words ("VIDEO", "SCREEN_SHARE"; "AUDIO", "SCREEN_SHARE_AUDIO").
fn tracks_of(url: String, api_key: String, secret: String, room: String, identity: String) -> Vec<(String, String)> {
    let parts = participants(url, api_key, secret, room);
    let mut out: Vec<(String, String)> = Vec::new();
    for p in parts.iter() {
        if p.get("identity").and_then(|v| v.as_str()).unwrap_or("") != identity {
            continue;
        }
        if let Some(Value::Array(tracks)) = p.get("tracks") {
            for t in tracks.iter() {
                let kind = t.get("type").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let source = t.get("source").and_then(|v| v.as_str()).unwrap_or("").to_string();
                out.push((kind, source));
            }
        }
    }
    out
}

/// Share Screen publishes a real screen track: LiveKit lists A's SCREEN_SHARE
/// video, B (in the same channel) gets a tile, and stopping the share takes
/// the track out of the room. Chrome's fake media flags supply the screen.
#[robot_test]
fn a_screen_share_reaches_the_media_server_and_the_other_browser(app: &mut App) {
    signed_out_off_call(app);
    assert!(livekit_configured(), "screen share needs a live provider");
    let url = env::var("LIVEKIT_URL").unwrap_or_default();
    let api_key = env::var("LIVEKIT_API_KEY").unwrap_or_default();
    let secret = env::var("LIVEKIT_API_SECRET").unwrap_or_default();

    let (a, sid) = voice_crew_with_sid(app, "sharea");
    let chans = a.call("list_channels", json!({ "server_id": sid }));
    let chans = chans.get("Ok").unwrap_or(&chans).clone();
    let cid = chans
        .as_array()
        .and_then(|list| list.iter().find(|c| c.get("kind").and_then(|v| v.as_str()) == Some("voice")))
        .and_then(|c| c.get("id").and_then(|v| v.as_i64()))
        .unwrap_or(0);
    assert!(cid > 0, "no voice channel listed: {chans:?}");
    let room = format!("rwnd-ch-{cid}");
    let me = a.id.to_string();

    // A joins and shares the screen.
    call_voice(app);
    app.text("Voice Connected").assert_visible();
    app.test_id("user-share").click();
    app.set_timeout(WAIT);
    assert!(
        eventually(20, || tracks_of(url.clone(), api_key.clone(), secret.clone(), room.clone(), me.clone())
            .iter()
            .any(|(kind, source)| kind == "VIDEO" && source == "SCREEN_SHARE")),
        "LiveKit never listed A's screen-share track in {room}: {:?}",
        tracks_of(url.clone(), api_key.clone(), secret.clone(), room.clone(), me.clone())
    );
    // The screen's sound rides as its own track (Chrome's fake capture
    // supplies one, as a shared tab or a Windows screen would).
    assert!(
        eventually(20, || tracks_of(url.clone(), api_key.clone(), secret.clone(), room.clone(), me.clone())
            .iter()
            .any(|(kind, source)| kind == "AUDIO" && source == "SCREEN_SHARE_AUDIO")),
        "LiveKit never listed A's screen-share audio track in {room}"
    );

    // B joins the same channel and gets a tile for A's share.
    let inv = a.call("create_invite", json!({ "server_id": sid }));
    let granted = a.call("set_everyone_permissions", json!({ "server_id": sid, "permissions": 196609 }));
    assert!(granted.as_i64().unwrap_or(0) >= 196609, "granted no perms: {granted:?}");
    let code = inv.get("code").and_then(|v| v.as_str()).unwrap_or("").to_string();
    assert!(!code.is_empty(), "minted no invite: {inv:?}");
    let mut second = open_browser("share");
    second.app.set_timeout(WAIT);
    let b = Account::new("sharew");
    sign_in_as(&mut second.app, &b);
    redeem_invite(&mut second.app, &code);
    second.app.text("Joined!").assert_visible();
    second.app.test_id("join-server-done").click();
    second.app.set_timeout(WAIT);
    second.app.test_id("live-voice-row").click();
    second.app.set_timeout(WAIT);
    assert!(
        eventually(20, || second.app.test_id("voice-tile").count() > 0),
        "B never got a tile for A's screen share"
    );
    let shot = second.app.screenshot();
    eprintln!("[shot] share-tile-b: {shot}");

    // A stops sharing: the screen track leaves the room.
    app.test_id("user-share").click();
    assert!(
        eventually(20, || !tracks_of(url.clone(), api_key.clone(), secret.clone(), room.clone(), me.clone())
            .iter()
            .any(|(_, source)| source == "SCREEN_SHARE" || source == "SCREEN_SHARE_AUDIO")),
        "A's screen-share track stayed in {room} after stopping"
    );

    leave_voice(app);
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
    assert!(livekit_configured(), "hearing needs a live provider");
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

    leave_voice(app);
    signed_out_off_call(app);
}
