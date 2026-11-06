//! Client-side friends-list wrapper (live mode).
//!
//! Thin front-end for the backend seam's friend verbs
//! (`crate::backend::list_friends` etc) — same shape as `dms.rs` / `servers.rs`:
//! every call routes through the single backend interface, so the RPCs never
//! appear here and a different backend later replaces only the implementation
//! files under `crate::backend`.
//!
//! Every mutating call returns the actor's FRESH [`FriendList`] (one extra
//! `list_friends` round-trip): the backend pushes the `FriendChanged` realtime
//! frame to the *other* user only — the actor re-syncs from this return value,
//! the other side re-fetches in its gateway `FriendChanged` branch. Avatars of
//! every returned participant are pushed into the shared client avatar registry
//! so the friends rows draw uploaded images on a cold load.

pub use crate::backend::{FriendList};

/// Pull every participant of a list into the shared client avatar and
/// display-name registries.
fn remember_avatars(list: FriendList) {
    for group in
        list
            .incoming
            .iter()
            .chain(list.outgoing.iter())
            .chain(list.accepted.iter())
    {
        crate::auth::remember_avatar(group.username.as_str(), group.avatar_url.clone());
        crate::auth::remember_display_name(group.username.as_str(), group.display_name.clone());
    }
}

/// List the signed-in user's relationships: `incoming`, `outgoing`, `accepted`.
pub async fn list_friends() -> Result<FriendList, String> {
    let list = crate::backend::list_friends().await?;
    remember_avatars(list.clone());
    Ok(list)
}

/// Send a friend request to the user named by `username`; on success return the
/// actor's refreshed list (a crossed request auto-accepts server-side).
pub async fn send_friend_request(username: String) -> Result<FriendList, String> {
    crate::backend::send_friend_request(username).await?;
    list_friends().await
}

/// Accept the incoming request from `user_id`; return the refreshed list.
pub async fn accept_friend_request(user_id: i64) -> Result<FriendList, String> {
    crate::backend::accept_friend_request(user_id).await?;
    list_friends().await
}

/// Decline an incoming request from `user_id`, or cancel the caller's own
/// outgoing request to them; return the refreshed list.
pub async fn decline_or_cancel(user_id: i64) -> Result<FriendList, String> {
    crate::backend::decline_or_cancel(user_id).await?;
    list_friends().await
}

/// Remove the accepted friend `user_id`; return the refreshed list.
pub async fn remove_friend(user_id: i64) -> Result<FriendList, String> {
    crate::backend::remove_friend(user_id).await?;
    list_friends().await
}
