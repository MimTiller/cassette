//! Roles smoke: the Members sidebar tab now reads "Manage Members" (its
//! `settings-nav-members` anchor unchanged) and opens on the member roster;
//! then the role hierarchy `role_rules.rs` encodes is driven end-to-end
//! through the run's server: the owner mints a Manage Roles role and hands it
//! to an admin, who may hand out only a strictly lower role. A same-rank
//! hand-out and a change to the owner's own roles come back refused with the
//! rules' user-facing sentences.
//!
//! The hierarchy checks run over the server API (`support.srv` plus the
//! error-tolerant `srv_refused` below) rather than the settings panel: the
//! panel's picker only offers roles the caller may assign, so a refusal can
//! only be observed at the seam itself — the same reason `coverage.rs` reads
//! refusals straight off the reply's `Err` text.

use robot_test::{json, robot_test, App, Value};

use crate::support::{Account, fresh_name, sign_up, signed_out, srv};
use crate::support::WAIT;

/// A `#[server]` call the rules should refuse: returns the refusal's message
/// (the reply's `Err`/`Failed` text) instead of panicking, the way
/// `support.srv` panics on one.
fn srv_refused(function: &str, input: Value, token: Option<&str>) -> String {
    let body = json!([input]).to_string();
    let mut req = ureq::post(&format!("{}/_srv/{function}", crate::support::base()))
        .set("content-type", "application/json");
    if let Some(t) = token {
        req = req.set("authorization", &format!("Bearer {t}"));
    }
    let out: Value = match req.send_string(&body) {
        Ok(response) => response.into_json().unwrap_or_else(|e| panic!("{function}: unreadable reply: {e}")),
        // A refusal comes back as HTTP 400 with the envelope in the body:
        // ureq 2 hands back the Response inside Error.Status, the network
        // errors stay distinct so a transport failure never masquerades
        // as a refusal.
        Err(ureq::Error::Status(_, response)) =>
            response.into_json().unwrap_or_else(|_| panic!("{function}: refused with an unreadable body")),
        Err(e) => panic!("{function}: refused over the wire, not by the rules: {e}"),
    };
    match out.get("Err").or_else(|| out.get("Failed")) {
        // The envelope nests the message under `Failed`, so read the field
        // itself: the reply's Value is the HTTP client's serde_json, and
        // re-serializing through the test-side one would fail across them.
        Some(err) =>
            err.as_str()
                .or_else(|| err.get("Failed").or_else(|| err.get("error")).and_then(|m| m.as_str()))
                .unwrap_or_else(|| panic!("{function}: the refusal carried no message (reply {out})")).to_string(),
        None => panic!("{function}: succeeded when the rules should have refused it (reply {out})"),
    }
}

/// The `id` of a `#[server]` object reply (`create_server`, `create_role`).
fn id_of(reply: Value) -> i64 {
    reply["id"].as_i64().expect("object reply id")
}

/// Open the rail's create-server modal, name it, press Create, and wait for
/// the sidebar header to show the new server (same steps as `settings.rs`).
fn create_server(app: &mut App, name: String) {
    app.test_id("rail-create-server").click();
    app.text("Add a Server").assert_visible();
    app.test_id("create-server-name-input").fill(&name);
    app.test_id("create-server-create").click();
    app.test_id("server-header").assert_visible();
    app.text(&name).assert_visible();
}

/// Open the fresh server's Server Settings panel through its dropdown,
/// re-arming the create path's unfetched permission snapshot through the
/// robot hook (the gap `server.rs` documents).
fn open_server_settings(app: &mut App) {
    app.set_timeout(WAIT);
    if app.test_id("mobile-back").is_visible() {
        app.test_id("mobile-back").click();
    }
    app.test_id("server-header").click();
    app.test_id("menu-server-settings").assert_visible();
    app.test_id("menu-server-settings").click();
    app.test_id("settings-nav-overview").assert_visible();
    app.set_timeout(WAIT);
}

