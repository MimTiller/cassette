//! The state `app_shell` shares with its section builders: every signal and
//! shared buffer a section reads, gathered once in `app_shell`.

use super::*;

#[derive(Clone)]
pub(super) struct ShellState {
    pub(super) layout: Layout,
    pub(super) nav: Nav,
    pub(super) attach_busy: Signal<bool>,
    pub(super) attach_error: Signal<String>,
    pub(super) attach_next_key: Signal<i64>,
    pub(super) authed: Signal<bool>,
    pub(super) cc_busy: Signal<bool>,
    pub(super) cc_err: Signal<String>,
    pub(super) cc_kind: Signal<String>,
    pub(super) cc_name: Signal<String>,
    pub(super) cc_open: Signal<bool>,
    pub(super) composer: Signal<String>,
    pub(super) dc_busy: Signal<bool>,
    pub(super) dc_cid: Signal<Option<i64>>,
    pub(super) dc_err: Signal<String>,
    pub(super) dc_name: Signal<String>,
    pub(super) dc_open: Signal<bool>,
    pub(super) deafened: Signal<bool>,
    pub(super) emoji_open: Signal<bool>,
    pub(super) friends_add_error: Signal<String>,
    pub(super) friends_add_username: Signal<String>,
    pub(super) friends_entry: Rc<dyn Fn() -> Element>,
    pub(super) friends_open: Signal<bool>,
    pub(super) friends_snap: Signal<Vec<crate::friends_view::FriendRow>>,
    pub(super) friends_tab: Signal<u8>,
    pub(super) gateway: server::UseSocket<discord_server::server_settings::WsServerMsg, discord_server::server_settings::WsClientMsg>,
    pub(super) has_more_earlier: Signal<bool>,
    pub(super) invite_created: Signal<Option<discord_server::invites::Invite>>,
    pub(super) invite_err: Signal<String>,
    pub(super) invite_expiry: Signal<usize>,
    pub(super) invite_mode: Signal<bool>,
    pub(super) live_at_bottom: Signal<bool>,
    pub(super) live_attachments: Rc<RefCell<HashMap<i64, Vec<discord_server::server_settings::MessageAttachment>>>>,
    pub(super) live_ban_confirm: Signal<Option<i64>>,
    pub(super) live_chan_menu: Signal<Option<i64>>,
    pub(super) live_delete_confirm: Signal<Option<i64>>,
    pub(super) live_dm_active: Signal<Option<i64>>,
    pub(super) live_dm_messages: Rc<RefCell<Vec<(String, String, i64, Option<i64>)>>>,
    pub(super) live_dm_new_id: Signal<String>,
    pub(super) live_dm_revision: Signal<u32>,
    pub(super) live_dms: Signal<Vec<crate::dms::Dm>>,
    pub(super) live_edit_text: Signal<String>,
    pub(super) live_editing: Signal<Option<i64>>,
    pub(super) live_invite_code: Signal<Option<String>>,
    pub(super) live_invite_error: Signal<String>,
    pub(super) live_invite_mode: Signal<bool>,
    pub(super) live_kick_confirm: Signal<Option<i64>>,
    pub(super) live_member_error: Signal<String>,
    pub(super) live_member_menu: Signal<Option<i64>>,
    pub(super) live_members: Signal<Vec<crate::moderation::MemberInfo>>,
    pub(super) live_mention_rev: Signal<u32>,
    pub(super) live_mentioned: Rc<RefCell<HashMap<i64, i64>>>,
    pub(super) live_messages: Rc<RefCell<HashMap<i64, Vec<LiveMsg>>>>,
    pub(super) live_presence: Rc<RefCell<HashMap<i64, String>>>,
    pub(super) live_presence_rev: Signal<u32>,
    pub(super) live_revision: Signal<u32>,
    pub(super) live_scroll_handle: Rc<RefCell<Option<ScrollViewHandle>>>,
    pub(super) live_timeout_confirm: Signal<Option<i64>>,
    pub(super) live_timeout_duration: Signal<i64>,
    pub(super) live_typing: Rc<RefCell<HashMap<i64, (i64, String, i64)>>>,
    pub(super) live_typing_rev: Signal<u32>,
    pub(super) live_unread: Rc<RefCell<HashMap<i64, (i64, i64)>>>,
    pub(super) live_unread_rev: Signal<u32>,
    pub(super) load_busy: Signal<bool>,
    pub(super) moderation_mode: Signal<bool>,
    pub(super) muted: Signal<bool>,
    pub(super) notif_level: Signal<u32>,
    pub(super) notif_mode: Signal<bool>,
    pub(super) notif_toast_body: Signal<String>,
    pub(super) notif_toast_title: Signal<String>,
    pub(super) notif_toast_visible: Signal<bool>,
    pub(super) on_dm_message: Rc<dyn Fn(i64)>,
    pub(super) pending_attachment: Signal<Vec<PendingImage>>,
    pub(super) picker: Signal<Option<MessageKey>>,
    pub(super) pin_visible: Signal<bool>,
    pub(super) pinned_count: Signal<i64>,
    pub(super) pinned_error: Signal<String>,
    pub(super) pinned_messages: Signal<Vec<crate::pins::PinnedMessage>>,
    pub(super) pinned_panel_open: Signal<bool>,
    pub(super) pinned_revision: Signal<u32>,
    pub(super) profile_mode: Signal<bool>,
    pub(super) rail_unread: Rc<RefCell<HashMap<i64, (i64, i64)>>>,
    pub(super) rc_busy: Signal<bool>,
    pub(super) rc_cid: Signal<Option<i64>>,
    pub(super) rc_err: Signal<String>,
    pub(super) rc_name: Signal<String>,
    pub(super) rc_open: Signal<bool>,
    pub(super) reaction_rev: Signal<u32>,
    pub(super) reactions: Rc<RefCell<ReactionMap>>,
    pub(super) reply_ctx: Signal<Option<(String, String)>>,
    pub(super) reply_target: Signal<Option<i64>>,
    pub(super) send_failed: Signal<bool>,
    pub(super) shortcuts_mode: Signal<bool>,
    pub(super) ss_add_role: Signal<String>,
    pub(super) ss_add_user_id: Signal<String>,
    pub(super) ss_busy: Signal<bool>,
    pub(super) ss_err: Signal<String>,
    pub(super) ss_icon: Signal<String>,
    pub(super) ss_name: Signal<String>,
    pub(super) ss_open: Signal<bool>,
    pub(super) ss_owner: Signal<String>,
    pub(super) translate_key: Signal<Option<MessageKey>>,
    pub(super) user_status: Signal<Status>,
    pub(super) status_menu: Signal<bool>,

