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
#[allow(dead_code)]
mod invite;
mod pins;
// Client-side wrapper for the backend's message-reaction `#[server]` RPCs
// (`discord_server::server_settings::{add_reaction, remove_reaction}`) — the
// live (persisted, synced) half of the reaction feature; the mock rows stay
// session-local. Same thin-stub shape as `pins.rs`.
mod reactions;
mod typing_indicator;
mod notification_settings;
mod keyboard_shortcuts_overlay;
// The notification engine (in-app toast + browser Notification API + the
// localStorage persistence for the picker level / accent color). Wired into
// the gateway effect in `app.rs` and the `UserSettings` screen
// (`settings_suite`).
mod notify;
// The full-screen, three-tab user-settings screen (Notifications / Account /
// Appearance) opened by the settings gear; replaces the lone
// `notification_settings` screen as the gear's target.
mod settings_suite;
mod profile_settings;
mod dm_view;
mod dms;
mod live_dm_view;
// Client friends-list wrapper (RPC front-end for discord-server/src/friends.rs)
// + the live friends panel view.
mod friends;
mod friends_view;

// Standalone component, not wired into any screen yet — same "declared so it
// compiles, referenced by nothing" shape as `profile_settings`. Lives under
// `components/`; the `#[path]` keeps this a one-file add (no `mod.rs`).
#[path = "components/server_folder_group.rs"]
mod server_folder_group;
#[path = "components/idle_timer_badge.rs"]
mod idle_timer_badge;
#[path = "components/message_timestamp_tooltip.rs"]
mod message_timestamp_tooltip;
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
#[path = "components/load_earlier_button.rs"]
mod load_earlier_button;
#[path = "components/emoji_upload_slot.rs"]
mod emoji_upload_slot;
#[path = "components/rules_accept_checkbox.rs"]
mod rules_accept_checkbox;
#[path = "components/setup_checklist_item.rs"]
mod setup_checklist_item;
#[path = "components/translate_toggle.rs"]
mod translate_toggle;
#[path = "components/permission_override_row.rs"]
mod permission_override_row;
#[path = "components/message_action_bar.rs"]
mod message_action_bar;
#[path = "components/server_event_card.rs"]
mod server_event_card;
#[path = "components/connection_quality_indicator.rs"]
mod connection_quality_indicator;
#[path = "components/read_receipt_indicator.rs"]
mod read_receipt_indicator;
#[path = "components/transfer_ownership_confirm.rs"]
mod transfer_ownership_confirm;
#[path = "components/avatar_chip.rs"]
mod avatar_chip;
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
#[path = "components/icon_change_confirm.rs"]
mod icon_change_confirm;
#[path = "components/nickname_edit_field.rs"]
mod nickname_edit_field;
#[path = "components/pin_count_badge.rs"]
mod pin_count_badge;
#[path = "components/pin_confirm_toast.rs"]
mod pin_confirm_toast;
#[path = "components/pinned_messages_panel.rs"]
mod pinned_messages_panel;
#[path = "components/notification_toast.rs"]
mod notification_toast;
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
#[path = "components/poll_widget.rs"]
mod poll_widget;
#[path = "components/thread_list_row.rs"]
mod thread_list_row;
#[path = "components/spoiler_image_placeholder.rs"]
mod spoiler_image_placeholder;
#[path = "components/attachment_preview_card.rs"]
mod attachment_preview_card;
#[path = "components/reply_preview_bar.rs"]
mod reply_preview_bar;
#[path = "components/reply_reference_strip.rs"]
mod reply_reference_strip;
#[path = "components/mute_channel_toggle.rs"]
mod mute_channel_toggle;
#[path = "components/server_icon_upload_placeholder.rs"]
mod server_icon_upload_placeholder;
#[path = "components/kick_confirm_dialog.rs"]
mod kick_confirm_dialog;
#[path = "components/category_header.rs"]
mod category_header;
#[path = "components/custom_status_input.rs"]
mod custom_status_input;
#[path = "components/ban_list_row.rs"]
mod ban_list_row;
#[path = "components/blocked_user_row.rs"]
mod blocked_user_row;
#[path = "components/screen_share_indicator.rs"]
mod screen_share_indicator;
#[path = "components/delete_message_confirm.rs"]
mod delete_message_confirm;
#[path = "components/channel_topic_banner.rs"]
mod channel_topic_banner;
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
#[path = "components/discover_servers_modal.rs"]
mod discover_servers_modal;
#[path = "components/voice_join_button.rs"]
mod voice_join_button;
#[path = "components/server_template_card.rs"]
mod server_template_card;
#[path = "components/forward_message_dialog.rs"]
mod forward_message_dialog;
#[path = "components/two_factor_toggle.rs"]
mod two_factor_toggle;
#[path = "components/server_region_selector.rs"]
mod server_region_selector;
#[path = "components/delete_account_warning.rs"]
mod delete_account_warning;
#[path = "components/volume_slider.rs"]
mod volume_slider;
#[path = "components/char_counter.rs"]
mod char_counter;
#[path = "components/slowmode_indicator.rs"]
mod slowmode_indicator;
#[path = "components/message_search_bar.rs"]
mod message_search_bar;
#[path = "components/member_count_display.rs"]
mod member_count_display;
#[path = "components/slash_command_dropdown.rs"]
mod slash_command_dropdown;
#[path = "components/settings_tab_bar.rs"]
mod settings_tab_bar;

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
