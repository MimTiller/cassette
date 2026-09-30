//! Client-side moderation wrapper.
//!
//! Thin front-end for the backend's permission-checked `#[server]` fns in
//! [`discord_server::server_settings`]: reading the member roster, kicking a
//! member, and changing a member's role. Same shape as `servers.rs`: this
//! crate depends on `discord-server` with default features, so these compile
//! to the codegen'd RPC stubs -- no manual HTTP here. Every call authenticates
//! as the signed-in user via the session token the `server` SDK attaches.

pub use discord_server::server_settings::MemberInfo;

/// List the members of a server with their roles, owner-first.
pub async fn list_members(server_id: i64) -> Result<Vec<MemberInfo>, String> {
    let input = discord_server::server_settings::ListMembersInput { server_id };
    discord_server::server_settings::list_members(input)
        .await
        .map_err(|e| e.to_string())
}

/// Kick a member from a server. The backend enforces the real permission
/// check (moderator-or-above may call; the owner can't be removed) -- on a
/// rejection the backend's message comes back as the `Err` string.
pub async fn kick_member(server_id: i64, user_id: i64) -> Result<(), String> {
    let input = discord_server::server_settings::RemoveMemberInput { server_id, user_id };
    discord_server::server_settings::remove_member(input)
        .await
        .map_err(|e| e.to_string())?;
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
        .map_err(|e| e.to_string())?;
    Ok(())
}