    pub(super) create_open: Signal<bool>,
    pub(super) create_name: Signal<String>,
    pub(super) create_error: Signal<String>,
    pub(super) create_busy: Signal<bool>,
    pub(super) join_open: Signal<bool>,
    pub(super) join_code: Signal<String>,
    pub(super) join_result: Signal<Option<discord_server::invites::RedeemResult>>,
    pub(super) join_error: Signal<String>,
    pub(super) join_busy: Signal<bool>,
    pub(super) user_popup: Signal<Option<super::user_popups::UserPopup>>,
    pub(super) user_profile: Signal<Option<discord_server::profiles::UserProfile>>,
    pub(super) blocked_users: Signal<Vec<crate::blocks::BlockedUser>>,
    pub(super) revealed_blocked: Signal<Vec<i64>>,
    pub(super) gif_open: Signal<bool>,
    pub(super) msg_menu: Signal<Option<MessageKey>>,
    pub(super) members_pref: Signal<Option<bool>>,
}


#[derive(Clone)]
pub(super) struct Nav {
    pub(super) active_server: Signal<u32>,
    pub(super) active_live_server: Signal<Option<i64>>,
    pub(super) active_live_channel: Signal<Option<i64>>,
    pub(super) live_servers: Signal<Vec<crate::servers::ServerSummary>>,
    pub(super) live_channels: Signal<Vec<crate::servers::ChannelSummary>>,
    pub(super) joined_voice: Signal<Option<(u32, u32)>>,
    pub(super) live_dm_mode: Signal<bool>,
    pub(super) server_menu_open: Signal<bool>,
}

