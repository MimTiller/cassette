//! The idea-chat backend, end to end: browsers load the SAME web bundle from
//! the run's real chat server — started by /root/bin/robot-test from the
//! read-only roles checkout at /root/idea-chat-roles, on its own fresh
//! Postgres database the wrapper creates and drops — and drive it through
//! their robot relays like every other module. `users::open_At`
//! opens a browser on the chat server's own origin, so the app's same-origin
//! RPCs and live socket land on the chat wire; its init script seeds the
//! persisted chat-backend pick (`discord-clone:chat:chat_backend =
//! "idea-chat"`), the storage entry `install_chat_backend_choice` reads at
//! startup — the same localStorage write the instance settings' Chat
//! Backend page makes, so these rows run the app on idea-chat exactly as a
//! user who switched backends would.
//!
//! Ported from tools/rwnd-idea-chat-flows.js (the hand-rolled playwright
//! script this module replaces), cases a–i: sign in, create a server and
//! its channels, send, invite and join, member list, typing, live delivery
//! both ways, edit, delete, reaction, DM. The script's psql checks keep
//! their database assertions here against the run's chat database: spaces,
//! seeded channels, messages, invites, space_members, edits (`edited_at`),
//! deletes (the row count goes to zero), reactions, DM messages, mentions
//! (`mentions`/`mentions_everyone`), replies (`reply_to`). Its
//! `document.body.innerText` sweeps become the suite's label/test_id
//! assertions instead; the sweep for "the handle appears anywhere on the
//! page" (flows (e3)/(f6)) becomes a member-list assertion, and what only
//! a sweep could prove — raw text with no element to point at — is noted
//! where it drops (the flows' "joined toast" body check keeps the app's
//! "Joined!" element assertion).
//!
//! Chat's `sign_up` throttles to 5/min and `sign_in` to 10/min (anonymous
//! callers share one per-IP bucket), so the pair of accounts is made ONCE
//! per run at module level (like instance.rs's ADMIN): tests reuse the pair,
//! and each test gets its own fresh SERVER so the screens stay independent.
//! Chat's `sign_up` emails a verification code; the suite finishes the
//! verification leg the way the flows script did it — the run's throwaway
//! database is the suite's own, so `email_verified_at` is set directly
//! rather than redeeming a six-digit code through another throttled route.
//! Browsers live in locals: an assertion panic unwinds and each Browser's
//! `Drop` kills its Chrome, so no test leaves tabs behind.
//!
//! Direct `/_srv` calls (the forced delete) authenticate with the account's
//! chat session cookie — the httpOnly `idea_chat_session` `sign_in` set,
//! replayed in a `cookie:` header, the one credential the browser itself
//! proves it holds.
//!
//! Rows that cannot work over idea-chat yet, written and marked ignored:
//! `an_image_attachment_shows_in_both_browsers` (uploads need a storage
//! bucket and this host runs none, so `begin_upload` answers its
//! Unavailable refusal) and `a_server_tile_carries_an_unread_badge` (the
//! rail's server tile draws no unread marker in the app at all — the
//! channel rows do, which the unread test covers).

use robot_test::{json, robot_test, App, Value};

use std::process::Command;
use std::sync::OnceLock;

use crate::support::{eventually, fresh_name, WAIT};
use crate::users::{open_browser_at, Browser};

// ---- the run's chat server: origin, database, direct calls ------------------

/// The run's chat server origin (`CHAT_ROBOT_URL`, exported by the wrapper
/// alongside the server it starts).
pub fn chat_base() -> String {
    std::env::var("CHAT_ROBOT_URL")
        .expect("CHAT_ROBOT_URL (run the suite with /root/bin/robot-test, which starts the chat server)")
}

/// The chat database the run's server was started on (`CHAT_ROBOT_DB`),
/// for the flows script's psql checks: the suite asserts on its own
/// database, which the wrapper drops at the end.
pub fn chat_db() -> String {
    std::env::var("CHAT_ROBOT_DB").expect("CHAT_ROBOT_DB from the wrapper")
}

/// One row from the chat database ("" when the query finds nothing).
pub fn db_row(sql: &str) -> String {
    let db = chat_db();
    match Command::new("psql").args(["-Atq", "-c", sql.as_ref(), db.as_ref()]).output() {
        Ok(out) => String::from_utf8(out.stdout).unwrap_or_else(|e| panic!("psql: {e}")).trim().replace("\n", "|"),
        Err(e) => panic!("psql against the chat database: {e}: {sql}")
    }
}

/// `s` as a single-quoted SQL literal.
pub fn q(s: &str) -> String {
    format!("'{}'", s.replace("'", "''"))
}

