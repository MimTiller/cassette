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
//! the login UI ([`app/login_screen.rs`]) can work with a plain
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
        .map_err(|e| crate::errors::friendly(&e))
}

// Sign-in (`login`), sign-up, password change/reset and logout live in
// `crate::identity` — the `AuthProvider` seam calls this module's session
// storage (`save_session`/`load_session`) and writes the process-wide
// holders below when a provider hands back a signed-in identity.

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
/// session (not logged in), or a transport-level `ServerError`.
/// The message is user-facing text from `errors::friendly`.
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
        Err(error) => Err(crate::errors::friendly(&error)),
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
/// with the real name (the strip's own name).
/// Empty means "not logged in" (guest mode) — the sentinel the holder starts
/// at. Written by [`login`] (which signup also routes through), and restored
/// from the persisted session on a page refresh (see [`save_session`] /
/// [`load_session`]).
pub(crate) static CURRENT_USERNAME: std::sync::Mutex<String> =
    std::sync::Mutex::new(String::new());

/// The backend host the current session belongs to — the base URL
/// `configure_server` pointed the RPC stubs at. The persisted session is a
/// map keyed by this, and `current_token`/`current_username` (which read the
/// process-wide holders) answer for the CURRENT host's entry. Today one host
/// is in play; a future multi-host client holds one entry per host.
pub(crate) static CURRENT_HOST: std::sync::Mutex<String> =
    std::sync::Mutex::new(String::new());

/// The backend host in effect (the base URL `configure_server` installed), or
/// an empty string before startup wired it up.
pub(crate) fn current_host() -> String {
    CURRENT_HOST.lock().map(|h| h.clone()).unwrap_or_else(|p| p.into_inner().clone())
}

/// The current session token, or an empty string when not logged in.
pub(crate) fn current_token() -> String {
    CURRENT_TOKEN.lock().map(|t| t.clone()).unwrap_or_default()
}

/// The signed-in account's username, or an empty string when not logged in
/// (guest mode). The profile strip shows no name for an empty value.
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

/// Display names by username, for every user the client has heard about
/// (members, friends, DM partners, profile cards). See [`shown_name`].
pub(crate) static NAME_MAP: std::sync::Mutex<Vec<(String, String)>> =
    std::sync::Mutex::new(Vec::new());

/// Record `username`'s display name, or forget it with `None`.
pub(crate) fn remember_display_name(username: &str, display_name: Option<String>) {
    if username.is_empty() {
        return;
    }
    let mut map = NAME_MAP.lock().unwrap_or_else(|p| p.into_inner());
    map.retain(|(u, _)| *u != username);
    if let Some(name) = display_name {
        map.push((username.to_string(), name));
    }
}

