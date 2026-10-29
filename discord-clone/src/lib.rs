//! `discord-clone` — a Discord-style layout (server rail, channel sidebar,
//! message view + composer) on the Idealyst runtime. Pure platform-agnostic
//! Rust; the entry point is `src/main.rs`, one `idealyst::entry!(discord_clone)`
//! line whose shell the target triple selects. The app is composed in
//! `src/app.rs`; the mock data lives in `src/data.rs`.

mod data;
mod style_helpers;
mod app;
mod auth;
mod servers;
mod moderation;
mod invite;
mod pins;
// Client-side wrapper for the backend's message-reaction `#[server]` RPCs
// (`discord_server::server_settings::{add_reaction, remove_reaction}`) — the
// live (persisted, synced) half of the reaction feature; the mock rows stay
// session-local. Same thin-stub shape as `pins.rs`.
mod reactions;
// Static Unicode emoji table for the picker — names + keywords + categories.
mod emoji_data;
mod typing_indicator;
mod notification_settings;
mod keyboard_shortcuts_overlay;
// The notification engine (in-app toast + browser Notification API + the
// localStorage persistence for the picker level / accent color). Wired into
// the gateway effect in `app.rs` and the `UserSettings` screen
// (`settings_suite`).
mod notify;
mod mentions;
mod roles_settings;
// The full-screen, three-tab user-settings screen (Notifications / Account /
// Appearance) opened by the settings gear; replaces the lone
// `notification_settings` screen as the gear's target.
mod settings_suite;
mod profile_settings;
mod dms;
mod styles;
mod controls;
mod notif_prefs;
mod notif_menu;
mod embeds;
mod channel_permissions;
mod live_dm_view;
mod group_dm;
// Client friends-list wrapper (RPC front-end for discord-server/src/friends.rs)
// + the live friends panel view.
mod friends;
mod friends_view;
// User-facing text for server-call errors.
mod errors;
// The colour theme: named roles, installed as idealyst tokens.
pub mod theme;
// Blocked list + block/unblock + the blocked-message placeholder.
mod blocks;

#[path = "components/message_timestamp_tooltip.rs"]
mod message_timestamp_tooltip;
#[path = "components/edited_label.rs"]
mod edited_label;
#[path = "components/verified_badge.rs"]
mod verified_badge;
#[path = "components/jump_to_bottom_button.rs"]
mod jump_to_bottom_button;
#[path = "components/load_earlier_button.rs"]
mod load_earlier_button;
#[path = "components/translate_toggle.rs"]
mod translate_toggle;
#[path = "components/message_action_bar.rs"]
mod message_action_bar;
#[path = "components/transfer_ownership_confirm.rs"]
mod transfer_ownership_confirm;
#[path = "components/avatar_chip.rs"]
mod avatar_chip;
#[path = "components/link_embed_preview.rs"]
mod link_embed_preview;
#[path = "components/member_role_list.rs"]
mod member_role_list;
#[path = "components/timeout_duration_picker.rs"]
mod timeout_duration_picker;
#[path = "components/send_failed_indicator.rs"]
mod send_failed_indicator;
#[path = "components/pin_confirm_toast.rs"]
mod pin_confirm_toast;
#[path = "components/pinned_messages_panel.rs"]
mod pinned_messages_panel;
#[path = "components/notification_toast.rs"]
mod notification_toast;
#[path = "components/ban_reason_input.rs"]
mod ban_reason_input;
#[path = "components/invite_expiry_selector.rs"]
mod invite_expiry_selector;
#[path = "components/reply_preview_bar.rs"]
mod reply_preview_bar;
#[path = "components/reply_reference_strip.rs"]
mod reply_reference_strip;
#[path = "components/server_icon_upload_placeholder.rs"]
mod server_icon_upload_placeholder;
#[path = "components/kick_confirm_dialog.rs"]
mod kick_confirm_dialog;
#[path = "components/ban_list_row.rs"]
mod ban_list_row;
#[path = "components/blocked_user_row.rs"]
mod blocked_user_row;
#[path = "components/delete_message_confirm.rs"]
mod delete_message_confirm;
#[path = "components/spoiler_tag.rs"]
mod spoiler_tag;
#[path = "components/invite_link_display.rs"]
mod invite_link_display;
#[path = "components/invite_modal.rs"]
mod invite_modal;
#[path = "components/join_modal.rs"]
mod join_modal;
#[path = "components/create_server_modal.rs"]
mod create_server_modal;
#[path = "components/create_channel_modal.rs"]
mod create_channel_modal;
#[path = "components/rename_channel_modal.rs"]
mod rename_channel_modal;
#[path = "components/delete_channel_modal.rs"]
mod delete_channel_modal;
#[path = "components/server_settings_panel.rs"]
mod server_settings_panel;
#[path = "components/server_region_selector.rs"]
mod server_region_selector;
#[path = "components/slash_command_dropdown.rs"]
mod slash_command_dropdown;
#[path = "components/emoji_picker.rs"]
pub mod emoji_picker;
#[path = "components/gif_picker.rs"]
pub mod gif_picker;

pub use app::app;
pub use auth::{login, signup};

// SDK-handler registration seam, invoked by the CLI-generated wrappers (web
// `start_in`/`hydrate_in`, macOS `run_with`, iOS `run_in_view`, terminal
// `run`) after `runtime_vocabulary::register_builtins`. Registry-generic over
// the scene `Host` so ONE seam serves every backend. This app renders only
// core primitives, so there is nothing extra to register.
pub fn register_scene_extensions<H>(
    registry: &mut runtime_scene::Registry<H>,
)
where
    H: runtime_vocabulary::style_attach::StyleServices + runtime_vocabulary::caps::TextOps + 'static,
{
    // The markdown SDK's payload handler — see `markdown::register` docs.
    markdown::register(registry);
}

// Recorder-side registration seam for the runtime-server sidecar
// (`dev_server::sidecar::run_newcore`) — the recorder's scene-registry twin
// of `register_scene_extensions`. Gated by `sidecar` (set only by the
// generated sidecar wrapper) so device/web builds never pull `dev-server`.
#[cfg(feature = "sidecar")]
pub fn register_scene_extensions_recorder(registry: &mut dev_server::newcore::SceneRegistry) {
    markdown::register(registry);
}

// Android entry: the generated Android wrapper's `attach` mounts
// `scene_app()` through `backend_android::newcore::start`. `app()` already
// returns the scene `Element`, so this is a plain re-export shim.
pub fn scene_app() -> runtime_core::Element {
    app()
}