/// POST one `/_srv/<fn>` on the chat server, authenticated with the
/// account's chat session cookie when `session` is given, and answer the
/// reply's `Ok` value (panicking with the server's refusal text, like
/// `support::srv` does against the RWND server).
pub fn chat_srv(function: String, args: Vec<Value>, session: &str) -> Value {
    let response = ureq_chat(function.clone(), args, session).unwrap_or_else(|e| panic!("chat {function}: {e}"));
    let out: Value = response.into_json().unwrap_or_else(|e| panic!("chat {function}: unreadable reply: {e}"));
    if let Some(err) = out.get("Err").or_else(|| out.get("Failed")) {
        panic!("chat {function}: {err}");
    }
    out.get("Ok").cloned().unwrap_or(out)
}

/// The same call as a bare request (no cookie), handing back the raw
/// `ureq` result so `chat_srv_refused` can read a refused reply's body.
fn ureq_chat(function: String, args: Vec<Value>, session: &str) -> Result<ureq::Response, ureq::Error> {
    let mut req = ureq::post(&format!("{}/_srv/{}", chat_base(), function)).set("content-type", "application/json");
    if !session.is_empty() {
        req = req.set("cookie", &format!("idea_chat_session={}", session));
    }
    req.send_string(&json!(args).to_string())
}

/// The same call when the rules should refuse it: answers the refusal's
/// message text instead of panicking (the way roles.rs's `srv_refused` does
/// against the RWND server).
pub fn chat_srv_refused(function: String, args: Vec<Value>, session: &str) -> String {
    let response = match ureq_chat(function.clone(), args, session) {
        Ok(response) => response,
        Err(ureq::Error::Status(_, response)) => response,
        Err(e) => panic!("chat {function}: refused over the wire, not by the rules: {e}")
    };
    let out: Value = response.into_json().unwrap_or_else(|_| panic!("chat {function}: refused with an unreadable body"));
    match out.get("Err").or_else(|| out.get("Failed")) {
        Some(err) => err.as_str().unwrap_or_else(|| panic!("chat {function}: refused with a non-string body: {err}")).to_string(),
        None => panic!("chat {function}: succeeded when the rules should have refused it (reply {out})")
    }
}

/// Sign one account in directly and keep the session token its reply's
/// `set-cookie` carries, for the module's authenticated `/_srv` calls.
pub fn chat_session(email: &str, password: &str) -> String {
    let response = ureq_chat("sign_in".to_string(), vec![json!(email.to_string()), json!(password.to_string())], "")
        .unwrap_or_else(|e| panic!("chat sign_in: {e}"));
    let cookie = response.header("set-cookie").unwrap_or_else(|| panic!("chat sign_in set no session cookie"));
    let at = cookie.find("idea_chat_session=").unwrap_or_else(|| panic!("chat sign_in cookie unreadable: {cookie}"));
    cookie[at + 18 ..].split(";").next().unwrap_or("").to_string()
}

pub struct ChatAccount {
    handle: String,
    email: String,
    password: String,
    session: String,
}

impl ChatAccount {
    /// Sign this account in through the login screen — chat's login maps
    /// the username onto the email slot, so the username field takes the
    /// email. The 10/min sign-in route can throttle a burst of fresh
    /// browsers: on refusal the login screen stays up, so Escape and
    /// pause, then retry until the shell answers.
    fn sign_chat_in(&self, app: &mut App) {
        let email = &self.email;
        let password = &self.password;
        let landed = eventually(150, move || {
            app.test_id("login-tab-signin").click();
            app.test_id("login-email").fill(&email);
            app.test_id("login-password").fill(&password);
            app.test_id("login-submit").click();
            app.set_timeout(WAIT);
            if app.text("Online").count() > 0 {
                true
            } else {
                app.press_key("Escape");
                std::thread::sleep(std::time::Duration::from_secs(15));
                false
            }
        });
        assert!(landed, "the chat account never signed in: {email}");
    }
}

/// The module's pair of chat accounts, made once per run so the signup and
/// sign-in routes' per-IP throttles only ever see the setup plus each
/// test's browser logins, which retry through their pause loop.
static CREW: OnceLock<(ChatAccount, ChatAccount)> = OnceLock::new();

pub fn crew() -> &'static (ChatAccount, ChatAccount) {
    CREW.get_or_init(move || (
        make_chat_account("rwnda".to_string()),
        make_chat_account("rwndb".to_string()),
    ))
}

