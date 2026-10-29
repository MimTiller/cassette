//! `app_shell` section, split out of `app.rs`. Reads the shared state
//! through `ShellState`; returns the elements the final layout places.

use crate::theme;
use super::*;

pub(super) struct MessagesParts {
    pub(super) header: Element,
    pub(super) sent_messages: Element,
    pub(super) typing_row: Element,
}

pub(super) fn build(s: &ShellState) -> MessagesParts {
    let ShellState { active_live_channel, active_live_server, blocked_users, gateway, has_more_earlier, live_at_bottom, live_attachments, live_channels, live_delete_confirm, live_edit_text, live_editing, live_invite_mode, live_messages, live_revision, live_scroll_handle, live_servers, live_typing, live_typing_rev, load_busy, members_pref, picker, pin_visible, pinned_count, pinned_error, pinned_messages, pinned_panel_open, pinned_revision, reaction_rev, reactions, reply_ctx, reply_target, revealed_blocked, translate_key, user_popup, user_profile, .. } = s.clone();
    // --- main view -------------------------------------------------------
    let header_style = StyleRules {
        padding_top: Some(px(10.0)),
        padding_bottom: Some(px(10.0)),
        padding_left: Some(px(16.0)),
        padding_right: Some(px(16.0)),
        border_bottom_width: Some(border_width(1.0)),
        border_bottom_color: Some(col(BORDER)),
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        gap: Some(px(8.0)),
        ..Default::default()
    };
    let hash_style = StyleRules { font_size: Some(px(20.0)), color: Some(col(TEXT_MUTED)), ..Default::default() };
    let title_style = StyleRules { font_size: Some(px(15.0)), font_weight: Some(FontWeight::SemiBold), color: Some(col(TEXT)), ..Default::default() };
    // Small muted-grey icon button (the gavel) that opens the Server
    // Moderation panel. A `Copy` handle on `moderation_open` is captured by
    // its `.on_touch` closure below.
    let moderation_style = StyleRules {
        width: Some(px(24.0)),
        height: Some(px(24.0)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        background: Some(col(COMPOSER)),
        ..rounded(12.0)
    };

    // Live-mode header: a live channel's name has no mock counterpart, so it's
    // looked up from `live_channels` (fetched when the live server was
    // selected). Backend channels have no topic, so the header shows just the
    // channel name. `live_channels` is a `Copy` `Signal`, so
    // capturing it here is cheap; the arm re-runs whenever
    // `active_live_channel` (in the key) changes. `None` falls through to the
    // mock header below, byte-for-byte unchanged.
    // (the mock moderation panel this copy drove was removed with the mock
    // data, so `moderation_mode` isn't read here anymore)
    // Live-mode invite entry: the `USER_PLUS` action in the live header flips
    // this (a clone of `live_invite_mode`, declared above). The live header arm
    // is the only place a real backend `server_id` (`active_live_server`) is
    // in scope, so the invite flow lives here — the mock moderation panel has
    // no real server to mint against.
    let live_invite_open = live_invite_mode.clone();
    // Opening a search result: switch to its channel the way the sidebar
    // does, then let `jump` page back until the message is loaded.
    let on_jump: Rc<dyn Fn(i64, i64)> = Rc::new({
        let live_messages = live_messages.clone();
        let live_scroll_handle = live_scroll_handle.clone();
        let gw = gateway;
        move |channel_id: i64, message_id: i64| {
            if active_live_channel.get() != Some(channel_id) {
                active_live_channel.set(Some(channel_id));
                if let Some(sid) = active_live_server.get() {
                    crate::auth::save_last_view_channel(sid, channel_id);
                }
                *live_scroll_handle.borrow_mut() = None;
                gw.send(discord_server::server_settings::WsClientMsg::History { channel_id, before_id: None });
            }
            live_at_bottom.set(false);
            super::jump::start(super::jump::Jump {
                channel_id,
                message_id,
                live_messages: live_messages.clone(),
                has_more_earlier,
                load_busy,
                live_revision,
                load_older: Rc::new(move |before: i64| {
                    gw.send(discord_server::server_settings::WsClientMsg::History { channel_id, before_id: Some(before) });
                }),
            });
        }
    });
    let header = switch(
        move || {
            (
                active_live_channel.get(),
                pinned_count.get(),
            )
        },
        move |(live_cid, pinned_cnt): &(Option<i64>, i64)| {
            let header_style = header_style.clone();
            let hash_style = hash_style.clone();
            let title_style = title_style.clone();
            let moderation_style = moderation_style.clone();
            let search_server_name = live_servers.get().into_iter().find(|x| Some(x.id) == active_live_server.get()).map(|x| x.name).unwrap_or_default();
            // [Feature 16] Real account with no live channel: don't fall
            // through to a mock header (it would show the demo `SERVERS[0]`
            // channel). Show a neutral onboarding title during the login ->
            // server-fetch race / for a zero-server account.
            match *live_cid {
            None => ui! {
                    view(style = header_style) {
                        { mobile::back_button("mobile-back", || mobile::show(mobile::Pane::List)) }
                        text(style = StyleRules {
                            color: Some(col(TEXT_MUTED)),
                            ..hash_style
                        }) { "#".to_string() }
                        text(style = title_style) { "No channel selected".to_string() }
                    }
                },
            Some(live_id) => {
                let title = live_channels
                    .get()
                    .iter()
                    .find(|c| c.id == live_id)
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                // Hoisted before `ui!` (the macro has no `let`-child form):
                // `pin_live` feeds the pin icon's open handler; `pin_count_label`
                // is the badge's count text (read from the `switch` key's tuple).
                let pin_live = live_id;
                let pin_count_label = pinned_cnt.to_string();
                // The live header early-returns here (before the mock server
                // lookup at the bottom of this `switch` render, which would
                // `unwrap()` a non-mock id and panic in live mode).
                return ui! {
                    view(style = header_style) {
                        { mobile::back_button("mobile-back", || mobile::show(mobile::Pane::List)) }
                        text(style = hash_style) { "#".to_string() }
                        text(style = title_style) { title }
                        // Spacer: eats the row's remaining width so the pin /
                        // invite actions sit at the far right.
                        view(style = StyleRules { flex_grow: Some(grow(1.0)), ..Default::default() }) {}
                        // Pin: opens the "Pinned messages" panel and fetches this
                        // channel's REAL pinned list (the panel's Unpin buttons
                        // and the badge below both read the same signals the
                        // backend round trips update). `pinned_count > 0` is
                        // folded into the `switch` key so the badge count
                        // re-renders when an Unpin lands.
                        view(style = moderation_style.clone(), test_id = "pin-live") {
                            icon(PIN).color(|| theme::live(TEXT_MUTED)).size(15.0)
                        }
                        .on_touch(move |_ev| {
                            let open_p = pinned_panel_open;
                            let msgs_p = pinned_messages;
                            let err_p = pinned_error;
                            let rev_p = pinned_revision;
                            let cnt_p = pinned_count;
                            open_p.set(true);
                            err_p.set(String::new());
                            runtime_core::driver::spawn_async(async move {
                                match crate::pins::list_pinned(pin_live).await {
                                    Ok(list) => {
                                        msgs_p.set(list);
                                        rev_p.set(rev_p.get() + 1);
                                    }
                                    Err(e) => {
                                        err_p.set(e);
                                        rev_p.set(rev_p.get() + 1);
                                    }
                                }
                                // Refresh the badge count in the same round trip
                                // (the backend also returns it on Unpin, but on
                                // open we pull it once so the badge is fresh).
                                if let Ok(c) = crate::pins::pinned_count(pin_live).await {
                                    cnt_p.set(c);
                                }
                            });
                            TouchResponse::CONSUMED
                        })
                        // Real pin count badge (replaces the static mock `3`).
                        // `pinned_cnt` comes from the `switch` key above; a
                        // zero count mounts no node (the runtime `if` lowers to
                        // the `when` primitive). Same pill style as the mock
                        // `PinCountBadge`.
                        if *pinned_cnt > 0 {
                            view(style = StyleRules {
                                flex_direction: Some(FlexDirection::Row),
                                align_items: Some(AlignItems::Center),
                                gap: Some(px(4.0)),
                                padding_left: Some(px(7.0)),
                                padding_right: Some(px(7.0)),
                                padding_top: Some(px(3.0)),
                                padding_bottom: Some(px(3.0)),
                                background: Some(col(theme::BG_HOVER)),
                                ..rounded(8.0)
                            }) {
                                text(style = StyleRules {
                                    font_size: Some(px(13.0)),
                                    color: Some(col(theme::TEXT_STRONG)),
                                    ..Default::default()
                                }) { "📌".to_string() }
                                text(style = StyleRules {
                                    font_size: Some(px(12.0)),
                                    font_weight: Some(FontWeight::SemiBold),
                                    color: Some(col(theme::TEXT_STRONG)),
                                    ..Default::default()
                                }) { pin_count_label }
                            }
                        }
                        // Invite: opens the live "Invite People" overlay, which
                        // mints a real invite for this live server.
                        view(style = moderation_style.clone(), test_id = "invite-live") {
                            icon(USER_PLUS).color(|| theme::live(TEXT_MUTED)).size(15.0)
                        }
                        .on_touch(move |_ev| {
                            live_invite_open.set(true);
                            TouchResponse::CONSUMED
                        })
                        // Show / hide the member list.
                        view(style = moderation_style.clone(), test_id = "toggle-members") {
                            icon(USERS).color(move || theme::live(if super::members_visible(members_pref) { theme::TEXT_STRONG } else { TEXT_MUTED })).size(15.0)
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                members_pref.set(Some(!super::members_visible(members_pref)));
                            }
                            TouchResponse::CONSUMED
                        })
                        { super::search::search_box(active_live_server, search_server_name.clone(), on_jump.clone()) }
                    }
                };
            }
            }
        },
    );

    // Slowmode indicator for the active channel: a channel-keyed `switch` that
    // resolves the current channel's `slowmode_secs` (`Option<u32>`) and feeds
    // it to `SlowmodeIndicator(secs)`. When the active channel has no slowmode
    // (`None`) — or when on a live channel (backend channels have no slowmode
    // column yet) — it passes `0`, and the component renders *nothing* (no
    // pill, no empty node). Only the couple of mock channels with
    // `slowmode_secs: Some(_)` show the pill. Gated per-channel rather than
    // unconditional, so the "every channel shows a timer" anti-pattern (the one
    // that spliced a bare `SlowmodeIndicator()` into the composer column) is
    // gone. `active_server`/`active_channel` are `Copy` signals, so capturing
    // them is cheap; the arm re-runs only on a real channel/server change.
    // Slowmode was a mock-channel-only feature (live backend channels have no
    // slowmode column); deleted with the mock data.

    // Message list — the gateway-fed buffer for the live channel (the mock
    // seed-message render is gone). `sent_read` handled the mock send log
    // below; it's now unused there but kept for the live on_send path.
    let live_read = live_messages.clone();
    // Attachments per wire `message_id` cloned for the row renderer below —
    // the gateway effect stored each frame's list; a row looks its id up.
    let attach_render = live_attachments.clone();
    // Handles for the per-row edit/delete mode (see `live_edit_row`). Cloned so
    // the `sent_messages` closure moves its own copies (the closures below all
    // `move`-capture their handles rather than sharing one non-`Copy` value).
    let live_editing_k = live_editing.clone();
    let live_delete_k = live_delete_confirm.clone();
    // Shared reply/pin handles cloned so the `sent_messages` closure can move
    // its own copies (the action-bar wiring in `message_row_with_actions`).
    let reply_ctx_k = reply_ctx.clone();
    let reply_target_k = reply_target.clone();
    let pin_visible_k = pin_visible.clone();
    let translate_key_k = translate_key.clone();
    // LIVE pin state cloned so the `sent_messages` closure can move its own
    // copies into each live row's real "Pin message" handler (the mock rows
    // instead flash the `pin_visible_k` toast).
    let pinned_msgs_k = pinned_messages.clone();
    let pinned_count_k = pinned_count.clone();
    let pinned_rev_k = pinned_revision.clone();
    let pinned_err_k = pinned_error.clone();
    // The message list. `rows` turns the channel's buffer into plain row
    // data; the keyed `for` renders it. A row's key hashes everything it
    // shows, so a new message, an edit or a reaction rebuilds only the rows
    // it touches and every other row keeps its nodes.
    let popup_ctx = user_popups::UserPopupCtx {
        popup: user_popup,
        profile: user_profile,
        server: active_live_server,
        blocked: blocked_users,
        live_rev: live_revision,
    };
    crate::mentions::set_open_user(Rc::new(move |uid, x, y| popup_ctx.open(uid, x, y, false)));
    let reactions_read = reactions.clone();
    let rows = memo(move || -> Vec<ListRow> {
        let names_rev = crate::mentions::names_version();
        let _ = (live_revision.get(), reaction_rev.get());
        let Some(live_id) = active_live_channel.get() else { return Vec::new() };
        let mine: Vec<LiveMsg> = live_read.borrow().get(&live_id).cloned().unwrap_or_default();
        if mine.is_empty() {
            return Vec::new();
        }
        let now = crate::data::now_secs();
        let self_id = crate::auth::CURRENT_USER_ID.load(std::sync::atomic::Ordering::SeqCst);
        let editing = live_editing_k.get();
        let confirming = live_delete_k.get();
        let jumped = super::jump::target();
        let blocked_now = blocked_users.get();
        let revealed_now = revealed_blocked.get();
        let channel = live_channels
            .get()
            .iter()
            .find(|c| c.id == live_id)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let mut out = vec![ListRow::Top {
            more: has_more_earlier.get(),
            busy: load_busy.get(),
            channel,
            channel_id: live_id,
            oldest: mine.first().map(|m| m.0),
        }];
        // The previous row's author and time (for grouping) and local day
        // (a new day gets a divider and breaks the group).
        let mut prev: Option<(String, i64)> = None;
        let mut prev_day: Option<i64> = None;
        // Messages from blocked users collapse into one row per run.
        let mut hidden_run: Vec<i64> = Vec::new();
        for (mid, aid, author, body, ts, edited_at, reply_to) in mine.iter() {
            if crate::blocks::is_blocked(&blocked_now, *aid) && !revealed_now.contains(mid) {
                hidden_run.push(*mid);
                continue;
            }
            if !hidden_run.is_empty() {
                out.push(ListRow::Hidden(std::mem::take(&mut hidden_run)));
                prev = None;
            }
            let day = local_day_clock(*ts).0;
            let new_day = prev_day != Some(day);
            if new_day {
                out.push(ListRow::Day(format_day_label(*ts)));
            }
            // An edited message or a reply keeps its own header, so neither
            // joins the group above.
            let grouped = prev.as_ref().is_some_and(|(pa, pts)| same_group(pa, *pts, author.as_str(), *ts))
                && edited_at.is_none()
                && !new_day
                && reply_to.is_none();
            let reply = reply_to.and_then(|orig_id| {
                mine.iter()
                    .find(|o| o.0 == orig_id)
                    .map(|o| (o.2.clone(), crate::reply_reference_strip::reply_snippet(&o.3)))
            });
            out.push(ListRow::Message(Box::new(MsgRow {
                id: *mid,
                author_id: *aid,
                author: author.clone(),
                body: body.clone(),
                when: format_when(*ts, now),
                full: format_full_when(*ts),
                edited: edited_at.is_some(),
                mine: self_id != 0 && *aid == self_id,
                grouped,
                reply,
                editing: editing == Some(*mid),
                confirming: confirming == Some(*mid),
                mentions_me: crate::mentions::mentions_me(*mid),
                jumped: jumped == Some(*mid),
                attachments: attach_render.borrow().get(mid).cloned().unwrap_or_default(),
                reactions: reactions_read.borrow().get(&MessageKey::Live { id: *mid }).cloned().unwrap_or_default(),
                names_rev,
            })));
            prev = Some((author.clone(), *ts));
            prev_day = Some(day);
        }
        if !hidden_run.is_empty() {
            out.push(ListRow::Hidden(hidden_run));
        }
        out
    });
    let ctx = RowCtx {
        gateway,
        editing: live_editing,
        edit_text: live_edit_text,
        delete_confirm: live_delete_confirm,
        picker,
        reply_ctx: reply_ctx_k,
        reply_target: reply_target_k,
        pin_visible: pin_visible_k,
        translate_key: translate_key_k,
        pinned_messages: pinned_msgs_k,
        pinned_count: pinned_count_k,
        pinned_revision: pinned_rev_k,
        pinned_error: pinned_err_k,
        reactions: reactions.clone(),
        revealed_blocked,
        live_revision,
        load_busy,
        popup_ctx,
    };
    // Which of the list's three states shows: no channel, an empty channel,
    // or the messages.
    let list_state = memo(move || match active_live_channel.get() {
        None => 0u8,
        Some(_) if rows.get().is_empty() => 1,
        Some(_) => 2,
    });
    let sent_messages = switch(
        move || list_state.get(),
        move |state: &u8| {
            let hint = match *state {
                0 => "Create or join a server to start messaging.",
                1 => "No messages yet -- say hi!",
                _ => {
                    let ctx = ctx.clone();
                    return ui! {
                        view(style = StyleRules {
                            flex_direction: Some(FlexDirection::Column),
                            ..Default::default()
                        }) {
                            for row in rows, key = row.key() {
                                { ctx.render(&row) }
                            }
                        }
                    };
                }
            };
            ui! {
                view(style = StyleRules {
                    flex_grow: Some(grow(1.0)),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    padding_top: Some(px(24.0)),
                    ..Default::default()
                }) {
                    text(style = StyleRules {
                        font_size: Some(px(14.0)),
                        color: Some(col(TEXT_MUTED)),
                        ..Default::default()
                    }) { hint.to_string() }
                }
            }
        },
    );

    // Typing indicator row — sits directly above the composer. In live mode
    // (a real backend channel is open) it renders the latest typer in
    // `live_typing`, keyed on `(active_live_channel, live_typing_rev)`. The
    // mock fallback (server 1, channel 101 = #general, reads "dyno is
    // typing...") is kept for the mock path. Both branches render a row with
    // the same line-box height (4/4/16/16 paddings + 12px font) so the
    // composer's y-offset never jumps on channel switch.
    let typing_live_chan = active_live_channel.clone();
    let typing_rev_for_key = live_typing_rev.clone();
    let typing_map_for_key = live_typing.clone();
    // Typing indicator row: the live gateway typer (the mock "dyno is
    // typing..." fallback was deleted with the mock channel data). Same line
    // box height (4/4/16/16 paddings + 12px font) as before so the composer's
    // y-offset never jumps.
    let typing_row = switch(
        move || (typing_live_chan.get(), typing_rev_for_key.get()),
        move |(live_chan, _rev): &(Option<i64>, u32)| {
            match *live_chan {
            None => ui! {
                    view(style = StyleRules { height: Some(px(0.0)), ..Default::default() }) {}
                },
            Some(live_id) => {
                let label = {
                    let map = typing_map_for_key.borrow();
                    map.get(&live_id).map(|e| e.1.clone())
                };
                if let Some(label) = label {
                    ui! { TypingIndicator(name = label.clone()) }
                } else {
                    ui! {
                        view(style = StyleRules {
                            flex_direction: Some(FlexDirection::Row),
                            align_items: Some(AlignItems::Center),
                            padding_top: Some(px(4.0)),
                            padding_bottom: Some(px(4.0)),
                            padding_left: Some(px(16.0)),
                            padding_right: Some(px(16.0)),
                            ..Default::default()
                        }) {
                            text(style = StyleRules {
                                font_size: Some(px(12.0)),
                                ..Default::default()
                            }) { "".to_string() }
                        }
                    }
                }
            }
            }
        },
    );

    MessagesParts { header, sent_messages, typing_row }
}

