//! The pluggable identity seam: who can sign in, and how.
//!
//! A front-end for an [`AuthProvider`] implementation. Today the only
//! provider is [`PasswordAuth`], which performs the backend's username +
//! password RPCs ([`discord_server::auth::login`], [`discord_server::auth::signup`]
//! etc); a future service that signs users in through a central identity
//! authority (the authority certifies each user's key and community servers
//! trust it instead of storing passwords) adds another impl of the same
//! trait and a dispatch arm — the screens keep calling this module's entry
//! points.
//!
//! Every successful sign-in hands back an [`Identity`] — the signed-in
//! account's user id, username and session token (plus the display name and
//! avatar the backend echoes) — and the entry points record it in
//! the process-wide statics in `crate::auth` and persist the session via
//! `save_session`, tagging which provider created it so a saved session
//! restores against the same provider.

/// The signed-in account, as a provider sees it: enough to seed every
/// process-wide identity holder in `crate::auth` and to persist the session.
pub struct Identity {
    /// The account's numeric id on the backend.
    pub user_id: i64,
    /// The account's username (globally unique).
    pub username: String,
    /// The signed, expiring session token to authenticate later calls with.
    pub token: String,
    /// The display name shown in place of the username, when set.
    pub display_name: Option<String>,
    /// The account's stored avatar (a `/media/…` path), or `None` for the
    /// default initial circle.
    pub avatar_url: Option<String>,
}

/// Which identity service a session came from. New providers add a variant
/// here and a `provider_id()` answer; a saved session's recorded id maps
/// back to the provider that created it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderKind {
    Password,
}

/// The id a [`ProviderKind`] records in (and is matched against) the
/// persisted session's provider key. An id an older build never wrote maps
/// back to `Password`, so pre-seam sessions still load.
pub(crate) fn provider_id(kind: ProviderKind) -> String {
    match kind {
        ProviderKind::Password => "password".to_string(),
    }
}

/// Resolve a persisted provider id back to a [`ProviderKind`]. Unknown ids
/// (and the empty value an older build left behind) fall back to
/// `Password`: the only provider that exists today, and the one every
/// legacy session was created by.
pub(crate) fn parse_provider_id(raw: &str) -> ProviderKind {
    match raw.trim() {
        "password" => ProviderKind::Password,
        _ => ProviderKind::Password,
    }
}

#[cfg(test)]
mod seam_tests {
    use crate::identity::parse_provider_id;
    use crate::identity::provider_id;
    use crate::identity::ProviderKind;
    use crate::identity::identity_from_login;
    use discord_server::auth::LoginResult;

    #[test]
    fn the_password_provider_round_trips_its_id() {
        assert_eq!(
            parse_provider_id(&provider_id(ProviderKind::Password)),
            ProviderKind::Password,
        );
    }

    #[test]
    fn unknown_or_missing_provider_ids_default_to_password() {
        // A session persisted before the seam recorded a provider id must
        // still restore, as must a hand-edited bogus value.
        assert_eq!(parse_provider_id(""), ProviderKind::Password);
        assert_eq!(parse_provider_id("   "), ProviderKind::Password);
        assert_eq!(parse_provider_id("not-a-provider"), ProviderKind::Password);
    }

    #[test]
    fn a_login_result_maps_onto_the_full_identity() {
        let result = LoginResult {
            user_id: 9,
            token: "signed-token".to_string(),
            username: "ada".to_string(),
            display_name: Some("Ada L.".to_string()),
            avatar_url: Some("/media/ab12.png".to_string()),
            blocked_ids: Vec::from([3, 4]),
        };
        let id = identity_from_login(result);
        assert_eq!(id.user_id, 9);
        assert_eq!(id.token, "signed-token");
        assert_eq!(id.username, "ada");
        assert_eq!(id.display_name.as_deref(), Some("Ada L."));
        assert_eq!(id.avatar_url.as_deref(), Some("/media/ab12.png"));
    }

    #[test]
    fn an_identity_without_optional_fields_maps_cleanly() {
        let result = LoginResult {
            user_id: 1,
            token: "t".to_string(),
            username: "u".to_string(),
            display_name: None,
            avatar_url: None,
            blocked_ids: Vec::new(),
        };
        let id = identity_from_login(result);
        assert_eq!(id.display_name, None);
        assert_eq!(id.avatar_url, None);
    }
}

