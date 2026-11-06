//! Client-side moderation wrapper.
//!
//! Thin front-end for the backend's permission-checked `#[server]` fns in
//! [`discord_server::server_settings`]: reading the member roster, kicking a
//! member, and changing a member's role. Same shape as `servers.rs`: this
//! crate depends on `discord-server` with default features, so these compile
//! to the codegen'd RPC stubs -- no manual HTTP here. Every call authenticates
//! as the signed-in user via the session token the `server` SDK attaches.

pub use crate::backend::MemberInfo;

/// List the members of a server with their roles, owner-first. Every row's
/// avatar (a `data:image/…;base64,` URL or `None`) is pushed into the shared
/// client avatar registry (`crate::auth::remember_avatar`) so the member
/// panel, message rows, and DMs can draw it by username; the roster is
/// authoritative, so a `None` row wipes a stale entry.
pub async fn list_members(server_id: i64) -> Result<Vec<MemberInfo>, String> {
    let members = crate::backend::list_members(server_id).await?;
    for m in members.iter() {
        crate::auth::remember_avatar(m.username.as_str(), m.avatar_url.clone());
    }
    Ok(members)
}

/// Kick a member from a server. The backend enforces the real permission
/// check (moderator-or-above may call; the owner can't be removed) -- on a
/// rejection the backend's message comes back as the `Err` string.
pub async fn kick_member(server_id: i64, user_id: i64) -> Result<(), String> {
    crate::backend::remove_member(server_id, user_id).await.map(|_| ())
}

/// Ban a member from a server: remove them AND bar rejoining (an invite
/// redeem or a direct re-add is refused until the ban is lifted). The backend
/// enforces the real permission check (admin-or-above may call; the owner
/// can't be banned) -- on a rejection the backend's message comes back as the
/// `Err` string. `reason` is an optional free-text note stored on the ban
/// record.
pub async fn ban_member(server_id: i64, user_id: i64, reason: Option<&str>) -> Result<(), String> {
    crate::backend::ban_member(server_id, user_id, reason.map(|s| s.to_string())).await.map(|_| ())
}

/// Time out a member of a server: silence them from posting messages for
/// `duration_secs` without removing their membership (a kick/ban drop the row;
/// a timeout only sets a deadline they can post again after). The backend
/// enforces the real permission check (admin-or-above may call; the owner
/// can't be timed out; the duration must be positive) -- on a rejection the
/// backend's message comes back as the `Err` string. A re-timeout overwrites
/// the existing deadline in place.
pub async fn timeout_member(server_id: i64, user_id: i64, duration_secs: i64) -> Result<(), String> {
    crate::backend::timeout_member(server_id, user_id, duration_secs).await.map(|_| ())
}

/// Add a user to a server as a plain member. The backend's rejection
/// message comes back as the `Err` string.
pub async fn add_member(server_id: i64, user_id: i64) -> Result<(), String> {
    crate::backend::add_member(server_id, user_id).await.map(|_| ())
}