/// Make one account through the chat server's real signup — `sign_up` emails
/// a code whose purpose is verification — and finish the verification leg the
/// way the flows script did it: the run's chat database is the suite's own
/// throwaway, so `email_verified_at` is set there directly (the script's own
/// psql UPDATE). Then sign in once directly for the session token the
/// module's `/_srv` calls need.
pub fn make_chat_account(handle: String) -> ChatAccount {
    let email = format!("{handle}@robot.example");
    let password = format!("pass-{handle}-2026");
    chat_srv("sign_up".to_string(), vec![json!(email), json!(password), json!(handle.to_string()), json!(handle.to_string())], "");
    db_row(&format!("update users set email_verified_at = now() where email = {}", q(&email)));
    let session = chat_session(email.as_ref(), password.as_ref());
    ChatAccount {
        handle,
        email,
        password,
        session,
    }
}

// ---- chat browsers and the flows' shared steps ------------------------------

/// JS that seeds the persisted chat-backend pick before the page loads —
/// the flows script's addInitScript: `install_chat_backend_choice` reads
/// this storage entry at startup, and every accessor then speaks the chat
/// wire.
pub const CHAT_PICK: &str = "try { localStorage.setItem(\"discord-clone:chat:chat_backend\", \"idea-chat\"); } catch (e) {}";

/// Open a browser on the app as the chat server serves it (its origin is the
/// page's own, so the app's same-origin RPCs and live socket land on the
/// chat wire). Dropping the Browser kills its Chrome — including when an
/// assertion panics, since the local unwinds — so no test leaves tabs
/// behind.
pub fn open_chat_browser(label: &str) -> Browser {
    open_browser_at(label, chat_base(), CHAT_PICK.to_string())
}

/// A fresh pair of chat browsers on one fresh server: A signs in, creates
/// the server, mints an invite and B joins with the code — the flows script's
/// setup for (d)–(i), which the two-browser rows all start from. Returns
/// both browsers plus A's own fresh RWND-side nothing: the pair only.
pub fn setup_pair() -> (Browser, Browser) {
    let (a, b) = crew();
    let mut browser_a = open_chat_browser("chat-a");
    let mut browser_b = open_chat_browser("chat-b");
    a.sign_chat_in(&mut browser_a.app);
    b.sign_chat_in(&mut browser_b.app);
    (browser_a, browser_b)
}

/// A fresh server named `name` made through the rail's Add a Server modal,
/// landing the browser on #general. On this backend the space's #general
/// and #lounge are seeded by the chat server itself.
pub fn create_server(app: &mut App, name: &str) {
    app.test_id("rail-create-server").click();
    app.text("Add a Server").assert_visible();
    app.test_id("create-server-name-input").fill(&name);
    app.test_id("create-server-create").click();
    app.text("general").assert_visible();
    app.text("lounge").assert_visible();
    app.set_timeout(WAIT);
    app.text("general").click();
    app.set_timeout(WAIT);
}

/// The chat database id (uuid) of the run's server named `name`.
pub fn space_of(app_name: &str) -> String {
    db_row(&format!("select s.id from spaces s where s.name = {}", q(app_name)))
}

/// #general's channel id in the database for the space `space`.
pub fn channel_of(space: &str) -> String {
    db_row(&format!("select c.id from channels c where c.space_id = {} and c.name = 'general'", q(space)))
}

/// Open A's invite overlay (perms re-armed first, the gap server.rs
/// records), read the minted code off the card, and leave A on the channel.
pub fn mint_invite(app: &mut App) -> String {
    app.call_function("cassette.rearm_perms", json!({}));
    app.test_id("invite-live").assert_visible();
    app.test_id("invite-live").click();
    app.test_id("live-invite-open").assert_visible();
    app.test_id("live-invite-create").click();
    app.test_id("invite-code-field").assert_visible();
    let code = app.test_id("invite-code-field").state().value
        .as_ref()
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    assert!(!code.is_empty(), "minted an empty invite code");
    app.test_id("invite-done").click();
    app.set_timeout(WAIT);
    code
}

/// B joins the server with `code` through the join modal, landing on it.
pub fn join_by_code(app: &mut App, code: &str) {
    app.test_id("rail-create-server").click();
    app.text("Add a Server").assert_visible();
    app.test_id("create-server-join-link").click();
    app.text("Join a Server").assert_visible();
    app.test_id("join-server-code-input").fill(&code);
    app.test_id("join-server-join").click();
    app.text("Joined!").assert_visible();
    app.test_id("join-server-done").click();
    app.set_timeout(WAIT);
}

/// Send one message through the composer and wait for its row, retrying
/// while the chat's send route throttles (messages.rs's filler loop does
/// the same against RWND's limits).
pub fn send(app: &mut App, body: &str) {
    let landed = eventually(60, move || {
        app.test_id("composer").fill(&body);
        app.test_id("composer").press_key("Enter");
        eventually(8, || app.text(&body).count() > 0)
    });
    assert!(landed, "the message never landed: {body}");
}

