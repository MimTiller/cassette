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
//! `Result<i64, String>`.

/// Authenticate with a username + password and return the account's numeric
/// id on success, or an error message on failure.
///
/// A thin wrapper over the backend `#[server]` fn: it packs the credentials
/// into [`discord_server::auth::LoginInput`], awaits the RPC stub, and
/// flattens the result. On success the returned id is the value the rest of
/// the client sends back as the `x-user-id` header (the stand-in session
/// convention used by every other endpoint); on failure the `ServerError`
/// (which implements `Display`) is flattened into its message.
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
        Ok(result) => Ok(result.user_id),
        Err(error) => Err(error.to_string()),
    }
}

/// The signed-in account's numeric id, kept process-wide so it outlives the
/// login screen (which unmounts the moment `authed` flips). `0` means "not
/// logged in" — the sentinel the store starts at. This is the value the app
/// threads back as the `x-user-id` header on every backend call, the stand-in
/// session convention every other endpoint reads.
pub(crate) static CURRENT_USER_ID: std::sync::atomic::AtomicI64 =
    std::sync::atomic::AtomicI64::new(0);

/// Point the `#[server]` RPC stubs at a base URL and attach the signed-in
/// user id as the `x-user-id` header on every outgoing call.
///
/// Called once at startup, before any backend call. The base URL comes from
/// `server::dev_base_url()` (the dev server on native, `None` on the web
/// target, where the stubs are same-origin and need no explicit base) with a
/// localhost fallback. The credential provider reads [`CURRENT_USER_ID`] on
/// each call, so the header appears only after a successful login.
pub fn configure_server() {
    let base = server::dev_base_url().unwrap_or_else(|| "http://127.0.0.1:3000".into());
    server::configure(
        server::ClientConfig::new(base).with_credentials(server::credentials_from_fn(|| {
            let id = CURRENT_USER_ID.load(std::sync::atomic::Ordering::SeqCst);
            if id > 0 {
                vec![("x-user-id".to_string(), id.to_string())]
            } else {
                Vec::new()
            }
        })),
    );
}
