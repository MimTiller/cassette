//! Robot E2E suite, run by `idealyst test --web discord-clone` (see e2e/README.md).
//!
//! Each test drives the app the way a user does, through `test_id`s and
//! visible labels, against the run's own isolated server and database. Tests
//! must not depend on run order: each makes its own fresh accounts.
//!
//! The suite's own browser is `app`. A test that needs more people opens
//! more browsers with [`users::open_browser`], each a full driver of its own.

#[path = "robot/support.rs"]
mod support;
#[path = "robot/users.rs"]
mod users;

#[path = "robot/auth.rs"]
mod auth;
#[path = "robot/multi_user.rs"]
mod multi_user;
#[path = "robot/server.rs"]
mod server;
#[path = "robot/settings.rs"]
mod settings;
#[path = "robot/profile_status.rs"]
mod profile_status;
#[path = "robot/two_users.rs"]
mod two_users;
#[path = "robot/messages.rs"]
mod messages;
#[path = "robot/dms_friends.rs"]
mod dms_friends;
#[path = "robot/notifications.rs"]
mod notifications;
