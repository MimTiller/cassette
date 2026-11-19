//! Gaps the earlier modules left open: a sticker sent from the sticker
//! picker beside the GIF button in a DM shows for the other friend without a
//! reload; an animated WebP server emoji uploads whole (the picker keeps
//! animations unflattened) and renders in a message; and a still larger than
//! the 128×128 draw box is shrunk on the browser's canvas before upload and
//! still renders.
//!
//! Like `coverage.rs`, the Upload button's native file pick cannot be
//! tapped by the robot, so uploads run through the `cassette.upload_server_emoji`
//! and `cassette.upload_server_sticker` hooks, which feed the same
//! animated/still split and canvas shrink the button performs after its pick.

use robot_test::{json, robot_test, App, Value};

use crate::support::{Account, eventually, fresh_name, sign_in_as, signed_out};
use crate::support::WAIT;
use crate::users::open_browser;

/// The prove...
const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAIAAABLbSncAAAACXBIWXMAAAABAAAAAQBPJcTWAAAAEUlEQVR4nGOQjVqOFTEMLQkAzd1HgU4BMj8AAAAASUVORK5CYII=";

/// A real 2-frame animated WebP, the same bytes the discord-server
/// uploads_are_validated test ships through its sniffing media store: under
/// the cap, so the picker's animated() sniff must keep it whole.
const WEBP_ANIM: &str = "UklGRsAAAABXRUJQVlA4WAoAAAACAAAABwAABwAAQU5JTQYAAAD/////AABBTk1GSAAAAAAAAAAAAAcAAAcAAMgAAAJWUDggMAAAANABAJ0BKggACAACADQloAJ0ugH4AAOwAP7w6Pf/ILlhdcjX/yA/4yp8ZU/48gAAAEFOTUZEAAAAAAAAAAAABwAABwAAyAAAAFZQOCAsAAAAlAEAnQEqCAAIAAAANCWgAnS6AAOYAP75k2//kB//kB//kB//ID/iF3sgMAA=";

/// A 400×400 PNG — over the 128×128 draw box but under the 256 KB cap, so
/// the picker shrinks it on a canvas (encode_still_resized) before uploading.
const BIG_PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAZAAAAGQCAYAAACAvzbMAAAFnElEQVR4nO3VMQ0AIQDAQCQg4aXg3xRo+C6E5Ibbu3XMb20A+GvcDgDgTQYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQGAkBiIAAkBgJAYiAAJAYCQGIgACQHPTszgU2A9d4AAAAASUVORK5CYII=";

/// Screenshot moment, named like the suite's files.
fn shot(app: &mut App, label: &str) {
    let path = app.screenshot();
    assert!(path.ends_with(".png"), "screenshot was not saved: {path}");
    eprintln!("[shot] {label}: {path}");
}

/// Open the rail's create-server modal, name it, press Create, and land in
/// the new server with its permission snapshot re-armed.
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

/// Wait until the client's emoji table knows `name`, the way the
/// EmojiChanged frame refetches it.
fn waits_for_emoji(app: &mut App, name: &str) {
    let listed = eventually(
        20,
        || {
            let reply = app.call_function("cassette.custom_emoji_names", json!({}));
            match reply.get("Ok").unwrap_or(&reply) {
                Value::Array(items) => items.iter().any(|v| v.as_str().unwrap_or("") == name),
                _ => false,
            }
        },
    );
    assert!(listed, "the uploaded emoji never reached the emoji table");
}

#[robot_test(suites = [legacy, look_and_feel])]
fn an_animated_webp_server_emoji_uploads_and_renders_in_a_message(app: &mut App) {
    signed_out(app);
    let a = Account::new("covwebp");
    sign_in_as(app, &a);
    let server = fresh_name("Cov Webp ");
    create_and_enter(app, &server);

    // Upload the animated WebP: the picker's animated() sniff keeps it whole
    // (no canvas flattening) and the server stores it as .webp.
    app.call_function(
        "cassette.upload_server_emoji",
        json!({ "name": "covwebp", "mime": "image/webp", "data_b64": WEBP_ANIM }),
    );
    waits_for_emoji(app, "covwebp");

    app.text("general").click();
    app.set_timeout(WAIT);
    app.test_id("composer").fill("watch :covwebp: move");
    app.test_id("composer").press_key("Enter");
    app.test_id("msg-row").assert_visible();
    app.test_id("emoji-image").assert_visible();
    app.text(":covwebp:").assert_hidden();
    shot(app, "webp-emoji-render");
}