/// The name to show for `username`: their display name when they have one.
pub(crate) fn shown_name(username: &str) -> String {
    let map = NAME_MAP.lock().unwrap_or_else(|p| p.into_inner());
    map.iter()
        .find(|(u, _)| *u == username)
        .map(|(_, n)| n.clone())
        .unwrap_or_else(|| username.to_string())
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

/// Point the `#[server]` RPC stubs at a base URL and attach credentials on
/// every outgoing call.
///
/// Called once at startup, before any backend call. The base URL comes from
/// `server::dev_base_url()` (the dev server on native, `None` on the web
/// target), then the page origin on web (same-origin), then a localhost
/// fallback. The credential provider reads [`current_token()`] on each call,
/// so the header appears only after a successful login; on WEB an empty token
/// slot is normal — the browser attaches the httpOnly `session` cookie
/// automatically to every same-origin request (the backend accepts either),
/// which is what keeps the token out of any client-readable storage.
pub fn configure_server() {
    let base = server::dev_base_url()
        .or_else(page_origin)
        .unwrap_or_else(|| "http://127.0.0.1:3000".into());
    // The host the session map is keyed by: the persisted session lives
    // under this base URL's entry, and `current_token`/`current_username`
    // answer for it.
    *CURRENT_HOST.lock().unwrap_or_else(|p| p.into_inner()) = base.clone();
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

/// Turn a backend media path (`/media/<sha>.<ext>`, what the DB and wire frames
/// carry) into the ABSOLUTE URL `image()` accepts: relative paths are rejected
/// on every backend, so the base URL `configure_server` installed is prefixed.
/// Anything else (`data:` previews, `http(s)://` URLs) passes through untouched.
pub fn media_url(value: String) -> String {
    match server::base_url() {
        Some(base) if value.starts_with("/media/") => format!("{}{}", base.trim_end_matches('/'), value),
        _ => value,
    }
}

// --- persisted session ------------------------------------------------------
//
// The WASM module reloads its process-wide statics from scratch on every page
// refresh. Two SDK-backed stores keep the returning visitor signed in:
//
// - the session TOKEN is a secret: on NATIVE targets it lives in the
//   platform's real secure store (Keychain / Keystore / OS vault) via the
//   `credentials` SDK. On WEB the token is never stored client-side at all —
//   the `login` server fn sets an httpOnly `session` cookie that the browser
//   replays on every request AND the WebSocket upgrade, while JS cannot read
//   it (the credentials README's BFF pattern; calling `credentials` on web
//   errors loudly, which is exactly why this half is web-gated off).
// - the non-secret identity (user id, username, avatar) rides the plaintext
//   `storage` SDK — `localStorage` namespaced `discord-clone:` on web and
//   the native key-value store otherwise, so a restored session keeps its
//   profile strip on every target.
//
// A stale cookie / invalid token does not restore a broken shell: identity
// alone cannot authenticate, so the first backend call after restore fails
// and the app shell routes back to the login screen (see `RESTORED` in
// `crate::app`). Legacy pre-migration keys (`discord_session`, which held
// the RAW TOKEN in localStorage, and `discord.last_view.<id>`) are removed
// on load — old sessions simply require one fresh login.

/// The shared storage-SDK namespace for every non-secret client key
/// (identity, last view, notification prefs). Namespacing is what lets
/// `clear()` and key reads stay collision-free across the origin/device.
pub(crate) const STORE_NS: &str = "discord-clone";

/// The credentials-SDK namespace (an OS-vault scope, not a storage key).
const CREDS_NS: &str = "discord-clone";

/// The storage-SDK key for the session token on NATIVE targets (the OS
/// secure-store slot — never a `localStorage` key: on web the httpOnly
/// `session` cookie set by the `login` server fn carries the session, and
/// the credentials SDK errors there on purpose).
const TOKEN_CRED_KEY: &str = "session_token";

/// Storage-SDK keys for the NON-SECRET identity parts (the plaintext half —
/// `storage` is explicitly not for secrets, so the token never appears here).
const ID_USER_KEY: &str = "identity.user_id";
const ID_USERNAME_KEY: &str = "identity.username";
const ID_AVATAR_KEY: &str = "identity.avatar";
// Which provider created the session ("password" for the username +
// password RPCs); an older build's session has no key and restores as the
// password provider (see `crate::identity::parse_provider_id`).
const ID_PROVIDER_KEY: &str = "identity.provider";

// --- host-keyed session map -------------------------------------------------
//
// The saved session is a MAP keyed by the backend host it was signed against
// (the base URL `configure_server` installed), so a client that later
// connects to several community hosts at once keeps one identity per host.
// Today exactly one entry exists: the current host's. The keys above are the
// LEGACY un-namespaced form (one session, no host) and migrate into the map
// on load, so an account signed in before the restructure restores signed-in
// rather than being signed out.
//
// A host-scoped key is `<field>@<host>` inside the same `discord-clone:`
// namespace, plus a `session.hosts` line listing which hosts have entries.
// Hosts contain no `@`, and the list is ':'-free, so the grammar stays a
// compact tagged line (no JSON parser dep).

/// The storage-SDK key listing the hosts that have session entries.
const HOSTS_KEY: &str = "session.hosts";

/// Split the host-list value. The separator is `|` (never `:` — a base URL
/// like `http://127.0.0.1:3000` carries colons; a URL never contains `|`).
pub(crate) fn parse_host_list(raw: &str) -> Vec<String> {
    raw.split('|').filter(|h| !h.is_empty()).map(String::from).collect()
}

/// The host-list value for a set of hosts.
pub(crate) fn format_host_list(hosts: &[String]) -> String {
    hosts.join("|")
}

/// The host-scoped key for an identity field. Hosts contain no `@` and the
/// field name has a fixed `.` prefix, so `<field>@<host>` never collides with
/// another key.
pub(crate) fn host_key(field: &str, host: &str) -> String {
    format!("{field}@{host}")
}

/// The map's `HostSession` for `host`, read straight off the storage-SDK
/// keys (its own credential slot per host, so it lives under
/// `host_key(TOKEN_CRED_KEY, host)`). `None` when the host has no saved
/// entry (no identity id) — a legacy un-namespaced session is not in the
/// map, and `load_session` migrates it in before consulting this.
pub(crate) fn host_entry(store: std::sync::Arc<dyn storage::Storage>, host: &str) -> Option<HostSession> {
    if host.is_empty() {
        return None;
    }
    let raw_id = store.get_now(&host_key(ID_USER_KEY, host)).ok().flatten().unwrap_or_default();
    let user_id = raw_id.trim().parse::<i64>().unwrap_or(0);
    if user_id == 0 {
        return None;
    }
    let username =
        store.get_now(&host_key(ID_USERNAME_KEY, host)).ok().flatten().unwrap_or_default();
    let avatar_raw =
        store.get_now(&host_key(ID_AVATAR_KEY, host)).ok().flatten().unwrap_or_default();
    let provider =
        store.get_now(&host_key(ID_PROVIDER_KEY, host)).ok().flatten().unwrap_or_default();
    // The secret half from this host's credential slot: on native the OS
    // secure store holds it; on web there is no client-side token (empty),
    // the httpOnly session cookie authenticates every replay instead.
    #[cfg(not(target_arch = "wasm32"))]
    let token =
        credentials::platform_credentials(CREDS_NS)
            .get(&host_key(TOKEN_CRED_KEY, host))
            .ok()
            .flatten()
            .unwrap_or_default();
    #[cfg(target_arch = "wasm32")]
    let token = String::new();
    let avatar_url = if avatar_raw.is_empty() { None } else { Some(avatar_raw) };
    Some(HostSession { host: host.to_string(), provider, user_id, username, token, avatar_url })
}

/// Write a `HostSession` into the map under `host` and index the host.
pub(crate) fn save_host_entry(store: std::sync::Arc<dyn storage::Storage>, entry: HostSession) {
    let host = entry.host.as_str();
    if host.is_empty() {
        return;
    }
    let uid_str = entry.user_id.to_string();
    let avatar = entry.avatar_url.unwrap_or_default();
    let name = entry.username;
    let prov = entry.provider;
    let tok = entry.token;
    let _ = store.set_now(&host_key(ID_USER_KEY, host), &uid_str);
    let _ = store.set_now(&host_key(ID_USERNAME_KEY, host), &name);
    let _ = store.set_now(&host_key(ID_AVATAR_KEY, host), &avatar);
    let _ = store.set_now(&host_key(ID_PROVIDER_KEY, host), &prov);
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = credentials::platform_credentials(CREDS_NS)
            .set(&host_key(TOKEN_CRED_KEY, host), &tok);
    }
    remember_host(host);
}

/// Add `host` to the persisted host index (no-op if already present).
pub(crate) fn remember_host(host: &str) {
    if host.is_empty() {
        return;
    }
    let store = store();
    let raw = store.get_now(HOSTS_KEY).ok().flatten().unwrap_or_default();
    let hosts = parse_host_list(&raw);
    if hosts.iter().any(|h| h == host) {
        return;
    }
    let updated = format_host_list(&hosts);
    let updated = if updated.is_empty() { host.to_string() } else { format!("{updated}|{host}") };
    let _ = store.set_now(HOSTS_KEY, &updated);
}

/// Remove `host` from the persisted host index (no-op if absent).
pub(crate) fn forget_host(host: &str) {
    if host.is_empty() {
        return;
    }
    let store = store();
    let raw = store.get_now(HOSTS_KEY).ok().flatten().unwrap_or_default();
    let hosts = parse_host_list(&raw);
    if hosts.iter().all(|h| h != host) {
        return;
    }
    let kept: Vec<String> = hosts.iter().filter(|h| *h != host)
        .map(|h| h.to_string()).collect();
    let updated = format_host_list(&kept);
    let _ = store.set_now(HOSTS_KEY, &updated);
}

/// The process-wide storage-SDK handle (`platform_storage(STORE_NS)`) —
/// `localStorage` prefixed `discord-clone:` on web, the platform's native
/// key-value store otherwise. Built on first use; backend failures surface
/// per-operation as `StorageError`, which every call site here folds into a
/// no-op (a missing store must not break the app, only persistence).
pub(crate) static STORE: std::sync::OnceLock<std::sync::Arc<dyn storage::Storage>> = std::sync::OnceLock::new();

pub(crate) fn store() -> std::sync::Arc<dyn storage::Storage> {
    STORE.get_or_init(|| storage::platform_storage(STORE_NS)).clone()
}

// The legacy pre-migration keys that used to hold the RAW session TOKEN in
// localStorage. They are removed on load; an old session simply requires one
// fresh login. (The old `discord.last_view.<id>` keys die the same way — the
// new keys live inside the `discord-clone:` storage-SDK namespace.)
#[cfg(target_arch = "wasm32")]
const LEGACY_SESSION_KEY: &str = "discord_session";

/// Persist the signed-in session through the framework SDKs:
/// - the TOKEN is a secret → the `credentials` SDK on native targets (OS
///   Keychain/Keystore/vault). On web the credentials SDK errors on purpose
///   and the token is NEVER stored client-side: the `login` server fn set an
///   httpOnly `session` cookie, which the browser replays on every request
///   and on the WebSocket upgrade, so restoring after a reload needs no
///   client-side token at all — an identity key in `storage` is the signal
///   that a session was started, and any authed backend call validates it.
/// - the non-secret identity (id, username, avatar) rides the `storage` SDK.
pub(crate) fn save_session(token: &str, user_id: i64, username: &str, avatar_url: Option<String>, provider: &str) {
    if user_id == 0 {
        return;
    }
    // The signed session is one entry in the host-keyed map: the CURRENT
    // host's slot (the base URL `configure_server` installed). Today that is
    // the only entry; a multi-host client signs into several hosts and keeps
    // one `HostSession` per host.
    let host = current_host();
    save_host_entry(store(), HostSession {
        host,
        provider: provider.to_string(),
        user_id,
        username: username.to_string(),
        token: token.to_string(),
        avatar_url,
    });
    // Sweep the legacy un-namespaced keys: a re-login replaces the single
    // pre-host session with this host's entry, so the stale copy must not
    // linger and get re-migrated on the next load.
    let _ = store().remove_now(ID_USER_KEY);
    let _ = store().remove_now(ID_USERNAME_KEY);
    let _ = store().remove_now(ID_AVATAR_KEY);
    let _ = store().remove_now(ID_PROVIDER_KEY);
    #[cfg(not(target_arch = "wasm32"))]
    {
        // The legacy secret slot (native): drop it once the fresh token is
        // written into this host's slot above.
        let _ = credentials::platform_credentials(CREDS_NS).remove(TOKEN_CRED_KEY);
    }
    #[cfg(target_arch = "wasm32")]
    {
        // Drop the pre-migration raw-token key: old builds kept the session
        // in localStorage; those accounts get one fresh login, not a
        // resurrected secret.
        if let Some(legacy) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
            let _ = legacy.remove_item(LEGACY_SESSION_KEY);
        }
    }
}