#[robot_test]
fn manage_members_is_the_member_roster_tab(app: &mut App) {
    signed_out(app);
    let user = fresh_name("mmbrs");
    sign_up(app, &user, &format!("pass-{user}"));
    app.text(&user).assert_visible();

    create_server(app, fresh_name("Roster Crew "));
    open_server_settings(app);

    // The sidebar's tab now reads "Manage Members" and keeps the anchor the
    // old "Members" label used, so nav clicks still find it by test_id.
    app.text("Manage Members").assert_visible();
    app.test_id("settings-nav-members").click();
    app.set_timeout(WAIT);
    // The section the nav lands on is the member-management one: its Add
    // Member row is live.
    app.text("Add a member").assert_visible();
    let shot = app.screenshot();
    assert!(shot.ends_with(".png"), "screenshot was not saved: {shot}");
    eprintln!("[shot] roles1-manage-members: {shot}");
    app.press_key("Escape");
}

#[robot_test]
fn a_manager_hands_out_only_roles_below_their_own(app: &mut App) {
    // The hierarchy itself is driven over the server API (see the header), so
    // the browser only carries the suite through sign-in/out.
    signed_out(app);

    let owner = Account::new("roleowner");
    let admin = Account::new("roleadmin");
    let plain = Account::new("roleplain");
    let admin_token = admin.token;
    let plain_token = plain.token;
    let (admin_id, plain_id) = (admin.id, plain.id);

    let sid = id_of(owner.call("create_server", json!({ "name": fresh_name("Hierarchy Crew ") })));
    owner.call("add_member", json!({ "server_id": sid, "user_id": admin_id }));
    owner.call("add_member", json!({ "server_id": sid, "user_id": plain_id }));

    // A Manage Roles role: created at the bottom, then edited to carry the
    // permission (the owner holds every permission, so the edit passes
    // refuse_unheld_change); then handed to the admin, who becomes a
    // manager at that role's position.
    let chief = owner.call("create_role", json!({
        "server_id": sid, "name": fresh_name("Crew Chief "), "color": "#2ecc71",
        "mentionable": false,
    }));
    let chief_name = chief["name"].as_str().unwrap().to_string();
    let chief_id = id_of(chief);
    owner.call("update_role", json!({
        "role_id": chief_id,
        "name": chief_name,
        "color": "#2ecc71",
        "mentionable": false,
        "permissions": 4096,
    }));
    owner.call("assign_role", json!({
        "role_id": chief_id, "user_id": admin_id, "assigned": true,
    }));

    // A second role sits below the chief's (new roles start under every
    // existing one), so the admin may hand it out — and does.
    let scout = owner.call("create_role", json!({
        "server_id": sid, "name": fresh_name("Scout "), "color": "#3498db",
        "mentionable": false,
    }));
    let scout_id = id_of(scout);
    srv("assign_role", Some(json!({
        "role_id": scout_id, "user_id": plain_id, "assigned": true,
    })), Some(&admin_token));

    // The admin cannot hand out the chief's own role: not strictly below
    // their highest. The refusal is the rules' user-facing sentence.
    let refused = srv_refused("assign_role", json!({
        "role_id": chief_id, "user_id": plain_id, "assigned": true,
    }), Some(&admin_token));
    assert_eq!(refused, "you can only manage roles below your highest role",
        "the admin handing out their own role is refused with the hierarchy sentence (got {refused})");

    // Nobody but the owner may change the owner's roles — even a manager who
    // outranks the role in question.
    let refused = srv_refused("assign_role", json!({
        "role_id": scout_id, "user_id": owner.id, "assigned": true,
    }), Some(&admin_token));
    assert_eq!(refused, "you can't change the server owner's roles",
        "the admin touching the owner's roles is refused with the owner gate (got {refused})");

    // The allowed hand-out did land: the plain member holds the scout role,
    // per the server's own snapshot.
    let snap = srv("server_roles", Some(json!({ "server_id": sid })), Some(&plain_token));
    let landed = match snap["assignments"].clone() {
        Value::Array(items) =>
            items.iter().any(|pair| match pair.clone() {
                Value::Array(holders) =>
                    holders.len() == 2
                        && holders[0].as_i64().unwrap_or(0) == plain_id
                        && holders[1].as_i64().unwrap_or(0) == scout_id,
                _ => false,
            }),
        _ => false,
    };
    assert!(landed, "the scout role never reached the plain member through the admin");

    let shot = app.screenshot();
    eprintln!("[shot] roles2-hierarchy: {shot}");
}