#[robot_test(suites = [legacy, look_and_feel])]
fn a_large_still_server_emoji_is_shrunk_before_upload(app: &mut App) {
    signed_out(app);
    let a = Account::new("covbig");
    sign_in_as(app, &a);
    let server = fresh_name("Cov Big ");
    create_and_enter(app, &server);

    // A 400×400 PNG fits the byte cap but not the 128×128 draw box, so the
    // picker redraws it on a canvas (encode_still_resized) and uploads the
    // shrunk webp; the message then renders that shrunk image.
    app.call_function(
        "cassette.upload_server_emoji",
        json!({ "name": "covbig", "mime": "image/png", "data_b64": BIG_PNG }),
    );
    waits_for_emoji(app, "covbig");

    app.text("general").click();
    app.set_timeout(WAIT);
    app.test_id("composer").fill("big :covbig: fits");
    app.test_id("composer").press_key("Enter");
    app.test_id("msg-row").assert_visible();
    app.test_id("emoji-image").assert_visible();
    app.text(":covbig:").assert_hidden();
    shot(app, "shrunk-emoji-render");
}

#[robot_test(suites = [legacy, look_and_feel])]
fn a_sticker_sent_from_the_dm_picker_shows_for_the_other_friend(app: &mut App) {
    // A and B are friends before either signs in: the DM search starts
    // conversations with friends, like the dms_friends suite seeds them.
    let a = Account::new("dmstka");
    let b = Account::new("dmstkb");
    a.call("send_friend_request", json!({ "username": b.name }));
    b.call("accept_friend_request", json!({ "user_id": a.id }));

    signed_out(app);
    sign_in_as(app, &a);
    app.set_timeout(WAIT);

    // A owns a server and uploads a sticker for it, so the picker has one.
    let server = fresh_name("DM Sticker ");
    create_and_enter(app, &server);
    let reply = app.call_function(
        "cassette.upload_server_sticker",
        json!({ "name": "dmwave", "related": "\u{1F44B}", "mime": "image/png", "data_b64": PNG }),
    );
    assert!(reply.as_str().is_some_and(|id| id.parse::<i64>().is_ok()), "sticker upload failed: {reply}");

    // A opens the DM screen and starts a conversation with B from the find
    // box, then sends the sticker through the picker beside the GIF button.
    app.test_id("rail-direct-messages").click();
    app.test_id("live-dm-find").assert_visible();
    app.test_id("live-dm-find").fill(&b.name);
    app.test_id("live-dm-find-result").assert_visible();
    app.test_id("live-dm-find-result").click();
    app.test_id("live-dm-composer").assert_visible();
    app.test_id("sticker-button").click();
    app.test_id("sticker-search-input").fill("wave");
    app.test_id("sticker-tile").assert_visible();
    app.test_id("sticker-tile").click();
    let sent = eventually(10, || app.test_id("sticker-image").is_visible());
    assert!(sent, "the sticker send never echoed back into the DM");
    shot(app, "dm-sticker-sent");

    // B opens its DM screen: the brand-new conversation is in the list live
    // (DmChanged frame), and opening it shows A's sticker.
    let mut second = open_browser("b");
    second.app.set_timeout(WAIT);
    sign_in_as(&mut second.app, &b);
    second.app.test_id("rail-direct-messages").click();
    let row = eventually(15, || second.app.test_id("live-dm-row").is_visible());
    assert!(row, "B never saw the DM conversation A started");
    second.app.test_id("live-dm-row").click();
    let seen = eventually(10, || second.app.test_id("sticker-image").is_visible());
    assert!(seen, "B never saw the sticker A sent from the DM picker");
    shot(&mut second.app, "dm-sticker-b-sees");
    second.app.test_id("dm-done-pill").click();

    // Leave A's DM pane as entered, so the sidebar is clickable for later
    // tests sharing this browser.
    app.test_id("dm-done-pill").click();
}