/// Map a backend [`discord_server::auth::LoginResult`] onto the provider's
/// [`Identity`] — the seam's own view of a signed-in account.
pub(crate) fn identity_from_login(result: discord_server::auth::LoginResult) -> Identity {
    Identity {
        user_id: result.user_id,
        username: result.username,
        token: result.token,
        display_name: result.display_name,
        avatar_url: result.avatar_url,
    }
}

/// The shape every identity service implements. Today one provider exists,
/// so dispatch is a direct call per kind; a future provider (the central
/// identity authority) adds an impl below and an arm in the entry points —
/// the screens never learn the difference. Failure is always user-facing
/// text (`Err(message)` already flattened through `crate::errors::friendly`).
pub trait AuthProvider {
    /// Authenticate an existing account with its credentials.
    async fn sign_in(username: &str, password: &str) -> Result<Identity, String>;
    /// Create a new account and hand back the signed-in identity for it.
    async fn sign_up(username: &str, password: &str) -> Result<Identity, String>;
    /// Change the signed-in account's password, verifying the current one.
    async fn change_password(current: &str, next: &str) -> Result<(), String>;
    /// Finish a password-reset link: set the new password with the link's
    /// token and hand back the account's username so the login form can
    /// fill itself in.
    async fn reset_password(token: &str, new_password: &str) -> Result<String, String>;
    /// Log out this device: clear its session.
    fn sign_out_here();
    /// Log out everywhere: revoke every session this account has.
    fn sign_out_everywhere();
}

/// The username + password provider: the backend's own auth RPCs.
pub struct PasswordAuth {}

impl AuthProvider for PasswordAuth {
    // Sign in through the backend's `login` RPC: pack the credentials, await
    // the stub, flatten the `ServerError` into user-facing text, and map the
    // `LoginResult` onto the seam's [`Identity`].
    async fn sign_in(username: &str, password: &str) -> Result<Identity, String> {
        let creds = discord_server::auth::LoginInput {
            username: username.to_string(),
            password: password.to_string(),
        };
        match discord_server::auth::login(creds).await {
            Ok(result) => Ok(identity_from_login(result)),
            Err(error) => Err(crate::errors::friendly(&error)),
        }
    }

    // Create the account, then sign it in with the same credentials so the
    // caller lands on the app shell without retyping — the recorded identity
    // (and persisted session) come from the sign-in, exactly as before.
    async fn sign_up(username: &str, password: &str) -> Result<Identity, String> {
        let input = discord_server::auth::SignupInput {
            username: username.to_string(),
            password: password.to_string(),
        };
        match discord_server::auth::signup(input).await {
            Ok(_result) => PasswordAuth::sign_in(username, password).await,
            Err(error) => Err(crate::errors::friendly(&error)),
        }
    }

    // The session token is a signature over the user id, not the password,
    // so the caller stays signed in across the change.
    async fn change_password(current: &str, next: &str) -> Result<(), String> {
        let input = discord_server::auth::ChangePasswordInput {
            current_password: current.to_string(),
            new_password: next.to_string(),
        };
        match discord_server::auth::change_password(input).await {
            Ok(_) => Ok(()),
            Err(error) => Err(crate::errors::friendly(&error)),
        }
    }

    async fn reset_password(token: &str, new_password: &str) -> Result<String, String> {
        let input = discord_server::password_resets::ResetPasswordInput {
            token: token.to_string(),
            new_password: new_password.to_string(),
        };
        match discord_server::password_resets::reset_password(input).await {
            Ok(result) => Ok(result.username),
            Err(error) => Err(crate::errors::friendly(&error)),
        }
    }

    // Fire-and-forget logout: the client-side `clear_session()` already
    // dropped the identity keys, so a failed RPC cannot resurrect the shell.
    fn sign_out_here() {
        runtime_core::driver::spawn_detached(async move {
            let _ = discord_server::auth::logout().await;
        });
    }

    fn sign_out_everywhere() {
        runtime_core::driver::spawn_detached(async move {
            if discord_server::auth::logout_everywhere().await.is_ok() {
                crate::auth::clear_session();
                #[cfg(target_arch = "wasm32")]
                if let Some(w) = web_sys::window() {
                    let _ = w.location().reload();
                }
            }
        });
    }
}

