//! What the message list needs to draw mentions and role colours for the
//! active server: its members, its named roles and who holds them, which
//! loaded messages mention the signed-in user, and how to open a user's
//! card from a mention. It also holds what the signed-in user may do there
//! ([`can`], [`can_in`], [`can_here`], [`my_rank`], [`rank_of`]), which
//! decides which controls show; the server enforces the same rules.
//!
//! The data lives here (one active server at a time) rather than being
//! threaded through every row builder; the server switch effect fills it,
//! and a `RolesChanged` gateway frame refreshes it. Changing it bumps the
//! message list's revision signal so rows redraw.

use discord_server::permissions::{self, MyPermissions, Permissions, Rank};
use discord_server::server_roles::{self, CustomRole, MentionSeg, RolesSnapshot};
use runtime_core::Signal;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

#[derive(Default)]
struct Ctx {
    server_id: Option<i64>,
    users: Vec<(i64, String)>,
    /// The server owner's user id.
    owner_id: Option<i64>,
    /// What the signed-in user may do in the server (`None` until fetched).
    perms: Option<MyPermissions>,
    /// Bumped when `perms` changes, so reactive reads of [`can`] redraw.
    perms_rev: Option<Signal<u32>>,
    /// The open channel, for [`can_here`].
    channel: Option<Signal<Option<i64>>>,
    roles: RolesSnapshot,
    mentions_me: HashSet<i64>,
    rev: Option<Signal<u32>>,
    /// Counts changes to names, members and roles (see [`names_version`]).
    names_version: u64,
    open_user: Option<Rc<dyn Fn(i64, f32, f32)>>,
}

thread_local! {
    static CTX: RefCell<Ctx> = RefCell::new(Ctx::default());
}

fn with<R>(f: impl FnOnce(&mut Ctx) -> R) -> R {
    CTX.with(|c| f(&mut c.borrow_mut()))
}

fn bump() {
    if let Some(rev) = with(|c| {
        c.names_version += 1;
        c.rev
    }) {
        rev.set(rev.get() + 1);
    }
}

/// How many times names, members or roles have changed. Unlike
/// [`revision`] (the message list's counter, bumped by every message), this
/// only moves when what a row shows about its author can change.
pub fn names_version() -> u64 {
    with(|c| c.names_version)
}

/// Read the revision signal, so a reactive closure that calls this redraws
/// when names, roles or members change.
pub fn revision() -> u32 {
    with(|c| c.rev).map(|rev| rev.get()).unwrap_or(0)
}

/// The message list's revision signal, bumped whenever roles change.
pub fn install(rev: Signal<u32>) {
    let perms_rev = runtime_core::signal(0u32);
    with(|c| {
        c.rev = Some(rev);
        c.perms_rev = Some(perms_rev);
    });
}

