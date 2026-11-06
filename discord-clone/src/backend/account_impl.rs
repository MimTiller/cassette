//! Today's account operations: the `discord-server` auth, session and
//! profile RPC stubs.

use super::*;

impl AccountApi for CassetteBackend {
    async fn login(username: String, password: String) -> Result<LoginResult, ServerError> {
        discord_server::auth::login(discord_server::auth::LoginInput { username, password }).await
    }
    async fn signup(username: String, password: String) -> Result<SignupResult, ServerError> {
        discord_server::auth::signup(discord_server::auth::SignupInput { username, password }).await
    }
    async fn logout() -> Result<LogoutResult, ServerError> {
        discord_server::auth::logout().await
    }
    async fn logout_everywhere() -> Result<LogoutResult, ServerError> {
        discord_server::auth::logout_everywhere().await
    }
    async fn change_password(current_password: String, new_password: String) -> Result<ChangePasswordResult, ServerError> {
        discord_server::auth::change_password(
            discord_server::auth::ChangePasswordInput { current_password, new_password },
        ).await
    }
    async fn change_username(new_username: String) -> Result<ChangeUsernameResult, ServerError> {
        discord_server::auth::change_username(
            discord_server::auth::ChangeUsernameInput { new_username },
        ).await
    }
    async fn reset_password(token: String, new_password: String) -> Result<ResetPasswordResult, ServerError> {
        discord_server::password_resets::reset_password(
            discord_server::password_resets::ResetPasswordInput { token, new_password },
        ).await
    }
    async fn set_avatar(avatar_url: Option<String>) -> Result<AvatarResult, ServerError> {
        discord_server::auth::set_avatar(
            discord_server::auth::SetAvatarInput { avatar_url },
        ).await
    }
    async fn get_user_profile(user_id: i64, server_id: Option<i64>) -> Result<UserProfile, ServerError> {
        discord_server::profiles::get_user_profile(
            discord_server::profiles::GetUserProfileInput { user_id, server_id },
        ).await
    }
    async fn set_profile(display_name: String, about: String) -> Result<ProfileFields, ServerError> {
        discord_server::profiles::set_profile(
            discord_server::profiles::SetProfileInput { display_name, about },
        ).await
    }
    async fn set_avatar_decoration(decoration: Option<String>) -> Result<Option<String>, ServerError> {
        discord_server::profiles::set_avatar_decoration(
            discord_server::profiles::SetAvatarDecorationInput { decoration },
        ).await
    }
    async fn set_profile_effect(effect: Option<String>) -> Result<Option<String>, ServerError> {
        discord_server::profiles::set_profile_effect(
            discord_server::profiles::SetProfileEffectInput { effect },
        ).await
    }
}
