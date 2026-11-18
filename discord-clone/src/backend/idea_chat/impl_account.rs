//! Account: the auth surface rides the idea-chat RPCs (`sign_in`, `sign_out`,
//! …) and the profile fields come from the reducer state's visible `Me`/`User`.

use crate::backend::*;
use crate::backend::idea_chat::{client, live, model};
use chat_model::{User, Relation};
use serde_json::Value;

impl AccountApi for IdeaChatBackend {
    // idea-chat signs in by email. On the Cassette model signup stored
    // `email: username`, but the real server refuses a non-email, so the
    // UI's own path is `idea_chat_sign_in` below, where the login form's
    // first field is labelled Email. This impl stays for any caller that
    // only wants the signed-in-or-refused answer; an unverified account
    // answers the `Unverified` refusal through the plain error channel.
    async fn login(email: String, password: String) -> Result<LoginResult, ServerError> {
        let args = sign_in_args(email, password);
        match client::call_as::<model::Session>("sign_in", args).await {
            Err(e) => return Err(e),
            Ok(session) => Ok(login_from_session(session)),
        }
    }

    // The real server makes accounts with an email: the username is the
    // handle and display name, and a verification code rides an email.
    // `signup` on the trait stays for callers that only want create-or-refuse
    // (it still cannot reach the idea-chat server on the username alone);
    // the UI's path is `idea_chat_signup` below, which also lands the
    // email-code step.
    async fn signup(username: String, password: String) -> Result<SignupResult, ServerError> {
        let email = username;
        let args = sign_up_args(email.clone(), password, email.clone(), email.clone());
        match client::post("sign_up", args).await {
            Err(e) => Err(e),
            Ok(_) => Ok(signup_from_email(email)),
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
                    Some(member) => Some(live::role_word(space.clone(), uuid, member.clone())),
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

/// Pack a `Session` answer into the `LoginResult` the identity seam records.
fn login_from_session(session: model::Session) -> LoginResult {
    let me = session.me.user;
    // The id table is keyed by the signed-in account's name, and `apply_identity` only records that name AFTER this result is
    // built. Numbering "me" first would put the account's own id in the empty-name table, and every later lookup (which uses the
    // account's table) would number it differently, so the app no longer knew which messages were the user's own (no Edit or
    // Delete on them). Record the name before numbering anything.
    *crate::auth::CURRENT_USERNAME.lock().unwrap_or_else(|p| p.into_inner()) = me.handle.clone();
    LoginResult {
        user_id: live::see(me.id),
        // Web: `token` is null and the httpOnly `session` cookie the same
        // response set carries the session; native: the bearer.
        token: session.token.unwrap_or_default(),
        username: me.handle.clone(),
        display_name: shown_or_none(me.clone()),
        avatar_url: me.avatar.clone(),
        blocked_ids: blocked_ids_of_me(),
    }
}

/// What `login` maps onto for the caller that only wants create-or-refuse.
fn signup_from_email(email: String) -> SignupResult {
    SignupResult { user_id: 0, username: email }
}

// --- the email-code auth surface the login/sign-up screens drive ------------
//
// The idea-chat server signs people in and up with an EMAIL, and a new
// account must confirm an emailed code before it can be used. These entry
// points (not part of `AccountApi`: the Cassette backend has no email half
// of the model) sit beside the trait impl so the `crate::backend` accessors
// below can dispatch on `chat_idea_chat()`. The argument builders and the
// answer classifier are pure and unit-tested against the recorded wire
// shapes; the refusals' `message` text reaches the form through
// `crate::errors::friendly` like every other server error.

/// Which step the login form shows after a `sign_in` answer: signed in, or
/// the emailed-code step for that email.
pub enum SignInAnswer {
    /// The server answered a Session; the account is signed in.
    SignedIn(LoginResult),
    /// The account exists but its email is unverified: a code was just sent
    /// to the email — the form moves to the code step.
    VerifyEmail(String),
}

pub fn sign_in_args(email: String, password: String) -> Vec<Value> {
    Vec::from([client::arg(&email), client::arg(&password)])
}

pub fn sign_up_args(email: String, password: String, handle: String, display_name: String) -> Vec<Value> {
    Vec::from([
        client::arg(&email),
        client::arg(&password),
        client::arg(&handle),
        client::arg(&display_name),
    ])
}

pub fn verify_email_args(email: String, code: String) -> Vec<Value> {
    Vec::from([client::arg(&email), client::arg(&code)])
}

pub fn resend_code_args(email: String) -> Vec<Value> {
    Vec::from([client::arg(&email)])
}

/// Create an account with an email, password, handle and display name — in
/// the wire's declaration order — and answer the email the verification
/// code was sent to.
pub async fn signup_email(email: String, password: String, handle: String, display_name: String) -> Result<String, ServerError> {
    let args = sign_up_args(email.clone(), password, handle, display_name);
    match client::post("sign_up", args).await {
        Err(e) => Err(e),
        Ok(answer) => Ok(email_from_signup(answer, email)),
    }
}

/// Sign in with an email and password, answering which step the form shows.
pub async fn sign_in_email(email: String, password: String) -> Result<SignInAnswer, ServerError> {
    let args = sign_in_args(email.clone(), password);
    match client::post_envelope("sign_in", args).await {
        Err(e) => Err(e),
        Ok(envelope) => sign_in_step(envelope, email),
    }
}

/// Confirm the emailed code: the answer is a Session and the browser's
/// `session` cookie is set by the same response, so the caller lands signed
/// in exactly as after a normal login.
pub async fn verify_email(email: String, code: String) -> Result<LoginResult, ServerError> {
    let args = verify_email_args(email.clone(), code);
    match client::call_as::<model::Session>("verify_email", args).await {
        Err(e) => Err(e),
        Ok(session) => Ok(login_from_session(session)),
    }
}

/// Send another verification code. The server is deliberately silent for
/// unknown or already-verified addresses; success only says the call went
/// through.
pub async fn resend_code(email: String) -> Result<(), ServerError> {
    let args = resend_code_args(email);
    match client::post("resend_code", args).await {
        Err(e) => Err(e),
        Ok(_) => Ok(()),
    }
}

/// Decide which step the login form shows from a raw `sign_in` envelope: the
/// Session answer signs the account in, the `Unverified` refusal (the server
/// also re-sent a code) moves the form to the code step for the email it
/// names, and every other refusal reads as its `message`.
pub(crate) fn sign_in_step(envelope: Value, submitted: String) -> Result<SignInAnswer, ServerError> {
    if envelope.get("Err").is_some() || envelope.get("e").is_some() {
        return match unverified_email(envelope.clone(), submitted) {
            Some(email) => Ok(SignInAnswer::VerifyEmail(email)),
            None => match client::open_envelope(envelope) {
                Ok(_) => Err(ServerError::failed("The server refused without an error.".to_string())),
                Err(e) => Err(e),
            },
        };
    }
    match client::open_envelope(envelope) {
        Err(e) => Err(e),
        Ok(answer) => match serde_json::from_value::<model::Session>(answer) {
            Err(e) => Err(ServerError::Codec(e.to_string())),
            Ok(session) => Ok(SignInAnswer::SignedIn(login_from_session(session))),
        },
    }
}

/// The email an `Unverified` refusal names, in either envelope shape
/// (`{"Err":{"Failed":{"why":"unverified","email":…}}}` or `{"e":{…}}`).
pub(crate) fn unverified_email(envelope: Value, submitted: String) -> Option<String> {
    let payload: Value = match envelope.get("Err") {
        Some(err) => match err.get("Failed") {
            None => err.clone(),
            Some(failed) => failed.clone(),
        },
        None => match envelope.get("e") {
            None => envelope,
            Some(older) => older.clone(),
        },
    };
    match payload.get("why") {
        Some(Value::String(why)) if why == "unverified" =>
            Some(match payload.get("email") {
                Some(Value::String(email)) if !email.trim().is_empty() => email.clone(),
                _ => submitted.clone(),
            }),
        _ => None,
    }
}

/// The email `sign_up` echoed back (`SignedUp { email }`), or the submitted
/// one when the answer carries no usable email.
pub(crate) fn email_from_signup(answer: Value, submitted: String) -> String {
    let held = match serde_json::from_value::<model::SignedUp>(answer) {
        Err(_) => return submitted.clone(),
        Ok(signed_up) => signed_up,
    };
    match held.email.trim() {
        "" => submitted.clone(),
        email => email.to_string(),
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