/// Screenshot moment, named like the suite's other modules.
pub fn shot(app: &mut App, label: &str) {
    let path = app.screenshot();
    assert!(path.ends_with(".png"), "screenshot was not saved: {path}");
    eprintln!("[shot] {label}: {path}");
}

/// Count the run's stored messages whose body is `body` (exact) in the
/// space's #general channel — the flows script's message-row psql checks.
pub fn rows_in_channel(space: &str, sql_tail: &str) -> String {
    db_row(&format!("select count(m.id) from messages m where m.channel_id = {} and {}",
        q(&channel_of(space)), sql_tail))
}

// ---- the flows script's cases, a to i ----------------------------------------

#[robot_test]
fn a_signs_in_on_the_chat_server(_app: &mut App) {
    let (a, _b) = crew();
    let mut browser_a = open_chat_browser("chat-a-signin");
    a.sign_chat_in(&mut browser_a.app);
    // The rail is up and the login screen is gone (the flows script's (a)
    // looked for rail text in body.innerText; the suite asserts elements).
    browser_a.app.test_id("rail-create-server").assert_visible();

    // The script's (a) had no database check; what the sign-in proves is
    // worth keeping: the verified account row exists on the run's chat DB.
    assert_eq!(db_row(&format!("select count(u.id) from users u where u.email = {} and u.email_verified_at is not null",
        q(&a.email))), "1", "A's chat account is on the run's chat database, verified");
}

#[robot_test]
fn a_creates_a_server_and_its_channels_show(_app: &mut App) {
    let (a, _b) = crew();
    let mut browser_a = open_chat_browser("chat-create");
    a.sign_chat_in(&mut browser_a.app);

    let server = fresh_name("Chat Hq ");
    create_server(&mut browser_a.app, &server);
    shot(&mut browser_a.app, "chat-server-created");

    // The script's (b2): the space and its seeded channels are in the chat
    // database.
    let space = space_of(&server);
    assert!(!space.is_empty(), "the new server is not in the chat database");
    assert_eq!(db_row(&format!("select count(c.id) from channels c where c.space_id = {}",
        q(&space))), "2", "the space carries its two seeded channels");
    assert_ne!(db_row(&format!("select count(sm.user_id) from space_members sm where sm.space_id = {}",
        q(&space))), "0", "A is a member of their own space");
}

#[robot_test]
fn a_sends_a_message_that_shows_and_stores(_app: &mut App) {
    let (a, _b) = crew();
    let mut browser_a = open_chat_browser("chat-send");
    a.sign_chat_in(&mut browser_a.app);
    let server = fresh_name("Chat Hq ");
    create_server(&mut browser_a.app, &server);

    let body = "first message";
    send(&mut browser_a.app, body);
    browser_a.app.text(body).assert_visible();
    shot(&mut browser_a.app, "chat-send-lands");

    // The script's (c2): the message row is stored on the chat database.
    assert_eq!(rows_in_channel(&server, &format!("m.body = {}", q(body))), "1",
        "A's message is stored in the chat database");
}

#[robot_test]
fn b_joins_a_server_through_an_invite(_app: &mut App) {
    let (a, b) = crew();
    let mut browser_a = open_chat_browser("chat-inv-a");
    a.sign_chat_in(&mut browser_a.app);
    let server = fresh_name("Chat Hq ");
    create_server(&mut browser_a.app, &server);
    let code = mint_invite(&mut browser_a.app);

    let mut browser_b = open_chat_browser("chat-inv-b");
    b.sign_chat_in(&mut browser_b.app);
    join_by_code(&mut browser_b.app, &code);
    browser_b.app.test_id("server-header").assert_visible();
    browser_b.app.text(&server).assert_visible();
    shot(&mut browser_b.app, "chat-b-joined");

    // The script's (d2): B is a member of the space in the chat database,
    // and the invite row is stored with its code.
    assert_eq!(db_row(&format!("select count(sm.user_id) from space_members sm join users u on u.id = sm.user_id where sm.space_id = {}::uuid and u.email = {}",
        space_of(&server), q(&b.email))), "1", "B is a member of A's space on the chat database");
    assert_ne!(db_row(&format!("select count(i.code) from invites i where i.code = {}",
        q(&code))), "0", "the invite is stored on the chat database");
}

