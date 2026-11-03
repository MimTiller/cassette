//! Several real browsers at once, each signed in as its own account.

use robot_test::{robot_test, App};

use crate::support::{signed_out, sign_in_as, Account};
use crate::users::open_browser;

/// Two extra browsers sign in as two other accounts beside the suite's own:
/// each keeps its own session, and each can be screenshotted.
#[robot_test]
fn three_browsers_are_three_users(app: &mut App) {
    let (a, b, c) = (Account::new("mua"), Account::new("mub"), Account::new("muc"));
    signed_out(app);
    sign_in_as(app, &a);

    let mut second = open_browser("second");
    second.app.text("Welcome back!").assert_visible();
    sign_in_as(&mut second.app, &b);
    let mut third = open_browser("third");
    sign_in_as(&mut third.app, &c);

    // Each browser shows its own account in the user panel, not another's.
    app.text(&a.name).assert_visible();
    second.app.text(&b.name).assert_visible();
    second.app.text(&a.name).assert_hidden();
    third.app.text(&c.name).assert_visible();
    third.app.text(&b.name).assert_hidden();

    // The suite's own browser still answers after the others connected.
    app.test_id("user-settings").assert_visible();

    for (who, shot) in [("first", app.screenshot()), ("second", second.app.screenshot()), ("third", third.app.screenshot())] {
        assert!(shot.ends_with(".png"), "{who} browser screenshot was not saved: {shot}");
        eprintln!("[shot] {who}: {shot}");
    }
}