// The provider in effect: `Password` unless a persisted session named a
// different one (restored through `remember_provider` before the first call).
pub(crate) static ACTIVE_PROVIDER: std::sync::Mutex<ProviderKind> =
    std::sync::Mutex::new(ProviderKind::Password);

/// The [`ProviderKind`] in effect right now.
pub(crate) fn current_provider() -> ProviderKind {
    *ACTIVE_PROVIDER.lock().unwrap_or_else(|p| p.into_inner())
}

/// Record which provider created a restored session, so sign-outs and
/// later calls route through the same impl. Called by
/// `crate::auth::load_session` while it reads the persisted identity.
pub fn remember_provider(raw_id: String) {
    *ACTIVE_PROVIDER.lock().unwrap_or_else(|p| p.into_inner()) =
        parse_provider_id(&raw_id);
}

// --- seam entry points -----------------------------------------------------
//
// The login, sign-up, reset and account-settings screens call these instead
// of the backend RPCs. Each picks the active provider and records a
// successful sign-in's identity in `crate::auth`'s process-wide holders
// (and the persisted session) exactly as before.

/// Authenticate with username + password; returns the account's numeric id
/// on success, or a user-facing message on failure.
pub async fn login(username: &str, password: &str) -> Result<i64, String> {
    let kind = current_provider();
    match kind {
        ProviderKind::Password =>
            apply_identity(PasswordAuth::sign_in(username, password).await, kind),
    }
}

/// Create a new account and sign it in; returns the account's numeric id.
pub async fn signup(username: &str, password: &str) -> Result<i64, String> {
    let kind = current_provider();
    match kind {
        ProviderKind::Password =>
            apply_identity(PasswordAuth::sign_up(username, password).await, kind),
    }
}

/// Change the signed-in account's password.
pub async fn change_password(current: &str, next: &str) -> Result<(), String> {
    match current_provider() {
        ProviderKind::Password => PasswordAuth::change_password(current, next).await,
    }
}

/// Set a new password with the token from a reset link; returns the
/// account's username so the login form can be filled in.
pub async fn reset_password(token: &str, new_password: &str) -> Result<String, String> {
    match current_provider() {
        ProviderKind::Password => PasswordAuth::reset_password(token, new_password).await,
    }
}

/// Ask the provider to log this device out (fire-and-forget).
pub fn request_logout() {
    match current_provider() {
        ProviderKind::Password => PasswordAuth::sign_out_here(),
    }
}

/// Ask the provider to log this account out on every device.
pub fn request_logout_everywhere() {
    match current_provider() {
        ProviderKind::Password => PasswordAuth::sign_out_everywhere(),
    }
}

/// Fold a provider's successful sign-in into the process-wide holders: the
/// id, token and username the rest of the app reads, the persisted session
/// (tagged with the provider), and the avatar/display-name registries.
pub(crate) fn apply_identity(signed_in: Result<Identity, String>, kind: ProviderKind) -> Result<i64, String> {
    match signed_in {
        Ok(identity) => {
            crate::auth::CURRENT_USER_ID
                .store(identity.user_id, std::sync::atomic::Ordering::SeqCst);
            *crate::auth::CURRENT_TOKEN.lock().unwrap_or_else(|p| p.into_inner()) =
                identity.token.clone();
            *crate::auth::CURRENT_USERNAME.lock().unwrap_or_else(|p| p.into_inner()) =
                identity.username.clone();
            // Persist the signed session so a page refresh can restore it
            // without a re-login — provider included, so the restore routes
            // back through the same impl.
            crate::auth::save_session(
                identity.token.as_str(),
                identity.user_id,
                identity.username.as_str(),
                identity.avatar_url.clone(),
                provider_id(kind).as_str()
            );
            crate::auth::remember_display_name(
                identity.username.as_str(),
                identity.display_name.clone(),
            );
            // Register the author's avatar so every other surface (message
            // rows, member lists, DMs) resolves it by username.
            if !identity.username.is_empty() {
                crate::auth::remember_avatar(
                    identity.username.as_str(),
                    identity.avatar_url.clone(),
                );
            }
            Ok(identity.user_id)
        }
        Err(message) => Err(message),
    }
}