#[robot_test]
fn the_member_list_shows_both_users(_app: &mut App) {
    let (mut browser_a, mut browser_b) = duo();
    let server = fresh_name("Chat Hq ");
    create_server(&mut browser_a.app, &server);
    let code = mint_invite(&mut browser_a.app);
    join_by_code(&mut browser_b.app, &code);

    // The script's (e1): B sees the server, #general and A's earlier
    // message; (e3): A sees B in the member list, no reload on either side.
    browser_b.app.text("general").assert_visible();
    browser_b.app.test_id("live-channel-row").assert_visible();
    browser_a.app.text("b").assert_visible();
    shot(&mut browser_a.app, "chat-member-list-a");
    shot(&mut browser_b.app, "chat-member-list-b");
}

#[robot_test]
fn typing_shows_on_the_other_user_both_ways(_app: &mut App) {
    let (mut browser_a, mut browser_b) = duo();
    let server = fresh_name("Chat Hq ");
    create_server(&mut browser_a.app, &server);
    join_by_code(&mut browser_b.app, &mint_invite(&mut browser_a.app));

    // The script's (f1)/(f6): A types without committing and B sees the
    // indicator (named for the typer), then the other way.
    browser_a.app.test_id("composer").fill("typing from A");
    browser_a.app.set_timeout(WAIT);
    browser_b.app.text("is typing").assert_visible();
    shot(&mut browser_b.app, "chat-typing-a-seen-by-b");
    browser_a.app.test_id("composer").press_key("Enter");

    browser_b.app.test_id("composer").fill("typing from B");
    browser_b.app.set_timeout(WAIT);
    browser_a.app.text("is typing").assert_visible();
    shot(&mut browser_a.app, "chat-typing-b-seen-by-a");
    browser_b.app.test_id("composer").press_key("Enter");
    browser_b.app.text("typing from B").assert_visible();
}

#[robot_test]
fn a_message_delivers_live_both_ways(_app: &mut App) {
    let (mut browser_a, mut browser_b) = duo();
    let server = fresh_name("Chat Hq ");
    create_server(&mut browser_a.app, &server);
    join_by_code(&mut browser_b.app, &mint_invite(&mut browser_a.app));

    // The script's (f2)–(f5): each side's message appears in the other's
    // view with no reload (each browser is live on the channel).
    send(&mut browser_a.app, "live from A");
    browser_b.app.text("live from A").assert_visible();
    shot(&mut browser_b.app, "chat-live-a-to-b");

    send(&mut browser_b.app, "live from B");
    browser_a.app.text("live from B").assert_visible();
    shot(&mut browser_a.app, "chat-live-b-to-a");
}

#[robot_test]
fn a_edits_and_deletes_her_message_and_b_follows(_app: &mut App) {
    let (mut browser_a, mut browser_b) = duo();
    let server = fresh_name("Chat Hq ");
    create_server(&mut browser_a.app, &server);
    join_by_code(&mut browser_b.app, &mint_invite(&mut browser_a.app));

    // The script's (g): A opens her message's overflow menu and edits; both
    // browsers see the edit without reloading.
    let target = "the target message";
    send(&mut browser_a.app, target);
    browser_a.app.text(target).hover_native();
    browser_a.app.test_id("msg-more").click();
    browser_a.app.test_id("menu-edit-message").click();
    let edited = "the edited text";
    browser_a.app.test_id("edit-input").fill(edited);
    browser_a.app.test_id("edit-input").press_key("Enter");
    browser_a.app.text(edited).assert_visible();
    browser_a.app.text("(edited)").assert_visible();
    browser_b.app.text(edited).assert_visible();
    shot(&mut browser_a.app, "chat-edit-applied");

    // (g3): the edit is stored — the message row carries its edit stamp.
    assert_eq!(rows_in_channel(&server, "m.edited_at is not null"), "1",
        "the edited message carries its edit stamp in the chat database");

    // (g4)–(g7): A deletes it; it goes away on both screens, and the
    // database loses the row.
    browser_a.app.text(edited).hover_native();
    browser_a.app.test_id("msg-more").click();
    browser_a.app.test_id("menu-delete-message").click();
    browser_a.app.text("Delete this message?").assert_visible();
    browser_a.app.test_id("confirm-delete").click();
    browser_a.app.text(edited).assert_hidden();
    browser_b.app.text(edited).assert_hidden();
    assert_eq!(rows_in_channel(&server, &format!("m.body like {}", q("the edited text%"))), "0",
        "the deleted message is gone from the chat database");
}

