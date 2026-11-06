//! Client-side moderation wrapper.
//!
//! Client-side moderation wrapper.
//!
//! Thin front-end for the backend's permission-checked kick: roster reads,
//! bans, timeouts and adds go straight through `crate::backend`. The
//! backend enforces the real permission check (moderator-or-above may kick;
//! the owner can't be removed) — on a rejection the backend's message comes
//! back as the `Err` string.

pub use crate::backend::MemberInfo;

/// Kick a member from a server. The backend enforces the real permission
/// check (moderator-or-above may call; the owner can't be removed) -- on a
/// rejection the backend's message comes back as the `Err` string.
pub async fn kick_member(server_id: i64, user_id: i64) -> Result<(), String> {
    crate::backend::remove_member(server_id, user_id).await.map(|_| ())
}
