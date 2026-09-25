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
#[path = "components/verified_badge.rs"]
mod verified_badge;
#[path = "components/jump_to_bottom_button.rs"]
mod jump_to_bottom_button;
#[path = "components/emoji_upload_slot.rs"]
mod emoji_upload_slot;
#[path = "components/rules_accept_checkbox.rs"]
mod rules_accept_checkbox;
#[path = "components/welcome_banner.rs"]
mod welcome_banner;
#[path = "components/translate_toggle.rs"]
mod translate_toggle;
#[path = "components/permission_override_row.rs"]
mod permission_override_row;
#[path = "components/server_icon_fallback.rs"]
mod server_icon_fallback;
#[path = "components/message_rating.rs"]
mod message_rating;
#[path = "components/message_action_bar.rs"]
mod message_action_bar;
#[path = "components/server_event_card.rs"]
mod server_event_card;
#[path = "components/connection_quality_indicator.rs"]
mod connection_quality_indicator;
#[path = "components/transfer_ownership_confirm.rs"]
mod transfer_ownership_confirm;
#[path = "components/activity_card.rs"]
mod activity_card;
#[path = "components/server_stats_card.rs"]
mod server_stats_card;
#[path = "components/audit_log_row.rs"]
mod audit_log_row;
#[path = "components/link_embed_preview.rs"]
mod link_embed_preview;
#[path = "components/member_role_list.rs"]
mod member_role_list;
#[path = "components/timeout_duration_picker.rs"]
mod timeout_duration_picker;
#[path = "components/notification_permission_prompt.rs"]
mod notification_permission_prompt;
#[path = "components/category_filter_chips.rs"]
mod category_filter_chips;
#[path = "components/send_failed_indicator.rs"]
mod send_failed_indicator;
#[path = "components/featured_server_banner.rs"]
mod featured_server_banner;
#[path = "components/code_block.rs"]
mod code_block;
#[path = "components/user_bio_block.rs"]
mod user_bio_block;
#[path = "components/server_tag_chips.rs"]
mod server_tag_chips;
#[path = "components/reaction_summary.rs"]
mod reaction_summary;
#[path = "components/setup_checklist_item.rs"]
mod setup_checklist_item;
#[path = "components/dm_request_card.rs"]
mod dm_request_card;
#[path = "components/icon_change_confirm.rs"]
mod icon_change_confirm;
#[path = "components/pin_count_badge.rs"]
mod pin_count_badge;
#[path = "components/ban_reason_input.rs"]
mod ban_reason_input;
#[path = "components/template_gallery_grid.rs"]
mod template_gallery_grid;
#[path = "components/invite_expiry_selector.rs"]
mod invite_expiry_selector;
#[path = "components/latency_badge.rs"]
mod latency_badge;
#[path = "components/soundboard_button.rs"]
mod soundboard_button;

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