/// Rewrite the persisted identity's avatar after a set/clear that did not go
/// through login, so a page refresh restores the NEW avatar (the value written
/// at login time would be stale). No-op when not signed in.
pub(crate) fn refresh_saved_avatar(avatar_url: Option<String>) {
    let user_id = CURRENT_USER_ID.load(std::sync::atomic::Ordering::SeqCst);
    if user_id == 0 {
        return;
    }
    let host = current_host();
    if host.is_empty() {
        return;
    }
    let avatar = avatar_url.unwrap_or_default();
    let host_own = current_host();
    let _ = store().set_now(&host_key(ID_AVATAR_KEY, &host_own), &avatar);
}

/// A saved session for one backend host: the identity that host certified
/// this device with. The persisted session is a map of these keyed by `host`
/// (the base URL `configure_server` installed); today the map holds exactly
/// the current host's entry. A future multi-host client holds one per host.
pub(crate) struct HostSession {
    /// The backend host this entry belongs to (its base URL).
    pub host: String,
    /// Which provider created it ("password" today); an older build's entry
    /// has no tag and restores as the password provider.
    pub provider: String,
    /// The account's numeric id on that host.
    pub user_id: i64,
    /// The account's username.
    pub username: String,
    /// The signed session token (empty on web — the cookie authenticates).
    pub token: String,
    /// The account's stored avatar (`None` = the default initial circle).
    pub avatar_url: Option<String>,
}