/// The open channel's signal, which [`can_here`] reads.
pub fn install_channel(channel: Signal<Option<i64>>) {
    with(|c| c.channel = Some(channel));
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

/// The active server's members. Switching servers drops the old server's
/// roles and permissions until [`load_roles`] fetches the new ones; returns
/// whether it switched.
fn set_members(server_id: Option<i64>, members: &[discord_server::server_settings::MemberInfo]) -> bool {
    let switched = with(|c| {
        let switched = c.server_id != server_id;
        if Some(c.roles.server_id) != server_id {
            c.roles = RolesSnapshot::default();
        }
        if switched {
            c.perms = None;
        }
        c.server_id = server_id;
        c.users = members.iter().map(|m| (m.user_id, m.username.clone())).collect();
        // `role` only still marks the owner.
        c.owner_id = members.iter().find(|m| m.role == "owner").map(|m| m.user_id);
        switched
    });
    if switched {
        bump_perms();
    }
    switched
}

/// A freshly fetched roster for `server_id`: record it, then redraw. The
/// first roster for a server (entering it from the rail, creating it, a
/// refresh) also fetches its roles and the signed-in user's permissions.
pub fn members_loaded(server_id: i64, members: &[discord_server::server_settings::MemberInfo]) {
    for m in members {
        crate::auth::remember_display_name(&m.username, m.display_name.clone());
    }
    let switched = set_members(Some(server_id), members);
    bump();
    if switched {
        load_roles(server_id);
    }
}

/// Fetch `server_id`'s roles and the signed-in user's permissions there,
/// then redraw. Runs on entering a server and on every `RolesChanged`.
pub fn load_roles(server_id: i64) {
    crate::notif_prefs::load();
    crate::custom_emoji::load(server_id);
    runtime_core::driver::spawn_async(async move {
        let input = server_roles::ServerRolesInput { server_id };
        if let Ok(snap) = server_roles::server_roles(input).await {
            with(|c| c.roles = snap);
            bump();
        }
        let input = permissions::MyPermissionsInput { server_id };
        if let Ok(mine) = permissions::my_permissions(input).await {
            let current = with(|c| {
                let current = c.server_id.is_none_or(|s| s == mine.server_id);
                if current {
                    c.perms = Some(mine);
                }
                current
            });
            if current {
                bump_perms();
                bump();
            }
        }
    });
}

fn bump_perms() {
    if let Some(rev) = with(|c| c.perms_rev) {
        rev.set(rev.get() + 1);
    }
}

/// Read the permissions' revision, so a reactive closure redraws when they
/// change.
fn track_perms() {
    if let Some(rev) = with(|c| c.perms_rev) {
        rev.get();
    }
}

/// Whether the signed-in user's permissions for the active server have
/// loaded (controls that are normally there wait for them before hiding).
pub fn perms_known() -> bool {
    track_perms();
    with(|c| c.perms.is_some())
}

/// Whether the signed-in user has `perm` server-wide in the active server.
pub fn can(perm: Permissions) -> bool {
    track_perms();
    with(|c| c.perms.as_ref().is_some_and(|p| p.server.contains(perm)))
}

/// Whether the signed-in user has `perm` in `channel_id`.
pub fn can_in(channel_id: i64, perm: Permissions) -> bool {
    track_perms();
    with(|c| {
        c.perms.as_ref().is_some_and(|p| {
            p.channels.iter().any(|(id, have)| *id == channel_id && have.contains(perm))
        })
    })
}

/// Whether the signed-in user has `perm` in the open channel.
pub fn can_here(perm: Permissions) -> bool {
    let channel = with(|c| c.channel).and_then(|ch| ch.get());
    channel.is_some_and(|id| can_in(id, perm))
}

/// The signed-in user's standing in the active server.
pub fn my_rank() -> Rank {
    track_perms();
    with(|c| c.perms.as_ref().map_or(Rank::Everyone, |p| p.rank))
}

/// `user_id`'s standing in the active server: owner, else their highest
/// role's position, else plain member.
pub fn rank_of(user_id: i64) -> Rank {
    with(|c| rank_in(&c.roles, c.owner_id, user_id))
}

fn rank_in(roles: &RolesSnapshot, owner_id: Option<i64>, user_id: i64) -> Rank {
    Rank::of(owner_id == Some(user_id), roles.roles_of(user_id).into_iter().map(|r| r.position))
}

pub fn server_id() -> Option<i64> {
    with(|c| c.server_id)
}

pub fn roles() -> RolesSnapshot {
    with(|c| c.roles.clone())
}

/// Whether the signed-in user's `@everyone` / `@here` notify anyone.
pub fn can_mention_everyone() -> bool {
    can(Permissions::MENTION_EVERYONE)
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
pub fn completion_hint(name: &str) -> Option<String> {
    match name {
        "everyone" => Some("Notify everyone in this server".into()),
        "here" => Some("Notify everyone online".into()),
        _ if with(|c| c.roles.roles.iter().any(|r| r.name == name)) => Some("Role".into()),
        _ => Some(crate::auth::shown_name(name)).filter(|shown| shown != name),
    }
}

/// The user ids of the active server's members.
pub fn member_ids() -> Vec<i64> {
    with(|c| c.users.iter().map(|(id, _)| *id).collect())
}

/// The username of member `user_id` in the active server.
pub fn username_of(user_id: i64) -> Option<String> {
    with(|c| c.users.iter().find(|(id, _)| *id == user_id).map(|(_, n)| n.clone()))
}

/// Redraw everything that shows names (after a display name changed).
pub fn names_changed() {
    bump();
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

    fn role(id: i64, position: i64) -> CustomRole {
        CustomRole {
            id,
            server_id: 1,
            name: format!("r{id}"),
            color: String::new(),
            mentionable: false,
            position,
            permissions: Permissions::NONE,
        }
    }

    #[test]
    fn rank_comes_from_owner_then_highest_role() {
        let snap = RolesSnapshot {
            server_id: 1,
            roles: vec![role(10, 3), role(11, 1)],
            assignments: vec![(2, 11), (3, 10), (3, 11)],
            ..Default::default()
        };
        assert_eq!(rank_in(&snap, Some(1), 1), Rank::Owner);
        assert_eq!(rank_in(&snap, Some(1), 2), Rank::Role(1));
        assert_eq!(rank_in(&snap, Some(1), 3), Rank::Role(3));
        assert_eq!(rank_in(&snap, Some(1), 4), Rank::Everyone);
        assert!(Rank::Role(3).outranks(rank_in(&snap, Some(1), 2)));
        assert!(!rank_in(&snap, Some(1), 2).outranks(Rank::Role(1)));
    }

    #[test]
    fn tint_reads_hex() {
        assert_eq!(tint("#e91e63", 0.1), "rgba(233, 30, 99, 0.1)");
    }
}
