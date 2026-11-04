//! Messages suite (ported from e2e/test_messages.py): reply to a message
//! through the hover bar's ↩ pill and see it with the "Replying to" preview;
//! add a reaction through the 😊 pill and the emoji picker, then tap the pill
//! back off; pin through the overflow menu, see the toast, the header badge
//! and the pins panel listing the message; markdown (bold and inline code)
//! parses its styled runs; scrolling up shows the Jump to bottom pill and
//! it jumps back; a second browser's uncommitted typing shows "… is typing..."
//! on the first user's screen; an @mention paints its pill.
//!
//! The Python suite found the emoji picker cell and the reaction pill by
//! painted text positions; here the picker's cells carry `emoji-cell`, the
//! pill carries `reaction-pill`, the pins panel's title and close carry
//! `pins-panel-title` / `pins-panel-close`, and the header's count pill
//! `pin-count-badge`, so every step is a test_id or exact text.
//!
//! The message row's markdown runs are painted through the markdown SDK's
//! scene (semantic DOM built outside the robot registry), so the robot sees
//! no "boldword" text node: the parse the renderer paints with is exposed
//! as the `cassette.md_runs` hook in robot_hooks.rs and asserted as data.
//!
//! Not ported: the Python "a long message wraps" / "leaves the member list
//! its width" checks (pixel geometry the robot cannot read, and wrapping is
//! not in the ported behaviour list) and the Python "no uncaught page
//! exceptions" check — the same robot gap `server.rs` records (no page-error
//! log surface).

use robot_test::{json, robot_test, App, Value};

use crate::support::{Account, eventually, fresh_name, sign_in_as, signed_out};
use crate::support::WAIT;
use crate::users::open_browser;

/// A fresh pair of accounts plus a fresh server A owns and B is a member of
/// (with its seeded #general), made through the server like the Python
/// `_srv` setup; every test calls this so no state crosses tests.
fn crew(prefix: &str) -> (Account, Account, String) {
    let a = Account::new(prefix);
    let b = Account::new(&format!("{prefix}b"));
    let server = fresh_name("Msg Crew ");
    let sid = a.call("create_server", json!({ "name": server }))["id"].as_i64().expect("created server id");
    a.call("add_member", json!({ "server_id": sid, "user_id": b.id }));
    (a, b, server)
}

/// Screenshot moment, named like the Python suite's files.
fn shot(app: &mut App, label: &str) {
    let path = app.screenshot();
    assert!(path.ends_with(".png"), "screenshot was not saved: {path}");
    eprintln!("[shot] {label}: {path}");
}

#[robot_test]
fn replying_shows_the_preview_and_lands_in_the_thread(app: &mut App) {
    let (a, _b, server) = crew("reply");
    signed_out(app);
    sign_in_as(app, &a);
    app.text(&server).assert_visible();
    app.text("general").click();
    app.set_timeout(WAIT);

    let target = "the target message";
    app.test_id("composer").fill(&target);
    app.test_id("composer").press_key("Enter");
    app.text(&target).assert_visible();

    // Hover the row so its action bar mounts, then press the reply pill.
    app.test_id("msg-row").hover_native();
    app.test_id("msg-reply").click();

    // The composer's preview bar names the message being replied to.
    app.text("Replying to").assert_visible();
    shot(app, "msg1-reply-preview");

    let reply = "reply body here";
    app.test_id("composer").fill(&reply);
    app.test_id("composer").press_key("Enter");
    // The reply arrives in the same channel list as its own row.
    app.text(&reply).assert_visible();
    shot(app, "msg1-reply");
}

#[robot_test]
fn a_reaction_pill_is_added_and_tapped_back_off(app: &mut App) {
    let (a, _b, server) = crew("react");
    signed_out(app);
    sign_in_as(app, &a);
    app.text(&server).assert_visible();
    app.text("general").click();
    app.set_timeout(WAIT);

    // The fresh create path never fetches the server's permission snapshot
    // (the gap server.rs records), so the react pill stays collapsed until the
    // perms are re-armed through the robot hook — no page reload on the robot.
    app.call_function("cassette.rearm_perms", json!({}));

    let target = "the target message";
    app.test_id("composer").fill(&target);
    app.test_id("composer").press_key("Enter");
    app.text(&target).assert_visible();

    // The hover bar's 😊 pill opens the shared emoji picker card.
    app.test_id("msg-row").hover_native();
    app.test_id("msg-react").click();
    app.test_id("emoji-cell").assert_visible();

    // The first cell of the grid (Smileys & People opens with 😀); tapping
    // it adds the reaction through the server and closes the card.
    app.test_id("emoji-cell").nth(0).click();
    app.test_id("reaction-pill").assert_visible();
    shot(app, "msg2-react-pill");

    // Tapping the pill toggles the reaction back off; the pill unmounts.
    app.test_id("reaction-pill").click();
    app.test_id("reaction-pill").assert_hidden();
    shot(app, "msg2-react-removed");
}

#[robot_test]
fn pinning_toasts_badges_the_header_and_opens_the_pins_panel(app: &mut App) {
    let (a, _b, server) = crew("pin");
    signed_out(app);
    sign_in_as(app, &a);
    app.text(&server).assert_visible();
    app.text("general").click();
    app.set_timeout(WAIT);

    // The pin action needs MANAGE_MESSAGES, and the fresh create path leaves
    // the snapshot unfetched (the gap server.rs records): re-arm first.
    app.call_function("cassette.rearm_perms", json!({}));

    let target = "the target message";
    app.test_id("composer").fill(&target);
    app.test_id("composer").press_key("Enter");
    app.text(&target).assert_visible();

    app.test_id("msg-row").hover_native();
    app.test_id("msg-more").click();
    app.test_id("menu-pin-message").click();

    // The toast confirms the pin as the user saw it.
    app.text("Message pinned!").assert_visible();
    // The header's pin count badge shows the pin.
    app.test_id("pin-count-badge").assert_visible();
    shot(app, "msg3-pinned-badge");

    // The header's pin icon opens the "Pinned messages" panel.
    app.test_id("pin-live").click();
    app.test_id("pins-panel-title").assert_visible();
    // The panel lists the pinned message's row.
    app.text(&target).assert_visible();
    shot(app, "msg2-pins-panel");

    // The panel's close button dismisses it.
    app.test_id("pins-panel-close").click();
    app.test_id("pins-panel-title").assert_hidden();
}