/// A persisted session `(token, user_id, username, avatar_url)` for the
/// CURRENT host, or `None` when there is none to restore. The persisted
/// session is a map keyed by host (see [`HostSession`]); this reads the
/// current host's entry. A session saved before the host-keyed restructure
/// (a single un-namespaced identity) migrates into the map under the
/// current host here, so a returning account stays signed in rather than
/// being signed out. On NATIVE the token comes back from the `credentials`
/// SDK (the OS secure store) under this host's slot; on WEB the token slot
/// is always empty — the httpOnly `session` cookie the browser replays
/// authenticates every request and the WebSocket upgrade after a reload, and
/// JS can (by design) never read the cookie to put it back in the slot. The
/// identity keys decide whether a reload restores a signed-in shell. The
/// username segment is absent in sessions persisted before it was persisted
/// (they restore with an empty name), and the avatar is `None` for accounts
/// that had none when saved.
pub(crate) fn load_session() -> Option<(String, i64, String, Option<String>)> {
    let store = store();
    let host = current_host();
    // Read the current host's entry from the map.
    if let Some(entry) = host_entry(store.clone(), &host) {
        // Which provider created this session: route the restored session
        // back through the provider that created it (an older build wrote no
        // tag, which `remember_provider` maps back to the password provider).
        crate::identity::remember_provider(entry.provider);
        return Some((entry.token, entry.user_id, entry.username, entry.avatar_url));
    }
    // Migrate a legacy single saved session (pre-host-restructure) into the
    // map under the current host, so an existing account restores signed-in.
    let legacy_id = store.get_now(ID_USER_KEY).ok().flatten().unwrap_or_default();
    let legacy_uid = legacy_id.trim().parse::<i64>().unwrap_or(0);
    if legacy_uid != 0 && !host.is_empty() {
        let legacy_username =
            store.get_now(ID_USERNAME_KEY).ok().flatten().unwrap_or_default();
        let legacy_avatar_raw =
            store.get_now(ID_AVATAR_KEY).ok().flatten().unwrap_or_default();
        let legacy_provider =
            store.get_now(ID_PROVIDER_KEY).ok().flatten().unwrap_or_default();
        // The native secret half rides in the legacy credential slot; web has
        // no client-side token (the cookie authenticates).
        #[cfg(not(target_arch = "wasm32"))]
        let legacy_token =
            credentials::platform_credentials(CREDS_NS)
                .get(TOKEN_CRED_KEY)
                .ok()
                .flatten()
                .unwrap_or(String::new());
        #[cfg(target_arch = "wasm32")]
        let legacy_token = String::new();
        let legacy_avatar =
            if legacy_avatar_raw.is_empty() { None } else { Some(legacy_avatar_raw) };
        crate::identity::remember_provider(legacy_provider.clone());
        let entry = HostSession {
            host: host.to_string(),
            provider: legacy_provider,
            user_id: legacy_uid,
            username: legacy_username,
            token: legacy_token,
            avatar_url: legacy_avatar,
        };
        // Snapshot the restore tuple before the entry moves into the map.
        let restored = (entry.token.clone(), entry.user_id, entry.username.clone(), entry.avatar_url.clone());
        save_host_entry(store.clone(), entry);
        // Drop the legacy keys so the migrated entry is not re-written (and
        // can't resurrect after a logout) on the next load.
        let _ = store.remove_now(ID_USER_KEY);
        let _ = store.remove_now(ID_USERNAME_KEY);
        let _ = store.remove_now(ID_AVATAR_KEY);
        let _ = store.remove_now(ID_PROVIDER_KEY);
        #[cfg(not(target_arch = "wasm32"))]
        {
            // The legacy secret slot's token rode into the host's slot above
            // (via `save_host_entry`); just drop the old slot.
            let _ = credentials::platform_credentials(CREDS_NS).remove(TOKEN_CRED_KEY);
        }
        return Some(restored);
    }
    // Nothing to restore — and sweep the pre-migration raw-token keys so an
    // old session's secret never lingers in localStorage once a fresh login
    // rewrites through the SDK stores.
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(legacy) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
            let _ = legacy.remove_item(LEGACY_SESSION_KEY);
        }
    }
    None
}

