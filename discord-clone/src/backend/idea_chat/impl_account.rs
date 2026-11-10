//! Account: the auth surface rides the idea-chat RPCs (`sign_in`, `sign_out`,
//! …) and the profile fields come from the reducer state's visible `Me`/`User`.

use crate::backend::*;
use crate::backend::idea_chat::{client, live, model};
use chat_model::{User, Relation};

impl AccountApi for IdeaChatBackend {
    // RWND signs in by username; idea-chat by email. On this server's account
    // model signup stores `email: username`, so the username rides the email
    // slot — see `signup` below, which does the same.
    async fn login(username: String, password: String) -> Result<LoginResult, ServerError> {
        let mut args = Vec::new();
        args.push(client::arg(&username));
        args.push(client::arg(&password));
        let session = match client::call_as::<model::Session>("sign_in", args).await {
            Err(e) => return Err(e),
            Ok(session) => session,
        };
        let me = session.me.user;
        Ok(LoginResult {
            user_id: live::see(me.id),
            // Web: `token` is null and the httpOnly `session` cookie the same
            // response set carries the session; native: the bearer.
            token: session.token.unwrap_or_default(),
            username: me.handle.clone(),
            display_name: shown_or_none(me.clone()),
            avatar_url: me.avatar.clone(),
            blocked_ids: blocked_ids_of_me(),
        })
    }

    // idea-chat signs up by email + password + handle + display name and
    // leaves a verification code on its way; RWND's flow answers the claim and
    // the next `login` gets a session once the email is verified. The username
    // is the handle and doubles as the email (see `login`).
    async fn signup(username: String, password: String) -> Result<SignupResult, ServerError> {
        let mut args = Vec::new();
        args.push(client::arg(&username));
        args.push(client::arg(&password));
        args.push(client::arg(&username));
        args.push(client::arg(&username));
        match client::post("sign_up", args).await {
            Err(e) => Err(e),
            Ok(_) => Ok(SignupResult { user_id: 0, username }),
        }
    }

    async fn logout() -> Result<LogoutResult, ServerError> {
        match client::post("sign_out", Vec::new()).await {
            Err(e) => Err(e),
            Ok(_) => Ok(LogoutResult {}),
        }
    }

    async fn logout_everywhere() -> Result<LogoutResult, ServerError> {
        match client::post("sign_out_everywhere", Vec::new()).await {
            Err(e) => Err(e),
            Ok(_) => Ok(LogoutResult {}),
        }
    }

    // The wire answers a fresh `Session`; RWND's result just confirms the
    // change went through (the client re-reads its own bearer from here).
    async fn change_password(current_password: String, new_password: String) -> Result<ChangePasswordResult, ServerError> {
        let mut args = Vec::new();
        args.push(client::arg(&current_password));
        args.push(client::arg(&new_password));
        match client::post("change_password", args).await {
            Err(e) => Err(e),
            Ok(_) => Ok(ChangePasswordResult {}),
        }
    }

    // This pinned rev's `auth.rs` carries `set_handle` (the wire study,
    // written against an older checkout, listed username change as RWND-only
    // — see its §7 Q4). The answer's `User` is the renamed account.
    async fn change_username(new_username: String) -> Result<ChangeUsernameResult, ServerError> {
        let mut args = Vec::new();
        args.push(client::arg(&new_username));
        match client::call_as::<User>("set_handle", args).await {
            Err(e) => Err(e),
            Ok(user) => Ok(ChangeUsernameResult { username: user.handle }),
        }
    }

    // RWND holds one opaque token from the emailed reset link; idea-chat takes
    // (email, code). The token rides as the code against the account's email.
    async fn reset_password(token: String, new_password: String) -> Result<ResetPasswordResult, ServerError> {
        let mut args = Vec::new();
        args.push(client::arg(crate::auth::current_username()));
        args.push(client::arg(&token));
        args.push(client::arg(&new_password));
        match client::call_as::<model::Session>("reset_password", args).await {
            Err(e) => Err(e),
            Ok(session) => Ok(ResetPasswordResult { username: session.me.user.handle }),
        }
    }

    // `set_avatar(upload?)` wants an upload id from the triplet
    // (begin/PUT/finish); RWND hands inline image data, which
    // `upload_data_url` sniffs before sending.
    async fn set_avatar(avatar_url: Option<String>) -> Result<AvatarResult, ServerError> {
        let upload = match avatar_url {
            None => None,
            Some(url) => match live::upload_data_url("avatar".to_string(), url, model::Purpose::Avatar).await {
                Err(e) => return Err(e),
                Ok(id) => Some(id),
            },
        };
        let mut args = Vec::new();
        args.push(client::arg(upload));
        match client::call_as::<User>("set_avatar", args).await {
            Err(e) => Err(e),
            Ok(user) => Ok(AvatarResult {
                user_id: live::see(user.id),
                avatar_url: user.avatar,
            }),
        }
    }