/// One row of the message list, as plain data (see `build`'s `rows`).
#[derive(Debug, Clone, PartialEq)]
enum ListRow {
    /// "Load earlier messages", or the start-of-channel anchor.
    Top { more: bool, busy: bool, channel: String, channel_id: i64, oldest: Option<i64> },
    /// A date divider.
    Day(String),
    /// A run of messages from blocked users, collapsed.
    Hidden(Vec<i64>),
    Message(Box<MsgRow>),
}

/// Everything one message row shows.
#[derive(Debug, Clone, PartialEq)]
struct MsgRow {
    id: i64,
    author_id: i64,
    author: String,
    body: String,
    when: String,
    full: String,
    edited: bool,
    mine: bool,
    grouped: bool,
    /// The quoted original (author, snippet) when this is a reply.
    reply: Option<(String, String)>,
    editing: bool,
    confirming: bool,
    mentions_me: bool,
    jumped: bool,
    attachments: Vec<discord_server::server_settings::MessageAttachment>,
    reactions: Vec<super::reactions_ui::Reaction>,
    /// Bumped when names or roles change, so rows redraw them.
    names_rev: u64,
}

impl ListRow {
    /// The keyed list's key: a hash of the whole row, so any change to what
    /// it shows rebuilds it. Rows are unique (messages by id, days by date,
    /// hidden runs by their ids, one top row).
    fn key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        format!("{self:?}").hash(&mut h);
        h.finish()
    }
}

