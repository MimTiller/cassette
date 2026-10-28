//! User profiles for the profile card: another user's name, avatar, account
//! age, their role and join date in a server, the servers both users share,
//! and how the two are related (friends, pending request, blocked).
//!
//! ## Design
//!
//! The same shape as [`crate::friends`]: a pure, DB-backed service
//! ([`ProfileService`]) takes the DB handle and the acting user id as plain
//! arguments and is unit-tested against a temp-file database; the
//! `#[server]` fn is a thin adapter that resolves the acting user from the
//! verified session token.
//!
//! ## Visibility
//!
//! A profile is visible to its owner and to anyone who already has a reason
//! to see the user: a shared server, any friendship row (pending or
//! accepted), or a DM. Anyone else gets the same "unknown user" error as a
//! missing id, so the endpoint cannot be used to discover accounts.

use serde::{Deserialize, Serialize};
use server::{server, ServerError};

#[cfg(feature = "server")]
use crate::db::Db;

#[cfg(feature = "server")]
use crate::session::acting_user;

/// Input for [`get_user_profile`]: whose profile, and optionally the server
/// it is being looked at in (for the role and join date there).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetUserProfileInput {
    pub user_id: i64,
    #[serde(default)]
    pub server_id: Option<i64>,
}

/// Longest display name, in characters.
pub const MAX_DISPLAY_NAME_CHARS: usize = 32;
/// Longest "about me", in characters.
pub const MAX_ABOUT_CHARS: usize = 190;

/// Check a display name: trimmed, at most [`MAX_DISPLAY_NAME_CHARS`], no
/// control characters. Empty means "show my username" (`None`).
pub fn validate_display_name(name: &str) -> Result<Option<String>, String> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(None);
    }
    if name.chars().count() > MAX_DISPLAY_NAME_CHARS {
        return Err(format!("display names can be at most {MAX_DISPLAY_NAME_CHARS} characters"));
    }
    if name.chars().any(char::is_control) {
        return Err("display names can't contain line breaks or control characters".into());
    }
    Ok(Some(name.to_string()))
}

/// Check an "about me": trimmed, at most [`MAX_ABOUT_CHARS`]; line breaks
/// are fine.
pub fn validate_about(about: &str) -> Result<String, String> {
    let about = about.trim();
    if about.chars().count() > MAX_ABOUT_CHARS {
        return Err(format!("\"About me\" can be at most {MAX_ABOUT_CHARS} characters"));
    }
    if about.chars().any(|c| c.is_control() && c != '\n') {
        return Err("\"About me\" can't contain control characters".into());
    }
    Ok(about.to_string())
}

/// Input for [`set_profile`].
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SetProfileInput {
    /// Empty to show the username instead.
    pub display_name: String,
    pub about: String,
}

/// What [`set_profile`] stored.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileFields {
    pub display_name: Option<String>,
    pub about: String,
}

/// One user's profile, as the acting user may see it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserProfile {
    pub user_id: i64,
    pub username: String,
    /// Shown in place of the username when set.
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub about: String,
    pub avatar_url: Option<String>,
    /// When the account was created, epoch milliseconds ("Member Since").
    pub created_at: i64,
    /// The role in the requested server, when the user is a member of it.
    pub server_role: Option<String>,
    /// When the user joined the requested server, epoch milliseconds.
    pub joined_server_at: Option<i64>,
    /// Servers both users are in, `(id, name)`. Empty on one's own profile.
    pub mutual_servers: Vec<(i64, String)>,
    /// How the acting user relates to this one: `"self"`, `"friends"`,
    /// `"outgoing"` (the acting user sent a request), `"incoming"` (this user
    /// sent one), or `"none"`.
    pub relationship: String,
    /// Whether the acting user has blocked this user.
    pub blocked: bool,
}

/// The profile service: visibility check + assembly from existing tables.
#[cfg(feature = "server")]
pub struct ProfileService {
    db: Db,
}

#[cfg(feature = "server")]
impl ProfileService {
    pub fn new(db: Db) -> Self {
        ProfileService { db }
    }

