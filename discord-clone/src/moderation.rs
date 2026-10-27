//! Client-side moderation wrapper.
//!
//! Thin front-end for the backend's permission-checked `#[server]` fns in
//! [`discord_server::server_settings`]: reading the member roster, kicking a
//! member, and changing a member's role. Same shape as `servers.rs`: this
//! crate depends on `discord-server` with default features, so these compile
//! to the codegen'd RPC stubs -- no manual HTTP here. Every call authenticates
//! as the signed-in user via the session token the `server` SDK attaches.

pub use discord_server::server_settings::MemberInfo;

/// List the members of a server with their roles, owner-first. Every row's
/// avatar (a `data:image/…;base64,` URL or `None`) is pushed into the shared
/// client avatar registry (`crate::auth::remember_avatar`) so the member
/// panel, message rows, and DMs can draw it by username; the roster is
/// authoritative, so a `None` row wipes a stale entry.
pub async fn list_members(server_id: i64) -> Result<Vec<MemberInfo>, String> {
    let input = discord_server::server_settings::ListMembersInput { server_id };
    let members = discord_server::server_settings::list_members(input)
        .await
        .map_err(|e| crate::errors::friendly(&e))?;
    for m in members.iter() {
        crate::auth::remember_avatar(m.username.as_str(), m.avatar_url.clone());
    }
    Ok(members)
}

/// Kick a member from a server. The backend enforces the real permission
/// check (moderator-or-above may call; the owner can't be removed) -- on a
/// rejection the backend's message comes back as the `Err` string.
pub async fn kick_member(server_id: i64, user_id: i64) -> Result<(), String> {
    let input = discord_server::server_settings::RemoveMemberInput { server_id, user_id };
    discord_server::server_settings::remove_member(input)
        .await
        .map_err(|e| crate::errors::friendly(&e))?;
    Ok(())
}

/// Ban a member from a server: remove them AND bar rejoining (an invite
/// redeem or a direct re-add is refused until the ban is lifted). The backend
/// enforces the real permission check (admin-or-above may call; the owner
/// can't be banned) -- on a rejection the backend's message comes back as the
/// `Err` string. `reason` is an optional free-text note stored on the ban
/// record.
pub async fn ban_member(server_id: i64, user_id: i64, reason: Option<&str>) -> Result<(), String> {
    let input = discord_server::server_settings::BanMemberInput {
        server_id,
        user_id,
        reason: reason.map(|s| s.to_string()),
    };
    discord_server::server_settings::ban_member(input)
        .await
        .map_err(|e| crate::errors::friendly(&e))?;
    Ok(())
}

/// Time out a member of a server: silence them from posting messages for
/// `duration_secs` without removing their membership (a kick/ban drop the row;
/// a timeout only sets a deadline they can post again after). The backend
/// enforces the real permission check (admin-or-above may call; the owner
/// can't be timed out; the duration must be positive) -- on a rejection the
/// backend's message comes back as the `Err` string. A re-timeout overwrites
/// the existing deadline in place.
pub async fn timeout_member(server_id: i64, user_id: i64, duration_secs: i64) -> Result<(), String> {
    let input = discord_server::server_settings::TimeoutMemberInput {
        server_id,
        user_id,
        duration_secs,
    };
    discord_server::server_settings::timeout_member(input)
        .await
        .map_err(|e| crate::errors::friendly(&e))?;
    Ok(())
}

/// Change a member's role. `role` is one of `'member'` / `'moderator'` /
/// `'admin'` (the backend rejects assigning `'owner'` or changing the owner's
/// role). The backend's rejection message comes back as the `Err` string.
pub async fn set_member_role(server_id: i64, user_id: i64, role: &str) -> Result<(), String> {
    let input = discord_server::server_settings::SetMemberRoleInput {
        server_id,
        user_id,
        role: role.to_string(),
    };
    discord_server::server_settings::set_member_role(input)
        .await
        .map_err(|e| crate::errors::friendly(&e))?;
    Ok(())
}

/// Add a member (or re-role one) in a server. `role` is one of `'member'` /
/// `'moderator'` / `'admin'` (the backend rejects assigning `'owner'`, and
/// requires admin+ to grant `admin`). The backend's rejection message comes
/// back as the `Err` string, exactly like [`set_member_role`].
pub async fn add_member(server_id: i64, user_id: i64, role: &str) -> Result<(), String> {
    let input = discord_server::server_settings::AddMemberInput {
        server_id,
        user_id,
        role: role.to_string(),
    };
    discord_server::server_settings::add_member(input)
        .await
        .map_err(|e| crate::errors::friendly(&e))?;
    Ok(())
}