#[robot_test]
fn markdown_bold_and_inline_code_get_their_own_runs(app: &mut App) {
    let (a, _b, server) = crew("md");
    signed_out(app);
    sign_in_as(app, &a);
    app.text(&server).assert_visible();
    app.text("general").click();
    app.set_timeout(WAIT);

    let bold_line = "bold check **boldword** end";
    app.test_id("composer").fill(&bold_line);
    app.test_id("composer").press_key("Enter");

    let code_line = "code `codeword` end";
    app.test_id("composer").fill(&code_line);
    app.test_id("composer").press_key("Enter");
    shot(app, "msg3-markdown");

    // The SDK paints the rows through its scene, whose text runs are not
    // robot elements; the parse the renderer styles is read through the
    // md_runs hook (robot_hooks.rs), which reports each run as "text~flags".
    let runs = md_runs(app, &bold_line);
    assert!(
        runs.iter().any(|run| run == "boldword~bold"),
        "the bold line parses `boldword` as its own bold run: {runs:?}"
    );
    let runs2 = md_runs(app, &code_line);
    assert!(
        runs2.iter().any(|run| run == "codeword~code"),
        "the code line parses `codeword` as its own code run: {runs2:?}"
    );
}

/// One call of the `cassette.md_runs` hook: the run lines, decoded from the
/// `Ok`-wrapped reply.
fn md_runs(app: &mut App, body: &str) -> Vec<String> {
    let reply = app.call_function("cassette.md_runs", json!({ "body": body }));
    match reply.get("Ok").unwrap_or(&reply) {
        Value::Array(items) =>
            items.iter().map(|v| v.as_str().unwrap_or("").to_string()).collect(),
        other => panic!("md_runs answered {other:?}"),
    }
}

#[robot_test]
fn scrolling_up_offers_the_jump_to_bottom_pill(app: &mut App) {
    let (a, _b, server) = crew("jump");
    signed_out(app);
    sign_in_as(app, &a);
    app.text(&server).assert_visible();
    app.text("general").click();
    app.set_timeout(WAIT);

    // Enough rows that the newest sit off-screen when the list is scrolled
    // back to the top: 40 short rows are far taller than the chat pane.
    // The server rate-limits a user to 20 sends per 10 s, so past that the
    // send is refused and retried: each `eventually` waits for the row to
    // land, and once the window rolls a retry lands the message.
    for i in 1..=40 {
        let filler = format!("filler{}", i);
        app.test_id("composer").fill(&filler);
        app.test_id("composer").press_key("Enter");
        while !eventually(2, || app.text(&filler).count() > 0) {
            app.test_id("composer").fill(&filler);
            app.test_id("composer").press_key("Enter");
        }
    }
    // The follow-the-tail behaviour keeps the reader at the bottom after
    // every send, so wait until the pane has actually scrolled down (the
    // clamped offset proves the range and the pill-hidden state).
    assert!(
        eventually(10, || app.test_id("msg-scroll").state().scroll.map(|s| s.y).unwrap_or(0.0) > 100.0),
        "the list never settled at the bottom"
    );
    app.text("filler40").assert_in_viewport();
    // Scrolling back to the top moves the pane — the movement fires
    // `on_scroll`, which recomputes "away from bottom" and shows the pill.
    app.test_id("msg-scroll").scroll_to(0.0, 0.0);

    app.test_id("jump-to-bottom").assert_visible();
    shot(app, "msg4-jump");
    app.test_id("jump-to-bottom").click();
    app.test_id("jump-to-bottom").assert_hidden();
    app.text("filler40").assert_in_viewport();
    shot(app, "msg4-jumped");
}

#[robot_test]
fn the_other_browser_sees_the_typing_indicator(app: &mut App) {
    let (a, b, server) = crew("type");
    signed_out(app);
    sign_in_as(app, &a);
    app.text(&server).assert_visible();
    app.text("general").click();
    app.set_timeout(WAIT);

    let mut other = open_browser("btyping");
    sign_in_as(&mut other.app, &b);
    other.app.text(&server).assert_visible();
    other.app.set_timeout(WAIT);

    // B fills the composer but never presses Enter: the client emits a
    // Typing frame the server fans out to A, whose typing row shows it.
    other.app.test_id("composer").fill("someone is typing now");
    app.text(&format!("{} is typing...", b.name))
        .assert_visible();
    shot(app, "msg5-typing-a");
    shot(&mut other.app, "msg5-typing-b");
}

#[robot_test]
fn a_mention_carries_its_pill(app: &mut App) {
    let (a, b, server) = crew("mention");
    signed_out(app);
    sign_in_as(app, &a);
    app.text(&server).assert_visible();
    app.text("general").click();
    app.set_timeout(WAIT);

    let msg = format!("@{} hello", b.name);
    app.test_id("composer").fill(&msg);
    app.test_id("composer").press_key("Enter");
    app.text("hello").assert_visible();
    // The mention renders as a pill carrying the `mention-pill` test id.
    app.test_id("mention-pill").assert_visible();
    shot(app, "msg6-mention-pill");
}