/// The handles rows need for their actions.
#[derive(Clone)]
struct RowCtx {
    gateway: server::UseSocket<discord_server::server_settings::WsServerMsg, discord_server::server_settings::WsClientMsg>,
    editing: Signal<Option<i64>>,
    edit_text: Signal<String>,
    delete_confirm: Signal<Option<i64>>,
    picker: Signal<Option<MessageKey>>,
    reply_ctx: Signal<Option<(String, String)>>,
    reply_target: Signal<Option<i64>>,
    pin_visible: Signal<bool>,
    translate_key: Signal<Option<MessageKey>>,
    pinned_messages: Signal<Vec<crate::pins::PinnedMessage>>,
    pinned_count: Signal<i64>,
    pinned_revision: Signal<u32>,
    pinned_error: Signal<String>,
    reactions: Rc<RefCell<super::reactions_ui::ReactionMap>>,
    revealed_blocked: Signal<Vec<i64>>,
    live_revision: Signal<u32>,
    load_busy: Signal<bool>,
    popup_ctx: user_popups::UserPopupCtx,
}

impl RowCtx {
    fn render(&self, row: &ListRow) -> Element {
        match row {
            ListRow::Top { more: true, channel_id, oldest, .. } => {
                let (busy, gw, cid, oldest) = (self.load_busy, self.gateway, *channel_id, *oldest);
                let on_load = Rc::new(move || {
                    busy.set(true);
                    if let Some(before) = oldest {
                        gw.send(discord_server::server_settings::WsClientMsg::History { channel_id: cid, before_id: Some(before) });
                    }
                });
                load_earlier_button(self.load_busy, on_load)
            }
            ListRow::Top { channel, .. } => channel_anchor(channel),
            ListRow::Day(label) => date_divider(label.clone()),
            ListRow::Hidden(ids) => crate::blocks::hidden_run_row(ids.clone(), self.revealed_blocked, self.live_revision),
            ListRow::Message(m) => self.message(m),
        }
    }

