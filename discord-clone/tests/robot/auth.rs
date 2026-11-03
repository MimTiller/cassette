//! Signing up, out and back in through the login screen.

use robot_test::{robot_test, App};

use crate::support::{fresh_name, sign_in, sign_out, sign_up, signed_out};

#[robot_test]
fn sign_up_sign_out_and_sign_back_in(app: &mut App) {
    signed_out(app);
    let user = fresh_name("auth");
    let password = format!("pass-{user}");

    sign_up(app, &user, &password);
    app.text(&user).assert_visible();

    sign_out(app);
    app.text("Online").assert_hidden();

    sign_in(app, &user, &password);
    app.text(&user).assert_visible();
}

#[robot_test]
fn wrong_password_is_refused(app: &mut App) {
    signed_out(app);
    let user = fresh_name("authbad");
    let password = format!("pass-{user}");
    sign_up(app, &user, &password);
    sign_out(app);

    app.test_id("login-tab-signin").click();
    app.test_id("login-email").fill(&user);
    app.test_id("login-password").fill("not-the-password");
    app.test_id("login-submit").click();
    app.text("Welcome back!").assert_visible();
    app.text("Online").assert_hidden();
}
