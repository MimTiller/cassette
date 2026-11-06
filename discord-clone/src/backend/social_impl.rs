//! Today's friends and blocks: the `discord-server` RPC stubs.

use super::*;

impl SocialApi for CassetteBackend {
    async fn list_friends() -> Result<FriendList, ServerError> {
        discord_server::friends::list_friends().await
    }
    async fn send_friend_request(username: String) -> Result<SendFriendRequestResult, ServerError> {
        discord_server::friends::send_friend_request(
            discord_server::friends::SendFriendRequestInput { username },
        ).await
    }
    async fn accept_friend_request(user_id: i64) -> Result<FriendUser, ServerError> {
        discord_server::friends::accept_friend_request(
            discord_server::friends::AcceptFriendRequestInput { user_id },
        ).await
    }
    async fn decline_or_cancel(user_id: i64) -> Result<FriendUser, ServerError> {
        discord_server::friends::decline_or_cancel(
            discord_server::friends::DeclineOrCancelInput { user_id },
        ).await
    }
    async fn remove_friend(user_id: i64) -> Result<FriendUser, ServerError> {
        discord_server::friends::remove_friend(
            discord_server::friends::RemoveFriendInput { user_id },
        ).await
    }

    async fn list_blocked() -> Result<Vec<BlockedUser>, ServerError> {
        discord_server::blocks::list_blocked().await
    }
    async fn block_user(user_id: i64) -> Result<BlockedUser, ServerError> {
        discord_server::blocks::block_user(
            discord_server::blocks::BlockUserInput { user_id },
        ).await
    }
    async fn unblock_user(user_id: i64) -> Result<BlockedUser, ServerError> {
        discord_server::blocks::unblock_user(
            discord_server::blocks::BlockUserInput { user_id },
        ).await
    }
}