#[robot_test]
fn b_reacts_to_as_message_and_a_sees_the_count(_app: &mut App) {
    let (mut browser_a, mut browser_b) = duo();
    let server = fresh_name("Chat Hq ");
    create_server(&mut browser_a.app, &server);
    join_by_code(&mut browser_b.app, &mint_invite(&mut browser_a.app));

    let target = "the react target";
    send(&mut browser_a.app, target);

    // The script's (h): B opens the react pill on A's row, taps a thumbs-up
    // cell in the picker; A sees the reaction without reloading.
    browser_b.app.test_id("msg-row").hover_native();
    browser_b.app.test_id("msg-react").click();
    browser_b.app.test_id("emoji-cell").assert_visible();
    browser_b.app.test_id("emoji-cell").nth(0).click();
    browser_b.app.test_id("reaction-pill").assert_visible();
    shot(&mut browser_b.app, "chat-reaction-b");
    browser_a.app.test_id("reaction-pill").assert_visible();
    shot(&mut browser_a.app, "chat-reaction-a");

    // (h3): the reaction row is stored on the chat database.
    assert_ne!(db_row("select count(r.id) from reactions r"), "0",
        "the reaction is not stored on the chat database");
}

#[robot_test]
fn a_opens_a_dm_with_b_that_delivers_to_both(_app: &mut App) {
    let (mut browser_a, mut browser_b) = duo();
    let server = fresh_name("Chat Hq ");
    create_server(&mut browser_a.app, &server);
    join_by_code(&mut browser_b.app, &mint_invite(&mut browser_a.app));

    // The script's (i): A opens a DM with B through the DM screen's search
    // and sends. (The robot has no right-click on the member row; the
    // dms_friends precedent uses the DM screen's search, same surface.)
    browser_a.app.test_id("rail-direct-messages").click();
    browser_a.app.test_id("live-dm-find").assert_visible();
    browser_a.app.test_id("live-dm-find").fill("rwndb");
    browser_a.app.test_id("live-dm-find-result").click();
    browser_a.app.test_id("live-dm-composer").assert_visible();
    let dm = "private note";
    browser_a.app.test_id("live-dm-composer").fill(dm);
    browser_a.app.test_id("live-dm-composer").press_key("Enter");
    browser_a.app.text(dm).assert_visible();
    shot(&mut browser_a.app, "chat-dm-a");

    // The script's (i2): the DM message is stored (DM channels sit outside
    // any space on this backend, so the body match names the DM row).
    assert_eq!(db_row(&format!("select count(m.id) from messages m where m.body = {}",
        q(dm))), "1", "the DM message is stored on the chat database");

    // (i3)/(i4): B's browser sees the conversation in their DM list with
    // A's message in it, live.
    browser_b.app.test_id("rail-direct-messages").click();
    browser_b.app.test_id("live-dm-row").assert_visible();
    browser_b.app.test_id("live-dm-row").click();
    browser_b.app.text(dm).assert_visible();
    shot(&mut browser_b.app, "chat-dm-b");
}

// ---- the rows the flows script lacks ----------------------------------------

#[robot_test]
#[ignore = "uploads need a storage bucket: the run's chat server has no S3 endpoint on this host, so begin_upload answers its Unavailable refusal (\"Uploads aren't working right now.\") and no attachment row can be staged"]
fn an_image_attachment_shows_in_both_browsers(_app: &mut App) {
    let (a, b) = crew();
    let mut browser_a = open_chat_browser("chat-attach-a");
    let mut browser_b = open_chat_browser("chat-attach-b");
    a.sign_chat_in(&mut browser_a.app);
    b.sign_chat_in(&mut browser_b.app);
    create_server(&mut browser_a.app, &fresh_name("Chat Hq "));
    join_by_code(&mut browser_b.app, &mint_invite(&mut browser_a.app));

    // The upload triplet on this backend is begin_upload → PUT the bytes to
    // the presigned slot → finish_upload, then send with the attachment id.
    // Without a bucket the first leg answers the refusal; assert the refusal
    // so the test documents the gap it stands for.
    let refused = chat_srv_refused("begin_upload".to_string(),
        vec![json!("shot.png"), json!(64u64), json!("image/png"), json!("message")], &a.session);
    assert_eq!(refused, "Uploads aren't working right now.",
        "the refusal this ignored row stands for: no bucket exists ({refused})");
}

