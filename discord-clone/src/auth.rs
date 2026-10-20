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

/// Set (or clear) the SIGNED-IN account's avatar on the backend. `avatar` is
/// `None` to clear, or a `data:image/{png,jpeg,webp,gif};base64,` URL (≤256 KB)
/// to set; the backend validates the prefix and size and its rejection comes
/// back as the `Err` string. Self-only: the acting user is the bearer token.
pub async fn set_avatar(avatar: Option<String>) -> Result<Option<String>, String> {
    let input = discord_server::auth::SetAvatarInput {
        avatar_url: avatar,
    };
    discord_server::auth::set_avatar(input)
        .await
        .map(|r| r.avatar_url)
        .map_err(|e| e.to_string())
}

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
            let username = result.username.clone();
            *CURRENT_USERNAME.lock().unwrap_or_else(|p| p.into_inner()) = username.clone();
            // Persist the signed session so a page refresh (which wipes these
            // statics and reloads the WASM module) can restore it without a
            // re-login — username included, so the profile strip keeps the
            // real identity after a refresh too. `signup` funnels through
            // `login`, so one call site covers both. The avatar URL rides
            // along so a refresh restores the uploaded image without a
            // second round trip.
            save_session(&result.token, result.user_id, &username, result.avatar_url.clone());
            // Register the author's avatar so every other surface (message
            // rows, member lists, DMs) resolves it by username.
            if !username.is_empty() {
                crate::auth::remember_avatar(
                    username.as_str(),
                    result.avatar_url.clone(),
                );
            }
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
/// at. Written by [`login`] (which signup also routes through), and restored
/// from the persisted session on a page refresh (see [`save_session`] /
/// [`load_session`]).
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

// --- avatar registry (client-side, keyed by username) ----------------------
//
// Every live payload the client reads (member roster, DM participants,
// message-author frames, login) carries the author's avatar URL; we stash the
// username -> avatar mapping in one process-wide map so every initial-circle
// render site (`message_row`, `member_row`, `user_panel`, the live member
// panel, the DM rail) resolves an author by the name it already draws.
// Usernames are globally unique (the `users.username` UNIQUE constraint), so
// the name is a sound key. Mock identities are never in the map, so they
// keep rendering their initial circles.

pub(crate) static AVATAR_MAP: std::sync::Mutex<Vec<(String, String)>> =
    std::sync::Mutex::new(Vec::new());

/// Record a user's avatar (or erase it with `None` — the roster is
/// authoritative, so a `None` member row wipes a stale entry).
pub(crate) fn remember_avatar(username: &str, avatar_url: Option<String>) {
    if username.is_empty() {
        return;
    }
    let mut map = AVATAR_MAP.lock().unwrap_or_else(|p| p.into_inner());
    map.retain(|(u, _)| *u != username);
    if let Some(url) = avatar_url {
        map.push((username.to_string(), url.clone()));
    }
}

