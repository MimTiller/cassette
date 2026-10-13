//! Client-side auth wrapper.
//!
//! Thin front-end for the backend's `#[server] fn login`
//! ([`discord_server::auth::login`]). This crate is the client half of the
//! full-stack pair: it depends on `discord-server` with default features, so
//! `login` here is the codegen'd RPC *stub* — calling it `.await`s the same
//! typed transport every other `#[server]` fn in this codebase uses. There is
//! no manual HTTP in this crate; the stub + `server` SDK's transport do the
//! work.
//!
//! The wrapper flattens the backend's `Result<LoginResult, ServerError>` into
//! the numeric user id on success or a human-readable message on failure, so
//! the login UI (a separate follow-up) can work with a plain
//! `Result<i64, String>`. On success it also records the signed session token
//! process-wide ([`CURRENT_TOKEN`]) so every subsequent call authenticates as
//! `Authorization: Bearer <token>`.

/// Authenticate with a username + password and return the account's numeric
/// id on success, or an error message on failure.
///
/// A thin wrapper over the backend `#[server]` fn: it packs the credentials
/// into [`discord_server::auth::LoginInput`], awaits the RPC stub, and
/// flattens the result. On success the returned id is recorded in
/// [`CURRENT_USER_ID`] and the signed session token is stored in
/// [`CURRENT_TOKEN`]; the client threads the token back as
/// `Authorization: Bearer <token>` on every backend call. On failure the
/// `ServerError` (which implements `Display`) is flattened into its message.
///
/// # Errors
/// Returns `Err(message)` for any failure — bad credentials, account
/// lockout, or a transport/protocol-level `ServerError` (network, codec,
/// version). The message is the `ServerError`'s `Display` form.
#[allow(dead_code)] // consumed by the login UI in a follow-up task.
pub async fn login(username: &str, password: &str) -> Result<i64, String> {
    let creds = discord_server::auth::LoginInput {
        username: username.to_string(),
        password: password.to_string(),
    };

    match discord_server::auth::login(creds).await {
        Ok(result) => {
            CURRENT_USER_ID
                .store(result.user_id, std::sync::atomic::Ordering::SeqCst);
            *CURRENT_TOKEN.lock().unwrap_or_else(|p| p.into_inner()) = result.token.clone();
            *CURRENT_USERNAME.lock().unwrap_or_else(|p| p.into_inner()) = result.username;
            // Persist the signed session so a page refresh (which wipes these
            // statics and reloads the WASM module) can restore it without a
            // re-login. `signup` funnels through `login`, so one call site
            // covers both.
            save_session(&result.token, result.user_id);
            Ok(result.user_id)
        }
        Err(error) => Err(error.to_string()),
    }
}

/// Create a new account with a username + password and return the account's
/// numeric id on success, or an error message on failure.
///
/// A thin wrapper over the backend `#[server]` fn
/// [`discord_server::auth::signup`]: it packs the credentials into
/// [`discord_server::auth::SignupInput`], awaits the RPC stub, and flattens
/// the result. On success it immediately logs the new account in via
/// [`login`] with the same credentials, so the user lands signed in without
/// retyping their password (the numeric id is recorded in [`CURRENT_USER_ID`]
/// and the signed session token in [`CURRENT_TOKEN`]); the id returned by the
/// login is the value returned here. On a signup failure (e.g. "username
/// already taken") or a transport `ServerError`, the message is flattened
/// into `Err`.
///
/// # Errors
/// Returns `Err(message)` for any failure — a duplicate username, or a
/// transport/protocol-level `ServerError` (network, codec, version). The
/// message is the `ServerError`'s `Display` form.
#[allow(dead_code)] // consumed by the login UI's sign-up mode.
pub async fn signup(username: &str, password: &str) -> Result<i64, String> {
    let input = discord_server::auth::SignupInput {
        username: username.to_string(),
        password: password.to_string(),
    };

    match discord_server::auth::signup(input).await {
        Ok(_result) => {
            // Account created. Log the new account straight in with the same
            // credentials so the caller lands signed in — this also records
            // the signed session token in [`CURRENT_TOKEN`].
            login(username, password).await
        }
        Err(error) => Err(error.to_string()),
    }
}

/// Rename the signed-in account's handle on the backend, then refresh the
/// process-wide [`CURRENT_USERNAME`] so the profile strip reflects the new
/// name without a re-login.
///
/// Thin wrapper over the backend `#[server]` fn
/// [`discord_server::auth::change_username`]: it packs the new handle into
/// [`discord_server::auth::ChangeUsernameInput`], awaits the RPC stub (which
/// authenticates as the bearer token from [`CURRENT_TOKEN`]), and flattens the
/// result. On success the fresh name is recorded in [`CURRENT_USERNAME`]; on
/// failure (e.g. a duplicate handle, or being unauthenticated) the
/// `ServerError`'s message is returned as `Err`.
///
/// # Errors
/// Returns `Err(message)` for any failure — a duplicate handle, a missing
/// session (not logged in), or a transport-level `ServerError`. The message is
/// the `ServerError`'s `Display` form.
pub async fn change_username(new_username: &str) -> Result<String, String> {
    let input = discord_server::auth::ChangeUsernameInput {
        new_username: new_username.to_string(),
    };

    match discord_server::auth::change_username(input).await {
        Ok(result) => {
            *CURRENT_USERNAME.lock().unwrap_or_else(|p| p.into_inner()) =
                result.username.clone();
            Ok(result.username)
        }
        Err(error) => Err(error.to_string()),
    }
}