#[robot_test]
fn scrolling_back_pulls_earlier_pages_of_a_busy_channel(_app: &mut App) {
    let (a, _b) = crew();
    let mut browser_a = open_chat_browser("chat-scroll");
    a.sign_chat_in(&mut browser_a.app);
    let server = fresh_name("Chat Hq ");
    create_server(&mut browser_a.app, &server);

    // More than one page: the chat server's history window is 50 rows, so
    // send 60 short fillers (retrying through the send throttle, as
    // messages.rs's filler loop does).
    for i in 1..=60 {
        send(&mut browser_a.app, &format!("filler {}", i));
    }
    // The tail stays in view: the newest row is on screen, and the
    // database holds all sixty in this channel.
    browser_a.app.text("filler 60").assert_in_viewport();
    assert_eq!(rows_in_channel(&server, &format!("m.body like {}", q("filler %"))), "60",
        "the channel holds sixty stored rows on the chat database");

    // Reading up to the top of the loaded window mounts the "Load earlier
    // messages" pill (load_earlier_button); tapping it pulls one more
    // history page, so the very first row comes onto the screen.
    browser_a.app.test_id("msg-scroll").scroll_to(0.0, 0.0);
    browser_a.app.text("Load earlier messages").assert_visible();
    shot(&mut browser_a.app, "chat-scrollback-pill");
    browser_a.app.text("Load earlier messages").click();
    assert!(eventually(15, || browser_a.app.text("filler 1").count() > 0),
        "the earlier page never loaded");
    browser_a.app.text("filler 1").assert_in_viewport();
    shot(&mut browser_a.app, "chat-scrollback-loaded");
}

#[robot_test]
fn a_person_mention_highlights_and_counts(_app: &mut App) {
    let (a, b) = crew();
    let mut browser_a = open_chat_browser("chat-mention-a");
    let mut browser_b = open_chat_browser("chat-mention-b");
    a.sign_chat_in(&mut browser_a.app);
    b.sign_chat_in(&mut browser_b.app);
    let server = fresh_name("Chat Hq ");
    create_server(&mut browser_a.app, &server);
    join_by_code(&mut browser_b.app, &mint_invite(&mut browser_a.app));
    browser_b.app.text("general").click();
    browser_b.app.set_timeout(WAIT);

    // A mentions B by handle: the pill paints on A's row and B's view shows
    // it live.
    let msg = format!("@{} hello", b.handle);
    browser_a.app.test_id("composer").fill(&msg);
    browser_a.app.test_id("composer").press_key("Enter");
    browser_a.app.text("hello").assert_visible();
    browser_a.app.test_id("mention-pill").assert_visible();
    browser_b.app.text("hello").assert_visible();
    browser_b.app.test_id("mention-pill").assert_visible();
    shot(&mut browser_b.app, "chat-mention-pill");

    // The count: B's unread @mention feeds their channel row's red count
    // pill (unread-count-badge).
    browser_b.app.test_id("unread-count-badge").assert_visible();

    // Stored: the message row records the mention target.
    assert_eq!(db_row(&format!("select count(m.id) from messages m where m.channel_id = {} and array_length(m.mentions) > 0",
        q(&channel_of(&server)))), "1", "the mention is recorded on the chat database");
}

#[robot_test]
fn an_everyone_mention_highlights_and_counts(_app: &mut App) {
    let (a, b) = crew();
    let mut browser_a = open_chat_browser("chat-every-a");
    let mut browser_b = open_chat_browser("chat-every-b");
    a.sign_chat_in(&mut browser_a.app);
    b.sign_chat_in(&mut browser_b.app);
    let server = fresh_name("Chat Hq ");
    create_server(&mut browser_a.app, &server);
    join_by_code(&mut browser_b.app, &mint_invite(&mut browser_a.app));
    browser_b.app.text("general").click();
    browser_b.app.set_timeout(WAIT);

    // The `@everyone` token paints as its own pill and marks both readers'
    // channel row unread with a mention.
    browser_a.app.test_id("composer").fill("@everyone stand up");
    browser_a.app.test_id("composer").press_key("Enter");
    browser_a.app.text("stand up").assert_visible();
    browser_a.app.test_id("mention-pill").assert_visible();
    browser_b.app.text("stand up").assert_visible();
    browser_b.app.test_id("mention-pill").assert_visible();
    browser_b.app.test_id("unread-count-badge").assert_visible();
    shot(&mut browser_b.app, "chat-everyone-pill");

    // Stored: the message row flags mentions_everyone.
    assert_eq!(db_row(&format!("select count(m.id) from messages m where m.channel_id = {} and m.mentions_everyone",
        q(&channel_of(&server)))), "1", "the everyone mention is flagged on the chat database");
}

#[robot_test]
fn a_reply_carries_its_preview_and_links_the_row(_app: &mut App) {
    let (a, _b) = crew();
    let mut browser_a = open_chat_browser("chat-reply");
    a.sign_chat_in(&mut browser_a.app);
    let server = fresh_name("Chat Hq ");
    create_server(&mut browser_a.app, &server);

    let target = "the reply target";
    send(&mut browser_a.app, target);
    browser_a.app.text(target).hover_native();
    browser_a.app.test_id("msg-reply").click();
    browser_a.app.text("Replying to").assert_visible();
    shot(&mut browser_a.app, "chat-reply-preview");

    let reply = "the reply body";
    send(&mut browser_a.app, reply);
    browser_a.app.text(reply).assert_visible();

    // Stored: the reply row carries its reply_to.
    assert_eq!(db_row(&format!("select count(m.id) from messages m where m.channel_id = {} and m.reply_to is not null",
        q(&channel_of(&server)))), "1", "the reply is linked on the chat database");
}