/// Drop the persisted session (logout), so a refresh doesn't restore it. The
/// web half also relies on the `logout` server fn having cleared the httpOnly
/// cookie — both halves are needed: keys gone AND cookie gone. Only the
/// CURRENT host's entry is dropped from the map; a future multi-host client
/// logging out of one host leaves the others' sessions intact.
pub(crate) fn clear_session() {
    // Only the SIGNED-IN user's last-view key is dropped — other accounts
    // sharing this browser/origin keep their own last-view so logging A out
    // never erases B's landing spot. `CURRENT_USER_ID` still holds the id
    // being logged out at this call site (logout clears the statics after).
    let uid = CURRENT_USER_ID.load(std::sync::atomic::Ordering::SeqCst);
    if uid != 0 {
        let _ = store().remove_now(&format!("last_view.{uid}"));
    }
    // Drop the current host's map entry and de-index it.
    let host = current_host();
    if !host.is_empty() {
        let _ = store().remove_now(&host_key(ID_USER_KEY, &host));
        let _ = store().remove_now(&host_key(ID_USERNAME_KEY, &host));
        let _ = store().remove_now(&host_key(ID_AVATAR_KEY, &host));
        let _ = store().remove_now(&host_key(ID_PROVIDER_KEY, &host));
        forget_host(&host);
    }
    // Sweep any legacy un-namespaced identity left by a pre-restructure build
    // that never migrated (a logout before any reload), so it can't restore.
    let _ = store().remove_now(ID_USER_KEY);
    let _ = store().remove_now(ID_USERNAME_KEY);
    let _ = store().remove_now(ID_AVATAR_KEY);
    let _ = store().remove_now(ID_PROVIDER_KEY);
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Drop both the current host's secret slot and the legacy slot.
        let creds = credentials::platform_credentials(CREDS_NS);
        if !host.is_empty() {
            let _ = creds.remove(&host_key(TOKEN_CRED_KEY, &host));
        }
        let _ = creds.remove(TOKEN_CRED_KEY);
    }
    // Drop the pre-migration raw-token key that lived in localStorage.
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(legacy) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
            let _ = legacy.remove_item(LEGACY_SESSION_KEY);
        }
    }
}

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
mod host_session_tests {
    use crate::auth::host_key;
    use crate::auth::parse_host_list;
    use crate::auth::format_host_list;
    use crate::auth::ID_USER_KEY;
    use crate::auth::ID_USERNAME_KEY;

