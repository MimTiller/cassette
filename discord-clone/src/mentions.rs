//! What the message list needs to draw mentions and role colours for the
//! active server: its members, its named roles and who holds them, which
//! loaded messages mention the signed-in user, and how to open a user's
//! card from a mention.
//!
//! The data lives here (one active server at a time) rather than being
//! threaded through every row builder; the server switch effect fills it,
//! and a `RolesChanged` gateway frame refreshes it. Changing it bumps the
//! message list's revision signal so rows redraw.

use discord_server::server_roles::{self, CustomRole, MentionSeg, RolesSnapshot};
use runtime_core::Signal;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

#[derive(Default)]
struct Ctx {
    server_id: Option<i64>,
    users: Vec<(i64, String)>,
    /// The signed-in user's level in the server ("owner", "admin", …).
    my_level: String,
    roles: RolesSnapshot,
    mentions_me: HashSet<i64>,
    rev: Option<Signal<u32>>,
    open_user: Option<Rc<dyn Fn(i64, f32, f32)>>,
}

thread_local! {
    static CTX: RefCell<Ctx> = RefCell::new(Ctx::default());
}

fn with<R>(f: impl FnOnce(&mut Ctx) -> R) -> R {
    CTX.with(|c| f(&mut c.borrow_mut()))
}

fn bump() {
    if let Some(rev) = with(|c| c.rev) {
        rev.set(rev.get() + 1);
    }
}

/// The message list's revision signal, bumped whenever roles change.
pub fn install(rev: Signal<u32>) {
    with(|c| c.rev = Some(rev));
}

/// How a mention opens a user's card (set by the message list).
pub fn set_open_user(f: Rc<dyn Fn(i64, f32, f32)>) {
    with(|c| c.open_user = Some(f));
}

pub fn open_user(user_id: i64, x: f32, y: f32) {
    if let Some(f) = with(|c| c.open_user.clone()) {
        f(user_id, x, y);
    }
}

/// The active server's members, and the signed-in user's level there.
pub fn set_members(server_id: Option<i64>, members: &[discord_server::server_settings::MemberInfo], self_id: i64) {
    with(|c| {
        if Some(c.roles.server_id) != server_id {
            c.roles = RolesSnapshot::default();
        }
        c.server_id = server_id;
        c.users = members.iter().map(|m| (m.user_id, m.username.clone())).collect();
        c.my_level = members
            .iter()
            .find(|m| m.user_id == self_id)
            .map(|m| m.role.clone())
            .unwrap_or_default();
    });
}

/// Fetch `server_id`'s roles, then redraw.
pub fn load_roles(server_id: i64) {
    runtime_core::driver::spawn_async(async move {
        let input = server_roles::ServerRolesInput { server_id };
        if let Ok(snap) = server_roles::server_roles(input).await {
            with(|c| c.roles = snap);
            bump();
        }
    });
}

pub fn server_id() -> Option<i64> {
    with(|c| c.server_id)
}

pub fn roles() -> RolesSnapshot {
    with(|c| c.roles.clone())
}

/// Whether the signed-in user may manage roles (admin or owner).
pub fn can_manage_roles() -> bool {
    with(|c| c.my_level == "admin" || c.my_level == "owner")
}

/// Whether the signed-in user's `@everyone` / `@here` notify anyone.
pub fn can_mention_everyone() -> bool {
    with(|c| matches!(c.my_level.as_str(), "owner" | "admin" | "moderator"))
}

/// Split message text into text and mentions for the active server.
pub fn segments(text: &str) -> Vec<MentionSeg> {
    if !text.contains('@') {
        return vec![MentionSeg::Text(text.to_string())];
    }
    with(|c| server_roles::segments(text, &c.users, &c.roles.roles))
}

/// `text` with its mentions taken out (to judge whether the rest is Markdown).
pub fn without_mentions(text: &str) -> String {
    segments(text)
        .into_iter()
        .filter_map(|s| match s {
            MentionSeg::Text(t) => Some(t),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The role colour for `username`'s name, if they hold a coloured role.
pub fn name_color(username: &str) -> Option<String> {
    with(|c| {
        let id = c.users.iter().find(|(_, n)| n == username)?.0;
        c.roles.color_of(id).map(str::to_string)
    })
}

/// The roles `user_id` holds in the active server, highest first.
pub fn roles_of(user_id: i64) -> Vec<CustomRole> {
    with(|c| c.roles.roles_of(user_id).into_iter().cloned().collect())
}

/// Record whether a loaded message mentions the signed-in user.
pub fn mark_message(message_id: i64, mentions_me: bool) {
    with(|c| {
        if mentions_me {
            c.mentions_me.insert(message_id);
        } else {
            c.mentions_me.remove(&message_id);
        }
    });
}

pub fn mentions_me(message_id: i64) -> bool {
    with(|c| c.mentions_me.contains(&message_id))
}

/// What `@` can complete to: members, then `everyone` / `here`, then roles
/// (only the mentionable ones unless the user may mention everyone).
pub fn completion_pool() -> Vec<String> {
    let everyone = can_mention_everyone();
    with(|c| {
        let mut out: Vec<String> = c.users.iter().map(|(_, n)| n.clone()).collect();
        out.sort_by_key(|n| n.to_lowercase());
        out.push("everyone".into());
        out.push("here".into());
        out.extend(c.roles.roles.iter().filter(|r| everyone || r.mentionable).map(|r| r.name.clone()));
        out
    })
}

/// What a completion is, for the dropdown's hint: `None` for a member.
pub fn completion_hint(name: &str) -> Option<&'static str> {
    match name {
        "everyone" => Some("Notify everyone in this server"),
        "here" => Some("Notify everyone online"),
        _ if with(|c| c.roles.roles.iter().any(|r| r.name == name)) => Some("Role"),
        _ => None,
    }
}

/// `#rrggbb` at `alpha`, as `rgba(…)` (for tinted role pills).
pub fn tint(hex: &str, alpha: f32) -> String {
    let h = hex.trim_start_matches('#');
    let v = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
    format!("rgba({}, {}, {}, {alpha})", v(0), v(2), v(4))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tint_reads_hex() {
        assert_eq!(tint("#e91e63", 0.1), "rgba(233, 30, 99, 0.1)");
    }
}