    /// `target`'s profile as `caller` sees it, or `"unknown user"` when the
    /// id does not exist or `caller` has no reason to see it.
    pub fn get(&self, caller: i64, target: i64, server_id: Option<i64>) -> Result<UserProfile, String> {
        let unknown = || "unknown user".to_string();
        let (_, username) = self.db.user_by_id(target).ok_or_else(unknown)?;
        let caller_servers = self.db.servers_for_user(caller);
        let mutual_servers: Vec<(i64, String)> = if caller == target {
            Vec::new()
        } else {
            let theirs: Vec<i64> = self.db.servers_for_user(target).into_iter().map(|s| s.0).collect();
            caller_servers
                .iter()
                .filter(|s| theirs.contains(&s.0))
                .map(|s| (s.0, s.1.clone()))
                .collect()
        };
        let friendship = self.db.friendship_between(caller, target);
        let visible = caller == target
            || !mutual_servers.is_empty()
            || friendship.is_some()
            || self.db.dm_channel_between(caller, target).is_some();
        if !visible {
            return Err(unknown());
        }
        let relationship = match &friendship {
            _ if caller == target => "self",
            Some((_, _, _, status)) if status == "accepted" => "friends",
            Some((_, requester, _, _)) if *requester == caller => "outgoing",
            Some(_) => "incoming",
            None => "none",
        };
        let (server_role, joined_server_at) = match server_id {
            Some(sid) => self.membership(sid, target),
            None => (None, None),
        };
        Ok(UserProfile {
            user_id: target,
            username,
            display_name: self.db.user_display_name(target),
            about: self.db.user_about(target),
            avatar_url: self.db.user_avatar(target),
            created_at: self.created_at(target),
            server_role,
            joined_server_at,
            mutual_servers,
            relationship: relationship.to_string(),
            blocked: caller != target && self.db.blocked_ids_for_user(caller).contains(&target),
        })
    }

    /// Set `user_id`'s display name and about text.
    pub fn set_fields(&self, user_id: i64, display_name: &str, about: &str) -> Result<ProfileFields, String> {
        let display_name = validate_display_name(display_name)?;
        let about = validate_about(about)?;
        self.db
            .set_user_profile(user_id, display_name.as_deref(), &about)
            .map_err(|e| format!("failed to save your profile: {e}"))?;
        Ok(ProfileFields { display_name, about })
    }

    fn created_at(&self, user_id: i64) -> i64 {
        let Ok(c) = self.db.lock() else {
            return 0;
        };
        c.query_row("SELECT created_at FROM users WHERE id = ?1", rusqlite::params![user_id], |r| r.get(0))
            .unwrap_or(0)
    }

    fn membership(&self, server_id: i64, user_id: i64) -> (Option<String>, Option<i64>) {
        let Ok(c) = self.db.lock() else {
            return (None, None);
        };
        c.query_row(
            "SELECT role, joined_at FROM server_memberships WHERE server_id = ?1 AND user_id = ?2",
            rusqlite::params![server_id, user_id],
            |r| Ok((Some(r.get::<_, String>(0)?), Some(as_epoch_ms(r.get::<_, i64>(1)?)))),
        )
        .unwrap_or((None, None))
    }
}

/// `server_memberships.joined_at` is written in two units: milliseconds by
/// server creation, seconds by an invite join and by add-member. Any value
/// below 10^11 (that is before 1973 as milliseconds, or after year 5000 as
/// seconds) is read as seconds.
#[cfg(feature = "server")]
fn as_epoch_ms(t: i64) -> i64 {
    if t < 100_000_000_000 {
        t * 1000
    } else {
        t
    }
}

/// Another user's profile for the profile card. See the module docs for who
/// may see what. The acting user is resolved from the verified session token.
#[server]
pub async fn get_user_profile(input: GetUserProfileInput) -> Result<UserProfile, ServerError> {
    let db = db_from_state()?;
    let user = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ProfileService::new(db)
        .get(user, input.user_id, input.server_id)
        .map_err(ServerError::failed)
}

/// Set the signed-in user's display name (empty to show the username) and
/// "about me", and tell everyone who shares a server with them.
#[server]
pub async fn set_profile(input: SetProfileInput) -> Result<ProfileFields, ServerError> {
    let db = db_from_state()?;
    let user = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let fields = ProfileService::new(db.clone())
        .set_fields(user, &input.display_name, &input.about)
        .map_err(ServerError::failed)?;
    let username = db.user_by_id(user).map(|(_, n)| n).unwrap_or_default();
    crate::realtime::hub().publish(
        &crate::realtime::presence_audience(&db, user),
        &crate::server_settings::WsServerMsg::ProfileChanged {
            user_id: user,
            username,
            display_name: fields.display_name.clone(),
        },
    );
    Ok(fields)
}

