//! Client-side friends-list wrapper (live mode).
//!
//! Thin front-end for the backend's `#[server]` fns in [`discord_server::friends`]
//! — same shape as `dms.rs` / `servers.rs`: this crate depends on `discord-server`
//! with default features, so these compile to the codegen'd RPC stubs (the acting
//! user rides the signed session token the credential provider attaches).
//!
//! Every mutating call returns the actor's FRESH [`FriendList`] (one extra
//! `list_friends` round-trip): the backend pushes the `FriendChanged` realtime
//! frame to the *other* user only — the actor re-syncs from this return value,
//! the other side re-fetches in its gateway `FriendChanged` branch. Avatars of
//! every returned participant are pushed into the shared client avatar registry
//! so the friends rows draw uploaded images on a cold load.

pub use discord_server::friends::{FriendList};

/// Pull every participant of a list into the shared client avatar registry.
fn remember_avatars(list: FriendList) {
    for group in
        list
            .incoming
            .iter()
            .chain(list.outgoing.iter())
            .chain(list.accepted.iter())
    {
        crate::auth::remember_avatar(group.username.as_str(), group.avatar_url.clone());
    }
}

/// List the signed-in user's relationships: `incoming`, `outgoing`, `accepted`.
pub async fn list_friends() -> Result<FriendList, String> {
    let list = discord_server::friends::list_friends()
        .await
        .map_err(|e| e.to_string())?;
    remember_avatars(list.clone());
    Ok(list)
}

/// Send a friend request to the user named by `username`; on success return the
/// actor's refreshed list (a crossed request auto-accepts server-side).
pub async fn send_friend_request(username: String) -> Result<FriendList, String> {
    let input = discord_server::friends::SendFriendRequestInput { username };
    discord_server::friends::send_friend_request(input)
        .await
        .map_err(|e| e.to_string())?;
    list_friends().await
}

/// Accept the incoming request from `user_id`; return the refreshed list.
pub async fn accept_friend_request(user_id: i64) -> Result<FriendList, String> {
    let input = discord_server::friends::AcceptFriendRequestInput { user_id };
    discord_server::friends::accept_friend_request(input)
        .await
        .map_err(|e| e.to_string())?;
    list_friends().await
}

/// Decline an incoming request from `user_id`, or cancel the caller's own
/// outgoing request to them; return the refreshed list.
pub async fn decline_or_cancel(user_id: i64) -> Result<FriendList, String> {
    let input = discord_server::friends::DeclineOrCancelInput { user_id };
    discord_server::friends::decline_or_cancel(input)
        .await
        .map_err(|e| e.to_string())?;
    list_friends().await
}

/// Remove the accepted friend `user_id`; return the refreshed list.
pub async fn remove_friend(user_id: i64) -> Result<FriendList, String> {
    let input = discord_server::friends::RemoveFriendInput { user_id };
    discord_server::friends::remove_friend(input)
        .await
        .map_err(|e| e.to_string())?;
    list_friends().await
}
