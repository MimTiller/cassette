//! `discord-server` — the backend half of the Discord clone.
//!
//! One crate, two halves:
//!
//! - **Client surface (always compiled):** the `#[server]` fns of
//!   `auth` (signup / login / password + username / avatar changes),
//!   `servers` (create / join / channel listings), `dms`, `friends`,
//!   `blocks`, `invites`, `roles`, and `server_settings` (channel +
//!   member management, pins, reactions, unread counts, the realtime
//!   message-gateway types). With default features these compile to
//!   typed wire structs + client RPC stubs the `discord-clone` UI crate
//!   calls over the wire.
//! - **Server surface (compiled with the `server` cargo feature):** the
//!   SQLite data layer (`db`, migrations under `migrations/`), the
//!   realtime WebSocket gateway (`realtime`), rate limiting (`rate_limit`),
//!   session identity (`session`), and the KDF-backed service bodies the
//!   stubs above dispatch to, hosted by the `--bin server` full-stack
//!   binary (API at `/_srv/*` + static wasm bundle at `/`, one process).
//!
//! Standalone scaffold (NOT a member of the parent workspace): the empty
//! `[workspace]` table below detaches it so cargo doesn't pull it in and it
//! keeps its own warm `target/`. Framework crates are path deps to the local
//! checkout (the idealyst registry is flaky in this environment) — same
//! convention as the sibling `discord-clone` UI project.

#[macro_use]
pub mod components;
pub mod roles;
pub mod server_roles;
pub mod server_settings;
// `invites` compiles in BOTH halves, like `auth` / `server_settings`: the
// `#[server]` fns emit client RPC stubs by default (the wire types
// `CreateInviteInput` / `Invite` + the stubs are the client surface
// `discord-clone` calls), while the DB-backed `InviteService` + `db_from_state`
// live behind `#[cfg(feature = "server")]` inside the module. Gating the whole
// module here (as `db` / `session` are) would hide the stubs from the
// default-features client build.
pub mod invites;
// `auth` is compiled in BOTH halves: the `#[server]` fns emit client RPC stubs
// by default (the wire types `LoginInput` / `SignupInput` + the stubs are the
// client surface `discord-clone` calls), while the DB-backed service (`Db`,
// `rusqlite`, KDF) lives inside the module behind `#[cfg(feature = "server")]`.
// Gating the WHOLE module here would hide the stubs from the default-features
// wasm client build, so the module itself is always compiled and only the
// server-only internals are gated (see `src/auth.rs`).
pub mod auth;
pub mod servers;
// Not feature-gated (like `servers` / `auth` above): the `#[server]` fns
// compile to client RPC stubs by default; only the DB-touching internals are
// gated `#[cfg(feature = "server")]` inside the module.
pub mod dms;
// Not feature-gated (like `servers` / `auth` / `dms` / `friends` above): the
// `#[server]` fns compile to client RPC stubs by default; only the DB-touching
// internals are gated `#[cfg(feature = "server")]` inside the module.
pub mod friends;
// Not feature-gated (like `friends` above): the `#[server]` fns
// (`block_user` / `unblock_user` / `list_blocked`) compile to client RPC stubs
// by default; only the DB-backed `BlockService` is gated
// `#[cfg(feature = "server")]` inside the module.
pub mod blocks;
// Not feature-gated (like `friends` above): `get_user_profile` compiles to a
// client RPC stub; the DB-backed `ProfileService` is server-only.
pub mod profiles;

// GIF search for the composer's GIF picker (Giphy, key kept server-side).
pub mod gifs;

#[cfg(feature = "server")]
pub mod db;
#[cfg(feature = "server")]
pub mod media;
#[cfg(feature = "server")]
pub mod realtime;
#[cfg(feature = "server")]
pub mod rate_limit;
#[cfg(feature = "server")]
pub mod session;
// Style substrate shared by the remaining component mocks.
pub mod style_helpers;

/// Shared e2e serialization guard. Every HTTP/WebSocket e2e test installs its
/// `Db` into the same process-wide `server::install_state` slot (the macro
/// reads it back via `use_state`), so all such tests MUST run one at a time
/// or they clobber each other's state mid-test. Defined here (rather than in
/// `server_settings::tests`) so every e2e module shares the ONE lock.
#[cfg(test)]
pub(crate) mod e2e_lock {
    static E2E_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Acquire the e2e serialization guard; hold it for the whole test body.
    /// (Poisoned if a prior holder panicked — recover rather than re-panic.)
    pub(crate) fn guard() -> std::sync::MutexGuard<'static, ()> {
        E2E_LOCK.lock().unwrap_or_else(|p| p.into_inner())
    }
}

// Recorder-side registration seam for the runtime-server sidecar
// (`dev_server::sidecar::run_newcore`) — the recorder's scene-registry
// twin of `register_scene_extensions`. Gated by `sidecar` (set only by
// the generated sidecar wrapper) so device/web builds never pull
// `dev-server`. This app registers no third-party scene handlers.
#[cfg(feature = "sidecar")]
pub fn register_scene_extensions_recorder(_registry: &mut dev_server::newcore::SceneRegistry) {}

// SDK-handler registration seam, invoked by the CLI-generated wrappers
// (web `start_in`/`hydrate_in`, macOS `run_with`, iOS `run_in_view`,
// terminal `run`) after `runtime_vocabulary::register_builtins`.
// Registry-generic over the scene `Host` so ONE seam serves every
// backend (each wrapper's call site pins `H` to its concrete backend);
// a project that adds an SDK with a caps-generic handler (codeblock,
// table, markdown, …) calls its `register` here, and one with a
// backend-CONCRETE handler specializes `H` to that backend instead.
//
// Registration is mandatory for anything the tree renders: an
// unregistered payload panics at realize.
pub fn register_scene_extensions<H: runtime_scene::Host>(
    _registry: &mut runtime_scene::Registry<H>,
) {
}
