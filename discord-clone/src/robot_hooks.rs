//! Robot functions the E2E suite (`tests/robot.rs`) reads app state through,
//! for state no element shows: the colour theme, the message a search result
//! jumped to, and the chosen app icon. They are read-only, and like all robot
//! functions they do nothing outside an `idealyst dev` / `idealyst test` build.

/// The active colour theme's key (`"dark"`, `"ash"`, ...).
#[runtime_core::robot_fn(name = "cassette.theme")]
fn theme_key() -> Result<String, String> {
    Ok(crate::theme::current().key().to_string())
}

/// The id of the message a jump (search result, reply, pin) is highlighting.
#[runtime_core::robot_fn(name = "cassette.jump_target")]
fn jump_target() -> Result<Option<i64>, String> {
    Ok(crate::app::jump_target())
}

/// The chosen app icon's key.
#[runtime_core::robot_fn(name = "cassette.app_icon")]
fn app_icon() -> Result<String, String> {
    Ok(crate::app_icon::current().key().to_string())
}

/// The colour the active server's named role of this name is drawn in, as
/// stored in the role snapshot ("" when no such role).
#[runtime_core::robot_fn(name = "cassette.role_named")]
fn role_named_color(name: String) -> Result<String, String> {
    let color = crate::mentions::roles()
        .roles
        .into_iter()
        .find(|r| r.name == name)
        .map(|r| r.color.clone())
        .unwrap_or_default();
    Ok(color)
}

/// The colour `username`'s name is drawn in, from their highest held role
/// ("" when they hold no coloured role).
#[runtime_core::robot_fn(name = "cassette.name_color")]
fn name_color(username: String) -> Result<String, String> {
    Ok(crate::mentions::name_color(username.as_str()).unwrap_or_default())
}

/// The runs the markdown SDK's renderer paints for `body`, one line per run
/// as `text~flags` (e.g. `boldword~bold`, `codeword~code`, plain runs bare):
/// the SDK builds its semantic DOM outside the robot registry, so the suite
/// reads the parse this way instead of hunting text nodes it cannot see.
/// The walk mirrors the SDK's own pulldown-cmark state machine
/// (`markdown::parse`) with the same option flags.
#[runtime_core::robot_fn(name = "cassette.md_runs")]
fn md_runs(body: String) -> Result<Vec<String>, String> {
    use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_TASKLISTS);
    let mut out: Vec<String> = Vec::new();
    let mut bold: usize = 0;
    let mut italic: usize = 0;
    let mut strike: usize = 0;
    for ev in Parser::new_ext(body.as_str(), opts) {
        match ev {
            Event::Start(Tag::Strong) => bold += 1,
            Event::End(TagEnd::Strong) => bold = bold.saturating_sub(1),
            Event::Start(Tag::Emphasis) => italic += 1,
            Event::End(TagEnd::Emphasis) => italic = italic.saturating_sub(1),
            Event::Start(Tag::Strikethrough) => strike += 1,
            Event::End(TagEnd::Strikethrough) => strike = strike.saturating_sub(1),
            Event::Text(t) => out.push(run(&t, bold, italic, strike, false)),
            Event::Code(t) => out.push(run(&t, bold, italic, strike, true)),
            Event::SoftBreak => out.push(run(" ", bold, italic, strike, false)),
            Event::HardBreak => out.push(run("\n", bold, italic, strike, false)),
            _ => {},
        }
    }
    Ok(out)
}

/// One run line: the text plus its flags joined after `~` (no suffix for a
/// plain run).
fn run(t: &str, bold: usize, italic: usize, strike: usize, code: bool) -> String {
    let mut flags: Vec<String> = Vec::new();
    if bold > 0 { flags.push("bold".to_string()) }
    if italic > 0 { flags.push("italic".to_string()) }
    if strike > 0 { flags.push("strike".to_string()) }
    if code { flags.push("code".to_string()) }
    match flags.is_empty() {
        true => t.to_string(),
        false => t.to_string() + "~" + &flags.join("+"),
    }
}

/// Re-enter the active server the way a page reload does: fetch its roles and
/// the signed-in user's permissions, arming the dropdown's permission rows.
#[runtime_core::robot_fn(name = "cassette.rearm_perms")]
fn rearm_perms() -> Result<(), String> {
    match crate::mentions::server_id() {
        Some(sid) => {
            crate::mentions::load_roles(sid);
            Ok(())
        }
        None => Err("no active server".to_string()),
    }
}

/// Open a user's context menu (Profile, Mention, Message, friend and block
/// actions) the way a right-click on their name does: the robot has no
/// secondary press, so the suite calls this instead of right-clicking.
#[runtime_core::robot_fn(name = "cassette.open_user_menu")]
fn open_user_menu(user_id: i64) -> Result<(), String> {
    crate::mentions::open_user_menu(user_id, 200.0, 200.0);
    Ok(())
}

/// Register the functions; called once from `app()`.
pub(crate) fn register() {
    runtime_vocabulary::robot_fn::register(theme_key::robot());
    runtime_vocabulary::robot_fn::register(jump_target::robot());
    runtime_vocabulary::robot_fn::register(app_icon::robot());
    runtime_vocabulary::robot_fn::register(role_named_color::robot());
    runtime_vocabulary::robot_fn::register(name_color::robot());
    runtime_vocabulary::robot_fn::register(rearm_perms::robot());
    runtime_vocabulary::robot_fn::register(open_user_menu::robot());
    runtime_vocabulary::robot_fn::register(md_runs::robot());
}