    #[test]
    fn host_scoped_keys_are_unique_per_host() {
        // The map's keys carry the host so two community servers keep
        // separate entries inside the same storage namespace.
        let a = host_key(ID_USER_KEY, "http://127.0.0.1:8085");
        let b = host_key(ID_USER_KEY, "http://other.host:9000");
        assert_ne!(a, b);
        assert!(a.contains("@http://127.0.0.1:8085"));
        // Same field on the same host round-trips identically.
        assert_eq!(host_key(ID_USERNAME_KEY, "http://x"), host_key(ID_USERNAME_KEY, "http://x"));
    }

    #[test]
    fn the_host_index_survives_colons_in_urls() {
        // Base URLs carry ':' (scheme and port) — the index separator must not
        // be ':' or every host would split into fragments.
        let hosts: Vec<String> = Vec::from([
            "http://127.0.0.1:8085".to_string(),
            "https://a.b:1/c".to_string(),
        ]);
        assert_eq!(parse_host_list(&format_host_list(&hosts)), hosts);
        assert!(hosts.iter().all(|h| !h.contains("|")));
    }

    #[test]
    fn an_empty_or_bogus_host_index_parses_cleanly() {
        // Hand-edited localStorage garbage must not crash the restore.
        let empty: Vec<String> = Vec::new();
        assert_eq!(parse_host_list(""), empty);
        assert_eq!(parse_host_list("||"), empty);
        assert_eq!(parse_host_list("http://a||http://b"),
            Vec::from(["http://a", "http://b"]));
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

#[cfg(target_arch = "wasm32")]
const LEGACY_LAST_VIEW_PREFIX: &str = "discord.last_view.";

/// Remember the live channel `channel_id` inside server `server_id` as this
/// user's last view. Written on every live-server enter and every live-channel
/// selection, so the persisted spot is exactly where the user last clicked —
/// a non-first server AND a non-first channel. Rides the storage SDK (every
/// target: localStorage on web, the native store elsewhere), keyed per user
/// id inside the namespace.
pub(crate) fn save_last_view_channel(server_id: i64, channel_id: i64) {
    let uid = CURRENT_USER_ID.load(std::sync::atomic::Ordering::SeqCst);
    if uid == 0 {
        return; // guest: no identity to key a per-user view on
    }
    let _ = store().set_now(&format!("last_view.{uid}"), &format_last_view_channel(server_id, channel_id));
}

/// Remember the open live DM conversation `dm_channel_id` as this user's last
/// view — it takes precedence over the channel form (a DM being the last open
/// surface is the "last thing open" the refresh must restore).
pub(crate) fn save_last_view_dm(dm_channel_id: i64) {
    let uid = CURRENT_USER_ID.load(std::sync::atomic::Ordering::SeqCst);
    if uid == 0 {
        return;
    }
    let _ = store().set_now(&format!("last_view.{uid}"), &format_last_view_dm(dm_channel_id));
}

/// The signed-in user's persisted last view as `(kind, a, b)`: kind `0` =
/// live server+channel (`a` = server id, `b` = channel id); kind `1` = live
/// DM (`a` = DM channel id). `None` when nothing is saved, the value is
/// unparseable, or the user has no key (guest / first visit / logged out
/// since).
pub(crate) fn load_last_view() -> Option<(i64, i64, i64)> {
    let uid = CURRENT_USER_ID.load(std::sync::atomic::Ordering::SeqCst);
    if uid == 0 {
        return None;
    }
    // A value from a pre-migration key ("discord.last_view.<id>" outside the
    // SDK namespace) is removed on load; those sessions fall back to the
    // default landing spot once.
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(legacy) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
            let _ = legacy.remove_item(&format!("{LEGACY_LAST_VIEW_PREFIX}{uid}"));
        }
    }
    let raw = store().get_now(&format!("last_view.{uid}")).ok().flatten().unwrap_or_default();
    if raw.is_empty() {
        return None;
    }
    // Shared, tested parser (the pure half of the grammar).
    parse_last_view(&raw)
}

