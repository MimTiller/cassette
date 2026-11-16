//! Instance Settings: the admin's Chat Backend page. The run's first
//! instance-admin seat is empty, so the suite's first test signs up a fresh
//! host through the API, signs the browser into it, and claims the seat
//! through the setup card. The later tests sign the browser back into that
//! SAME account (the run's DB is shared, and its admin row persists) rather
//! than claiming again. The page shows the picker with Built-in selected,
//! the idea-chat URL field, and the chat "Test connection" answers — an
//! unusable URL and an unreachable one each paint their exact refusal in
//! red. Nothing here switches the robot instance to idea-chat: a switch
//! arms the confirm dialog, and Cancel plus Reset drop the draft back.

use std::sync::OnceLock;

use robot_test::{json, robot_test, App, Value};

use crate::support::{Account, eventually, sign_in_as, signed_out};
use crate::support::WAIT;

/// The admin this module's first test claimed: the run's DB is shared, so
/// the later tests sign back into the SAME account (its admin row persists)
/// instead of claiming again — a second claim would hit the "someone is
/// already admin" refusal.
static ADMIN: OnceLock<Account> = OnceLock::new();

fn admin() -> &'static Account {
    ADMIN.get_or_init(move || Account::new("insthost"))
}

/// Open the Instance Settings area from the User Settings sidebar's row.
fn open_instance_settings(app: &mut App) {
    app.test_id("user-settings").click();
    app.test_id("settings-nav-instance-settings").click();
}

#[robot_test]
fn the_chat_backend_picker_opens_on_built_in(app: &mut App) {
    signed_out(app);
    let host = admin();
    sign_in_as(app, host);
    app.text("Online").assert_visible();

    open_instance_settings(app);
    // The seat was empty at signup, so the area opened on the setup card.
    app.test_id("instance-claim").assert_visible();
    app.test_id("instance-claim").click();
    app.text("Call provider").assert_visible();

    // The picker: Built-in leads, the idea-chat row and its URL field show.
    app.text("Built-in RWND server").assert_visible();
    app.text("idea-chat server").assert_visible();
    app.test_id("inst-idea-input").assert_visible();

    // Built-in is the selected backend: a Test connection with nothing
    // typed answers with the no-URL refusal, i.e. no idea-chat server was
    // ever saved.
    app.test_id("instance-chat-test").click();
    let refused = "Test connection: the idea-chat URL is not usable: it must start with http:// or https://";
    let shown = eventually(10, move || app.text(refused).is_visible());
    assert!(shown, "the empty-URL refusal should paint under the button");
}

#[robot_test]
fn the_chat_connection_test_refuses_a_bad_url(app: &mut App) {
    signed_out(app);
    let host = admin();
    sign_in_as(app, host);
    app.text("Online").assert_visible();

    open_instance_settings(app);
    app.text("Call provider").assert_visible();

    app.test_id("inst-idea-input").fill("ftp://nope");
    app.test_id("instance-chat-test").click();
    let refused = "Test connection: the idea-chat URL is not usable: it must start with http:// or https://";
    let shown = eventually(10, move || app.text(refused).is_visible());
    assert!(shown, "the URL-shape refusal should paint under the button");
}

#[robot_test]
fn an_unreachable_idea_chat_server_answers_clearly(app: &mut App) {
    signed_out(app);
    let host = admin();
    sign_in_as(app, host);
    app.text("Online").assert_visible();

    open_instance_settings(app);
    app.text("Call provider").assert_visible();

    app.test_id("inst-idea-input").fill("http://127.0.0.1:9/");
    app.set_timeout(WAIT);
    app.test_id("instance-chat-test").click();
    let refused = "Test connection: the idea-chat server could not be reached: connection refused (a TLS failure answers the same way)";
    let shown = eventually(15, move || app.text(refused).is_visible());
    assert!(shown, "the unreachable-server refusal should paint under the button");
}

#[robot_test]
fn switching_the_chat_backend_asks_to_confirm(app: &mut App) {
    signed_out(app);
    let host = admin();
    sign_in_as(app, host);
    app.text("Online").assert_visible();

    open_instance_settings(app);
    app.text("Call provider").assert_visible();

    // Pick idea-chat with a URL, then Save: the switch arms the confirm
    // dialog instead of saving — its explainer and both buttons show.
    app.label("idea-chat server").click();
    app.test_id("inst-idea-input").fill("http://127.0.0.1:9/");
    app.test_id("instance-save").click();
    app.text("switching chat backends: existing RWND messages are NOT moved and every client reloads onto the new backend").assert_visible();
    app.test_id("instance-chat-confirm-yes").assert_visible();
    app.test_id("instance-chat-confirm-no").assert_visible();

    // Cancel: the pick drops back to Built-in and nothing saved — after a
    // Reset clears the drafts, the test answers with the empty-URL refusal
    // again, proving the saved backend is still built-in.
    app.test_id("instance-chat-confirm-no").click();
    app.label("Built-in RWND server").click();
    app.test_id("instance-reset").click();
    app.test_id("instance-chat-test").click();
    let refused = "Test connection: the idea-chat URL is not usable: it must start with http:// or https://";
    let shown = eventually(10, || app.text(refused).is_visible());
    assert!(shown, "after Cancel the saved backend is still built-in");
    app.text("Built-in RWND server").assert_visible();
}