/// Change the signed-in account's password, after the backend verifies the
/// current one. The current password is never sent anywhere but this call and
/// is never stored client-side.
///
/// Thin wrapper over the backend `#[server]` fn
/// [`discord_server::auth::change_password`]: it packs the old + new passwords
/// into [`discord_server::auth::ChangePasswordInput`], awaits the RPC stub,
/// and flattens the result. The session token is unchanged by a password
/// change (the token is a signature over the user id, not the password), so
/// the caller stays signed in.
///
/// # Errors
/// Returns `Err(message)` for any failure — a wrong current password, a blank
/// new password, being unauthenticated, or a transport-level `ServerError`.
/// The message is the `ServerError`'s `Display` form.
pub async fn change_password(current_password: &str, new_password: &str) -> Result<(), String> {
    let input = discord_server::auth::ChangePasswordInput {
        current_password: current_password.to_string(),
        new_password: new_password.to_string(),
    };

    match discord_server::auth::change_password(input).await {
        Ok(_) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

/// The signed-in account's numeric id, kept process-wide so it outlives the
/// login screen (which unmounts the moment `authed` flips). `0` means "not
/// logged in" — the sentinel the store starts at. It still rides the realtime
/// gateway's connect URL as a query arg, while the actual auth credential
/// threaded on backend calls is the signed session token in [`CURRENT_TOKEN`].
pub(crate) static CURRENT_USER_ID: std::sync::atomic::AtomicI64 =
    std::sync::atomic::AtomicI64::new(0);

/// The signed, expiring session token issued by [`login`], kept process-wide
/// so it outlives the login screen. The credential provider reads it on every
/// outgoing call and sends it as `Authorization: Bearer <token>`. Empty means
/// "not logged in" — the sentinel the holder starts at.
pub(crate) static CURRENT_TOKEN: std::sync::Mutex<String> =
    std::sync::Mutex::new(String::new());

/// The signed-in account's username, kept process-wide so it outlives the
/// login screen. The profile strip reads it to label the signed-in identity
/// with the real name (instead of the mock `USER.name` "you" placeholder).
/// Empty means "not logged in" (guest mode) — the sentinel the holder starts
/// at. Written by [`login`] (which signup also routes through), so one fix
/// covers both sign-in and sign-up.
pub(crate) static CURRENT_USERNAME: std::sync::Mutex<String> =
    std::sync::Mutex::new(String::new());

/// The current session token, or an empty string when not logged in.
pub(crate) fn current_token() -> String {
    CURRENT_TOKEN.lock().map(|t| t.clone()).unwrap_or_default()
}

/// The signed-in account's username, or an empty string when not logged in
/// (guest mode). The profile strip falls back to the mock `USER` identity for
/// an empty value.
pub(crate) fn current_username() -> String {
    CURRENT_USERNAME.lock().map(|u| u.clone()).unwrap_or_default()
}

/// The page's own origin on the web target (the bundle is served same-origin
/// with the API); `None` off-web.
#[cfg(target_arch = "wasm32")]
fn page_origin() -> Option<String> {
    web_sys::window()?.location().origin().ok()
}

#[cfg(not(target_arch = "wasm32"))]
fn page_origin() -> Option<String> {
    None
}

/// Point the `#[server]` RPC stubs at a base URL and attach the signed-in
/// user's session token as `Authorization: Bearer <token>` on every outgoing
/// call.
///
/// Called once at startup, before any backend call. The base URL comes from
/// `server::dev_base_url()` (the dev server on native, `None` on the web
/// target), then the page origin on web (same-origin), then a localhost
/// fallback. The credential provider reads [`current_token()`] on each call,
/// so the header appears only after a successful login.
pub fn configure_server() {
    let base = server::dev_base_url()
        .or_else(page_origin)
        .unwrap_or_else(|| "http://127.0.0.1:3000".into());
    server::configure(
        server::ClientConfig::new(base).with_credentials(server::credentials_from_fn(|| {
            let token = current_token();
            if !token.is_empty() {
                vec![("authorization".to_string(), format!("Bearer {token}"))]
            } else {
                Vec::new()
            }
        })),
    );
}

// --- persisted session ------------------------------------------------------
//
// The WASM module reloads its process-wide statics from scratch on every page
// refresh, wiping [`CURRENT_TOKEN`]/[`CURRENT_USER_ID`]. `window.localStorage`
// survives that, so a successful login writes the signed session there and
// `app()` reads it back before defaulting `authed` to false — a returning
// visitor lands signed in without retyping credentials. The display name
// needs no persistence: it is derived from the live `list_members` fetch the
// app shell runs on mount.

#[cfg(target_arch = "wasm32")]
fn save_session(token: &str, user_id: i64) {
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage
            .set_item("discord_session", &format!("{user_id}\u{0}{token}"));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn save_session(_token: &str, _user_id: i64) {}

/// A persisted session, or `None` when there is none to restore.
pub(crate) fn load_session() -> Option<(String, i64)> {
    #[cfg(target_arch = "wasm32")]
    {
        // `local_storage()` and `get_item()` return `Result<_, JsValue>`; fold
        // each into an `Option` so the `?` stays inside this `Option` fn.
        let raw = web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())?
            .get_item("discord_session")
            .ok()
            .flatten()?;
        match raw.find('\u{0}') {
            Some(i) => Some((raw[i + 1..].to_string(), raw[..i].parse().ok()?)),
            None => None,
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        None
    }
}

/// Drop the persisted session (logout), so a refresh doesn't restore it.
#[cfg(target_arch = "wasm32")]
pub(crate) fn clear_session() {
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.remove_item("discord_session");
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn clear_session() {}