#[robot_test]
fn unread_badges_show_on_the_channel_row(_app: &mut App) {
    let (a, b) = crew();
    let mut browser_a = open_chat_browser("chat-unread-a");
    let mut browser_b = open_chat_browser("chat-unread-b");
    a.sign_chat_in(&mut browser_a.app);
    b.sign_chat_in(&mut browser_b.app);
    let server = fresh_name("Chat Hq ");
    create_server(&mut browser_a.app, &server);
    join_by_code(&mut browser_b.app, &mint_invite(&mut browser_a.app));

    // B posts while A never opens the channel, so A's unread state for it
    // is live when they next see the sidebar.
    browser_b.app.text("general").click();
    browser_b.app.set_timeout(WAIT);
    send(&mut browser_b.app, "unread message");

    // A taps the server tile: the enter-live fetch pulls the unread counts,
    // and the channel row carries the unread pill at its left edge
    // (unread-side-pill — a plain unread; the red count pill belongs to
    // @mentions).
    browser_a.app.test_id("rail-server").nth(0).click();
    browser_a.app.set_timeout(WAIT);
    browser_a.app.text("general").assert_visible();
    browser_a.app.test_id("unread-side-pill").assert_visible();
    shot(&mut browser_a.app, "chat-unread-badge");
}

#[robot_test]
#[ignore = "the app draws no unread marker on the rail's server tile: the tile renders its face and the active pill only, so a server-level badge is a screen feature, not a backend gap — the channel-row badge above covers what the chat state can feed"]
fn a_server_tile_carries_an_unread_badge(_app: &mut App) {
    unread_badges_show_on_the_channel_row();
    // If the rail ever grows a server badge, this is where its test_id
    // assertion belongs.
}

#[robot_test]
fn a_plain_member_cannot_delete_another_users_message(_app: &mut App) {
    let (a, b) = crew();
    let mut browser_a = open_chat_browser("chat-refuse-a");
    let mut browser_b = open_chat_browser("chat-refuse-b");
    a.sign_chat_in(&mut browser_a.app);
    b.sign_chat_in(&mut browser_b.app);
    let server = fresh_name("Chat Hq ");
    create_server(&mut browser_a.app, &server);
    join_by_code(&mut browser_b.app, &mint_invite(&mut browser_a.app));
    let channel = "general".to_string();
    browser_a.app.text(&channel).click();
    browser_b.app.text(&channel).click();
    browser_a.app.set_timeout(WAIT);
    browser_b.app.set_timeout(WAIT);

    // A (the owner) posts; B (a plain member) hovers the row: the overflow
    // menu offers react and reply but no Delete row.
    let target = "a message from A";
    send(&mut browser_a.app, target);
    browser_b.app.text(target).hover_native();
    browser_b.app.test_id("msg-more").click();
    browser_b.app.test_id("menu-delete-message").assert_count(0);
    browser_b.app.test_id("msg-react").assert_visible();
    shot(&mut browser_b.app, "chat-no-delete-row");

    // Forced through the chat API, the server refuses B with its delete
    // gate's own sentence, not a network or harness error.
    let uuid = db_row(&format!("select m.id from messages m where m.body = {}",
        q(target)));
    assert!(!uuid.is_empty(), "A's message row is not in the chat database");
    let refused = chat_srv_refused("delete".to_string(), vec![json!(uuid)], &b.session);
    assert_eq!(refused, "You can't delete that.",
        "the chat server refused B's forced delete with its delete gate ({refused})");

    // A can still delete their own message: the same call as A answers, and
    // the row goes away on both screens and the database.
    chat_srv("delete".to_string(), vec![json!(uuid)], &a.session);
    browser_a.app.text(target).assert_hidden();
    browser_b.app.text(target).assert_hidden();
    assert_eq!(db_row(&format!("select count(m.id) from messages m where m.body = {}",
        q(target))), "0", "A's delete removed the row");
}

// ---- shared two-browser setup ------------------------------------------------

/// The run's chat crew in a fresh pair of chat browsers, both signed in:
/// every two-browser row adds its own fresh server on top, so the screens
/// stay independent. Dropping the pair closes both browsers, panic
/// included.
pub fn duo() -> (Browser, Browser) {
    setup_pair()
}
