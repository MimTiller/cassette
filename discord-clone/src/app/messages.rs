//! `app_shell` section, split out of `app.rs`. Reads the shared state
//! through `ShellState`; returns the elements the final layout places.

use crate::theme;
use super::*;

pub(super) struct MessagesParts {
    pub(super) header: Element,
    pub(super) slowmode_el: Element,
    pub(super) sent_messages: Element,
    pub(super) typing_row: Element,
}

pub(super) fn build(s: &ShellState) -> MessagesParts {
    let ShellState { active_channel, active_live_channel, active_live_server, active_server, blocked_users, gateway, has_more_earlier, is_real_user, live_at_bottom, live_attachments, live_channels, live_delete_confirm, live_edit_text, live_editing, live_invite_mode, live_messages, live_revision, live_servers, live_typing, live_typing_rev, load_busy, members_pref, moderation_mode, picker, pin_visible, pinned_count, pinned_error, pinned_messages, pinned_panel_open, pinned_revision, reaction_rev, reactions, reply_ctx, reply_target, revealed_blocked, revision, sent, translate_key, user_popup, user_profile, .. } = s.clone();
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
    let topic_style = StyleRules { font_size: Some(px(13.0)), color: Some(col(TEXT_MUTED)), max_width: Some(px(220.0)), height: Some(px(18.0)), overflow: Some(Overflow::Hidden), ..Default::default() };
    // Small muted-grey icon button (the gavel) that opens the Server
    // Moderation panel. A `Copy` handle on `moderation_open` is captured by
    // its `.on_touch` closure below.
    let moderation_style = StyleRules {
        width: Some(px(24.0)),
        height: Some(px(24.0)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        background: Some(col(COMPOSER)),
        border_top_left_radius: Some(px(12.0)),
        border_top_right_radius: Some(px(12.0)),
        border_bottom_left_radius: Some(px(12.0)),
        border_bottom_right_radius: Some(px(12.0)),
        ..Default::default()
    };

    // Live-mode header: a live channel's name has no mock counterpart, so it's
    // looked up from `live_channels` (fetched when the live server was
    // selected) and the topic is the static "No topic set yet" (backend
    // channels have no topic column). `live_channels` is a `Copy` `Signal`, so
    // capturing it here is cheap; the arm re-runs whenever
    // `active_live_channel` (in the key) changes. `None` falls through to the
    // mock header below, byte-for-byte unchanged.
    // `app_shell` owns `moderation_mode` and renders the overlay when it flips
    // on; the gavel button in the header below sets this copy.
    let moderation_open = moderation_mode.clone();
    // Live-mode invite entry: the `USER_PLUS` action in the live header flips
    // this (a clone of `live_invite_mode`, declared above). The live header arm
    // is the only place a real backend `server_id` (`active_live_server`) is
    // in scope, so the invite flow lives here — the mock moderation panel has
    // no real server to mint against.
    let live_invite_open = live_invite_mode.clone();
    // The header search's corpus: the active live channel's buffered
    // messages, (author, text). `live_messages` is not reactive, so
    // `live_revision` (bumped on every gateway message or scrollback page)
    // is read to subscribe.
    let search_messages = live_messages.clone();
    let search_corpus = memo(move || -> Vec<(String, String)> {
        let _rev = live_revision.get();
        let Some(cid) = active_live_channel.get() else { return Vec::new() };
        search_messages
            .borrow()
            .get(&cid)
            .map(|msgs| msgs.iter().map(|m| (m.2.clone(), m.3.clone())).collect())
            .unwrap_or_default()
    });
    let header = switch(
        move || {
            (
                active_server.get(),
                active_channel.get(),
                active_live_channel.get(),
                pinned_count.get(),
            )
        },
        move |(server_id, channel_id, live_cid, pinned_cnt): &(u32, u32, Option<i64>, i64)| {
            let header_style = header_style.clone();
            let hash_style = hash_style.clone();
            let title_style = title_style.clone();
            let topic_style = topic_style.clone();
            let moderation_style = moderation_style.clone();
            let search_server_name = live_servers.get().into_iter().find(|x| Some(x.id) == active_live_server.get()).map(|x| x.name).unwrap_or_default();
            // [Feature 16] Real account with no live channel: don't fall
            // through to the mock header (it would show the demo `SERVERS[0]`
            // channel). Show a neutral onboarding title during the login ->
            // server-fetch race / for a zero-server account.
            if is_real_user && live_cid.is_none() {
                return ui! {
                    view(style = header_style) {
                        text(style = StyleRules {
                            color: Some(col(TEXT_MUTED)),
                            ..hash_style
                        }) { "#".to_string() }
                        text(style = title_style) { "No channel selected".to_string() }
                    }
                };
            }
            if let Some(live_id) = *live_cid {
                let title = live_channels
                    .get()
                    .iter()
                    .find(|c| c.id == live_id)
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                let topic = "No topic set yet".to_string();
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
                        text(style = hash_style) { "#".to_string() }
                        text(style = title_style) { title }
                        text(style = topic_style) { topic }
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
                            icon(PIN).color(|| Color::from(TEXT_MUTED)).size(15.0)
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
                                border_top_left_radius: Some(px(8.0)),
                                border_top_right_radius: Some(px(8.0)),
                                border_bottom_left_radius: Some(px(8.0)),
                                border_bottom_right_radius: Some(px(8.0)),
                                ..Default::default()
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
                            icon(USER_PLUS).color(|| Color::from(TEXT_MUTED)).size(15.0)
                        }
                        .on_touch(move |_ev| {
                            live_invite_open.set(true);
                            TouchResponse::CONSUMED
                        })
                        // Show / hide the member list.
                        view(style = moderation_style.clone(), test_id = "toggle-members") {
                            icon(USERS).color(move || Color::from(if super::members_visible(members_pref) { theme::TEXT_STRONG } else { TEXT_MUTED })).size(15.0)
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                members_pref.set(Some(!super::members_visible(members_pref)));
                            }
                            TouchResponse::CONSUMED
                        })
                        MessageSearchBar(corpus = search_corpus.read_only(), server_name = search_server_name.clone())
                    }
                };
            }
            let server = SERVERS.iter().find(|x| x.id == *server_id).unwrap();
            let channel = server.find_text_channel(*channel_id).unwrap();
            let title = channel.name.to_string();
            let topic = channel.topic.to_string();
            ui! {
                view(style = header_style) {
                    text(style = hash_style) { "#".to_string() }
                    text(style = title_style) { title }
                    text(style = topic_style) { topic }
                    // Spacer: eats the row's remaining width so the pin badge
                    // and search field sit at the far right.
                    view(style = StyleRules { flex_grow: Some(grow(1.0)), ..Default::default() }) {}
                    PinCountBadge()
                    // Gavel: opens the Server Moderation panel (a full-screen
                    // overlay, same idiom as the Profile Settings overlay).
                    view(style = moderation_style, test_id = "server-moderation") {
                        icon(GAVEL).color(|| Color::from(TEXT_MUTED)).size(15.0)
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            moderation_open.set(true);
                        }
                        TouchResponse::CONSUMED
                    })
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
    let slowmode_el = switch(
        move || (active_server.get(), active_channel.get(), active_live_channel.get()),
        move |(server_id, channel_id, live_cid): &(u32, u32, Option<i64>)| {
            // Live channels and channels without slowmode resolve to `0` ->
            // the indicator renders nothing (the component's own static
            // `if secs > 0` branch mounts no node for `0`).
            let secs: u32 = if live_cid.is_some() {
                0
            } else {
                SERVERS
                    .iter()
                    .find(|x| x.id == *server_id)
                    .and_then(|s| s.find_text_channel(*channel_id))
                    .and_then(|c| c.slowmode_secs)
                    .unwrap_or(0)
            };
            ui! {
                SlowmodeIndicator(secs = secs)
            }
        },
    );

    // Message list — the channel's seed messages plus any sent here this session.
    // `sent_read` is a dedicated clone so this closure and `on_send` don't both
    // `move`-capture the same non-`Copy` `Rc`. The key includes `revision`
    // because `switch` dedups on scrutinee VALUE — without it, a send (which
    // keeps the same channel) would not rebuild the list.
    let sent_read = sent.clone();
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
    let sent_messages = switch(
        move || (
            active_server.get(),
            active_channel.get(),
            revision.get(),
            active_live_channel.get(),
            live_revision.get(),
            live_editing_k.get(),
            live_delete_k.get(),
            reaction_rev.get(),
            load_busy.get(),
            live_at_bottom.get(),
            has_more_earlier.get(),
        ),
        move |(server_id, channel_id, _revision, live_cid, _live_rev, _editing, _confirm, _react_rev, _load_busy, _at_bottom, _has_more): &(u32, u32, u32, Option<i64>, u32, Option<i64>, Option<i64>, u32, bool, bool, bool)| {
            // [Feature 16] Real account with no live channel: don't fall
            // through to the mock message list (it would render the demo
            // `SERVERS[0]` seed messages). Show a neutral onboarding hint
            // during the login -> server-fetch race / for a zero-server account.
            if is_real_user && live_cid.is_none() {
                return ui! {
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
                        }) { "Create or join a server to start messaging.".to_string() }
                    }
                };
            }
            // Live/real channel active: render the gateway-fed buffer instead of
            // the mock path. `None` falls through to the mock render unchanged.
            if let Some(live_id) = *live_cid {
                let now = crate::data::now_secs();
                let mine: Vec<LiveMsg> = live_read
                    .borrow()
                    .get(&live_id)
                    .cloned()
                    .unwrap_or_default();
                if mine.is_empty() {
                    // No history and nothing received yet: a centered, muted
                    // hint instead of a blank pane. The wrapper `flex_grow`s to
                    // fill the (tall) scroll pane, then centers the single line.
                    return ui! {
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
                            }) { "No messages yet -- say hi!".to_string() }
                        }
                    };
                }
                let self_id = crate::auth::CURRENT_USER_ID.load(std::sync::atomic::Ordering::SeqCst);
                let editing = live_editing;
                let delete_c = live_delete_confirm;
                let edit_text = live_edit_text;
                let gw = gateway;
                let picker_l = picker;
                let reply_l = reply_ctx_k;
                let trans_l = translate_key_k;
                let mut rows: Vec<Element> = Vec::with_capacity(mine.len() * 2);
                // Top-of-list scrollback affordance: a "Load earlier messages"
                // pill while more older history exists (`has_more_earlier`), or
                // a "beginning of channel" anchor once the reader has reached
                // the start (nothing older left to load). Both sit in-flow
                // above the oldest row — the reader scrolls up to find them
                // (NOT a floating pill, so no `live_at_bottom` gating). The
                // channel name resolves from `live_channels` (the same lookup
                // the header title uses). `mine` is oldest-first, so `mine[0]`
                // is the scrollback cursor for the next `History` page.
                let oldest_id = mine.first().map(|m| m.0);
                let chan_name: String = live_channels
                    .get()
                    .iter()
                    .find(|c| c.id == live_id)
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                if has_more_earlier.get() {
                    // `on_load` sets `load_busy` (gates the pill's tap + mutes
                    // its label) and fires the scrollback page. `load_busy` is
                    // cleared by the gateway effect on the `History` marker
                    // frame, which arrives before the page's `Message` frames.
                    let on_load = Rc::new({
                        let busy = load_busy.clone();
                        let cid2 = live_id;
                        let oldest = oldest_id;
                        move || {
                            busy.set(true);
                            if let Some(before) = oldest {
                                gw.send(discord_server::server_settings::WsClientMsg::History {
                                    channel_id: cid2,
                                    before_id: Some(before),
                                });
                            }
                        }
                    });
                    rows.push(load_earlier_button(load_busy.clone(), on_load));
                } else {
                    rows.push(channel_anchor(&chan_name));
                }
                // Message-grouping state carried across iterations (the
                // buffer is oldest-first): the previous row's author label +
                // timestamp, for the `same_group` test below.
                let mut prev: Option<(String, i64)> = None;
                // Local calendar day of the previous message — a new day gets
                // a `date_divider` row above the message and breaks any
                // continuation group (the first row also has `None`, so the
                // first loaded message gets a divider too).
                let mut prev_day: Option<i64> = None;
                let popup_ctx = user_popups::UserPopupCtx {
                    popup: user_popup,
                    profile: user_profile,
                    server: active_live_server,
                    blocked: blocked_users,
                    live_rev: live_revision,
                };
                // Messages from blocked users collapse into one "N Blocked
                // Messages — Show" row per consecutive run, unless revealed.
                let blocked_now = blocked_users.get();
                let revealed_now = revealed_blocked.get();
                let mut hidden_run: Vec<i64> = Vec::new();
                for m in mine.iter() {
                    let (mid, aid, author, body, ts, edited_at, reply_to) = m;
                    if crate::blocks::is_blocked(&blocked_now, *aid) && !revealed_now.contains(mid) {
                        hidden_run.push(*mid);
                        continue;
                    }
                    if !hidden_run.is_empty() {
                        rows.push(crate::blocks::hidden_run_row(
                            std::mem::take(&mut hidden_run),
                            revealed_blocked,
                            live_revision,
                        ));
                        prev = None;
                    }
                    let when = format_when(*ts, now);
                    let full = format_full_when(*ts);
                    let is_mine = self_id != 0 && *aid == self_id;
                    let lkey = MessageKey::Live { id: *mid };
                    // Day divider: when this row's local calendar day differs
                    // from the previous row's (or there is no previous row —
                    // the first loaded message), a date-divider row sits
                    // above it and any continuation group is broken.
                    let cur_day = local_day_clock(*ts).0;
                    let new_day = prev_day != Some(cur_day);
                    if new_day {
                        rows.push(date_divider(format_day_label(*ts)));
                    }
                    // Grouped with the row above? (same author, within the
                    // window). Edited messages keep their own header: an
                    // `(edited)` marker on a headerless row would be
                    // orphaned, so `edited_at.is_some()` breaks the group.
                    // A reply also always starts a new group: its reference
                    // strip sits above the row's header, which would be
                    // orphaned (and the strip stranded mid-group) otherwise.
                    let grouped = prev
                        .as_ref()
                        .map(|(pa, pts)| same_group(pa, *pts, author.as_str(), *ts))
                        .unwrap_or(false)
                        && edited_at.is_none()
                        && !new_day
                        && reply_to.is_none();
                    // Feature 5: a row with a `reply_to_message_id` gets the
                    // quoted reference strip rendered above it. The original's
                    // author + body are resolved from the already-loaded buffer
                    // (`mine`); if it isn't there yet (not loaded / deleted)
                    // there is nothing to quote, so no strip.
                    if let Some(orig_id) = *reply_to {
                        if let Some(orig) = mine.iter().find(|o| o.0 == orig_id) {
                            rows.push(crate::reply_reference_strip::reply_reference_strip(
                                orig.2.clone(),
                                crate::reply_reference_strip::reply_snippet(&orig.3),
                            ));
                        }
                    }
                    // Real "Pin message": persist via the backend `pin_message`
                    // RPC (moderator+), then update the header badge from the
                    // result's fresh `pinned_count` and refresh the panel's
                    // list (the result carries `channel_id`, so no extra id is
                    // needed). A rejection (e.g. a plain member) lands in
                    // `pinned_error` and bumps `pinned_revision` so the panel
                    // body surfaces it. `mid2` + the `Copy` pin signals are
                    // captured by the `move` handler so it owns them.
                    let mid2 = *mid;
                    let pin_msgs = pinned_msgs_k;
                    let pin_cnt = pinned_count_k;
                    let pin_rev = pinned_rev_k;
                    let pin_err = pinned_err_k;
                    let pin_c = Rc::new(move || {
                        runtime_core::driver::spawn_async(async move {
                            match crate::pins::pin_message(mid2).await {
                                Ok(res) => {
                                    pin_cnt.set(res.pinned_count);
                                    if let Ok(list) = crate::pins::list_pinned(res.channel_id).await {
                                        pin_msgs.set(list);
                                    }
                                    pin_rev.set(pin_rev.get() + 1);
                                }
                                Err(e) => {
                                    pin_err.set(e);
                                    pin_rev.set(pin_rev.get() + 1);
                                }
                            }
                        });
                    });
                    rows.push(live_edit_row(
                        *mid,
                        author.as_str(),
                        BUTTON,
                        body.as_str(),
                        when,
                        full,
                        edited_at.is_some(),
                        is_mine,
                        grouped,
                        editing,
                        edit_text,
                        delete_c,
                        gw,
                        lkey,
                        picker_l,
                        reply_l,
                        reply_target_k,
                        pin_visible_k,
                        pin_c,
                        trans_l,
                        Some(Rc::new(popup_ctx.on_press(*aid))),
                    ));
                    // Image attachments render UNDER the body: the persisted
                    // list the gateway effect stored for this wire id (empty
                    // rows push nothing, so every other row is unchanged).
                    let atts = attach_render.borrow().get(&*mid).cloned().unwrap_or_default();
                    if !atts.is_empty() {
                        rows.push(attachment_row(&atts));
                    }
                    // Reaction strip under each live row, keyed by the real wire
                    // `message_id` — the same session-local pill store the mock
                    // rows use, so live reactions behave exactly like mock ones.
                    rows.push(reaction_area(
                        lkey,
                        reactions.clone(),
                        reaction_rev,
                        picker_l,
                    ));
                    prev = Some((author.clone(), *ts));
                    prev_day = Some(cur_day);
                }
                if !hidden_run.is_empty() {
                    rows.push(crate::blocks::hidden_run_row(hidden_run, revealed_blocked, live_revision));
                }
                return ui! {
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Column),
                        ..Default::default()
                    }) { rows }
                };
            }
            let server = SERVERS.iter().find(|x| x.id == *server_id).unwrap();
            let channel = server.find_text_channel(*channel_id).unwrap();
            let cid = *channel_id;
            let now = crate::data::now_secs();
            let mut rows: Vec<Element> = Vec::with_capacity(channel.messages.len());
            // Message-grouping state carried across both loops (seed then
            // this session's sends — oldest-first): the previous row's author
            // + timestamp for the `same_group` test.
            let mut prev: Option<(&str, i64)> = None;
            for (i, m) in channel.messages.iter().enumerate() {
                let ts = m.ts_secs(now);
                let when = format_when(ts, now);
                // Mock seed messages carry no edit state: pass `false` so they
                // don't show a fake "(edited)" tag (the live path uses
                // `edited_at.is_some()`).
                let grouped = prev
                    .map(|(pa, pts)| same_group(pa, pts, m.author, ts))
                    .unwrap_or(false);
                // MOCK pin: flash the "Message pinned!" toast and auto-dismiss
                // it after a beat (the pre-backend idiom, now caller-supplied
                // since `message_row_with_actions` takes `on_pin` as a `Rc`).
                let pv = pin_visible_k;
                let on_pin = Rc::new(move || {
                    pv.set(true);
                    let pv_d = pv;
                    after_ms_detached(2_500, move || pv_d.set(false));
                });
                rows.push(message_row_with_actions(
                    m.author,
                    m.color,
                    m.body,
                    when,
                    format_full_when(ts),
                    false,
                    grouped,
                    MessageKey::Mock { cid, idx: i },
                    picker,
                    reply_ctx_k,
                    reply_target_k,
                    pin_visible_k,
                    translate_key_k,
                    on_pin,
                    None,
                    None,
                    None,
                ));
                // Reaction strip under each mock row, keyed by this row's
                // rendered-list position (seed messages first, so index `i`).
                rows.push(reaction_area(
                    MessageKey::Mock { cid, idx: i },
                    reactions.clone(),
                    reaction_rev,
                    picker,
                ));
                prev = Some((m.author, ts));
            }
            let mine: Vec<(u32, &'static str, String, i64)> = sent_read.borrow().clone();
            let mine: Vec<(u32, &'static str, String, i64)> = mine
                .into_iter()
                .filter(|(mcid, _, _, _)| *mcid == cid)
                .collect();
            for (i, (_mcid, author, body, ts)) in mine.iter().enumerate() {
                let when = format_when(*ts, now);
                // Own prior sends get the same hover bar as the seed rows. `false`
                // for edited -- these are freshly composed, never-edited sends.
                let grouped = prev
                    .map(|(pa, pts)| same_group(pa, pts, author, *ts))
                    .unwrap_or(false);
                // MOCK pin: flash the toast + auto-dismiss (see the seed loop
                // above — the same caller-supplied `on_pin` idiom).
                let pv = pin_visible_k;
                let on_pin = Rc::new(move || {
                    pv.set(true);
                    let pv_d = pv;
                    after_ms_detached(2_500, move || pv_d.set(false));
                });
                rows.push(message_row_with_actions(
                    author,
                    BUTTON,
                    body.as_str(),
                    when,
                    format_full_when(*ts),
                    false,
                    grouped,
                    MessageKey::Mock { cid, idx: channel.messages.len() + i },
                    picker,
                    reply_ctx_k,
                    reply_target_k,
                    pin_visible_k,
                    translate_key_k,
                    on_pin,
                    None,
                    None,
                    None,
                ));
                // Sent rows continue the list position after the seed rows, so
                // their key is `seed_len + i` (see `ReactionMap` doc).
                rows.push(reaction_area(
                    MessageKey::Mock { cid, idx: channel.messages.len() + i },
                    reactions.clone(),
                    reaction_rev,
                    picker,
                ));
                prev = Some((author, *ts));
            }
            ui! {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Column),
                    ..Default::default()
                }) { rows }
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
    let typing_row = switch(
        move || {
            let live = typing_live_chan.get();
            let (server, channel) = (active_server.get(), active_channel.get());
            // The key includes the live revision so the row re-renders when a
            // Typing frame arrives or the auto-clear fires. The mock key is
            // `(server, channel)` (unchanged from the previous shape) — for
            // live channels both are irrelevant (the arm below keys on the
            // live channel + rev).
            (server, channel, live, typing_rev_for_key.get())
        },
        move |(server_id, channel_id, live_chan, _rev): &(u32, u32, Option<i64>, u32)| {
            // [Feature 16] Real account with no live channel: render an empty
            // spacer (no mock typing indicator) during the login ->
            // server-fetch race / for a zero-server account.
            if is_real_user && live_chan.is_none() {
                return ui! {
                    view(style = StyleRules { height: Some(px(0.0)), ..Default::default() }) {}
                };
            }
            // Live mode: show the typer from the gateway, or the empty
            // spacer if nobody is typing right now.
            if let Some(live_id) = *live_chan {
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
            } else if *server_id == 1 && *channel_id == 101 {
                // `TypingIndicator`'s own root view already carries the row
                // paddings (4/4/16/16) + a 12px line, so it is mounted bare —
                // no wrapping view (that would double the padding). A `ui!`
                // block with a single component child returns that child's
                // `Element` directly.
                ui! { TypingIndicator(name = "dyno".to_string()) }
            } else {
                // Empty spacer matching the indicator's exact line box: same
                // paddings + a 12px (empty) text node, so the composer's
                // y-offset never jumps when leaving #general.
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
        },
    );

    MessagesParts { header, slowmode_el, sent_messages, typing_row }
}
