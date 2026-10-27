//! `app_shell` section, split out of `app.rs`. Reads the shared state
//! through `ShellState`; returns the elements the final layout places.

use super::*;

pub(super) struct GatewayEffectsParts {
    pub(super) gateway_status: Element,
    pub(super) gateway_incoming_effect: Element,
    pub(super) mention_reset_effect: Element,
}

pub(super) fn build(s: &ShellState) -> GatewayEffectsParts {
    let ShellState { active_live_channel, active_live_server, friends_snap, gateway, has_more_earlier, live_attachments, live_delete_confirm, live_edit_text, live_editing, live_members, live_mention_rev, live_mentioned, live_messages, live_presence, live_presence_rev, live_revision, live_typing, live_typing_rev, live_unread, live_unread_rev, load_busy, notif_level, notif_toast_body, notif_toast_title, notif_toast_visible, reaction_rev, reactions, send_failed, .. } = s.clone();
    // Realtime gateway: the signed session token authenticates the connect
    // (the backend verifies it). Declared above (before the auto-enter
    // effect) so that effect can send the restored channel's initial
    // `History` frame — see the note there.
    // Compact single-line status label for the 72px rail: "● Open" etc.
    // (the wide `messaging: {:?}` form clipped/wrapped). The label is
    // reactive: `switch` re-runs the arm whenever the status string
    // changes, so the rail tracks Connecting -> Open -> Closed/Error live.
    // Per-status color: green Open, amber Connecting, red Closed/Error —
    // inline hex like the rest of the rail's status accents.
    let gateway_status = switch(
        move || format!("{:?}", gateway.status()),
        move |status: &String| {
            let (label, color) = match status.as_str() {
                "Open" => ("Open", "#23a55a"),
                "Connecting" => ("Connecting", "#f0b232"),
                // Closed and Error share the red accent.
                _ => ("Closed", "#f23f43"),
            };
            // The status dot: a small filled `view` circle in the same 11px
            // shape as the member-list presence dots (Lucide's `circle` is an
            // outline, so a plain colored view reads better at this size).
            let dot_style = StyleRules {
                width: Some(px(11.0)),
                height: Some(px(11.0)),
                background: Some(col(color)),
                border_top_left_radius: Some(px(6.0)),
                border_top_right_radius: Some(px(6.0)),
                border_bottom_left_radius: Some(px(6.0)),
                border_bottom_right_radius: Some(px(6.0)),
                ..Default::default()
            };
            let status_style = StyleRules {
                font_size: Some(px(10.0)),
                color: Some(col(color)),
                text_align: Some(TextAlign::Center),
                ..Default::default()
            };
            ui! {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    gap: Some(px(6.0)),
                    ..Default::default()
                }) {
                    view(style = dot_style) {}
                    text(style = status_style) { label.to_string() }
                }
            }
        },
    );

    // Connection-lost banner for the message column. Keyed on the same reactive
    // status string as `gateway_status` above, but rendered as a full-width red
    // bar at the top of the message pane (NOT the rail dot). It shows only when
    // the socket is `Closed` or `Error`; while `Open` / `Connecting` it renders
    // a 0-height view (nothing), so the layout is unchanged in the healthy
    // state. The socket auto-reconnects (`message_gateway` owns the loop), so
    // this is a passive "you're disconnected" indicator, not a retry button.
    let _conn_lost_banner = switch(
        move || format!("{:?}", gateway.status()),
        move |status: &String| {
            let lost = status == "Closed" || status == "Error";
            if !lost {
                // Healthy: render an inert 0-height view (the no-op-render
                // idiom) so the switch's value is a valid `Element` and the
                // layout is unchanged.
                return ui! {
                    view(style = StyleRules {
                        height: Some(px(0.0)),
                        ..Default::default()
                    }) {}
                };
            }
            ui! {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    gap: Some(px(8.0)),
                    padding_top: Some(px(8.0)),
                    padding_bottom: Some(px(8.0)),
                    background: Some(col("#f23f43")),
                    ..Default::default()
                }) {
                    text(style = StyleRules {
                        font_size: Some(px(13.0)),
                        font_weight: Some(FontWeight::SemiBold),
                        color: Some(col("#ffffff")),
                        ..Default::default()
                    }) { "Connection lost — reconnecting…".to_string() }
                }
            }
        },
    );

    // Reactive "watch every frame" effect: `switch` re-runs its arm whenever
    // frame. This one runs a SIDE EFFECT (absorb the message into `live_messages`
    // and bump `live_revision`), not a render — so it returns an inert 0-height
    // `view` (the no-op-render idiom; the element itself is never seen, mounted
    // in the rail below). Only `Message` frames are buffered; `History` replies
    // surface as a run of `Message { new: false }` frames, which land here too.
    // `created_at` arrives as epoch MILLISECONDS from the server; the client's
    // `format_when`/`now_secs` work in SECONDS, so divide by 1000 on ingest.
    let live_buf = live_messages.clone();
    let live_rev = live_revision.clone();
    let mentioned_buf = live_mentioned.clone();
    let mention_rev = live_mention_rev.clone();
    let open_chan = active_live_channel.clone();
    let unread_live = live_unread.clone();
    let unread_live_rev = live_unread_rev.clone();
    let my_id = crate::auth::CURRENT_USER_ID.load(std::sync::atomic::Ordering::SeqCst);
    let typing_buf = live_typing.clone();
    let typing_rev = live_typing_rev.clone();
    // A send failure: an `Error` frame from the backend (the server answers a
    // failed `Send` / `EditMessage` / `DeleteMessage` on this socket) sets
    // `send_failed` so the "Failed to send - Retry" banner above the composer
    // appears. A successful send is echoed as a `Message` frame instead, which
    // is handled above — so only a genuine failure flips this.
    let send_failed_sig = send_failed.clone();
    // Scrollback markers: cleared/set by the `WsServerMsg::History` marker frame
    // the server sends before each page (see the `History` arm below). Cloned so
    // the `move` closure owns them (same idiom as `send_failed_sig`).
    let load_busy_sig = load_busy.clone();
    let has_more_sig = has_more_earlier.clone();
    let presence_buf = live_presence.clone();
    let presence_rev = live_presence_rev.clone();
    // Reaction handles cloned so the gateway effect's `move` closure owns them:
    // history `Message` frames restore persisted pills, and the `ReactionAdd`
    // / `ReactionRemove` broadcast frames update them live for every viewer.
    let reactions_map = reactions.clone();
    let reaction_rev_sig = reaction_rev.clone();
    // Persisted/live attachments cloned so the gateway effect can store each
    // message frame's `attachments` list for the row renderer below.
    let attach_buf = live_attachments.clone();
    // [Feature 14] A `MemberChanged` frame (another account joined or was
    // kicked/banned) re-fetches the active server's roster — the same
    // `list_members` call the kick/ban/role-change callbacks use — so the
    // live member panel's rows AND its "MEMBERS — N" header update for a
    // remote change (the pre-fix gap: those only ever re-fetched on the
    // local user's own action). Captured as clones so the `move` render
    // closure owns them, mirroring `presence_buf` / `presence_rev` above.
    let member_server = active_live_server.clone();
    let member_roster = live_members.clone();
    let notif_level_sig = notif_level.clone();
    let notif_toast_sig = notif_toast_visible.clone();
    let notif_toast_title_sig = notif_toast_title.clone();
    let notif_toast_body_sig = notif_toast_body.clone();
    let gateway_incoming_effect = switch(
        move || gateway.incoming().get(),
        move |frame: &Option<discord_server::server_settings::WsServerMsg>| {
            if let Some(discord_server::server_settings::WsServerMsg::Message {
                message_id,
                channel_id,
                author_id,
                author_username,
                author_avatar,
                content,
                created_at,
                edited_at,
                new,
                mentioned_user_ids,
                reply_to_message_id,
                reactions,
                attachments,
            }) = frame
            {
                // The server resolved the author's username for the frame
                // (`WsServerMsg::Message.author_username`), so the row shows a
                // real name instead of an id. `author_id` still rides along for
                // the "@me / self" checks below. The author's avatar rides the
                // same frame and lands in the shared registry, so the row's
                // avatar chip and the member list can draw it.
                let label = author_username.clone();
                crate::auth::remember_avatar(label.as_str(), author_avatar.clone());
                // Push the full 6-tuple: the wire `message_id` + the optional
                // `edited_at` (epoch millis -> secs, like `created_at`) ride
                // along so the row's edit affordance and "(edited)" marker can
                // find the message later. History frames (`new: false`) may
                // carry a non-None `edited_at` for messages already edited
                // before this session, so store it as-is.
                //
                // Sorted-insert (not a blind `.push`): the buffer is kept
                // (created_at_secs, message_id) ascending so a scrollback page —
                // strictly older than the loaded tail — stacks in at the front,
                // while a live frame (the newest) sorts to the end, all through
                // the same write path. Duplicate `message_id`s are skipped.
                let changed: bool; // set exactly once by the block below
                {
                    let mut buf = live_buf.borrow_mut();
                    let v = buf.entry(*channel_id).or_default();
                    changed = insert_live_msg(
                        v,
                        (
                            *message_id,
                            *author_id,
                            label,
                            content.clone(),
                            *created_at / 1000,
                            edited_at.map(|ms| ms / 1000),
                            *reply_to_message_id,
                        ),
                    );
                }
                // Bump the revision only when a message actually landed — a pure
                // dedup (re-delivery) is a no-op that must not force a rebuild.
                if changed {
                    live_rev.set(live_rev.get() + 1);
                }
                // Persist the frame's attachments for the row renderer (the
                // same frames — history pages AND live fan-out — carry the
                // persisted `MessageAttachment` lists from the server).
                if !attachments.is_empty() {
                    attach_buf.borrow_mut().insert(*message_id, attachments.clone());
                }
                // Live @mention indicator: a freshly posted (`new: true`) message
                // that names the signed-in user, on a channel that is NOT the one
                // currently open, bumps that channel's unread-mention count.
                // History frames (`new: false`) never count as unread.
                if *new
                    && my_id != 0
                    && mentioned_user_ids.contains(&my_id)
                    && open_chan.get() != Some(*channel_id)
                {
                    *mentioned_buf.borrow_mut().entry(*channel_id).or_insert(0) += 1;
                    mention_rev.set(mention_rev.get() + 1);
                }
                // Real unread counts: a freshly posted (`new: true`) message on a
                // channel that is NOT currently open adds one unread there (and
                // one unread mention when it names the signed-in user — the
                // `unread_mentions` invariant is `0..=count`, so both counters
                // move together). History frames (`new: false`) are the
                // initial-load backlog, already reflected by the `unread_counts`
                // fetch — never double-counted here. Opening the channel resets
                // the entry to `(0, 0)` (the effect below), so a frame landing
                // after the reset just re-adds it.
                if *new && open_chan.get() != Some(*channel_id) {
                    let mut map = unread_live.borrow_mut();
                    let entry = map.entry(*channel_id).or_insert((0, 0));
                    entry.0 += 1;
                    if my_id != 0 && mentioned_user_ids.contains(&my_id) {
                        entry.1 += 1;
                    }
                    drop(map);
                    unread_live_rev.set(unread_live_rev.get() + 1);
                }
                // [Feature 10] Notification: a freshly posted (`new: true`)
                // message that (a) matches the notification level (always for
                // `ALL_MESSAGES`, only when it names me for `MENTIONS_ONLY`,
                // never for `NOTHING`) and (b) lands on a channel that is NOT
                // currently open fires an in-app toast (always, so the user
                // sees it in the app) plus a browser `Notification` when the
                // Notification API is granted. History frames (`new: false`)
                // are the initial-load backlog and never notify. The toast
                // carries the message content + author; the browser
                // notification carries a short line. The channel name is not
                // readily available in this scope (it lives in a separate
                // map), so the toast uses the message content itself, which
                // is the most useful part.
                let is_mention = my_id != 0 && mentioned_user_ids.contains(&my_id);
                let level = notif_level_sig.get();
                let matches = match level {
                    crate::notify::NOTHING => false,
                    crate::notify::MENTIONS_ONLY => is_mention,
                    _ => true, // ALL_MESSAGES (0) or any future value
                };
                if *new && matches && open_chan.get() != Some(*channel_id) {
                    crate::notify::notify_message(
                        author_username,
                        content,
                        is_mention,
                        level,
                        notif_toast_sig,
                        notif_toast_title_sig,
                        notif_toast_body_sig,
                    );
                }
                // Cold-load persistence: a history frame (`new: false`) carries
                // the message's persisted reactions, so restore them into the
                // live pill map keyed by the wire `message_id` (`reacted_by_me`
                // came relative to the socket's own user, resolved server-side
                // from the verified token). Fresh-post echoes (`new: true`) are
                // empty — their later reaction changes ride the frames below.
                if !*new && !reactions.is_empty() {
                    let key = MessageKey::Live { id: *message_id };
                    let map_h = reactions_map.clone();
                    let mut m = map_h.borrow_mut();
                    let inner = m.entry(key).or_default();
                    for (emoji, count, reacted) in reactions {
                        if let Some(p) = inner.iter().position(|(e, _, _)| e == emoji) {
                            inner[p] = (emoji.clone(), *count, *reacted);
                        } else {
                            inner.push((emoji.clone(), *count, *reacted));
                        }
                    }
                    if m.get(&key).map_or(false, |v| v.is_empty()) {
                        m.remove(&key);
                    }
                    drop(m);
                    reaction_rev_sig.set(reaction_rev_sig.get() + 1);
                }
            }
            // A member — including the caller via its own echo — added or
            // removed an emoji reaction. The frame carries the message's
            // post-operation total for the emoji, so apply it authoritatively
            // (SET, not increment): duplicate frames and a cold-load race
            // converge. `count == 0` drops the pill. The highlight flips only
            // on a frame naming THIS client's user; other members' frames keep
            // whatever highlight the pill already has. Bumping `reaction_rev`
            // rebuilds the live list, so every connected client sees it.
            if let Some(discord_server::server_settings::WsServerMsg::ReactionAdd {
                message_id,
                channel_id,
                user_id,
                emoji,
                count,
            }) = frame
            {
                let _ = channel_id;
                let key = MessageKey::Live { id: *message_id };
                let reacted_hint = if *user_id == my_id { Some(true) } else { None };
                if apply_reaction(reactions_map.clone(), key, emoji.clone(), *count, reacted_hint) {
                    reaction_rev_sig.set(reaction_rev_sig.get() + 1);
                }
            }
            if let Some(discord_server::server_settings::WsServerMsg::ReactionRemove {
                message_id,
                channel_id,
                user_id,
                emoji,
                count,
            }) = frame
            {
                let _ = channel_id;
                let key = MessageKey::Live { id: *message_id };
                let reacted_hint = if *user_id == my_id { Some(false) } else { None };
                if apply_reaction(reactions_map.clone(), key, emoji.clone(), *count, reacted_hint) {
                    reaction_rev_sig.set(reaction_rev_sig.get() + 1);
                }
            }
            // A channel message the signed-in user edited: replace the stored
            // content + `edited_at` for `message_id` so the row shows the new
            // text and its "(edited)" marker (the marker reads `edited_at` is
            // `Some`). Mutating the buffer alone would not rebuild the list, so
            // bump `live_revision` (the list `switch`'s key) to re-render.
            if let Some(discord_server::server_settings::WsServerMsg::MessageEdited {
                message_id,
                channel_id,
                content,
                edited_at,
            }) = frame
            {
                let channel_id = *channel_id;
                let message_id = *message_id;
                let edited_secs = *edited_at / 1000;
                let changed = live_buf
                    .borrow_mut()
                    .get_mut(&channel_id)
                    .map(|msgs| {
                        msgs.iter_mut()
                            .find(|m| m.0 == message_id)
                            .map(|m| {
                                m.3 = content.clone();
                                m.5 = Some(edited_secs);
                            })
                            .is_some()
                    })
                    .unwrap_or(false);
                // Also clear a pending edit-mode/confirm for this message so the
                // row drops back to its normal (edited) rendering rather than
                // staying in an input that no longer matches the server.
                if changed {
                    if live_editing.get() == Some(message_id) {
                        live_editing.set(None);
                        live_edit_text.set(String::new());
                    }
                    if live_delete_confirm.get() == Some(message_id) {
                        live_delete_confirm.set(None);
                    }
                    live_rev.set(live_rev.get() + 1);
                }
            }
            // A channel message was deleted: drop its stored entry so it
            // disappears from the list. The `MessageDeleted` frame is the server's
            // authoritative confirmation, so no client-side optimistic removal is
            // needed — the row vanishes once this lands.
            if let Some(discord_server::server_settings::WsServerMsg::MessageDeleted {
                message_id,
                channel_id,
            }) = frame
            {
                let channel_id = *channel_id;
                let message_id = *message_id;
                let changed = live_buf
                    .borrow_mut()
                    .get_mut(&channel_id)
                    .map(|msgs| {
                        let before = msgs.len();
                        msgs.retain(|m| m.0 != message_id);
                        msgs.len() != before
                    })
                    .unwrap_or(false);
                // Clear any pending edit/delete state for the now-gone message so
                // a stale input/confirm can't outlive it.
                if changed {
                    if live_editing.get() == Some(message_id) {
                        live_editing.set(None);
                        live_edit_text.set(String::new());
                    }
                    if live_delete_confirm.get() == Some(message_id) {
                        live_delete_confirm.set(None);
                    }
                    live_rev.set(live_rev.get() + 1);
                }
            }
            // A presence change for a user on a shared server: record
            // `user_id -> status` in `live_presence` and bump `live_presence_rev`
            // so the live member panel's `switch` re-renders (the row's dot
            // updates). The backend sends 'online' on socket auth + activity,
            // 'away' after the idle timeout, and 'offline' on the user's last
            // socket disconnect — so every status we can receive is a real
            // server value (no client-invented states). `frame` is a `&WsServerMsg`
            // borrowed for this one call, so copy the fields out to owned values.
            if let Some(discord_server::server_settings::WsServerMsg::Presence {
                user_id,
                status,
            }) = frame
            {
                let user_id = *user_id;
                let status = status.clone();
                presence_buf.borrow_mut().insert(user_id, status);
                presence_rev.set(presence_rev.get() + 1);
            }
            // Typing frame from another member of the channel: record who is
            // typing in `live_typing` and bump `live_typing_rev` so the
            // typing_row `switch` re-renders. The auto-clear task self-cancels
            // on a newer frame (generation guard) — a fresh frame restarts the
            // 5-second window without clearing the still-visible indicator.
            if let Some(discord_server::server_settings::WsServerMsg::Typing {
                channel_id,
                user_id,
            }) = frame
            {
                // `frame` is a `&WsServerMsg` borrowed for this one call, so
                // copy the fields out to owned `i64`s — the auto-clear closure
                // below outlives `frame` and cannot borrow into it.
                let channel_id = *channel_id;
                let user_id = *user_id;
                let label = format!("user {}", user_id);
                let new_gen = {
                    let mut map = typing_buf.borrow_mut();
                    let entry = map
                        .entry(channel_id)
                        .or_insert((user_id, label.clone(), 0));
                    entry.0 = user_id;
                    entry.1 = label;
                    entry.2 += 1;
                    entry.2
                };
                typing_rev.set(typing_rev.get() + 1);
                // Schedule a one-shot auto-clear. The closure captures CLONES
                // of the shared handles (not the outer `Fn` closure's own
                // `typing_buf`/`typing_rev`, which it must keep using on every
                // frame). After 5 s it clears the entry only if no newer Typing
                // frame has arrived (the stored `gen` still equals `new_gen`).
                let clear_buf = typing_buf.clone();
                let clear_rev = typing_rev.clone();
                after_ms_detached(5_000, move || {
                    let should_clear = {
                        let map = clear_buf.borrow();
                        map.get(&channel_id).map(|e| e.2 == new_gen).unwrap_or(false)
                    };
                    if should_clear {
                        clear_buf.borrow_mut().remove(&channel_id);
                        clear_rev.set(clear_rev.get() + 1);
                    }
                });
            }
            // A member joined or left the server (another account redeemed an
            // invite, or was kicked/banned by someone else). Re-fetch the
            // active server's roster when the frame names it — the SAME
            // `list_members` call the kick/ban/role-change callbacks use — so
            // the live member panel's rows AND its "MEMBERS — N" header
            // reflect the change (the pre-fix gap: the roster only ever
            // re-fetched on the local user's own action). `user_id` /
            // `joined` are carried for completeness (a future
            // "just joined" / "just left" transition could use them), but the
            // re-fetch is the single source of truth, so it's the whole of the
            // handling here. `frame` is a `&WsServerMsg`, so copy the id out.
            if let Some(discord_server::server_settings::WsServerMsg::MemberChanged {
                server_id,
                ..
            }) = frame
            {
                let server_id = *server_id;
                if member_server.get() == Some(server_id) {
                    let sid = server_id;
                    let roster = member_roster.clone();
                    runtime_core::driver::spawn_async(async move {
                        if let Ok(fresh) = crate::moderation::list_members(sid).await {
                            roster.set(fresh);
                        }
                    });
                }
            }
            // A friends-list change involving the peer of one of THIS user's
            // relationships (the backend pushes `FriendChanged { friend_id,
            // action }` to the *other* user on every send/accept/decline/
            // cancel/remove): re-fetch `list_friends` and set the panel
            // snapshot — the same single-source-of-truth re-fetch idiom as the
            // `MemberChanged` branch above. The snapshot IS the panel's `switch`
            // key, so the rows rebuild with no extra revision counter. The
            // acting user re-syncs from their RPC return instead (in
            // `friends_view`), so they never wait on this frame.
            if let Some(discord_server::server_settings::WsServerMsg::FriendChanged { .. }) = frame {
                let snap_sig = friends_snap.clone();
                runtime_core::driver::spawn_async(
                    async move {
                        if let Ok(list) = crate::friends::list_friends().await {
                            snap_sig.set(crate::friends_view::snapshot(list));
                        }
                    },
                );
            }
            // Scrollback marker: the server sends a `History { channel_id,
            // has_more }` frame BEFORE the `Message { new: false }` frames of a
            // page. `has_more` is `true` when the page was a full `limit` (50),
            // meaning more older messages exist. Use it to clear the in-flight
            // guard and update the "Load earlier" pill's visibility. This arm
            // must run before the `Message` frames land (it arrives first on the
            // wire), so it sets state before the list rebuilds.
            if let Some(discord_server::server_settings::WsServerMsg::History {
                channel_id,
                has_more,
            }) = frame
            {
                let cid = *channel_id;
                let active = open_chan.get();
                // Only update scrollback state for the currently open channel —
                // a stale page for a previously open channel must not clobber
                // the new channel's state.
                if active == Some(cid) {
                    load_busy_sig.set(false);
                    has_more_sig.set(*has_more);
                }
            }
            // A backend error (a rejected `Send` / `EditMessage` / `DeleteMessage`
            // — see `message_gateway`, which answers failures on this socket):
            // set `send_failed` so the "Failed to send - Retry" banner above the
            // composer shows. A successful send is echoed as a `Message` frame
            // (handled above), so only a real failure flips this.
            if let Some(discord_server::server_settings::WsServerMsg::Error { .. }) = frame {
                send_failed_sig.set(true);
            }
            ui! {
                view(style = StyleRules {
                    height: Some(px(0.0)),
                    ..Default::default()
                }) {}
            }
        },
    );

    // Reset a channel's unread-mention count the moment it's opened, so the
    // red badge disappears as soon as the user reads it. This lives here as a
    // side-effect `switch` keyed on the active live channel (rather than in the
    // per-row click handler) because that handler can't mutate the shared `Rc`
    // mention buffer out of the sidebar's `FnMut` builder. Bumping
    // `live_mention_rev` forces the sidebar `switch` to re-read the now-zero
    // count and drop the badge.
    let mention_reset_buf = live_mentioned.clone();
    let mention_reset_rev = live_mention_rev.clone();
    let unread_reset_buf = live_unread.clone();
    let unread_reset_rev = live_unread_rev.clone();
    let msgs_for_mark_read = live_messages.clone();
    let mention_reset_effect = switch(
        move || active_live_channel.get(),
        move |chan: &Option<i64>| {
            if let Some(cid) = *chan {
                if *mention_reset_buf.borrow().get(&cid).unwrap_or(&0) > 0 {
                    mention_reset_buf.borrow_mut().insert(cid, 0);
                    mention_reset_rev.set(mention_reset_rev.get() + 1);
                }
                // Real backend read-state: mark this channel read up to its
                // most recent message (the newest buffered wire id, which is
                // the latest loaded history entry or live frame so far — the
                // `live_messages` buffer is oldest-first). The RPC is
                // fire-and-forget (a rejection is a backend auth issue, not a
                // UI state to surface here), and the local unread count is
                // cleared OPTIMISTICALLY right away: opening a channel has
                // read its buffered messages by definition, so the badge must
                // drop immediately rather than waiting for the ack.
                if *unread_reset_buf.borrow().get(&cid).unwrap_or(&(0, 0)) != (0, 0) {
                    unread_reset_buf
                        .borrow_mut()
                        .insert(cid, (0, 0));
                    unread_reset_rev.set(unread_reset_rev.get() + 1);
                }
                if let Some(last_id) = msgs_for_mark_read
                    .borrow()
                    .get(&cid)
                    .and_then(|msgs| msgs.last())
                    .map(|m| m.0)
                {
                    runtime_core::driver::spawn_async(async move {
                        let _ = crate::servers::mark_read(cid, last_id).await;
                    });
                }
            }
            ui! {
                view(style = StyleRules {
                    height: Some(px(0.0)),
                    ..Default::default()
                }) {}
            }
        },
    );

    GatewayEffectsParts { gateway_status, gateway_incoming_effect, mention_reset_effect }
}