impl Nav {
    /// Fresh signals for this group; wiring around them stays in `app_shell`.
    pub(super) fn new() -> Nav {
        // [Feature 16] `active_server`/`active_channel` are the *mock* selection
        // signals: every pane renders live (backend) content now, so these keep a
        // `SERVERS[0]` default that no real user path ever displays (see each
        // pane's live/empty-state gate below).
        let active_server: Signal<u32> = signal(SERVERS[0].id);
        // Live-mode selection, parallel to (and independent of) the mock
        // `active_server`/`active_channel` signals. `active_live_server` is the id
        // of a *real* backend server the user tapped (None = mock path is active);
        // `live_channels` holds that server's channels, fetched on demand. Tapping
        // a mock rail row sets `active_live_server` back to None, so the two modes
        // never overlap and the mock path keeps its exact existing behaviour.
        let active_live_server: Signal<Option<i64>> = signal(None);
        // The live channel the user tapped inside the active live server. Reset to
        // None whenever `active_live_server` changes (new live server selected, or
        // mock mode re-entered) — a channel id from a different server is stale.
        let active_live_channel: Signal<Option<i64>> = signal(None);
        // Live (backend) servers the signed-in user belongs to, fetched once from
        // the `#[server]` RPC after `configure_server()` has pointed the stubs at
        // a base URL. Rendered as its own section below the mock rows; see the
        // rail for why these rows are not yet interactive.
        let live_servers: Signal<Vec<crate::servers::ServerSummary>> = signal(Vec::new());
        let live_channels: Signal<Vec<crate::servers::ChannelSummary>> = signal(Vec::new());
        // The voice channel the user is currently joined to, keyed by (server_id,
        // voice_channel_id). Joining a voice row in the sidebar sets it; leaving
        // (from the profile strip) or switching servers clears it. `None` = not
        // in a voice channel. Drives the voice-row membership lists AND the
        // profile strip's voice-connection bar.
        let joined_voice: Signal<Option<(u32, u32)>> = signal(None);
        // --- live (backend) DM state -------------------------------------------
        // Parallel to (and independent of) the mock `dm_*` state above, and to the
        // `live_servers`/`live_channels` live-mode idiom. A separate, new entry
        // point: `live_dm_mode` toggles its own full-screen overlay region, and
        // the mock DM screen is left untouched.
        // `live_dms` holds the fetched conversation list (populated on first open
        // via `spawn_async` calling `list_dms`); `live_dm_active` the selected DM
        // channel id (None = none); `live_dm_messages`/`live_dm_revision` the
        // fetched history buffer + its rebuild counter (the `revision` bump is the
        // rebuild trigger, the same idiom as `live_messages`/`live_revision`).
        let live_dm_mode: Signal<bool> = signal(false);
        // The live sidebar header's server dropdown.
        let server_menu_open: Signal<bool> = signal(false);
        Nav {
            active_server: active_server,
            active_live_server: active_live_server,
            active_live_channel: active_live_channel,
            live_servers: live_servers,
            live_channels: live_channels,
            joined_voice: joined_voice,
            live_dm_mode: live_dm_mode,
            server_menu_open: server_menu_open,
        }
    }
}


#[derive(Clone)]
pub(super) struct Layout {
    pub(super) sidebar_width: Signal<f32>,
    pub(super) member_panel_width: Signal<f32>,
    pub(super) collapsed: Rc<RefCell<HashMap<u32, Signal<bool>>>>,
    pub(super) live_collapsed: Signal<Vec<i64>>,
}

impl Layout {
    /// Fresh signals for this group; wiring around them stays in `app_shell`.
    pub(super) fn new() -> Layout {
        // Resizable column widths (CSS pixels), driven by the thin drag handles
        // between the channel sidebar / main view and the main view / member list.
        // Both start at 240px; the handles
        // clamp to 180..=400. `Signal` is `Copy`, so the drag closures and the
        // reactive style closures below each capture their own handle.
        let sidebar_width: Signal<f32> = signal(240.0);
        let member_panel_width: Signal<f32> = signal(240.0);
        // Channel-category collapse state: one signal per category id, all
        // starting expanded (false). Held in an Rc<HashMap> so both the sidebar
        // `switch` key and its render loop can reach it. The key reads every
        // category's collapsed value (see below), so when the CategoryHeader
        // toggles its signal the key changes and the (non-reactive, plain-Rust)
        // row loop re-runs, re-gating each category's rows on its collapsed flag.
        // The map lives in `app_shell` scope, so state survives `switch` rebuilds
        // across server/channel/voice changes.
        let collapsed: Rc<RefCell<HashMap<u32, Signal<bool>>>> = Rc::new(RefCell::new(HashMap::new()));
        // The live sidebar's collapsed categories (channel ids). Plain data in one
        // app_shell-owned signal: a per-category signal created inside the
        // sidebar's render would be freed on the next rebuild.
        let live_collapsed: Signal<Vec<i64>> = signal(Vec::new());
        Layout {
            sidebar_width: sidebar_width,
            member_panel_width: member_panel_width,
            collapsed: collapsed,
            live_collapsed: live_collapsed,
        }
    }
}