/// Pull the shared `Db` out of server state (installed at startup by the
/// server bin via `server::install_state(db)`).
#[cfg(feature = "server")]
fn db_from_state() -> Result<Db, ServerError> {
    server::use_state::<Db>()
        .ok_or_else(|| ServerError::failed("database not installed"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Temp-file DB with four users; alice and bob share server 10, carol
    /// shares nothing with anyone, dave is only alice's pending friend.
    fn setup_db() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let db = Db::migrate(&dir.path().join("test.db"), &migrations_dir).unwrap();
        let c = db.lock().unwrap();
        c.execute_batch(
            "INSERT INTO users (id, username, email, created_at) VALUES
                (1, 'alice', 'a@example.com', 1000),
                (2, 'bob',   'b@example.com', 2000),
                (3, 'carol', 'c@example.com', 3000),
                (4, 'dave',  'd@example.com', 4000);
             INSERT INTO servers (id, name, owner_id, created_at) VALUES (10, 'Hangout', 1, 5000);
             INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES
                (10, 1, 'owner', 1700000000000),
                (10, 2, 'member', 1700000000);
             INSERT INTO friendships (min_user_id, max_user_id, requester_id, addressee_id, status, created_at)
                VALUES (1, 4, 4, 1, 'pending', 7000);",
        )
        .unwrap();
        drop(c);
        (dir, db)
    }

    #[test]
    fn a_server_mate_sees_role_join_date_and_mutual_servers() {
        let (_d, db) = setup_db();
        let p = ProfileService::new(db).get(1, 2, Some(10)).unwrap();
        assert_eq!(p.username, "bob");
        assert_eq!(p.created_at, 2000);
        assert_eq!(p.server_role.as_deref(), Some("member"));
        // Stored in seconds (an invite join), returned in milliseconds.
        assert_eq!(p.joined_server_at, Some(1_700_000_000_000));
        assert_eq!(p.mutual_servers, vec![(10, "Hangout".to_string())]);
        assert_eq!(p.relationship, "none");
        assert!(!p.blocked);
    }

    #[test]
    fn a_stranger_is_indistinguishable_from_a_missing_id() {
        let (_d, db) = setup_db();
        let svc = ProfileService::new(db);
        assert_eq!(svc.get(1, 3, None).unwrap_err(), "unknown user");
        assert_eq!(svc.get(1, 999, None).unwrap_err(), "unknown user");
    }

    #[test]
    fn a_pending_request_makes_the_profile_visible_both_ways() {
        let (_d, db) = setup_db();
        let svc = ProfileService::new(db);
        assert_eq!(svc.get(1, 4, None).unwrap().relationship, "incoming");
        assert_eq!(svc.get(4, 1, None).unwrap().relationship, "outgoing");
        // Not in the server asked about: no role or join date.
        assert_eq!(svc.get(1, 4, Some(10)).unwrap().server_role, None);
    }

    #[test]
    fn own_profile_and_blocks() {
        let (_d, db) = setup_db();
        let me = ProfileService::new(db.clone()).get(1, 1, Some(10)).unwrap();
        assert_eq!(me.relationship, "self");
        assert!(me.mutual_servers.is_empty());
        assert_eq!(me.server_role.as_deref(), Some("owner"));
        db.insert_user_block(1, 2);
        assert!(ProfileService::new(db.clone()).get(1, 2, None).unwrap().blocked);
        assert!(!ProfileService::new(db).get(2, 1, None).unwrap().blocked);
    }

    #[test]
    fn display_names_and_about_are_checked() {
        assert_eq!(validate_display_name("  Tim  "), Ok(Some("Tim".into())));
        assert_eq!(validate_display_name("   "), Ok(None));
        assert!(validate_display_name(&"x".repeat(33)).is_err());
        assert!(validate_display_name("a\nb").is_err());
        assert_eq!(validate_display_name("Ünïcødé 🎵"), Ok(Some("Ünïcødé 🎵".into())));
        assert_eq!(validate_about(" hi\nthere "), Ok("hi\nthere".into()));
        assert!(validate_about(&"x".repeat(191)).is_err());
        assert!(validate_about("bell\u{7}").is_err());
    }

    #[test]
    fn set_fields_stores_and_profiles_show_them() {
        let (_d, db) = setup_db();
        let svc = ProfileService::new(db.clone());
        let saved = svc.set_fields(2, " Bobby ", "I like trains").unwrap();
        assert_eq!(saved, ProfileFields { display_name: Some("Bobby".into()), about: "I like trains".into() });
        let p = svc.get(1, 2, None).unwrap();
        assert_eq!((p.display_name.as_deref(), p.about.as_str()), (Some("Bobby"), "I like trains"));
        // Clearing goes back to the username.
        svc.set_fields(2, "", "").unwrap();
        assert_eq!(svc.get(1, 2, None).unwrap().display_name, None);
        assert!(svc.set_fields(2, &"x".repeat(40), "").is_err());
    }
}
