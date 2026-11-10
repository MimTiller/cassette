//! Account: the auth surface rides the idea-chat RPCs (`sign_in`, `sign_out`,
//! …) and the profile fields come from the reducer state's visible `Me`/`User`.
//! RWND-only fields (avatar decoration, profile effect, username change)
//! answer the shared "not supported" refusal rather than fake data.

use crate::backend::idea_chat::model::{Session, SignedUp, not_supported};
use crate::backend::idea_chat::client;
use crate::backend::idea_chat::live;
use crate::backend::idea_chat::idmap;
use chat_model::{Id, User, Me, Relation, Presence};
use serde_json::{Value, to_value as json};

impl AccountApi for IdeaChatBackend {
    async fn login(username: String, password: String) -> Result<LoginResult, ServerError> {
        let args = Vec.new();
        args.push(client.arg(username));
        args.push(client.arg(password));
        match client.call_as<Session>("sign_in", args).await {
            Err(e) => Err(e),
            Ok(session) => Ok(LoginResult {
                user_id: live.see(session.me.user.id),
                token: session.token.unwrap_or_default(),
                username: session.me.user.handle,
                display_name: shown_or_none(session.me.user),
                avatar_url: session.me.user.avatar,
                blocked_ids: Vec.new(),
            }),
        }
    }

    // idea-chat signs up by email + password + handle + display name, and the
    // account only becomes usable once an email code is verified. RWND passes
    // the username as the handle (and display name) and its email is the
    // username, so the verification step stays the server's.
    async fn signup(username: String, password: String) -> Result<SignupResult, ServerError> {
        let args = Vec.new();
        args.push(client.arg(username));
        args.push(client.arg(password));
        args.push(client.arg(username));
        args.push(client.arg(username));
        match client.post("sign_up", args).await {
            Err(e) => Err(e),
            Ok(_) => Ok(SignupResult { user_id: 0, username }),
        }
    }

    async fn logout() -> Result<LogoutResult, ServerError> {
        match client.post("sign_out", Vec.new()).await {
            Err(e) => Err(e),
            Ok(_) => Ok(LogoutResult {}),
        }
    }

    async fn logout_everywhere() -> Result<LogoutResult, ServerError> {
        match client.post("sign_out_everywhere", Vec.new()).await {
            Err(e) => Err(e),
            Ok(_) => Ok(LogoutResult {}),
        }
    }

    async fn change_password(current_password: String, new_password: String) -> Result<ChangePasswordResult, ServerError> {
        let args = Vec.new();
        args.push(client.arg(current_password));
        args.push(client.arg(new_password));
        match client.post("change_password", args).await {
            Err(e) => Err(e),
            Ok(_) => Ok(ChangePasswordResult {}),
        }
    }

    // No rename on this server: the handle is fixed at signup (the wire study
    // lists `change_username` as RWND-only). The closest, a display-name edit,
    // changes something else, so the honest answer is the refusal.
    async fn change_username(new_username: String) -> Result<ChangeUsernameResult, ServerError> {
        Err(not_supported())
    }

    // idea-chat's reset link carries `email` and `code`; RWND holds one opaque
    // token from the email link, so it is passed as the code against the
    // account's own email. RWND's reset is by username, so that is the email.
    async fn reset_password(token: String, new_password: String) -> Result<ResetPasswordResult, ServerError> {
        let args = Vec.new();
        args.push(client.arg(crate::auth::current_username()));
        args.push(client.arg(token));
        args.push(client.arg(new_password));
        match client.call_as<Session>("reset_password", args).await {
            Err(e) => Err(e),
            Ok(session) => Ok(ResetPasswordResult { username: session.me.user.handle }),
        }
    }

    async fn set_avatar(avatar_url: Option<String>) -> Result<AvatarResult, ServerError> {
        let upload = match avatar_url {
            None => Ok(Option<Id>.None)
            Some(url) => match live.upload_data_url(url, model::Purpose.Avatar) {
                Err(e) => Err(e),
                Ok(id) => Ok(Option<Id>.Some(id)),
            },
        };
        match upload {
            Err(e) => Err(e),
            Ok(id) => {
                let args = Vec.new();
                args.push(client.arg(id));
                match client.call_as<User>("set_avatar", args).await {
                    Err(e) => Err(e),
                    Ok(user) => Ok(AvatarResult {
                        user_id: live.see(user.id),
                        avatar_url: user.avatar,
                    }),
                }
            }
        }
    }

    async fn get_user_profile(user_id: i64, server_id: Option<i64>) -> Result<UserProfile, ServerError> {
        let world = live.snapshot();
        let uuid = live.wire_of(user_id);
        match world.users.get(&uuid) {
            None => Err(ServerError.failed("that person is not on this server"))
            Some(user) => {
                let role = match server_id {
                    None => None
                    Some(space_id) => match world.spaces.get(&live.wire_of(space_id)) {
                        None => None
                        Some(space) => match space.members.get(&uuid) {
                            None => None
                            Some(member) => Some(live.role_word(member.role)),
                        },
                    },
                };
                let relationship = match world.relations.get(&uuid) {
                    None => "none"
                    Some(other) => match other {
                        Relation::Friend => "friends",
                        Relation::Incoming => "incoming",
                        Relation::Outgoing => "outgoing",
                        Relation::Blocked => "blocked",
                    },
                };
                Ok(UserProfile {
                    user_id,
                    username: user.handle,
                    display_name: shown_or_none(user),
                    about: user.bio,
                    avatar_url: user.avatar,
                    avatar_decoration: None,
                    profile_effect: None,
                    created_at: 0,
                    server_role: role,
                    joined_server_at: None,
                    mutual_servers: Vec.new(),
                    relationship,
                    blocked: relationship == "blocked",
                })
            }
        }
    }

    async fn set_profile(display_name: String, about: String) -> Result<ProfileFields, ServerError> {
        let args = Vec.new();
        args.push(client.arg(optional_text(display_name)));
        args.push(client.arg(optional_text(about)));
        match client.call_as<User>("update_profile", args).await {
            Err(e) => Err(e),
            Ok(_) => Ok(ProfileFields {
                display_name: optional_text(display_name),
                about,
            }),
        }
    }

    async fn set_avatar_decoration(decoration: Option<String>) -> Result<Option<String>, ServerError> {
        Err(not_supported())
    }

    async fn set_profile_effect(effect: Option<String>) -> Result<Option<String>, ServerError> {
        Err(not_supported())
    }
}

impl NotificationsApi for IdeaChatBackend {
    // Only a per-target mute exists on this server, and it rides the live
    // `MutedChanged` event — there is no settings table to read or save.
    async fn notification_settings(server_id: Option<i64>) -> Result<Vec<NotificationSetting>, ServerError> {
        Ok(Vec.new())
    }

    async fn set_notification_setting(setting: NotificationSetting) -> Result<NotificationSetting, ServerError> {
        Err(not_supported())
    }
}

/// The display name only when it differs from the handle (RWND treats
/// "no display name" as "show the username").
pub(crate) fn shown_or_none(user: User) -> Option<String> {
    if user.display_name == user.handle {
        None
    } else {
        Some(user.display_name)
    }
}

/// An empty RWND field means "unset" for the wire's `Option<String>`.
fn optional_text(text: String) -> Option<String> {
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}
