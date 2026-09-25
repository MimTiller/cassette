//! `discord-clone` — a Discord-style layout (server rail, channel sidebar,
//! message view + composer) on the Idealyst runtime. Pure platform-agnostic
//! Rust; the entry point is `src/main.rs`, one `idealyst::entry!(discord_clone)`
//! line whose shell the target triple selects. The app is composed in
//! `src/app.rs`; the mock data lives in `src/data.rs`.

mod data;
mod style_helpers;
mod app;
mod profile_settings;

// Standalone component, not wired into any screen yet — same "declared so it
// compiles, referenced by nothing" shape as `profile_settings`. Lives under
// `components/`; the `#[path]` keeps this a one-file add (no `mod.rs`).
#[path = "components/server_folder_group.rs"]
mod server_folder_group;
#[path = "components/idle_timer_badge.rs"]
mod idle_timer_badge;
#[path = "components/edited_label.rs"]
mod edited_label;
#[path = "components/server_discovery_card.rs"]
mod server_discovery_card;
#[path = "components/mic_selector.rs"]
mod mic_selector;

pub use app::app;

// SDK-handler registration seam, invoked by the CLI-generated wrappers (web
// `start_in`/`hydrate_in`, macOS `run_with`, iOS `run_in_view`, terminal
// `run`) after `runtime_vocabulary::register_builtins`. Registry-generic over
// the scene `Host` so ONE seam serves every backend. This app renders only
// core primitives, so there is nothing extra to register.
pub fn register_scene_extensions<H: runtime_scene::Host>(
    _registry: &mut runtime_scene::Registry<H>,
) {
}

// Recorder-side registration seam for the runtime-server sidecar
// (`dev_server::sidecar::run_newcore`) — the recorder's scene-registry twin
// of `register_scene_extensions`. Gated by `sidecar` (set only by the
// generated sidecar wrapper) so device/web builds never pull `dev-server`.
#[cfg(feature = "sidecar")]
pub fn register_scene_extensions_recorder(_registry: &mut dev_server::newcore::SceneRegistry) {}

// Android entry: the generated Android wrapper's `attach` mounts
// `scene_app()` through `backend_android::newcore::start`. `app()` already
// returns the scene `Element`, so this is a plain re-export shim.
pub fn scene_app() -> runtime_core::Element {
    app()
}
