//! The state `app_shell` shares with its section builders: every signal and
//! shared buffer a section reads, gathered once in `app_shell`.

use super::*;

#[derive(Clone)]
pub(super) struct ShellState {
    pub(super) active_live_channel: Signal<Option<i64>>,
    pub(super) active_live_server: Signal<Option<i64>>,
    pub(super) active_server: Signal<u32>,
    pub(super) attach_busy: Signal<bool>,
    pub(super) attach_error: Signal<String>,
    pub(super) attach_next_key: Signal<i64>,
    pub(super) authed: Signal<bool>,
    pub(super) cc_busy: Signal<bool>,
    pub(super) cc_err: Signal<String>,
    pub(super) cc_kind: Signal<String>,
    pub(super) cc_name: Signal<String>,
    pub(super) cc_open: Signal<bool>,
    pub(super) collapsed: Rc<RefCell<HashMap<u32, Signal<bool>>>>,
    pub(super) composer: Signal<String>,
    pub(super) dc_busy: Signal<bool>,
    pub(super) dc_cid: Signal<Option<i64>>,
    pub(super) dc_err: Signal<String>,
    pub(super) dc_name: Signal<String>,
    pub(super) dc_open: Signal<bool>,
    pub(super) deafened: Signal<bool>,
    pub(super) camera: Signal<bool>,
    pub(super) screen_share: Signal<bool>,
    pub(super) emoji_open: Signal<bool>,
    pub(super) friends_add_error: Signal<String>,
    pub(super) friends_add_username: Signal<String>,
    pub(super) friends_entry: Rc<dyn Fn() -> Element>,
    pub(super) friends_open: Signal<bool>,
    pub(super) friends_snap: Signal<Vec<crate::friends_view::FriendRow>>,
    pub(super) friends_tab: Signal<u8>,
    pub(super) gateway: crate::backend::Gateway,
    pub(super) has_more_earlier: Signal<bool>,
    pub(super) invite_created: Signal<Option<discord_server::invites::Invite>>,
    pub(super) invite_err: Signal<String>,
    pub(super) invite_expiry: Signal<usize>,
    pub(super) invite_mode: Signal<bool>,
    pub(super) joined_voice: Signal<Option<(u32, u32)>>,
    pub(super) live_voice_roster: Rc<RefCell<HashMap<i64, Vec<discord_server::voice::VoiceUser>>>>,
    pub(super) live_voice_rev: Signal<u32>,
    pub(super) voice_error: Signal<Option<(i64, String)>>,
    pub(super) live_at_bottom: Signal<bool>,
    pub(super) live_attachments: Rc<RefCell<HashMap<i64, Vec<discord_server::server_settings::MessageAttachment>>>>,
    pub(super) live_ban_confirm: Signal<Option<i64>>,
    pub(super) live_chan_menu: Signal<Option<i64>>,
    pub(super) live_channels: Signal<Vec<crate::servers::ChannelSummary>>,
    pub(super) live_delete_confirm: Signal<Option<i64>>,
    pub(super) live_dm_active: Signal<Option<i64>>,
    pub(super) live_dm_messages: Rc<RefCell<Vec<(String, String, i64, Option<i64>)>>>,
    pub(super) live_dm_mode: Signal<bool>,
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
    pub(super) live_servers: Signal<Vec<crate::servers::ServerSummary>>,
    pub(super) live_timeout_confirm: Signal<Option<i64>>,
    pub(super) live_timeout_duration: Signal<i64>,
    pub(super) live_typing: Rc<RefCell<HashMap<i64, (i64, String, i64)>>>,
    pub(super) live_typing_rev: Signal<u32>,
    pub(super) live_unread: Rc<RefCell<HashMap<i64, (i64, i64)>>>,
    pub(super) live_unread_rev: Signal<u32>,
    pub(super) load_busy: Signal<bool>,
    pub(super) member_panel_width: Signal<f32>,
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
    pub(super) sidebar_width: Signal<f32>,
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
    pub(super) live_collapsed: Signal<Vec<i64>>,
    pub(super) user_popup: Signal<Option<super::user_popups::UserPopup>>,
    pub(super) user_profile: Signal<Option<discord_server::profiles::UserProfile>>,
    pub(super) blocked_users: Signal<Vec<crate::blocks::BlockedUser>>,
    pub(super) revealed_blocked: Signal<Vec<i64>>,
    pub(super) server_menu_open: Signal<bool>,
    pub(super) gif_open: Signal<bool>,
    pub(super) msg_menu: Signal<Option<MessageKey>>,
    pub(super) members_pref: Signal<Option<bool>>,
}

/// Enter live mode on server `sid`: select it, clear channel + moderation
/// state left over from a previously live server, then pull its channels
/// (auto-selecting the first text channel), roster and unread counts in one
/// spawn anchored on the app shell, so every write lands in the callback
/// rather than across an await. Shared by the rail's tile handler, the Create
/// modal and the Join modal; `unread_buf` picks the unread map the caller
/// displays (`live_unread` for the panes, `rail_unread` for the rail).
pub(super) fn enter_live_server(
    s: ShellState,
    sid: i64,
    unread_buf: Rc<RefCell<HashMap<i64, (i64, i64)>>>,
) {
    // Picking a server from the rail while the DM screen is open leaves it.
    s.live_dm_mode.set(false);
    s.active_live_server.set(Some(sid));
    s.active_live_channel.set(None);
    s.live_kick_confirm.set(None);
    s.live_ban_confirm.set(None);
    s.live_timeout_confirm.set(None);
    s.live_member_error.set(String::new());
    s.live_members.set(Vec::new());

    let gw = s.gateway;
    let live_ch = s.live_channels;
    let live_ch_sel = s.active_live_channel;
    let lmem = s.live_members;
    let buf = unread_buf;
    let rev = s.live_unread_rev;
    runtime_core::spawn_then_in(
        &crate::app::shell_alive(),
        async move {
            let channels = crate::backend::list_channels(sid).await.ok();
            let members = crate::backend::list_members(sid).await.ok();
            let counts = crate::backend::unread_counts(sid).await.ok();
            (channels, members, counts)
        },
        move |(channels, members, counts)| {
            // Unread counts land BEFORE the first channel is auto-selected:
            // selecting the channel fires the channel-open effect, which reads
            // this buffer to offer the "N new messages" bar, so the buffer
            // must already hold the counts at that point.
            if let Some(counts) = counts {
                buf.borrow_mut()
                    .extend(counts.into_iter().map(|c| {
                        (c.channel_id, (c.count, c.unread_mentions))
                    }));
                rev.set(rev.get() + 1);
            }
            if let Some(channels) = channels {
                let first = channels
                    .iter()
                    .find(|c| c.kind == "text")
                    .map(|c| c.id);
                live_ch.set(channels);
                if let Some(id) = first {
                    live_ch_sel.set(Some(id));
                    // Persist the landing spot so a refresh re-enters here.
                    crate::auth::save_last_view_channel(sid, id);
                    // Auto-selecting does not populate the channel's messages
                    // on its own, so request History like a manual row tap.
                    gw.request_history(id, None);
                }
            }
            if let Some(members) = members {
                crate::mentions::members_loaded(sid, &members);
                lmem.set(members);
            }
        },
    );
}
