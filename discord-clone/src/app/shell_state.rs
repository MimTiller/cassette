//! The state `app_shell` shares with its section builders: every signal and
//! shared buffer a section reads, gathered once in `app_shell`.

use super::*;

#[derive(Clone)]
pub(super) struct ShellState {
    pub(super) overlays: Overlays,
    pub(super) members: Members,
    pub(super) messages: Messages,
    pub(super) composer_state: Composer,
    pub(super) layout: Layout,
    pub(super) nav: Nav,
    pub(super) authed: Signal<bool>,
    pub(super) cc_busy: Signal<bool>,
    pub(super) cc_err: Signal<String>,
    pub(super) cc_kind: Signal<String>,
    pub(super) cc_name: Signal<String>,
    pub(super) cc_open: Signal<bool>,
    pub(super) dc_busy: Signal<bool>,
    pub(super) dc_cid: Signal<Option<i64>>,
    pub(super) dc_err: Signal<String>,
    pub(super) dc_name: Signal<String>,
    pub(super) dc_open: Signal<bool>,
    pub(super) friends_add_error: Signal<String>,
    pub(super) friends_add_username: Signal<String>,
    pub(super) friends_entry: Rc<dyn Fn() -> Element>,
    pub(super) friends_open: Signal<bool>,
    pub(super) friends_snap: Signal<Vec<crate::friends_view::FriendRow>>,
    pub(super) friends_tab: Signal<u8>,
    pub(super) gateway: server::UseSocket<discord_server::server_settings::WsServerMsg, discord_server::server_settings::WsClientMsg>,
    pub(super) invite_created: Signal<Option<discord_server::invites::Invite>>,
    pub(super) invite_err: Signal<String>,
    pub(super) invite_expiry: Signal<usize>,
    pub(super) invite_mode: Signal<bool>,
    pub(super) live_dm_active: Signal<Option<i64>>,
    pub(super) live_dm_messages: Rc<RefCell<Vec<(String, String, i64, Option<i64>)>>>,
    pub(super) live_dm_new_id: Signal<String>,
    pub(super) live_dm_revision: Signal<u32>,
    pub(super) live_dms: Signal<Vec<crate::dms::Dm>>,
    pub(super) on_dm_message: Rc<dyn Fn(i64)>,
    pub(super) rc_busy: Signal<bool>,
    pub(super) rc_cid: Signal<Option<i64>>,
    pub(super) rc_err: Signal<String>,
    pub(super) rc_name: Signal<String>,
    pub(super) rc_open: Signal<bool>,
    pub(super) ss_add_role: Signal<String>,
    pub(super) ss_add_user_id: Signal<String>,
    pub(super) ss_busy: Signal<bool>,
    pub(super) ss_err: Signal<String>,
    pub(super) ss_icon: Signal<String>,
    pub(super) ss_name: Signal<String>,
    pub(super) ss_open: Signal<bool>,
    pub(super) ss_owner: Signal<String>,

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


#[derive(Clone)]
pub(super) struct Composer {
    pub(super) composer: Signal<String>,
    pub(super) emoji_open: Signal<bool>,
    pub(super) pending_attachment: Signal<Vec<PendingImage>>,
    pub(super) attach_next_key: Signal<i64>,
    pub(super) attach_busy: Signal<bool>,
    pub(super) attach_error: Signal<String>,
    pub(super) gif_open: Signal<bool>,
}

impl Composer {
    /// Fresh signals for this group; wiring around them stays in `app_shell`.
    pub(super) fn new() -> Composer {
        let composer: Signal<String> = signal(String::new());
        // Emoji-picker popover: `true` = the floating emoji grid is open above the
        // smile button. Pure local UI state (no backend), toggled by the smile
        // icon and closed by picking an emoji or tapping elsewhere.
        let emoji_open: Signal<bool> = signal(false);
        // Pending message-image attachments staged by the composer's attach
        // button: each entry is `(staging_key, filename, mime, data_url)` where
        // `data_url` is the encoded `data:image/…;base64,` URL, already filled by
        // the file-picker callback BEFORE the entry is staged (staging happens
        // when the async pick completes), so the string never mutates in place.
        // The thumbnail strip above the composer reads this signal (keyed
        // `for …, key =` loop); an 'x' on a thumbnail removes that entry; Send
        // consumes the list alongside the composer text. Keys are unique via
        // `attach_next_key`.
        let pending_attachment: Signal<Vec<PendingImage>> = signal(Vec::new());
        let attach_next_key: Signal<i64> = signal(0);
        // The attach flow's in-flight flag and visible rejection line (same
        // triple idiom as the profile screen's avatar upload).
        let attach_busy: Signal<bool> = signal(false);
        let attach_error: Signal<String> = signal(String::new());
        // The composer's GIF picker, and which message's "⋯" menu is open (see
        // `popups`).
        let gif_open: Signal<bool> = signal(false);
        Composer {
            composer: composer,
            emoji_open: emoji_open,
            pending_attachment: pending_attachment,
            attach_next_key: attach_next_key,
            attach_busy: attach_busy,
            attach_error: attach_error,
            gif_open: gif_open,
        }
    }
}


#[derive(Clone)]
pub(super) struct Messages {
    pub(super) live_attachments: Rc<RefCell<HashMap<i64, Vec<discord_server::server_settings::MessageAttachment>>>>,
    pub(super) live_messages: Rc<RefCell<HashMap<i64, Vec<LiveMsg>>>>,
    pub(super) live_revision: Signal<u32>,
    pub(super) live_editing: Signal<Option<i64>>,
    pub(super) live_edit_text: Signal<String>,
    pub(super) live_delete_confirm: Signal<Option<i64>>,
    pub(super) reactions: Rc<RefCell<ReactionMap>>,
    pub(super) reaction_rev: Signal<u32>,
    pub(super) picker: Signal<Option<MessageKey>>,
    pub(super) has_more_earlier: Signal<bool>,
    pub(super) live_at_bottom: Signal<bool>,
    pub(super) live_scroll_handle: Rc<RefCell<Option<ScrollViewHandle>>>,
    pub(super) live_mentioned: Rc<RefCell<HashMap<i64, i64>>>,
    pub(super) live_mention_rev: Signal<u32>,
    pub(super) live_unread: Rc<RefCell<HashMap<i64, (i64, i64)>>>,
    pub(super) live_unread_rev: Signal<u32>,
    pub(super) rail_unread: Rc<RefCell<HashMap<i64, (i64, i64)>>>,
    pub(super) live_typing: Rc<RefCell<HashMap<i64, (i64, String, i64)>>>,
    pub(super) live_typing_rev: Signal<u32>,
    pub(super) load_busy: Signal<bool>,
    pub(super) pinned_panel_open: Signal<bool>,
    pub(super) pinned_messages: Signal<Vec<crate::pins::PinnedMessage>>,
    pub(super) pinned_count: Signal<i64>,
    pub(super) pinned_revision: Signal<u32>,
    pub(super) pinned_error: Signal<String>,
    pub(super) pin_visible: Signal<bool>,
    pub(super) reply_ctx: Signal<Option<(String, String)>>,
    pub(super) reply_target: Signal<Option<i64>>,
    pub(super) translate_key: Signal<Option<MessageKey>>,
    pub(super) msg_menu: Signal<Option<MessageKey>>,
    pub(super) send_failed: Signal<bool>,
}

impl Messages {
    /// Fresh signals for this group; wiring around them stays in `app_shell`.
    pub(super) fn new() -> Messages {
        // Persisted/live attachments per wire message id (`message_id` ->
        // attachments carried on the `WsServerMsg::Message` frames — history
        // pages and live fan-out alike). Plain buffer (not a signal): the
        // message-list `switch` already rebuilds on `live_revision`, which the
        // gateway effect bumps whenever a message (and its attachments) land.
        let live_attachments: Rc<RefCell<HashMap<i64, Vec<discord_server::server_settings::MessageAttachment>>>> = Rc::new(RefCell::new(HashMap::new()));
        // --- live/real message state (composer image attachments ride this) ----
        // Separate from the mock `sent`/`revision` above: when `active_live_channel`
        // is `Some(id)` the message list reads from here instead of the mock path.
        // channel_id -> Vec<LiveMsg>, oldest-first (the gateway's History replies
        // arrive oldest-first, and new frames append in order). Mutating the Vec
        // does NOT trigger reactivity, so `live_revision` is bumped on every frame
        // the buffer absorbs and folded into the list `switch`'s key — exactly the
        // mock `revision` idiom.
        let live_messages: Rc<RefCell<HashMap<i64, Vec<LiveMsg>>>> = Rc::new(RefCell::new(HashMap::new()));
        let live_revision: Signal<u32> = signal(0);
        // Per-message edit mode: the wire `message_id` whose row currently shows
        // the inline edit input (None = none). One at a time — switching rows
        // just overwrites it, and the list rebuild (keyed on `live_revision` and
        // this signal's value) tears down the old input and mounts the new one.
        // It must be a top-level `Signal` (not a `Rc<RefCell>`) because the
        // `text_input value = …` prop needs a reactive `Signal<String>`.
        let live_editing: Signal<Option<i64>> = signal(None);
        // The draft text for the row being edited. A top-level `Signal` because the
        // inline `text_input value = …` prop needs a reactive handle; it is seeded
        // with the row's content the moment a row enters edit mode (below) and read
        // back on confirm. Reset to `""` after a confirmed edit so the next edit
        // starts from the row's (now-updated) content, not a stale draft.
        let live_edit_text: Signal<String> = signal(String::new());
        // Per-message delete confirm: the wire `message_id` whose row is showing
        // "are you sure?" (None = none). One step, mirroring the composer's
        // "simple confirm" ask — no two-step kick-style confirm.
        let live_delete_confirm: Signal<Option<i64>> = signal(None);
        // --- message reactions (shared by the MOCK and the LIVE message lists) --
        // Per-message reaction state: `reactions` is the pill map (see the `ReactionMap`
        // doc for the keying), `picker` names the message whose add-reaction overlay
        // is open (`None` while closed), and `reaction_rev` is the rebuild counter the
        // message-list `switch` folds into its key so a pill change re-renders the
        // list. MOCK pills are session-local (seed demo); LIVE pills are persisted
        // server-side via the `add_reaction` / `remove_reaction` RPCs and restored
        // on cold loads from `WsServerMsg::Message.reactions` (history frames) plus
        // updated live by the `ReactionAdd` / `ReactionRemove` broadcast frames.
        // Declared HERE (before `gateway_incoming_effect`, whose `move` closure
        // captures these handles — a `switch` effect cannot see a binding defined
        // after its own call) and read by both paths.
        let reactions: Rc<RefCell<ReactionMap>> = Rc::new(RefCell::new(HashMap::new()));
        let reaction_rev: Signal<u32> = signal(0);
        let picker: Signal<Option<MessageKey>> = signal(None);
        let has_more_earlier: Signal<bool> = signal(true);
        // Scroll-back state for the live message list:
        // - `live_at_bottom` is `true` while the scroll view is at (or near) the
        //   content bottom — the "follow the live tail" mode the list starts in.
        //   Flipped to `false` the moment the user scrolls up away from the bottom
        //   (the `on_scroll` max-offset heuristic) and back to `true` on
        //   `on_end_reached`. Drives the floating "jump to bottom" button.
        // - `live_scroll_handle` holds the `ScrollViewHandle` the scroll view hands
        //   us in its `on_handle` callback, so the "load earlier" and "jump to
        //   bottom" buttons can scroll the pane programmatically (and, when
        //   prepending, restore the user's reading position).
        let live_at_bottom: Signal<bool> = signal(true);
        let live_scroll_handle: Rc<RefCell<Option<ScrollViewHandle>>> = Rc::new(RefCell::new(None));
        // Live @mention indicator state, driven by the real backend data (the
        // mock path's `c.mentions`/`total_mentions` is its own, untouched copy).
        // `live_mentioned` maps a live channel id -> count of UNREAD messages that
        // mention the signed-in user. It is bumped by the gateway's inbound
        // `Message { mentioned_user_ids, new: true }` frames and zeroed when the
        // user opens the channel (the local "read" reset — the backend's own
        // per-channel unread-mentions is exposed via `unread_counts(server_id)`,
        // but a live client that is actively watching only needs the local count).
        // Mutating the HashMap does not trigger reactivity, so `live_mention_rev`
        // is bumped on every change and folded into the sidebar `switch` key —
        // exactly the `live_revision` idiom used for the message buffer.
        let live_mentioned: Rc<RefCell<HashMap<i64, i64>>> = Rc::new(RefCell::new(HashMap::new()));
        let live_mention_rev: Signal<u32> = signal(0);
        // Live per-channel UNREAD COUNTS from the backend, fetched alongside the
        // channel list whenever a live server is selected (`unread_counts` RPC).
        // `count` = messages not yet read; `unread_mentions` = the subset that
        // @mention the signed-in user (0..=count). This is the REAL data source
        // for the live sidebar's badges — independent of `live_mentioned` above,
        // which stays the gateway-incrementing local indicator for the currently
        // open session. Mutating the HashMap does not trigger reactivity, so
        // `live_unread_rev` is bumped on every change and folded into the
        // sidebar `switch` key — exactly the `live_mention_rev` idiom.
        let live_unread: Rc<RefCell<HashMap<i64, (i64, i64)>>> = Rc::new(RefCell::new(HashMap::new()));
        let live_unread_rev: Signal<u32> = signal(0);
        // A second `Rc` handle for the rail's live-section fetch closure: both
        // that `move` closure and the sidebar `switch`'s `move` render closure
        // need the map, and `Rc` is not `Copy`, so each captures its own clone
        // (the clones share the same `RefCell` — that's the point).
        // A second handle to `live_unread` for the rail's live-section fetch.
        // Live typing-indicator state, keyed per live channel: the user most
        // recently seen typing in that channel, as `(user_id, display_label, gen)`.
        // `gen` is bumped on every inbound `Typing` frame for the channel; it is the
        // guard that lets a 5-second auto-clear `after_ms_detached` task
        // self-cancel — it fires only if the captured `gen` still matches the map's
        // current entry for that channel, so a newer frame (which bumps `gen`)
        // turns the older, still-pending clear into a no-op instead of wiping a
        // fresh indicator. Only one user per channel is tracked — the latest typer
        // — which matches a single "… is typing…" line (the server fans out every
        // member's typing to the whole channel audience). Mutating the `Rc` does
        // not trigger reactivity, so `live_typing_rev` is bumped on every change
        // and folded into the typing row's `switch` key (the `live_revision` idiom).
        let live_typing: Rc<RefCell<HashMap<i64, (i64, String, i64)>>> = Rc::new(RefCell::new(HashMap::new()));
        let live_typing_rev: Signal<u32> = signal(0);
        // Scrollback paging for the live message list. `load_busy` is `true` while a
        // `History { before_id }` request is in flight (gates the "Load earlier"
        // pill's tap so a double-tap can't fire a second page; the pill's label
        // goes muted while `true`). The gateway effect clears it on the
        // `WsServerMsg::History` marker frame (which arrives before the page's
        // `Message { new: false }` frames). `has_more_earlier` is set by the same
        // marker frame from the server's `has_more` field (a full page = more
        // older messages exist). Both signals are declared HERE — before the
        // gateway effect below captures them — because Rust requires a value in
        // scope at the point a closure captures it (same pattern as `notif_level`
        // etc. above).
        let load_busy: Signal<bool> = signal(false);
        // Pinned-messages panel state (LIVE / backend). `pinned_panel_open` is the
        // full-screen overlay's show flag; `pinned_messages` the fetched list;
        // `pinned_count` the header badge value; `pinned_revision` the rebuild
        // trigger (bumped after every list/count mutation); `pinned_error` a
        // rejection surfaced in the panel body. The header pin icon opens the
        // panel and fetches the list; each row's Unpin button updates the count
        // and list (and bumps `pinned_revision`) after a backend round trip.
        let pinned_panel_open: Signal<bool> = signal(false);
        let pinned_messages: Signal<Vec<crate::pins::PinnedMessage>> = signal(Vec::new());
        let pinned_count: Signal<i64> = signal(0);
        let pinned_revision: Signal<u32> = signal(0);
        let pinned_error: Signal<String> = signal(String::new());
        let pin_visible: Signal<bool> = signal(false);
        // Reply context for the channel composer: `Some((author, snippet))` while a
        // message row's hover "Reply" button is composing a reply (the
        // `ReplyPreviewBar` strip reads it); `None` while not. `PinConfirmToast`'s
        // `visible` flag, flashed by the "Pin message" overflow-menu entry.
        let reply_ctx: Signal<Option<(String, String)>> = signal(None);
        // The wire id of the message the composer is replying to. Set by a row's
        // Reply action (from its `MessageKey`) and read by `do_send` (below) to
        // attach it to the outbound `Send` frame; cleared after a successful send
        // and by the preview bar's "×". Kept separate from `reply_ctx` (the display
        // author + snippet) so the server needs only the id, not the display text.
        let reply_target: Signal<Option<i64>> = signal(None);
        // The per-message "Translate message" overflow-menu entry reveals a
        // `TranslateToggle` on ONE row at a time (a deliberate single instance, not
        // a per-row widget). This shared signal names the active row's
        // `(channel, list_index)` key — the picker idiom — so opening another
        // row's menu or re-tapping dismisses it. Owned here next to the other mock
        // per-app signals; threaded into the mock message list only.
        let translate_key: Signal<Option<MessageKey>> = signal(None);
        let msg_menu: Signal<Option<MessageKey>> = signal(None);
        // A send failed: `true` = the most recent live send was rejected by the
        // backend (the gateway replies with an `Error` frame, handled below in
        // `gateway_incoming_effect`). The "Failed to send - Retry" banner above the
        // composer reads this; clicking it clears the signal to dismiss the banner
        // (the user resends to retry). Pure UI state — the gateway `send` itself is
        // fire-and-forget, so this is the app's signal for a failed send.
        let send_failed: Signal<bool> = signal(false);
        Messages {
            live_attachments: live_attachments,
            live_messages: live_messages,
            live_revision: live_revision,
            live_editing: live_editing,
            live_edit_text: live_edit_text,
            live_delete_confirm: live_delete_confirm,
            reactions: reactions,
            reaction_rev: reaction_rev,
            picker: picker,
            has_more_earlier: has_more_earlier,
            live_at_bottom: live_at_bottom,
            live_scroll_handle: live_scroll_handle,
            live_mentioned: live_mentioned,
            live_mention_rev: live_mention_rev,
            live_unread: live_unread.clone(),
            live_unread_rev: live_unread_rev,
            rail_unread: live_unread.clone(),
            live_typing: live_typing,
            live_typing_rev: live_typing_rev,
            load_busy: load_busy,
            pinned_panel_open: pinned_panel_open,
            pinned_messages: pinned_messages,
            pinned_count: pinned_count,
            pinned_revision: pinned_revision,
            pinned_error: pinned_error,
            pin_visible: pin_visible,
            reply_ctx: reply_ctx,
            reply_target: reply_target,
            translate_key: translate_key,
            msg_menu: msg_menu,
            send_failed: send_failed,
        }
    }
}


#[derive(Clone)]
pub(super) struct Members {
    pub(super) live_members: Signal<Vec<crate::moderation::MemberInfo>>,
    pub(super) live_member_error: Signal<String>,
    pub(super) live_member_menu: Signal<Option<i64>>,
    pub(super) live_chan_menu: Signal<Option<i64>>,
    pub(super) live_kick_confirm: Signal<Option<i64>>,
    pub(super) live_ban_confirm: Signal<Option<i64>>,
    pub(super) live_timeout_confirm: Signal<Option<i64>>,
    pub(super) live_timeout_duration: Signal<i64>,
    pub(super) live_presence: Rc<RefCell<HashMap<i64, String>>>,
    pub(super) live_presence_rev: Signal<u32>,
    pub(super) moderation_mode: Signal<bool>,
    pub(super) members_pref: Signal<Option<bool>>,
}

impl Members {
    /// Fresh signals for this group; wiring around them stays in `app_shell`.
    pub(super) fn new() -> Members {
        // Live member-roster state for the moderation panel (LIVE mode only). The
        // roster is fetched via the new backend `list_members` endpoint when a live
        // server is selected (see the rail on_touch below) and re-fetched after a
        // successful kick/role-change. `live_kick_confirm` is the two-step inline
        // confirm for a kick: None = no pending kick, Some(user_id) = "are you
        // sure?" is shown for that member's row. `live_member_error` surfaces a
        // backend authorization error (e.g. "you need moderator privileges") as
        // inline text rather than a modal.
        let live_members: Signal<Vec<crate::moderation::MemberInfo>> = signal(Vec::new());
        let live_member_error: Signal<String> = signal(String::new());
        // Which member row's Kick/Ban/Timeout popup menu is currently open
        // (`Some(user_id)` = that member's menu; `None` = closed). A member-row
        // tap sets it; a menu entry arms the per-action confirm signal above and
        // clears this. Part of the member panel's `switch` key so the portal
        // mounts/unmounts on tap.
        let live_member_menu: Signal<Option<i64>> = signal(None);
        // Which channel row's management menu (Rename / Delete) is currently open
        // (`Some(channel_id)`; `None` = closed). A gear tap on a hovered channel
        // row sets it; the gear itself renders ONLY while the row is hovered and
        // the acting user is moderator+ (the per-row idiom `message_action_bar`
        // uses for message rows — a per-row `hovered` signal fed by `on_hover`).
        let live_chan_menu: Signal<Option<i64>> = signal(None);
        let live_kick_confirm: Signal<Option<i64>> = signal(None);
        // The two-step inline confirm for a ban (the ban is the more severe
        // action, so it gets its own confirm rather than sharing the kick one —
        // a mis-tap that confirms a ban is harder to undo than a mis-tap kick).
        let live_ban_confirm: Signal<Option<i64>> = signal(None);
        // The two-step inline confirm for a timeout: None = no pending timeout,
        // Some(user_id) = "Timeout? [Yes/No]" is shown for that member's row. A
        // timeout silences a member from posting for `live_timeout_duration`
        // without removing their membership (the milder of the three severities).
        let live_timeout_confirm: Signal<Option<i64>> = signal(None);
        // The chosen timeout duration in seconds, shared with the duration picker
        // the confirm branch renders. Defaults to 60s (the picker's first option)
        // so a bare Timeout tap still has a sensible duration. The picker writes
        // to this signal as the user taps an option; the confirm's "Yes" reads it.
        let live_timeout_duration: Signal<i64> = signal(60);
        // Live presence: `user_id -> status string`. The backend broadcasts
        // `WsServerMsg::Presence` ('online' on socket auth + activity, 'away' after
        // `PRESENCE_IDLE_SECS` of no inbound frames, 'offline' on the user's last
        // socket disconnect) over the gateway, so the member panel can show a
        // per-row status dot. Mutating the `Rc` does not trigger reactivity, so
        // `live_presence_rev` is bumped on every frame and folded into the member
        // panel's `switch` key (the `live_revision` / `live_typing_rev` idiom). A
        // member with no frame yet (just after the roster loads) reads as absent
        // and is rendered as a grey "offline" dot.
        let live_presence: Rc<RefCell<HashMap<i64, String>>> = Rc::new(RefCell::new(HashMap::new()));
        let live_presence_rev: Signal<u32> = signal(0);
        // Server Moderation panel: a gavel icon in the channel header opens it as
        // a full-screen overlay (the same toggle idiom as `profile_mode` — built
        // once, shown/hidden by this signal, a "Done" pill flips it back off).
        // Mock-only: renders the eight unwired moderation components with hardcoded
        // rows; no backend call.
        let moderation_mode: Signal<bool> = signal(false);
        // The member list: `None` = automatic (shown when the window is wide
        // enough, see `members_visible`), `Some` = the header toggle's choice.
        let members_pref: Signal<Option<bool>> = signal(None);
        Members {
            live_members: live_members,
            live_member_error: live_member_error,
            live_member_menu: live_member_menu,
            live_chan_menu: live_chan_menu,
            live_kick_confirm: live_kick_confirm,
            live_ban_confirm: live_ban_confirm,
            live_timeout_confirm: live_timeout_confirm,
            live_timeout_duration: live_timeout_duration,
            live_presence: live_presence,
            live_presence_rev: live_presence_rev,
            moderation_mode: moderation_mode,
            members_pref: members_pref,
        }
    }
}


#[derive(Clone)]
pub(super) struct Overlays {
    pub(super) profile_mode: Signal<bool>,
    pub(super) notif_mode: Signal<bool>,
    pub(super) notif_level: Signal<u32>,
    pub(super) notif_toast_body: Signal<String>,
    pub(super) notif_toast_title: Signal<String>,
    pub(super) notif_toast_visible: Signal<bool>,
    pub(super) shortcuts_mode: Signal<bool>,
    pub(super) user_status: Signal<Status>,
    pub(super) status_menu: Signal<bool>,
    pub(super) muted: Signal<bool>,
    pub(super) deafened: Signal<bool>,
    pub(super) live_invite_mode: Signal<bool>,
    pub(super) live_invite_code: Signal<Option<String>>,
    pub(super) live_invite_error: Signal<String>,
}

impl Overlays {
    /// Fresh signals for this group; wiring around them stays in `app_shell`.
    pub(super) fn new() -> Overlays {
        // Profile Settings screen: a "Profile" entry in the user panel opens it as
        // a full-screen overlay (built once, shown/hidden by this signal — the same
        // toggle idiom as the DM rail). No re-mount, no rebuild of the underlying tree.
        let profile_mode: Signal<bool> = signal(false);
        // Notification Settings screen: the user panel's settings gear opens it as
        // a full-screen overlay, the same build-once/toggle idiom as `profile_mode`.
        let notif_mode: Signal<bool> = signal(false);
        // [Feature 10] Notification wiring. `notif_level` (all/mentions/nothing) is
        // the single source of truth shared with the `UserSettings` Notifications
        // tab — seeded from localStorage (falling back to 0 = all) so a user who
        // last picked "Nothing" isn't flooded before re-confirming their choice.
        // The three transient signals drive the top-right in-app toast, read
        // reactively by `notification_toast::notification_toast` (built once at the
        // root). Declared HERE — before the gateway effect below captures them —
        // because Rust requires a value in scope at the point a closure captures
        // it (the effect reads them inside its `move |frame|` arm).
        let notif_level: Signal<u32> = signal(crate::notify::load_mode().unwrap_or(crate::notify::ALL_MESSAGES));
        let notif_toast_body: Signal<String> = signal(String::new());
        let notif_toast_title: Signal<String> = signal(String::new());
        let notif_toast_visible: Signal<bool> = signal(false);
        // Keyboard Shortcuts overlay: the user panel's '?' icon opens it as a
        // full-screen overlay, the same build-once/toggle idiom as `profile_mode`.
        let shortcuts_mode: Signal<bool> = signal(false);
        // The signed-in user's live presence status. A signal (not the `USER`
        // constant) because the profile strip can change it; the member panel
        // renders the user under this status, so a change regroups them there too.
        let user_status: Signal<Status> = signal(Status::Online);
        // The status menu the user panel's avatar opens.
        let status_menu: Signal<bool> = signal(false);
        // Mic-mute / deafen toggles for the user panel's voice-action icon row.
        // Pure local UI state, owned here and passed into `user_panel`; the mic
        // swaps to a slashed glyph when muted, deafen colors the icon red.
        let muted: Signal<bool> = signal(false);
        let deafened: Signal<bool> = signal(false);
        // Live "Invite People" flow (real backend, live mode only): the `USER_PLUS`
        // action in the live channel header opens this overlay, which mints a real
        // invite via `invite::create_invite` against `active_live_server` and shows
        // the resulting code in `InviteLinkDisplay` (whose Copy button now writes
        // the link to the system clipboard). `live_invite_code` holds the freshly
        // minted code (None until created); `live_invite_error` carries a backend
        // rejection message. Built once here, painted last (same idiom as
        // `moderation_region`), inert outside live mode (no header action to open
        // it, and `active_live_server` is None when a mock server is active).
        let live_invite_mode: Signal<bool> = signal(false);
        let live_invite_code: Signal<Option<String>> = signal(None);
        let live_invite_error: Signal<String> = signal(String::new());
        Overlays {
            profile_mode: profile_mode,
            notif_mode: notif_mode,
            notif_level: notif_level,
            notif_toast_body: notif_toast_body,
            notif_toast_title: notif_toast_title,
            notif_toast_visible: notif_toast_visible,
            shortcuts_mode: shortcuts_mode,
            user_status: user_status,
            status_menu: status_menu,
            muted: muted,
            deafened: deafened,
            live_invite_mode: live_invite_mode,
            live_invite_code: live_invite_code,
            live_invite_error: live_invite_error,
        }
    }
}