    fn message(&self, m: &MsgRow) -> Element {
        let key = MessageKey::Live { id: m.id };
        let mid = m.id;
        let (pin_msgs, pin_cnt, pin_rev, pin_err) = (self.pinned_messages, self.pinned_count, self.pinned_revision, self.pinned_error);
        let on_pin = Rc::new(move || {
            runtime_core::driver::spawn_async(async move {
                match crate::pins::pin_message(mid).await {
                    Ok(res) => {
                        pin_cnt.set(res.pinned_count);
                        if let Ok(list) = crate::pins::list_pinned(res.channel_id).await {
                            pin_msgs.set(list);
                        }
                    }
                    Err(e) => pin_err.set(e),
                }
                pin_rev.set(pin_rev.get() + 1);
            });
        });
        let row = live_edit_row(
            m.id,
            m.author.as_str(),
            BUTTON,
            m.body.as_str(),
            m.when.clone(),
            m.full.clone(),
            m.edited,
            m.mine,
            m.grouped,
            self.editing,
            self.edit_text,
            self.delete_confirm,
            self.gateway,
            key,
            self.picker,
            self.reply_ctx,
            self.reply_target,
            self.pin_visible,
            on_pin,
            self.translate_key,
            Some(Rc::new(self.popup_ctx.on_press(m.author_id))),
        );
        let row = if m.mentions_me { mention_highlight(row) } else { row };
        let reply = m
            .reply
            .clone()
            .map(|(author, snippet)| crate::reply_reference_strip::reply_reference_strip(author, snippet));
        let attachments = (!m.attachments.is_empty()).then(|| attachment_row(&m.attachments));
        let reactions = reaction_area(key, self.reactions.clone(), self.picker);
        ui! {
            view(style = StyleRules { flex_direction: Some(FlexDirection::Column), ..Default::default() }) {
                if let Some(el) = reply {
                    el
                }
                row
                if let Some(el) = attachments {
                    el
                }
                reactions
            }
        }
    }
}
