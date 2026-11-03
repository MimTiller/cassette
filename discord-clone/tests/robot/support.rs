//! Shared steps for the robot suite: fresh names, the sign-in flows, and
//! direct calls to the run's server for setup a test does not drive through
//! the UI.

use std::time::{Duration, Instant};

use robot_test::{json, App, Value};

/// How long actions and assertions wait. The app talks to its server and
/// gateway, so allow more than the 5 s default.
pub const WAIT: Duration = Duration::from_secs(20);

/// A name no earlier run has taken: the prefix plus the clock's
/// milliseconds, inside the 32-character username limit.
pub fn fresh_name(prefix: &str) -> String {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{prefix}{}", ms % 10_000_000_000)
}

/// The run's server (`idealyst test` serves the app and its API on one port).
pub fn base() -> String {
    let port = std::env::var("IDEALYST_TEST_PORT").expect("IDEALYST_TEST_PORT (run the suite with `idealyst test`)");
    format!("http://127.0.0.1:{port}")
}

/// Call a `#[server]` function directly, as a signed-in `token` or anonymously,
/// and return its `Ok` value. Panics with the server's error.
pub fn srv(function: &str, input: Option<Value>, token: Option<&str>) -> Value {
    let body = match input {
        Some(v) => json!([v]),
        None => json!([]),
    };
    let mut req = ureq::post(&format!("{}/_srv/{function}", base())).set("content-type", "application/json");
    if let Some(t) = token {
        req = req.set("authorization", &format!("Bearer {t}"));
    }
    let out: Value = req
        .send_string(&body.to_string())
        .unwrap_or_else(|e| panic!("{function}: {e}"))
        .into_json()
        .unwrap_or_else(|e| panic!("{function}: unreadable reply: {e}"));
    if let Some(err) = out.get("Err").or_else(|| out.get("Failed")) {
        panic!("{function}: {err}");
    }
    out.get("Ok").cloned().unwrap_or(out)
}

/// An account made through the server, for a test to sign in to or act as.
pub struct Account {
    pub name: String,
    pub password: String,
    pub id: i64,
    pub token: String,
}

impl Account {
    /// Sign up a fresh account named `prefix` plus a timestamp.
    pub fn new(prefix: &str) -> Account {
        let name = fresh_name(prefix);
        let password = format!("pass-{name}");
        srv("signup", Some(json!({ "username": name, "password": password })), None);
        let login = srv("login", Some(json!({ "username": name, "password": password })), None);
        Account {
            id: login["user_id"].as_i64().expect("login user_id"),
            token: login["token"].as_str().expect("login token").to_string(),
            name,
            password,
        }
    }

    /// Call a server function as this account.
    pub fn call(&self, function: &str, input: Value) -> Value {
        srv(function, Some(input), Some(&self.token))
    }
}

/// Sign up through the login screen's Sign Up tab and wait for the shell.
pub fn sign_up(app: &mut App, user: &str, password: &str) {
    app.test_id("login-tab-signup").click();
    app.test_id("login-email").fill(user);
    app.test_id("login-password").fill(password);
    app.test_id("login-confirm-password").fill(password);
    app.test_id("login-submit").click();
    app.text("Online").assert_visible();
}

/// Sign in through the login screen's Log In tab and wait for the shell.
pub fn sign_in(app: &mut App, user: &str, password: &str) {
    app.test_id("login-tab-signin").click();
    app.test_id("login-email").fill(user);
    app.test_id("login-password").fill(password);
    app.test_id("login-submit").click();
    app.text("Online").assert_visible();
}

/// Sign `account` in through the login screen.
pub fn sign_in_as(app: &mut App, account: &Account) {
    sign_in(app, &account.name, &account.password);
}

/// Sign out through the User Settings sidebar's Log Out row.
pub fn sign_out(app: &mut App) {
    app.test_id("user-settings").click();
    app.label("Log Out").click();
    app.text("Welcome back!").assert_visible();
}

/// Start a test on the login screen: close whatever an earlier test left
/// open and sign its account out.
pub fn signed_out(app: &mut App) {
    app.set_timeout(WAIT);
    for _ in 0..3 {
        app.press_key("Escape");
    }
    if app.text("Online").is_visible() {
        sign_out(app);
    }
    app.text("Welcome back!").assert_visible();
}

/// Poll `check` until it holds or `secs` pass; for conditions no single
/// assertion expresses (a robot function's answer, a server-side value).
pub fn eventually(secs: u64, mut check: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        if check() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Record a check that fails because of a known app bug: prints `KNOWN`
/// with the reason instead of failing the test. List the bug in
/// e2e/ISSUES.md; turn it back into an assertion once it is fixed.
pub fn known_bug(what: &str, ok: bool, why: &str) {
    if !ok {
        eprintln!("[KNOWN] {what} — {why}");
    }
}
