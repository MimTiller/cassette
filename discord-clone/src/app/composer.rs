//! `app_shell` section, split out of `app.rs`. Reads the shared state
//! through `ShellState`; returns the elements the final layout places.

use crate::theme;
use super::*;
use crate::gif_picker::{GifPicker, gif_button};

pub(super) struct ComposerParts {
    pub(super) composer_area: Element,
}

pub(super) fn build(s: &ShellState, slowmode_el: Element) -> ComposerParts {
    let ShellState { active_channel, active_live_channel, active_server, attach_busy, attach_error, attach_next_key, composer, emoji_open, gateway, gif_open, live_channels, live_messages, pending_attachment, reply_ctx, reply_target, revision, send_failed, sent, .. } = s.clone();
    // --- composer --------------------------------------------------------
    let composer_style = StyleRules {
        padding_top: Some(px(12.0)),
        padding_bottom: Some(px(12.0)),
        padding_left: Some(px(16.0)),
        padding_right: Some(px(16.0)),
        border_top_width: Some(border_width(1.0)),
        border_top_color: Some(col(BORDER)),
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        gap: Some(px(8.0)),
        ..Default::default()
    };
    let input_style = StyleRules {
        flex_grow: Some(grow(1.0)),
        height: Some(px(40.0)),
        padding_left: Some(px(12.0)),
        padding_right: Some(px(12.0)),
        background: Some(col(COMPOSER)),
        color: Some(col(TEXT)),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        ..borderless_input()
    };
    // Circular muted-grey icon buttons flanking the composer input — the '+'
    // attachment affordance on the left (stages/unstages a mock attachment),
    // the smile emoji affordance on the right (toggles the emoji popover).
    let icon_button_style = StyleRules {
        width: Some(px(28.0)),
        height: Some(px(28.0)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        background: Some(col(COMPOSER)),
        border_top_left_radius: Some(px(14.0)),
        border_top_right_radius: Some(px(14.0)),
        border_bottom_left_radius: Some(px(14.0)),
        border_bottom_right_radius: Some(px(14.0)),
        ..Default::default()
    };

    // One send action, fired by the Enter key on the composer (Discord has no
    // visible Send button — Enter is the only submit affordance). Every captured
    // signal/Rc is cloned behind an `Rc` so the handler can be `Fn` (callable
    // from the `text_input on_key_down` closure).
    let send_composer = composer.clone();
    let sent_write = sent.clone();
    let revision_bump = revision.clone();
    // Live-send handles: `gateway` is a `Copy` `UseSocket`, so clone it in so
    // the (shared) `move` handler can send over the global socket. Only used
    // when `active_live_channel` is `Some`; otherwise the mock path runs.
    let live_chan = active_live_channel.clone();
    let gateway_send = gateway;
    let do_send = move || {
        let body = send_composer.get();
        let attached = pending_attachment.get();
        // A message must carry a body OR at least one staged image — the
        // same "not both empty" gate the backend enforces on the `Send`
        // frame, mirrored client-side so a no-op send never round-trips.
        if body.trim().is_empty() && attached.is_empty() {
            return;
        }
        // Live/real mode: stage the pending images as the wire frame's
        // `AttachmentInput` list — the data URLs were already validated +
        // encoded client-side (`stage_attachment`), and the backend
        // re-validates and persists them with the message row.
        let inputs: Vec<discord_server::server_settings::AttachmentInput> = attached
            .iter()
            .map(|(_, filename, mime, url)| discord_server::server_settings::AttachmentInput {
                filename: filename.clone(),
                mime: mime.clone(),
                data: url.clone(),
            })
            .collect();
        // Live/real mode: send over the gateway socket to the live channel.
        // The server echoes it back as a `Message` frame, which the incoming
        // effect buffers and bumps `live_revision`, rebuilding the list.
        crate::app::request_scroll_to_latest();
        if let Some(live_cid) = live_chan.get() {
            gateway_send.send(discord_server::server_settings::WsClientMsg::Send {
                channel_id: live_cid,
                content: body.clone(),
                reply_to_message_id: reply_target.get(),
                attachments: inputs,
            });
            send_composer.set(String::new());
            pending_attachment.set(Vec::new());
            reply_ctx.set(None);
            reply_target.set(None);
            return;
        }
        // Mock path: the per-session log carries only a body string, so
        // staged images ride it as `[attachment: <name>]` lines (the
        // pre-backend affordance; live mode sends real data URLs).
        let out_body = if attached.is_empty() {
            body
        } else {
            let names: Vec<String> = attached.iter().map(|(_, filename, _, _)| filename.clone()).collect();
            format!("{}\n[attachment: {}]", body, names.join(", "))
        };
        let cid = active_channel.get();
        sent_write.borrow_mut().push((cid, "Nova", out_body, crate::data::now_secs()));
        send_composer.set(String::new());
        pending_attachment.set(Vec::new());
        // Bump the revision so the message-list `switch` rebuilds (its key
        // dedups on value, so re-setting the same channel alone would not).
        revision_bump.set(revision_bump.get() + 1);
    };
    let on_send = Rc::new(do_send);
    // Robot test hook (`send.now`): Enter on the composer is a raw keyboard
    // event the robot bridge cannot synthesize, so an E2E test drives the
    // exact action Enter invokes — the same `on_send` `Rc` the `on_key_down`
    // handler calls — body + staged attachments included.
    runtime_vocabulary::robot_only! {
        register_send_now_hook(on_send.clone());
    };
    // Enter on the composer → send. Plain Enter only; Shift/Ctrl/Alt/Meta+
    // Enter fall through to the platform default. `PreventDefault` stops the
    // input's default Enter handling (no newline / no stray submit) now that
    // we've taken the action.
    let send_key = on_send.clone();
    let on_enter = move |ev: &KeyEvent| {
        if ev.key == "Enter" && !ev.shift && !ev.ctrl && !ev.alt && !ev.meta {
            (send_key)();
            KeyOutcome::PreventDefault
        } else {
            KeyOutcome::Default
        }
    };

    // --- Composer placeholder: "Message #<channel>" (Discord's real format,
    // channel-suffixed)
    // Derives the active channel's display name (same source of truth as the
    // header: the live backend's channel list when on a live channel, else the
    // mock server's text-channel table) and prefixes it with "Message #".
    // A `memo` keeps it reactive: switching channels re-derives the string
    // without the composer's `text_input` needing to rebuild its own node.
    // The `placeholder_reactive` prop wants `Reactive<Option<String>>`, so the
    // memo yields `Some("Message #<name>")` (a `None` placeholder would be
    // pointless — there is always an active channel).
    let ph_server = active_server.clone();
    let ph_channel = active_channel.clone();
    let ph_live = active_live_channel.clone();
    let ph_channels = live_channels.clone();
    let composer_placeholder = memo(move || {
        let name = if let Some(live_id) = ph_live.get() {
            ph_channels
                .get()
                .iter()
                .find(|c| c.id == live_id)
                .map(|c| c.name.to_string())
                .unwrap_or_default()
        } else {
            let server = SERVERS.iter().find(|x| x.id == ph_server.get()).unwrap();
            server
                .find_text_channel(ph_channel.get())
                .map(|c| c.name.to_string())
                .unwrap_or_default()
        };
        Some(format!("Message #{name}"))
    });

    // --- @-mention autocomplete (live mode) ------------------------------
    // Pure client-side: typing `@<prefix>` in the composer opens a dropdown
    // of the active server's members whose name starts with the prefix
    // (case-insensitive). The roster is derived from the already-buffered
    // gateway messages (every distinct author label that has posted) — no
    // backend call. `ac` mirrors the active query + highlight so both the key
    // handler and the dropdown's click handler can share it (the closures
    // must all be `Fn`).
    let ac: Rc<RefCell<(String, i64)>> = Rc::new(RefCell::new((String::new(), 0)));
    let suggestions: Signal<Vec<String>> = signal(Vec::new());
    let ac_hi: Signal<i64> = signal(0);
    let ac_sugg = suggestions.clone();
    let ac_hi_sig = ac_hi.clone();
    let ac_state = ac.clone();
    let composer_sig = composer.clone();
    let live_channels_sig = live_channels.clone();
    let live_messages_sig = live_messages.clone();
    // Recompute the filtered suggestion list from the current composer text +
    // active server roster, and reset the highlight to 0. Held in an `Rc` so
    // `update_on_type` can share it (closures are not `Clone`, `Rc` is `Copy`).
    let recompute = Rc::new(move || {
        let text = composer_sig.get();
        let q = mention_active_query(&text);
        let (members, list) = match &q {
            None => (String::new(), Vec::new()),
            Some(q) => {
                let members = if crate::mentions::server_id().is_some() {
                    crate::mentions::completion_pool()
                } else {
                    live_member_roster(&live_channels_sig.get(), &live_messages_sig.borrow())
                };
                let list = filter_members(q, members);
                (q.clone(), list)
            }
        };
        *ac_state.borrow_mut() = (members.clone(), 0);
        ac_hi_sig.set(0);
        ac_sugg.set(list);
    });
    let recompute2 = recompute.clone();
    // Throttle for the outbound Typing frame: channel_id -> epoch-millis of the
    // last Typing frame sent while typing in that channel. At most one per ~3 s
    // per channel, so a fast typer doesn't flood the gateway. Plain
    // `Rc<RefCell<HashMap>>` — no reactivity needed (it only gates the side
    // effect below, never renders anything).
    let typing_last_sent_h: Rc<RefCell<HashMap<i64, i64>>> =
        Rc::new(RefCell::new(HashMap::new()));
    let typing_chan_h = active_live_channel.clone();
    let typing_gateway_h = gateway; // `UseSocket` is `Copy`
    // Update on every keystroke: recompute from the (already-updated) value,
    // and — in live mode with actual text — emit a throttled Typing frame so
    // other members' clients see "… is typing…". The server fans that out to
    // the channel audience (minus the sender), so this is the client half of
    // the typing indicator; the inbound half is the `Typing` arm in the
    // gateway effect above.
    let update_on_type = move |s: String| {
        // Throttled outbound Typing frame: at most one per channel per ~3 s,
        // and only while there is actual text. Evaluated BEFORE
        // `composer_sig.set(s)` because `set` moves `s` — the check only borrows
        // it via `trim()`. The server fans the frame out to the channel
        // audience (minus the sender), so this is the client half; the inbound
        // half is the `Typing` arm in the gateway effect above.
        let open_live = typing_chan_h.get();
        if !s.trim().is_empty() {
            if let Some(cid) = open_live {
                let now_ms = runtime_shared::time::epoch_millis();
                let too_soon = typing_last_sent_h
                    .borrow()
                    .get(&cid)
                    .map(|last| now_ms - *last < 3_000)
                    .unwrap_or(false);
                if !too_soon {
                    typing_gateway_h
                        .send(discord_server::server_settings::WsClientMsg::Typing {
                            channel_id: cid,
                        });
                    typing_last_sent_h.borrow_mut().insert(cid, now_ms);
                }
            }
        }
        composer_sig.set(s);
        (recompute2)();
    };
    // Select a suggestion: replace the partial `@partial` with
    // `@fullusername ` (truncating at the active `@`) and close the dropdown.
    // Held in an `Rc` so both the key handler and the row click handler can
    // share it. Rc is Clone but NOT Copy, so each `move` closure that needs
    // its own handle gets a fresh `.clone()` from the original `ac`/`apply`
    // right before capture -- the earlier `ac_state` binding above was
    // already consumed by `recompute`'s closure.
    let ac_state = ac.clone();
    let apply = Rc::new(move |username: &str| {
        let mut text = composer_sig.get();
        if let Some(at) = text.rfind('@') {
            text.truncate(at);
        }
        text.push('@');
        text.push_str(username);
        text.push(' ');
        composer_sig.set(text);
        *ac_state.borrow_mut() = (String::new(), 0);
        ac_hi_sig.set(0);
        ac_sugg.set(Vec::new());
    });
    // Layered key handler: while the dropdown is open, Enter/Tab select the
    // highlighted suggestion (swallowing the key so it does NOT send the
    // message), arrows move the highlight, and Escape closes; everything else
    // falls through to the platform default (which still routes Enter to send
    // when the dropdown is closed).
    let ac_state = ac.clone();
    let apply_for_key = apply.clone();
    let ac_key = move |ev: &KeyEvent| {
        let (q, hi) = ac_state.borrow().clone();
        let open = !q.is_empty() || !ac_sugg.get().is_empty();
        if !open {
            return KeyOutcome::Default;
        }
        match ev.key.as_str() {
            "Enter" | "Tab" => {
                let list = ac_sugg.get().clone();
                match list.get(hi as usize) {
                    Some(u) => {
                        (apply_for_key)(u.as_str());
                        KeyOutcome::PreventDefault
                    }
                    None => {
                        // Highlight out of range: close the dropdown and let
                        // Enter send (Tab: swallow).
                        *ac_state.borrow_mut() = (String::new(), 0);
                        ac_hi_sig.set(0);
                        ac_sugg.set(Vec::new());
                        if ev.key == "Tab" {
                            KeyOutcome::PreventDefault
                        } else {
                            KeyOutcome::Default
                        }
                    }
                }
            }
            "ArrowDown" | "ArrowUp" => {
                let count = ac_sugg.get().len() as i64;
                if count > 0 {
                    let n = if ev.key == "ArrowDown" {
                        (hi + 1) % count
                    } else {
                        (hi - 1 + count) % count
                    };
                    ac_state.borrow_mut().1 = n;
                    ac_hi_sig.set(n);
                }
                KeyOutcome::PreventDefault
            }
            "Escape" => {
                *ac_state.borrow_mut() = (String::new(), 0);
                ac_hi_sig.set(0);
                ac_sugg.set(Vec::new());
                KeyOutcome::PreventDefault
            }
            _ => KeyOutcome::Default,
        }
    };
    // The dropdown element: absolutely positioned above the composer input,
    // showing only when there is an active query with matching members.
    let ac_render = ac.clone();
    let ac_hi_r = ac_hi.clone();
    let apply_touch = apply.clone();
    let ac_el = switch(
        move || suggestions.get(),
        move |list: &Vec<String>| {
            let q = ac_render.borrow().0.clone();
            let hi = ac_hi_r.get();
            if q.is_empty() {
                return ui! {
                    view(style = StyleRules { height: Some(px(0.0)), ..Default::default() }) {}
                };
            }
            let rows: Vec<Element> = list
                .iter()
                .enumerate()
                .map(|(i, name)| {
                    let on = (i as i64) == hi;
                    let hint = crate::mentions::completion_hint(name);
                    let n = name.clone();
                    // Fresh per-row clone: `apply_touch` is an `Rc`, not `Copy`,
                    // and this `.map()` runs once per suggestion row, so each
                    // row's `move` closure below needs its own handle (the
                    // same rule documented in components/connection_quality_
                    // indicator.rs for its per-bar StyleRules).
                    let apply_touch = apply_touch.clone();
                    ui! {
                        view(style = StyleRules {
                            height: Some(px(34.0)),
                            padding_left: Some(px(14.0)),
                            padding_right: Some(px(14.0)),
                            background: Some(col(if on { BG_ACTIVE } else { "transparent" })),
                            flex_direction: Some(FlexDirection::Row),
                            align_items: Some(AlignItems::Center),
                            ..Default::default()
                        }) {
                            text(style = StyleRules {
                                font_size: Some(px(14.0)),
                                color: Some(col(if on { TEXT } else { TEXT_CHANNEL })),
                                ..Default::default()
                            }) { format!("@{}", name) }
                            view(style = StyleRules { flex_grow: Some(grow(1.0)), ..Default::default() }) {}
                            if let Some(h) = hint {
                                text(style = StyleRules {
                                    font_size: Some(px(12.0)),
                                    color: Some(col(TEXT_MUTED)),
                                    ..Default::default()
                                }) { h.to_string() }
                            }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                (apply_touch)(n.as_str());
                            }
                            TouchResponse::CONSUMED
                        })
                    }
                })
                .collect();
            ui! {
                view(style = StyleRules {
                    position: Some(Position::Absolute),
                    left: Some(px(16.0)),
                    right: Some(px(16.0)),
                    bottom: Some(px(60.0)),
                    max_height: Some(px(190.0)),
                    background: Some(col(BG_SIDEBAR)),
                    border_top_left_radius: Some(px(8.0)),
                    border_top_right_radius: Some(px(8.0)),
                    border_bottom_left_radius: Some(px(8.0)),
                    border_bottom_right_radius: Some(px(8.0)),
                    overflow: Some(Overflow::Hidden),
                    ..Default::default()
                }) {
                    scroll_view(style = StyleRules {
                        flex_grow: Some(grow(1.0)),
                        ..Default::default()
                    }) { rows }
                }
            }
        },
    );

    // --- staged-attachment chip + emoji popover --------------------------
    // Both are pure-local-UI affordances for the two composer icon buttons
    // (`+` = stage/unstage a mock attachment, smile = toggle the emoji grid).
    // They are positioned siblings of the composer inside the same
    // `Position::Relative` ancestor that already holds `ac_el`, so both anchor
    // correctly: the chip is a normal (in-flow) row just above the input and
    // collapses to zero height when nothing is staged, while the emoji popover
    // is `Position::Absolute` floating just above the composer.

    // Staged-attachment strip: the composer's pending images, each a small
    // thumbnail cell with an 'x' that removes just that entry (the send
    // consumes the whole list). `switch` on the staged `Vec<PendingImage>`
    // rebuilds on every stage/removal/send; empty renders zero height.
    // `strip_pending` is a `Copy` handle on `pending_attachment`; the nested
    // `on_touch` `move` closure gets a fresh per-render copy (`rm`) so it
    // never moves the outer closure's handle (E0507 pattern from `chip_clear`).

    // Attach flow: open the file picker (multi-image) and stage every picked
    // file through `stage_attachment` — the SAME validate + encode the
    // avatar-upload picker callback runs (`profile_settings`), against the
    // message-attachment limits. Rejections (bad type / oversize / over
    // count) land in the visible `attach_error` line below the composer.
    // Same busy/error/picker triple idiom as the profile screen's avatar
    // upload; the '+' button calls it from its `on_touch`.
    let do_attach = move || {
        if attach_busy.get() {
            return;
        }
        let busy = attach_busy;
        let err = attach_error;
        let pending = pending_attachment;
        let nextk = attach_next_key;
        busy.set(true);
        err.set(String::new());
        runtime_core::driver::spawn_async(async move {
            let outcome = match FilePicker::new()
                .pick(PickRequest::media(MediaKind::Images).multiple())
                .await
            {
                Ok(outcome) => outcome,
                Err(e) => {
                    err.set(format!("file picker: {e}"));
                    busy.set(false);
                    return;
                }
            };
            match outcome {
                PickOutcome::Picked(files) => {
                    for file in files.iter() {
                        match file.read_all().await {
                            Ok(bytes) => stage_attachment(
                                pending.clone(),
                                nextk.clone(),
                                err.clone(),
                                file.name().to_string(),
                                file.mime().to_string(),
                                &bytes,
                            ),
                            Err(e) => err.set(format!("could not read the file: {e}")),
                        }
                    }
                }
                // Cancelled (dismissed picker): back to idle, not an error.
                _ => {}
            }
            busy.set(false);
        });
    };

    // Register the robot test hook so an MCP E2E can stage a composer
    // attachment through the same `stage_attachment` validate → encode path
    // the file-picker callback above runs — without driving the native file
    // dialog (which no in-page tool can tap). Rebuilt per screen open; a
    // no-op outside robot builds.
    runtime_vocabulary::robot_only! {
        register_attach_stage_hook(pending_attachment.clone(), attach_next_key.clone(), attach_error.clone());
    };

    let strip_pending = pending_attachment;
    let attach_chip = switch(
        move || pending_attachment.get(),
        move |list: &Vec<PendingImage>| {
            if list.is_empty() {
                return ui! {
                    view(style = StyleRules { height: Some(px(0.0)), ..Default::default() }) {}
                };
            }
            let cells: Vec<Element> = list.iter().map(|(key, filename, _mime, url)| {
                let k = *key;
                let rm = strip_pending;
                ui! {
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        gap: Some(px(6.0)),
                        ..Default::default()
                    }) {
                        crate::profile_settings::attachment_thumbnail(signal(crate::auth::media_url(url.clone())).into(), filename.clone(), 56.0)
                        view(style = StyleRules {
                            width: Some(px(16.0)),
                            height: Some(px(16.0)),
                            align_items: Some(AlignItems::Center),
                            justify_content: Some(JustifyContent::Center),
                            border_top_left_radius: Some(px(8.0)),
                            border_top_right_radius: Some(px(8.0)),
                            border_bottom_left_radius: Some(px(8.0)),
                            border_bottom_right_radius: Some(px(8.0)),
                            ..Default::default()
                        }) {
                            text(style = StyleRules {
                                font_size: Some(px(12.0)),
                                color: Some(col(TEXT_MUTED)),
                                ..Default::default()
                            }) { "✕".to_string() }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                let mut staged = rm.get();
                                staged.retain(|(entry_key, _, _, _)| *entry_key != k);
                                rm.set(staged);
                            }
                            TouchResponse::CONSUMED
                        })
                    }
                }
            }).collect();
            ui! {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Column),
                    margin_top: Some(px(8.0)),
                    margin_bottom: Some(px(8.0)),
                    margin_left: Some(px(16.0)),
                    margin_right: Some(px(16.0)),
                    gap: Some(px(8.0)),
                    ..Default::default()
                }) { cells }
            }
        },
    );

    // Emoji popover: `switch` on `emoji_open`. Closed = a zero-height spacer
    // (so it never occupies layout); open = a dark rounded panel floating just
    // above the composer, holding a wrapped grid of clickable glyph cells.
    // Clicking a glyph appends it to the composer text and closes the popover.
    let emoji_open_sig = emoji_open;
    let composer_e = composer;
    let emoji_popover = switch(
        move || emoji_open.get(),
        move |open: &bool| {
            if !open {
                return ui! {
                    view(style = StyleRules { height: Some(px(0.0)), ..Default::default() }) {}
                };
            }
            let c = composer_e;
            let eo = emoji_open_sig;
            let pick_cb: Rc<dyn Fn(String)> = Rc::new(move |glyph: String| {
                let mut t = c.get();
                t.push_str(&glyph);
                c.set(t);
                eo.set(false);
            });
            ui! {
                view(style = StyleRules {
                    position: Some(Position::Absolute),
                    right: Some(px(16.0)),
                    bottom: Some(px(76.0)),
                    z_index: Some(super::popups::POPUP_Z),
                    ..Default::default()
                }) {
                    EmojiPicker(on_pick = Some(pick_cb))
                }
            }
        },
    );

    // GIF popover: the same float as the emoji popover. Picking a GIF sends
    // it at once as its own message (the draft stays in the composer), as a
    // reply when one is active. Live channels only; the mock path has no
    // gateway to send through.
    let gif_popover = switch(
        move || gif_open.get(),
        move |open: &bool| {
            if !open {
                return ui! {
                    view(style = StyleRules { height: Some(px(0.0)), ..Default::default() }) {}
                };
            }
            let pick_cb: Rc<dyn Fn(discord_server::gifs::Gif)> = Rc::new(move |gif: discord_server::gifs::Gif| {
                gif_open.set(false);
                let Some(live_cid) = active_live_channel.get() else { return };
                crate::app::request_scroll_to_latest();
                gateway.send(discord_server::server_settings::WsClientMsg::Send {
                    channel_id: live_cid,
                    content: discord_server::gifs::gif_message(&gif),
                    reply_to_message_id: reply_target.get(),
                    attachments: Vec::new(),
                });
                reply_ctx.set(None);
                reply_target.set(None);
            });
            ui! {
                view(style = StyleRules {
                    position: Some(Position::Absolute),
                    right: Some(px(16.0)),
                    bottom: Some(px(76.0)),
                    z_index: Some(super::popups::POPUP_Z),
                    ..Default::default()
                }) {
                    GifPicker(on_pick = Some(pick_cb))
                }
            }
        },
    );

    // Layer the autocomplete key handler in front of the send (`on_enter`)
    // handler: autocomplete runs first and returns `Default` whenever the
    // dropdown is closed, so the send path still receives the key otherwise.
    let ac_key2 = ac_key.clone();
    let send_enter = on_enter.clone();
    let composer_key = move |ev: &KeyEvent| {
        let r = (ac_key2)(ev);
        if r != KeyOutcome::Default {
            return r;
        }
        (send_enter)(ev)
    };


    // The composer column: built from the send handlers (enter/stop/close)
    // defined in the closures above.
    let composer_area = ui! {
        view(style = StyleRules {
            position: Some(Position::Relative),
            flex_direction: Some(FlexDirection::Column),
            ..Default::default()
        }) {
            // "Failed to send - Retry" banner: UI chrome directly above
            // the composer (NOT in the message list). Reads the
            // `send_failed` signal — set when the backend answers a
            // send with an `Error` frame — and clears it on tap (the
            // user resends to retry). While unset it renders nothing.
            SendFailedIndicator(failed = send_failed.clone())
            // "Replying to <author>: <snippet>" strip: shows while a
            // message's Reply action is active (the per-row action bar
            // sets `reply_ctx`), directly above the composer. Its "×"
            // clears the same signal the bar reads. Zero-size (nothing
            // rendered) while no reply is active.
            reply_preview(reply_ctx, reply_target)
            attach_chip
            emoji_popover
            gif_popover
            ac_el
            // Per-channel slowmode indicator (gated): the channel-keyed
            // `slowmode_el` above resolves the active channel's
            // `slowmode_secs` and renders the pill only for
            // slowmode-enabled channels — nothing for the rest. Same
            // home b60 dropped the bare, unconditional
            // `SlowmodeIndicator()` into; now it reads real state.
            slowmode_el
            // Slash-command autocomplete: shown only while the composer
            // text actually starts with "/" (a real slash command is
            // being typed), not on every render. `composer` is the
            // live text signal.
            if composer.get().starts_with('/') {
                SlashCommandDropdown()
            }
            view(style = composer_style) {
                view(style = icon_button_style.clone()) {
                    icon(PLUS).color(|| theme::live(TEXT_MUTED)).size(16.0)
                }
                // `+` = open the file picker and stage a real image
                // attachment (up to the shared per-message cap). The
                // strip above the composer mirrors the staged list.
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        do_attach();
                    }
                    TouchResponse::CONSUMED
                })
                text_input(value = composer, on_change = update_on_type, placeholder_reactive = composer_placeholder.read_only(), style = input_style, test_id = "composer").on_key_down(composer_key)
                // GIF = open/close the GIF picker (closes the emoji one,
                // like the reference app). Chip shared with the DM composer.
                gif_button(gif_open, Some(emoji_open))
                view(style = icon_button_style) {
                    icon(SMILE).color(|| theme::live(TEXT_MUTED)).size(16.0)
                }
                // Smile = open/close the emoji popover (a `Copy` handle
                // on `emoji_open`, mirroring the attachment button).
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        gif_open.set(false);
                        emoji_open.set(!emoji_open.get());
                    }
                    TouchResponse::CONSUMED
                })
            }
            // Visible rejection line for the attach flow: a bad type
            // / oversize / over-count pick (or a picker error) shows
            // here in the app's error red; empty = nothing rendered.
            if !attach_error.get().is_empty() {
                text(style = StyleRules {
                    font_size: Some(px(12.0)),
                    color: Some(col(BADGE)),
                    margin_left: Some(px(16.0)),
                    ..Default::default()
                }) { move || attach_error.get().clone() }
            }
        }
    };
    ComposerParts { composer_area }
}