/// The stored avatar for `username`, or `None` = draw the initial circle.
pub(crate) fn avatar_for(username: &str) -> Option<String> {
    if username.is_empty() {
        return None;
    }
    let map = AVATAR_MAP.lock().unwrap_or_else(|p| p.into_inner());
    map.iter()
        .find(|(u, _)| *u == username)
        .map(|(_, url)| url.clone())
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
// refresh, wiping [`CURRENT_TOKEN`]/[`CURRENT_USER_ID`]/[`CURRENT_USERNAME`].
// `window.localStorage` survives that, so a successful login writes the signed
// session (token + user id + username) there and `app()` reads it back before
// defaulting `authed` to false — a returning visitor lands signed in without
// retyping credentials, with the profile strip showing the real identity
// (persisting the username is what a restored session used to miss: the
// display name has no other post-refresh source).

#[cfg(target_arch = "wasm32")]
pub(crate) fn save_session(token: &str, user_id: i64, username: &str, avatar_url: Option<String>) {
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        // The avatar segment is empty when the account has no avatar (the
        // NULL column's sentinel). Sessions saved before the avatar existed
        // carry no 4th segment — `load_session` parses those as `None`.
        let avatar = avatar_url.unwrap_or_default();
        let _ = storage.set_item(
            "discord_session",
            &format!("{user_id}\u{0}{token}\u{0}{username}\u{0}{avatar}"),
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn save_session(
    _token: &str,
    _user_id: i64,
    _username: &str,
    _avatar_url: Option<String>,
) {}

/// Rewrite the persisted session's avatar segment after a set/clear that did
/// not go through login, so a page refresh restores the NEW avatar (the
/// segment written at login time would be stale). No-op off-web (no storage).
pub(crate) fn refresh_saved_avatar(avatar_url: Option<String>) {
    let token = current_token();
    if token.is_empty() {
        return;
    }
    let user_id = CURRENT_USER_ID.load(std::sync::atomic::Ordering::SeqCst);
    let username = current_username();
    save_session(token.as_str(), user_id, username.as_str(), avatar_url);
}

/// A persisted session `(token, user_id, username, avatar_url)`, or `None`
/// when there is none to restore. The username segment is absent in sessions
/// saved before it was persisted (they restore with an empty name), and the
/// avatar segment is absent in sessions saved before the avatar existed
/// (they restore with `None` = the initial circle).
pub(crate) fn load_session() -> Option<(String, i64, String, Option<String>)> {
    #[cfg(target_arch = "wasm32")]
    {
        // `local_storage()` and `get_item()` return `Result<_, JsValue>`; fold
        // each into an `Option` so the `?` stays inside this `Option` fn.
        let raw = web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())?
            .get_item("discord_session")
            .ok()
            .flatten()?;
        let mut parts = raw.split('\u{0}');
        let user_id: i64 = parts.next()?.parse().ok()?;
        let token = parts.next().unwrap_or_default().to_string();
        let username = parts.next().unwrap_or_default().to_string();
        // Empty avatar segment = no avatar (the NULL sentinel), so map it to
        // None and keep the map value `Option<String>`.
        let avatar = parts.next().unwrap_or_default().to_string();
        let avatar = if avatar.is_empty() { None } else { Some(avatar) };
        Some((token, user_id, username, avatar))
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
        // Only the SIGNED-IN user's last-view key is dropped — other accounts
        // sharing this browser keep their own last-view so logging A out never
        // erases B's landing spot. `CURRENT_USER_ID` still holds the id being
        // logged out at this call site (logout clears the statics afterwards).
        let uid = CURRENT_USER_ID.load(std::sync::atomic::Ordering::SeqCst);
        if uid != 0 {
            let _ = storage.remove_item(&format!("discord.last_view.{}", uid));
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn clear_session() {}

// --- persisted last view (per user) ----------------------------------------
//
// Selection lives only in memory, so a page refresh drops the user back on
// whatever the auto-enter fallback picks. Persist the last ENTERED view the
// same way the session survives a refresh — one key PER USER ID
// (`discord.last_view.<user_id>`) so two accounts sharing one browser each
// land back on THEIR OWN last server + channel (or last open DM), never the
// first server or another account's spot. Guest ids (0) never write a key.
//
// Value grammar (a compact tagged line, not JSON — no parser dep):
//   `s:<server_id>:c:<channel_id>`   a live channel inside a live server
//   `d:<dm_channel_id>`             the live DM conversation that was open
// Anything unparseable (a hand-crafted bogus value from devtools, a key
// written by an older build) parses to `None` = fall back to the default
// behaviour.

/// The pure half of the last-view grammar: parse a saved value's text into
/// `(kind, a, b)` — kind `0` = live server+channel (`a` = server id, `b` =
/// channel id); kind `1` = live DM (`a` = DM channel id). `None` when
/// unparseable. Split out from `load_last_view` so it can be unit-tested on
/// any target (the localStorage read itself only exists on wasm).
pub(crate) fn parse_last_view(raw: &str) -> Option<(i64, i64, i64)> {
    let parts: Vec<&str> = raw.split(':').collect();
    if parts.len() < 2 {
        return None;
    }
    let tag = parts[0];
    match parts.len() {
        // `s:8:c:9` → ["s","8","c","9"]: 4 tokens (tag, server id, literal 'c',
        // channel id) — matches exactly what `save_last_view_channel` writes.
        4 if tag == "s" && parts[2] == "c" =>
            Some((0, parts[1].parse().ok()?, parts[3].parse().ok()?)),
        2 if tag == "d" => Some((1, parts[1].parse().ok()?, 0)),
        _ => None,
    }
}

#[cfg(test)]
mod last_view_tests {
    use crate::auth::parse_last_view;
    use crate::auth::format_last_view_channel;
    use crate::auth::format_last_view_dm;

    #[test]
    fn saved_channel_view_round_trips_through_the_parser() {
        // Regression for the 3-vs-4 token parse bug: `save_last_view_channel`
        // writes `s:<server>:c:<channel>` (4 tokens on ':'); a parser matching
        // on 3 tokens silently turns EVERY saved spot into a fallback.
        assert_eq!(parse_last_view(&format_last_view_channel(9, 12)), Some((0, 9, 12)));
        assert_eq!(parse_last_view(&format_last_view_channel(1, 1)), Some((0, 1, 1)));
    }

    #[test]
    fn saved_dm_view_round_trips_through_the_parser() {
        assert_eq!(parse_last_view(&format_last_view_dm(7)), Some((1, 7, 0)));
    }

    #[test]
    fn bogus_or_stale_values_parse_to_none() {
        // Hand-crafted garbage from devtools, old-build keys, stale ids:
        // all must fall back (None) rather than crash or restore garbage.
        assert_eq!(parse_last_view("s:9:c:abc"), None);
        assert_eq!(parse_last_view("s:notanum:c:1"), None);
        assert_eq!(parse_last_view("garbage"), None);
        assert_eq!(parse_last_view(""), None);
        assert_eq!(parse_last_view(":"), None);
        assert_eq!(parse_last_view("x:1"), None);
        assert_eq!(parse_last_view("s:9:c:1:extra"), None);
        assert_eq!(parse_last_view("d:1:c:2"), None);
    }
}

/// The saved-value text for a live server+channel view. Split from
/// `save_last_view_channel` (wasm-only) so the grammar is testable anywhere.
pub(crate) fn format_last_view_channel(server_id: i64, channel_id: i64) -> String {
    format!("s:{server_id}:c:{channel_id}")
}

/// The saved-value text for an open live DM. Split from `save_last_view_dm`
/// (wasm-only) so the grammar is testable anywhere.
pub(crate) fn format_last_view_dm(dm_channel_id: i64) -> String {
    format!("d:{dm_channel_id}")
}

/// Remember the live channel `channel_id` inside server `server_id` as this
/// user's last view. Written on every live-server enter and every live-channel
/// selection, so the persisted spot is exactly where the user last clicked —
/// a non-first server AND a non-first channel.
#[cfg(target_arch = "wasm32")]
pub(crate) fn save_last_view_channel(server_id: i64, channel_id: i64) {
    let uid = CURRENT_USER_ID.load(std::sync::atomic::Ordering::SeqCst);
    if uid == 0 {
        return; // guest: no identity to key a per-user view on
    }
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(
            &format!("discord.last_view.{}", uid),
            &format_last_view_channel(server_id, channel_id),
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn save_last_view_channel(_server_id: i64, _channel_id: i64) {}

/// Remember the open live DM conversation `dm_channel_id` as this user's last
/// view — it takes precedence over the channel form (a DM being the last open
/// surface is the "last thing open" the refresh must restore).
#[cfg(target_arch = "wasm32")]
pub(crate) fn save_last_view_dm(dm_channel_id: i64) {
    let uid = CURRENT_USER_ID.load(std::sync::atomic::Ordering::SeqCst);
    if uid == 0 {
        return;
    }
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(
            &format!("discord.last_view.{}", uid),
            &format_last_view_dm(dm_channel_id),
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn save_last_view_dm(_dm_channel_id: i64) {}

/// The signed-in user's persisted last view as `(kind, a, b)`: kind `0` =
/// live server+channel (`a` = server id, `b` = channel id); kind `1` = live
/// DM (`a` = DM channel id). `None` when nothing is saved, the value is
/// unparseable, or the user has no key (guest / first visit / logged out
/// since).
pub(crate) fn load_last_view() -> Option<(i64, i64, i64)> {
    #[cfg(target_arch = "wasm32")]
    {
        let uid = CURRENT_USER_ID.load(std::sync::atomic::Ordering::SeqCst);
        if uid == 0 {
            return None;
        }
        let raw = web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())?
            .get_item(&format!("discord.last_view.{}", uid))
            .ok()
            .flatten()?;
        // Shared, tested parser (the pure half of the grammar) — the wasm
        // reader owns only the localStorage lookup.
        parse_last_view(&raw)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Off wasm (native/test builds) there is no persisted view; the tests
        // exercise `parse_last_view` directly.
        None
    }
}