/// The token from a password reset link (`/?reset=<token>`), if the page was
/// opened from one.
#[cfg(target_arch = "wasm32")]
pub fn reset_token_in_url() -> Option<String> {
    reset_token_from_query(&web_sys::window()?.location().search().ok()?)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn reset_token_in_url() -> Option<String> {
    None
}

/// Take the reset token out of the address bar, so a reload shows the login
/// form rather than a used-up link.
#[cfg(target_arch = "wasm32")]
pub fn clear_reset_token_from_url() {
    if let Some(history) = web_sys::window().and_then(|w| w.history().ok()) {
        let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some("/"));
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn clear_reset_token_from_url() {}

#[cfg(any(target_arch = "wasm32", test))]
fn reset_token_from_query(search: &str) -> Option<String> {
    search.trim_start_matches('?').split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == discord_server::password_resets::URL_PARAM && !value.is_empty())
            .then(|| value.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::reset_token_from_query;

    #[test]
    fn finds_the_reset_token_in_a_query_string() {
        assert_eq!(reset_token_from_query("?reset=abc123").as_deref(), Some("abc123"));
        assert_eq!(reset_token_from_query("?x=1&reset=abc").as_deref(), Some("abc"));
        assert_eq!(reset_token_from_query("?reset="), None);
        assert_eq!(reset_token_from_query("?resets=abc"), None);
        assert_eq!(reset_token_from_query(""), None);
    }
}