    // No profile-read RPC for arbitrary users: the reducer's state holds every
    // user the viewer can see, so the profile is assembled from it.
    async fn get_user_profile(user_id: i64, server_id: Option<i64>) -> Result<UserProfile, ServerError> {
        let world = live::snapshot();
        let uuid = live::wire_of(user_id);
        let user = match world.users.get(&uuid) {
            None => return Err(ServerError::failed("that person is not on this server")),
            Some(user) => user.clone(),
        };
        let role = match server_id {
            None => None,
            Some(space_id) => match world.spaces.get(&live::wire_of(space_id)) {
                None => None,
                Some(space) => match space.members.get(&uuid) {
                    None => None,
                    Some(member) => Some(live::role_word(member.role)),
                },
            },
        };
        let self_id = match world.me {
            None => None,
            Some(me) => Some(me.user.id),
        };
        let relationship = match self_id {
            Some(me) => {
                if me == uuid {
                    "self".to_string()
                } else {
                    match world.relations.get(&uuid) {
                        None => "none".to_string(),
                        Some(other) => live::relation_word(other.clone()),
                    }
                }
            }
            None => match world.relations.get(&uuid) {
                None => "none".to_string(),
                Some(other) => live::relation_word(other.clone()),
            },
        };
        let joined = match server_id {
            None => None,
            Some(space_id) => match world.spaces.get(&live::wire_of(space_id)).and_then(move |s| s.members.get(&uuid)) {
                None => None,
                Some(member) => Some(live::millis_at(member.joined_at)),
            },
        };
        let mut mutual: Vec<(i64, String)> = Vec::new();
        for (space_id, space) in world.spaces.iter() {
            if space.members.contains_key(&uuid) {
                mutual.push((live::see(*space_id), space.name.to_string()));
            }
        }
        Ok(UserProfile {
            user_id,
            username: user.handle.clone(),
            display_name: shown_or_none(user.clone()),
            about: user.bio.clone(),
            avatar_url: user.avatar.clone(),
            avatar_decoration: None,
            profile_effect: None,
            created_at: 0,
            server_role: role,
            joined_server_at: joined,
            mutual_servers: mutual,
            relationship: relationship.clone(),
            blocked: relationship == "blocked",
        })
    }

    // `update_profile(display_name?, bio?)`: an empty RWND field means unset.
    async fn set_profile(display_name: String, about: String) -> Result<ProfileFields, ServerError> {
        let mut args = Vec::new();
        args.push(client::arg(optional_text(display_name)));
        args.push(client::arg(optional_text(about)));
        match client::call_as::<User>("update_profile", args).await {
            Err(e) => Err(e),
            Ok(user) => Ok(ProfileFields {
                display_name: shown_or_none(user.clone()),
                about: user.bio,
            }),
        }
    }

    // No decoration or profile-card effect anywhere in idea-chat's model.
    async fn set_avatar_decoration(_decoration: Option<String>) -> Result<Option<String>, ServerError> {
        Err(model::not_supported())
    }

    async fn set_profile_effect(_effect: Option<String>) -> Result<Option<String>, ServerError> {
        Err(model::not_supported())
    }
}

/// The signed-in account's blocked ids, from the reducer state (relations
/// carry `Blocked`), so `login` echoes them like the Cassette wire does.
fn blocked_ids_of_me() -> Vec<i64> {
    let world = live::snapshot();
    let mut out: Vec<i64> = Vec::new();
    for (user, relation) in world.relations.iter() {
        if *relation == Relation::Blocked {
            out.push(live::see(*user));
        }
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;
    use chat_model::Id;

    #[test]
    fn a_session_decodes_from_the_wires_answer() {
        // The recorded answer for `sign_in` from `docs/idea-chat-wire.md`'s
        // worked example, in chat-model's serde shape (web: token null).
        let held = r#"{"token":null,"me":{"user":{"id":"00000000-0000-0000-0000-000000000009","handle":"alice","display_name":"Alice","avatar":null,"bio":""},"email":"alice","status":{"presence":"online","text":null}}}"#;
        let session = serde_json::from_str::<model::Session>(held);
        match session {
            Err(e) => panic!("the wire shape moved: {e:?}"),
            Ok(session) => {
                assert_eq!(session.me.user.handle, "alice".to_string());
                assert_eq!(session.token, None);
            }
        }
    }

    #[test]
    fn a_native_session_answers_with_the_bearer() {
        // Same wire shape, the native arm: `token` carries the bearer the
        // Authorization header will ride.
        let held = r#"{"token":"t1.abc","me":{"user":{"id":"00000000-0000-0000-0000-000000000009","handle":"alice","display_name":"alice","avatar":null,"bio":""},"email":"alice","status":{"presence":"online","text":null}}}"#;
        let session = serde_json::from_str::<model::Session>(held);
        match session {
            Err(e) => panic!("the wire shape moved: {e:?}"),
            Ok(session) => {
                assert_eq!(session.token, Some("t1.abc".to_string()));
                assert_eq!(shown_or_none(session.me.user.clone()), None);
            }
        }
    }

    #[test]
    fn display_name_falls_back_to_the_handle() {
        assert_eq!(shown_or_none(User {
            id: Id::from_u128(1),
            handle: "nick".to_string(),
            display_name: "nick".to_string(),
            avatar: None,
            bio: "".to_string(),
        }), None);
        assert_eq!(shown_or_none(User {
            id: Id::from_u128(1),
            handle: "nick".to_string(),
            display_name: "Nick Jr".to_string(),
            avatar: None,
            bio: "".to_string(),
        }), Some("Nick Jr".to_string()));
    }

    #[test]
    fn an_empty_field_is_unset_on_the_wire() {
        assert_eq!(optional_text("".to_string()), None);
        assert_eq!(optional_text("hi".to_string()), Some("hi".to_string()));
    }

    #[test]
    fn refusals_read_as_plain_sentences() {
        let body = serde_json::from_str::<serde_json::Value>(r#"{"e":{"failed":{"message":"Those aren't our records. Try signing up?"}}}"#);
        match body {
            Err(e) => panic!("bad fixture: {e:?}"),
            Ok(body) => assert_eq!(crate::errors::friendly(&model::decode_error(body.clone())), "Those aren't our records. Try signing up?"),
        }
        assert_eq!(crate::errors::friendly(&model::not_supported()), "not supported on this server");
    }
}
