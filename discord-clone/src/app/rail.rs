//! `app_shell` section, split out of `app.rs`. Reads the shared state
//! through `ShellState`; returns the elements the final layout places.

use super::*;

pub(super) struct RailParts {
    pub(super) rail_style: StyleRules,
    pub(super) server_rows: Vec<Element>,
    pub(super) create_server_section: Element,
    pub(super) live_section: Element,
    pub(super) join_el: Element,
    pub(super) create_el: Element,
    pub(super) discover_el: Element,
    pub(super) cc_el: Element,
    pub(super) rc_el: Element,
    pub(super) dc_el: Element,
    pub(super) invite_el: Element,
    pub(super) ss_el: Element,
    pub(super) rail_actions: Element,
}

pub(super) fn build(s: &ShellState) -> RailParts {
    let ShellState { active_channel, active_live_channel, active_live_server, active_server, cc_busy, cc_err, cc_kind, cc_name, cc_open, dc_busy, dc_cid, dc_err, dc_name, dc_open, dm_mode, gateway, invite_created, invite_err, invite_expiry, invite_mode, is_real_user, joined_voice, live_ban_confirm, live_channels, live_dm_active, live_dm_mode, live_dms, live_kick_confirm, live_member_error, live_members, live_servers, live_timeout_confirm, live_unread, live_unread_rev, notif_mode, rail_unread, rc_busy, rc_cid, rc_err, rc_name, rc_open, ss_add_role, ss_add_user_id, ss_busy, ss_err, ss_icon, ss_name, ss_open, ss_owner, .. } = s.clone();
    // --- server rail -----------------------------------------------------
    let rail_style = StyleRules {
        width: Some(px(72.0)),
        background: Some(col(BG_RAIL)),
        flex_direction: Some(FlexDirection::Column),
        align_items: Some(AlignItems::Center),
        gap: Some(px(10.0)),
        padding_top: Some(px(12.0)),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };

    // Reactive list of the mock servers shown in the rail. Seeded with the
    // static `SERVERS` ids; the "Add a server" (+) flow appends new ids, and
    // the rail `switch` below rebuilds on that signal so new rows appear.
    let server_rail: Signal<Vec<u32>> = signal(SERVERS.iter().map(|s| s.id).collect());

    // [Feature 16] The mock demo servers are guest-only rail content. A real
    // account sees only its own live servers (the live section below) plus the
    // create / join affordances — the demo `SERVERS` are not theirs, so the
    // mock rows are gated off for real users (empty here; the onboarding
    // state is driven by the create affordance in the rail). Guests keep the
    // full demo rail.
    let server_rows: Vec<Element> = if is_real_user {
        Vec::new()
    } else {
        SERVERS.iter().map(|s| {
        let active = active_server.clone();
        let channel = active_channel.clone();
        let voice = joined_voice.clone();
        let live_server = active_live_server.clone();
        let live_channel = active_live_channel.clone();
        // Reactive style: re-runs whenever `active_server` changes.
        let row_style = move || StyleRules {
            width: Some(px(48.0)),
            height: Some(px(48.0)),
            // Positioning ancestor for the absolutely-positioned pill.
            position: Some(Position::Relative),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::Center),
            // The row keeps its squircle background on selection (Discord does
            // not invert the icon to white); the active-server pill on the far
            // left edge is the sole selection signal. Background is static
            // `BG_SIDEBAR` — no reactive white-invert.
            background: Some(col(BG_SIDEBAR)),
            // Discord's squircle is a 16px radius on a 48px icon, not a full
            // circle.
            border_top_left_radius: Some(px(16.0)),
            border_top_right_radius: Some(px(16.0)),
            border_bottom_left_radius: Some(px(16.0)),
            border_bottom_right_radius: Some(px(16.0)),
            ..Default::default()
        };
        let label_style = move || StyleRules {
            font_size: Some(px(18.0)),
            font_weight: Some(FontWeight::Bold),
            // The label keeps its own color on selection (no white-invert);
            // the left-edge pill alone signals the active server.
            color: Some(col(TEXT_CHANNEL)),
            ..Default::default()
        };
        ui! {
            view(style = row_style, test_id = server_test_id(s.id)) {
                // The active-server pill: Discord's white vertical sliver on
                // the far-left edge of the rail. The row is the positioning
                // ancestor (default Static is not, so the pill would anchor
                // to the rail — we want it at the row's left edge). The row
                // is 48px centered in the 72px rail (12px offset), so
                // left: -12 puts the pill flush with the screen edge.
                view(style = move || StyleRules {
                    position: Some(Position::Absolute),
                    left: Some(px(-12.0)),
                    top: Some(px(14.0)),
                    width: Some(px(4.0)),
                    height: Some(px(20.0)),
                    background: Some(col(PILL)),
                    border_top_left_radius: Some(px(2.0)),
                    border_top_right_radius: Some(px(2.0)),
                    border_bottom_left_radius: Some(px(2.0)),
                    border_bottom_right_radius: Some(px(2.0)),
                    opacity: Some(Tokenized::Literal(if active.get() == s.id {
                        1.0
                    } else {
                        0.0
                    })),
                    ..Default::default()
                }) {}
                text(style = label_style) { s.initial }
                // The badge: a red numbered pill for @mentions, a small
                // plain white dot for unread (no mention), nothing for read.
                // The row is the positioning ancestor (`position: Relative`
                // above); the mock totals are computed once at build — swap
                // for a gateway-fed signal later.
                if s.total_mentions() > 0 {
                    view(style = move || StyleRules {
                        position: Some(Position::Absolute),
                        right: Some(px(-4.0)),
                        bottom: Some(px(-4.0)),
                        min_width: Some(px(18.0)),
                        height: Some(px(18.0)),
                        background: Some(col(BADGE)),
                        border_top_left_radius: Some(px(9.0)),
                        border_top_right_radius: Some(px(9.0)),
                        border_bottom_left_radius: Some(px(9.0)),
                        border_bottom_right_radius: Some(px(9.0)),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        padding_left: Some(px(4.0)),
                        padding_right: Some(px(4.0)),
                        ..Default::default()
                    }) {
                        text(style = move || StyleRules {
                            font_size: Some(px(12.0)),
                            font_weight: Some(FontWeight::Bold),
                            color: Some(col(TEXT)),
                            ..Default::default()
                        }) { s.total_mentions().to_string() }
                    }
                } else if s.total_unread() > 0 {
                    // Plain dot: unread but no mention — Discord shows a
                    // small white indicator instead of the red badge.
                    view(style = StyleRules {
                        position: Some(Position::Absolute),
                        right: Some(px(-4.0)),
                        bottom: Some(px(-4.0)),
                        width: Some(px(10.0)),
                        height: Some(px(10.0)),
                        background: Some(col(TEXT)),
                        border_top_left_radius: Some(px(5.0)),
                        border_top_right_radius: Some(px(5.0)),
                        border_bottom_left_radius: Some(px(5.0)),
                        border_bottom_right_radius: Some(px(5.0)),
                        ..Default::default()
                    }) {}
                }
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    // Selecting a mock server exits live mode so the channel
                    // sidebar shows the mock channels, not the live ones.
                    live_server.set(None);
                    live_channel.set(None);
                    active.set(s.id);
                    if let Some(srv) = SERVERS.iter().find(|x| x.id == s.id) {
                        // Default to the first *text* channel — voice channels
                        // aren't selectable in this build.
                        channel.set(srv.first_text_channel().id);
                    }
                    // Switching servers drops the voice join: you can't stay
                    // connected in a channel you're no longer looking at
                    // (Discord's per-server voice behavior, simplified).
                    voice.set(None);
                }
                TouchResponse::CONSUMED
            })
        }
    }).collect()
    };

    // --- "Create a Server" reactive plumbing -------------------------------
    // `dynamic_servers` holds the servers the user creates this session
    // (leaked `&'static` behind an id — the same heap-leak trick the static
    // seed in `data.rs` uses). `next_id` mints fresh ids above the static max.
    // `server_rail` (seeded with the static ids above) is the rail's re-render
    // trigger: appending a new id makes the `switch` below rebuild and add the
    // row. Both the `Cell` and the `HashMap` live behind an `Rc` so the
    // `on_create` closure (a `Fn`) can mutate them through interior
    // mutability.
    let dynamic_servers: Rc<RefCell<HashMap<u32, &'static Server>>> =
        Rc::new(RefCell::new(HashMap::new()));
    let next_id: Rc<Cell<u32>> = Rc::new(Cell::new(
        SERVERS.iter().map(|s| s.id).max().map(|m| m + 1).unwrap_or(1),
    ));
    let create_open: Signal<bool> = signal(false);
    let create_name: Signal<String> = signal(String::new());
    // The create modal's inline error (the backend's own rejection wording)
    // and in-flight busy guard. Both owned by the app (like the join modal's
    // `join_error` / `join_busy`) and reset on re-open below.
    let create_error: Signal<String> = signal(String::new());
    let create_busy: Signal<bool> = signal(false);

    // Dynamically created servers — a separate section below the mock rows.
    // Rebuilt whenever `server_rail` changes (a new id appended). Each row
    // mirrors the mock row: squircle + initial + selection.
    let dyn_servers = dynamic_servers.clone();
    let create_server_section = switch(
        move || server_rail.get(),
        move |ids: &Vec<u32>| {
            let rows: Vec<Element> = ids.iter().filter_map(|id| {
                let s = *dyn_servers.borrow().get(id)?;
                let active = active_server.clone();
                let channel = active_channel.clone();
                let voice = joined_voice.clone();
                let live_server = active_live_server.clone();
                let live_channel = active_live_channel.clone();
                let row_style = move || StyleRules {
                    width: Some(px(48.0)),
                    height: Some(px(48.0)),
                    // Positioning ancestor for the active-server pill.
                    position: Some(Position::Relative),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    // No white-invert on selection — the left-edge pill alone
                    // signals the active created server (matches the mock rows).
                    background: Some(col(BG_SIDEBAR)),
                    // Discord's squircle is a 16px radius on a 48px icon.
                    border_top_left_radius: Some(px(16.0)),
                    border_top_right_radius: Some(px(16.0)),
                    border_bottom_left_radius: Some(px(16.0)),
                    border_bottom_right_radius: Some(px(16.0)),
                    ..Default::default()
                };
                let label_style = move || StyleRules {
                    font_size: Some(px(18.0)),
                    font_weight: Some(FontWeight::Bold),
                    color: Some(col(TEXT_CHANNEL)),
                    ..Default::default()
                };
                Some(ui! {
                    view(style = row_style) {
                        // The active-server pill on the far-left rail edge — the
                        // sole selection signal (the row is `Position::Relative`,
                        // 48px centered in the 72px rail, so left: -12 is flush).
                        view(style = move || StyleRules {
                            position: Some(Position::Absolute),
                            left: Some(px(-12.0)),
                            top: Some(px(14.0)),
                            width: Some(px(4.0)),
                            height: Some(px(20.0)),
                            background: Some(col(PILL)),
                            border_top_left_radius: Some(px(2.0)),
                            border_top_right_radius: Some(px(2.0)),
                            border_bottom_left_radius: Some(px(2.0)),
                            border_bottom_right_radius: Some(px(2.0)),
                            opacity: Some(Tokenized::Literal(if active.get() == s.id {
                                1.0
                            } else {
                                0.0
                            })),
                            ..Default::default()
                        }) {}
                        text(style = label_style) { s.initial }
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            // Selecting a created server exits live mode (same
                            // as a mock row) and defaults to its first text
                            // channel.
                            live_server.set(None);
                            live_channel.set(None);
                            active.set(s.id);
                            channel.set(s.first_text_channel().id);
                            voice.set(None);
                        }
                        TouchResponse::CONSUMED
                    })
                })
            }).collect();
            ui! {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Column),
                    align_items: Some(AlignItems::Center),
                    gap: Some(px(10.0)),
                    ..Default::default()
                }) {
                    rows
                }
            }
        },
    );

    // Live (backend) servers — a visually separate section below the mock
    // rows. Clicking a row enters live mode: it sets `active_live_server`
    // (so the channel sidebar swaps to the live branch) and fetches that
    // server's real channels into `live_channels`. The active row is
    // highlighted reactively (its key is `live_servers`, and the render
    // closure also reads `active_live_server` reactively). Tapping a mock
    // rail row clears `active_live_server`, exiting live mode.
    let live_section = switch(
        move || live_servers.get(),
        move |servers: &Vec<crate::servers::ServerSummary>| {
            let divider_style = StyleRules {
                width: Some(px(32.0)),
                height: Some(px(1.0)),
                background: Some(col(BORDER)),
                ..Default::default()
            };
            // Small "LIVE" badge under the thin divider: 9px uppercase,
            // semi-bold, muted text on a BG_SIDEBAR pill (radius 8). The bare
            // unstyled text version read as a floating label with no chrome;
            // the pill gives it badge styling and still marks the section's
            // purpose — backend-connected servers, as opposed to the mock rows
            // above the divider.
            let label_style = StyleRules {
                font_size: Some(px(9.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(TEXT_MUTED)),
                letter_spacing: Some(Tokenized::Literal(0.5)),
                text_transform: Some(TextTransform::Uppercase),
                background: Some(col(BG_SIDEBAR)),
                padding_top: Some(px(3.0)),
                padding_bottom: Some(px(3.0)),
                padding_left: Some(px(7.0)),
                padding_right: Some(px(7.0)),
                border_top_left_radius: Some(px(8.0)),
                border_top_right_radius: Some(px(8.0)),
                border_bottom_left_radius: Some(px(8.0)),
                border_bottom_right_radius: Some(px(8.0)),
                ..Default::default()
            };
            let row_style = StyleRules {
                width: Some(px(48.0)),
                height: Some(px(48.0)),
                // Positioning ancestor for the active-server pill (the mock rows
                // do the same).
                position: Some(Position::Relative),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                background: Some(col(BG_SIDEBAR)),
                border_top_left_radius: Some(px(16.0)),
                border_top_right_radius: Some(px(16.0)),
                border_bottom_left_radius: Some(px(16.0)),
                border_bottom_right_radius: Some(px(16.0)),
                ..Default::default()
            };
            let initial_style = StyleRules {
                font_size: Some(px(18.0)),
                font_weight: Some(FontWeight::Bold),
                color: Some(col(TEXT_CHANNEL)),
                ..Default::default()
            };
            let rows: Vec<Element> = servers.iter().map(|s| {
                let initial = s.name.chars().next().unwrap_or('?').to_string();
                // The uploaded icon fills the 48px tile with the tile's own
                // rounding; servers without one keep the initial letter.
                let face = match s.icon_url.clone().filter(|u| !u.is_empty()) {
                    Some(url) => ui! {
                        image(
                            src = crate::auth::media_url(url),
                            alt = s.name.clone(),
                            style = StyleRules {
                                width: Some(px(48.0)),
                                height: Some(px(48.0)),
                                flex_shrink: Some(grow(0.0)),
                                object_fit: Some(ObjectFit::Cover),
                                border_top_left_radius: Some(px(16.0)),
                                border_top_right_radius: Some(px(16.0)),
                                border_bottom_left_radius: Some(px(16.0)),
                                border_bottom_right_radius: Some(px(16.0)),
                                ..Default::default()
                            },
                        )
                    },
                    None => ui! { text(style = initial_style.clone()) { initial } },
                };
                let sid = s.id;
                let active_live = active_live_server.clone();
                let live_ch = live_channels.clone();
                let live_ch_sel = active_live_channel.clone();
                let unread_buf = rail_unread.clone();
                let unread_rev = live_unread_rev.clone();
                let live_mbrs = live_members.clone();
                let live_mbr_err = live_member_error.clone();
                let live_kick = live_kick_confirm.clone();
                let live_ban = live_ban_confirm.clone();
                let live_timeout = live_timeout_confirm.clone();
                let base = row_style.clone();
                ui! {
                    view(style = base.clone()) {
                        // The active-server pill on the far-left rail edge — the
                        // sole selection signal (the row is `Position::Relative`,
                        // 48px centered in the 72px rail, so left: -12 is flush).
                        view(style = move || StyleRules {
                            position: Some(Position::Absolute),
                            left: Some(px(-12.0)),
                            top: Some(px(14.0)),
                            width: Some(px(4.0)),
                            height: Some(px(20.0)),
                            background: Some(col(PILL)),
                            border_top_left_radius: Some(px(2.0)),
                            border_top_right_radius: Some(px(2.0)),
                            border_bottom_left_radius: Some(px(2.0)),
                            border_bottom_right_radius: Some(px(2.0)),
                            opacity: Some(Tokenized::Literal(if active_live.get() == Some(sid) {
                                1.0
                            } else {
                                0.0
                            })),
                            ..Default::default()
                        }) {}
                        { face }
                    }
                    // Enter live mode: select this real server and pull its
                    // channels (async `#[server]` RPC, same pattern as the
                    // `live_servers` fetch above). Any previously selected live
                    // channel is stale now (it belonged to a different server),
                    // so reset it alongside `active_live_server`.
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            active_live.set(Some(sid));
                            live_ch_sel.set(None);
                            // Clear any stale moderation state from a previous
                            // live server (kick + ban + timeout confirm and error text).
                            live_kick.set(None);
                            live_ban.set(None);
                            live_timeout.set(None);
                            live_mbr_err.set(String::new());
                            live_mbrs.set(Vec::new());
                            // Clone the `Rc` before handing it to the `async move`
                            // block: the block outlives the `on_touch` `Fn`
                            // closure, so moving the captured `unread_buf`
                            // straight into it would be a move out of the
                            // closure (E0507). The clone shares the same
                            // `RefCell`, so the fetch writes the shared map.
                            let unread_buf = unread_buf.clone();
                            runtime_core::driver::spawn_async(async move {
                                // Channels, the member roster, and the
                                // per-channel unread counts are independent
                                // `#[server]` RPCs; fetch all three in one
                                // spawn so the live view is populated together.
                                if let Ok(channels) =
                                    crate::servers::list_channels(sid).await
                                {
                                    // Auto-select the first text channel so the
                                    // main panes enter live mode: the header,
                                    // message list, and composer all key on
                                    // `active_live_channel`, which stays `None`
                                    // here until a channel row is tapped. The
                                    // mock tile handler does the same via
                                    // `first_text_channel()` — without it,
                                    // clicking a live tile updates only the
                                    // sidebar (keyed on `active_live_server`)
                                    // and the header / messages keep showing the
                                    // mock server. Resolved before `channels`
                                    // is moved into `live_ch`.
                                    let first = channels
                                        .iter()
                                        .find(|c| c.kind == "text")
                                        .map(|c| c.id);
                                    live_ch.set(channels);
                                    if let Some(id) = first {
                                        live_ch_sel.set(Some(id));
                                        // Persist the exact landing spot (this
                                        // server + this first channel) keyed by
                                        // user id, so a refresh re-enters HERE
                                        // — not the account's first server.
                                        crate::auth::save_last_view_channel(sid, id);
                                        // Auto-selecting only sets which channel
                                        // is "active" — it does NOT populate
                                        // `live_messages` for it. A manual row
                                        // tap also sends History (see the
                                        // channel row's `on_touch` below); do
                                        // the same here so the auto-selected
                                        // channel isn't stuck showing stale/no
                                        // messages until the user re-taps it.
                                        gateway.send(
                                            discord_server::server_settings::WsClientMsg::History {
                                                channel_id: id,
                                                before_id: None,
                                            },
                                        );
                                    }
                                }
                                if let Ok(members) =
                                    crate::moderation::list_members(sid).await
                                {
                                    live_mbrs.set(members);
                                }
                                if let Ok(counts) =
                                    crate::servers::unread_counts(sid).await
                                {
                                    // Clone the `Rc`: this `async move` block
                                    // outlives the `on_touch` `Fn` closure that
                                    // captured it, so a plain capture would be
                                    // a move out of the closure (E0507).
                                    let buf = unread_buf.clone();
                                    let rev = unread_rev;
                                    buf.borrow_mut()
                                        .extend(counts.into_iter().map(|c| {
                                            (c.channel_id, (c.count, c.unread_mentions))
                                        }));
                                    rev.set(rev.get() + 1);
                                }
                            });
                        }
                        TouchResponse::CONSUMED
                    })
                }
            }).collect();
            ui! {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Column),
                    align_items: Some(AlignItems::Center),
                    gap: Some(px(10.0)),
                    ..Default::default()
                }) {
                    view(style = divider_style) {}
                    text(style = label_style) { "live".to_string() }
                    rows
                }
            }
        },
    );

    // --- "Join a Server" (redeem an invite code) ---------------------------
    // Entry point is the rail's green PLUS circle (Discord's Add-a-Server
    // affordance). The modal's state lives here in `app_shell` scope so it
    // survives rail rebuilds; `join_open` gates the overlay, `join_code` is the
    // typed code, `join_result`/`join_error`/`join_busy` are the three state
    // signals the modal switches on (the `invite_modal.rs` ownership idiom).
    // On a successful redeem the backend membership is persisted, so we
    // re-fetch `live_servers` (the same RPC the rail's live section already
    // uses) to make the newly-joined server appear without a reload.
    let join_open: Signal<bool> = signal(false);
    let join_code: Signal<String> = signal(String::new());
    let join_result: Signal<Option<discord_server::invites::RedeemResult>> = signal(None);
    let join_error: Signal<String> = signal(String::new());
    let join_busy: Signal<bool> = signal(false);
    // The `#[component]` `name = expr` call form is only parsed inside `ui!`,
    // so the modal is wrapped in a `view` (the wrapper is a positioned
    // ancestor for the full-screen overlay, like the Profile/notif regions).
    let join_el = ui! {
        view {
            JoinServerModal(
                open = join_open,
                code = join_code,
                result = join_result,
                error = join_error,
                busy = join_busy,
                servers = live_servers,
            )
        }
    };

    // --- "Create a Server" modal ------------------------------------------
    // Trigger is the rail's COMPASS circle (the green PLUS is taken by the
    // Join flow). `on_create` mints a REAL backend server via the
    // `crate::servers::create_server` RPC (the backend seeds the owner's
    // membership row plus a default `#general` text channel, so the fresh
    // server is immediately usable), then re-fetches `live_servers` so the new
    // server appears in the rail's live section (the same refresh the Join
    // modal uses) and auto-enters it (selects it + pulls its channels, the
    // enter-live pattern below). `Signal` is `Copy`, so the captured `set`
    // stays `Fn`.
    let create_on_create: Rc<dyn Fn(String)> = Rc::new({
        let live_servers = live_servers.clone();
        let active_live_server = active_live_server.clone();
        let active_live_channel = active_live_channel.clone();
        let live_channels = live_channels.clone();
        let live_members = live_members.clone();
        let live_member_error = live_member_error.clone();
        let live_kick_confirm = live_kick_confirm.clone();
        let live_unread = live_unread.clone();
        let live_unread_rev = live_unread_rev.clone();
        let create_open = create_open;
        let create_error = create_error;
        let create_busy = create_busy;
        move |name: String| {
            let name_owned = name.clone();
            // Clone the `Rc` in the `Fn` body (not inside the `async move`):
            // the `async move` block would otherwise try to move the captured
            // `Rc` out of the `Fn` closure (E0507) — the enter-live handler
            // uses the same `let unread_buf = unread_buf.clone()` idiom.
            let buf = live_unread.clone();
            let c_err = create_error;
            let c_busy = create_busy;
            runtime_core::driver::spawn_async(async move {
                // Clear any prior rejection and mark in-flight; the modal's
                // Create button shows "Creating…" while this runs (the busy
                // guard also stops a fast double-tap from minting two).
                c_err.set(String::new());
                c_busy.set(true);
                match crate::servers::create_server(&name_owned).await {
                    Ok(summary) => {
                        // The backend persisted the server + default channel;
                        // re-fetch the live server list (the same RPC the rail's
                        // live section uses) so the new server appears without a
                        // reload.
                        if let Ok(list) = crate::servers::list_servers().await {
                            live_servers.set(list);
                        }
                        // Auto-enter the new server, mirroring the enter-live
                        // handler: select it, reset any stale channel/moderation
                        // state, and pull its channels + roster + unread counts.
                        active_live_server.set(Some(summary.id));
                        active_live_channel.set(None);
                        live_kick_confirm.set(None);
                        live_member_error.set(String::new());
                        live_members.set(Vec::new());
                        let sid = summary.id;
                        if let Ok(channels) = crate::servers::list_channels(sid).await {
                            // Auto-select the first text channel (same as the
                            // enter-live tile handler): the main panes key on
                            // `active_live_channel`, so a freshly created server
                            // would otherwise sit on a stale / mock main pane
                            // until a channel row is tapped.
                            let first = channels
                                .iter()
                                .find(|c| c.kind == "text")
                                .map(|c| c.id);
                            live_channels.set(channels);
                            if let Some(id) = first {
                                active_live_channel.set(Some(id));
                                // Persist the landing spot for this account so
                                // a refresh re-enters the just-created server's
                                // first channel.
                                crate::auth::save_last_view_channel(summary.id, id);
                                // See the enter-live tile handler's identical
                                // fix: auto-selecting doesn't fetch history on
                                // its own, so send it explicitly or the fresh
                                // server sits on an empty pane until a channel
                                // row is manually re-tapped.
                                gateway.send(
                                    discord_server::server_settings::WsClientMsg::History {
                                        channel_id: id,
                                        before_id: None,
                                    },
                                );
                            }
                        }
                        if let Ok(members) = crate::moderation::list_members(sid).await {
                            live_members.set(members);
                        }
                        if let Ok(counts) = crate::servers::unread_counts(sid).await {
                            let rev = live_unread_rev;
                            buf.borrow_mut()
                                .extend(counts.into_iter().map(|c| {
                                    (c.channel_id, (c.count, c.unread_mentions))
                                }));
                            rev.set(rev.get() + 1);
                        }
                        // Success: the server exists and we're auto-entering
                        // it, so close the modal. (On the error path below the
                        // modal stays open so the inline message is visible and
                        // the name field can be corrected and resubmitted.)
                        create_open.set(false);
                    }
                    Err(msg) => {
                        // Surface the backend's own rejection inline in the
                        // modal (e.g. an empty name -> "server name must not
                        // be empty"). The modal stays open; the name field is
                        // still editable so the user can correct it and hit
                        // Create again. We do NOT close here — closing on
                        // failure would hide the very message we just set.
                        eprintln!("create_server failed: {msg}");
                        c_err.set(msg);
                    }
                }
                c_busy.set(false);
            });
        }
    });
    let create_modal_el: Element =
        create_server_modal(create_open, create_name, create_error, create_busy, create_on_create);
    let create_el = ui! {
        view {
            { create_modal_el }
        }
    };

    // --- "Discover Servers" overlay ----------------------------------------
    // Trigger is a third rail circle (GLOBE) below the PLUS and COMPASS
    // circles — both existing icons are taken (Join / Create). `on_join`
    // mints the suggested server the same way Create does: `create_server` +
    // append to `server_rail` (the rail's re-render trigger), then close the
    // overlay. The mock list itself is static; no join backend.
    let discover_open: Signal<bool> = signal(false);
    let discover_on_join: Rc<dyn Fn(String)> = Rc::new({
        let next_id = next_id.clone();
        let dynamic_servers = dynamic_servers.clone();
        let server_rail = server_rail.clone();
        let discover_open = discover_open;
        move |name: String| {
            let new_id = create_server(&name, &*next_id, &dynamic_servers);
            let mut ids = server_rail.get();
            ids.push(new_id);
            server_rail.set(ids);
            discover_open.set(false);
        }
    });
    let discover_modal_el: Element = discover_servers_modal(discover_open, discover_on_join);
    let discover_el = ui! {
        view {
            { discover_modal_el }
        }
    };

    let cc_el = ui! {
        view {
            CreateChannelModal(
                open = cc_open,
                server_id = active_live_server,
                name = cc_name,
                kind = cc_kind,
                busy = cc_busy,
                error = cc_err,
                channels = live_channels,
            )
        }
    };
    let rc_el = ui! {
        view {
            RenameChannelModal(
                open = rc_open,
                server_id = active_live_server,
                channel_id = rc_cid,
                name = rc_name,
                busy = rc_busy,
                error = rc_err,
                channels = live_channels,
            )
        }
    };
    let dc_el = ui! {
        view {
            DeleteChannelModal(
                open = dc_open,
                server_id = active_live_server,
                channel_id = dc_cid,
                channel_name = dc_name,
                busy = dc_busy,
                error = dc_err,
                channels = live_channels,
            )
        }
    };
    let invite_el = ui! {
        view {
            InviteModal(
                open = invite_mode,
                server_id = active_live_server,
                expiry = invite_expiry,
                created = invite_created,
                created_err = invite_err,
            )
        }
    };
    let ss_el = ui! {
        view {
            ServerSettingsPanel(
                open = ss_open,
                server_id = active_live_server,
                name = ss_name,
                icon = ss_icon,
                new_owner_id = ss_owner,
                busy = ss_busy,
                error = ss_err,
                servers = live_servers,
                members = live_members,
                add_member_user_id = ss_add_user_id,
                add_member_role = ss_add_role,
            )
        }
    };
    // (Live channel action cluster + new-channel "+" builder live inside the
    // live arm of the sidebar `switch` render closure — see there. They need
    // the render closure's owned copies of the modal `Signal`s, which a
    // function-scope non-`move` closure could not borrow across the `move`
    // capture boundary.)
    // Bottom rail actions, grouped Discord-style: the server cluster (Add-a-
    // Server (+), Explore (compass), plus the clone's Discover (globe) and
    // Live-DM (speech-bubble) affordances, which power real functionality and
    // so are kept reachable above the divider) sits with the server list, and
    // the Direct Messages + Settings circles sit BELOW it. Real Discord's
    // bottom rail row is DM + Settings only — that is exactly what the bottom
    // holds here now.
    let rail_actions = {
        let circle_style = StyleRules {
            width: Some(px(48.0)),
            height: Some(px(48.0)),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::Center),
            background: Some(col(BG_SIDEBAR)),
            border_top_left_radius: Some(px(24.0)),
            border_top_right_radius: Some(px(24.0)),
            border_bottom_left_radius: Some(px(24.0)),
            border_bottom_right_radius: Some(px(24.0)),
            ..Default::default()
        };
        let divider_style = StyleRules {
            width: Some(px(32.0)),
            height: Some(px(1.0)),
            background: Some(col(BORDER)),
            ..Default::default()
        };
        let plus_open = join_open.clone();
        let plus_code = join_code.clone();
        let plus_result = join_result.clone();
        let plus_error = join_error.clone();
        let plus_busy = join_busy.clone();
        let create_open = create_open.clone();
        let create_name = create_name.clone();
        let create_error = create_error.clone();
        let create_busy = create_busy.clone();
        let discover_open = discover_open.clone();
        let dm_open = dm_mode.clone();
        let settings_open = notif_mode.clone();
        ui! {
            // --- Top cluster: server-related actions (above the divider) ----
            // Add-a-Server (+) + Explore (compass) sit with the server list;
            // the clone's Discover (globe) and Live-DM (speech-bubble)
            // affordances live here too — they power real functionality, so
            // they're kept reachable above the divider rather than in the
            // bottom DM+Settings row (which must stay DM + Settings only).
            view(style = circle_style.clone(), test_id = "rail-join-server") {
                icon(PLUS).color(|| Color::from("#23a55a")).size(20.0)
            }
            // Add-a-Server: open the join modal. Reset the per-open state so a
            // re-open always starts fresh (the same reset the caller owns).
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    plus_code.set(String::new());
                    plus_result.set(None);
                    plus_error.set(String::new());
                    plus_busy.set(false);
                    plus_open.set(true);
                }
                TouchResponse::CONSUMED
            })
            view(style = circle_style.clone(), test_id = "rail-explore-servers") {
                icon(COMPASS).color(|| Color::from(TEXT_MUTED)).size(20.0)
            }
            // Explore/Create-a-Server: reset the name field and open the create
            // modal (the same reset-on-open `create_server_modal`'s docs
            // describe). The compass is Discord's Explore affordance. Also
            // clear the prior attempt's inline error and busy state so a
            // re-open always starts fresh (the same per-open reset the join
            // modal applies to its `join_error` / `join_busy`).
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    create_name.set(String::new());
                    create_error.set(String::new());
                    create_busy.set(false);
                    create_open.set(true);
                }
                TouchResponse::CONSUMED
            })
            // Discover-a-Server: open the discover overlay (no per-open state
            // to reset — the suggested list is static). Moved up from the old
            // bottom slot so it stays reachable without cluttering the DM row.
            view(style = circle_style.clone(), test_id = "rail-discover-servers") {
                icon(GLOBE).color(|| Color::from(TEXT_MUTED)).size(20.0)
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    discover_open.set(true);
                }
                TouchResponse::CONSUMED
            })
            // Direct Messages (LIVE / backend): a separate, independent entry
            // point from the mock DM button below. Toggling it open fetches
            // the signed-in user's real DMs via `list_dms` (the same
            // `spawn_async` + `#[server]` RPC idiom as the `live_servers`
            // fetch) the first time it's opened; subsequent opens re-fetch so
            // the list stays current. Per-open state (`live_dm_active`,
            // `live_dm_messages`, `live_dm_revision`) is owned in `app_shell`
            // scope, so a re-open keeps the selected conversation and its
            // fetched history (the same persistence as the mock DM screen).
            // Moved up from the old bottom slot (which must be DM + Settings
            // only) so the live-DM feature stays reachable.
            view(style = circle_style.clone(), test_id = "rail-live-direct-messages") {
                icon(MESSAGE_CIRCLE).color(|| Color::from(TEXT_MUTED)).size(20.0)
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    let now_open = !live_dm_mode.get();
                    live_dm_mode.set(now_open);
                    if now_open {
                        // Persist the open conversation as this user's last
                        // view (DM form) so a refresh re-opens the DM panel on
                        // their own last conversation. None (nothing selected
                        // yet this visit) clears nothing; the DM row's select
                        // handler keeps the value current on every tap.
                        if let Some(dm_id) = live_dm_active.get() {
                            crate::auth::save_last_view_dm(dm_id);
                        }
                        // Re-fetch on every open so the list reflects the backend's
                        // current state (mirrors how a live channel is re-fetched
                        // when its server is re-selected).
                        let list = live_dms.clone();
                        runtime_core::driver::spawn_async(async move {
                            if let Ok(dms) = crate::dms::list_dms().await {
                                list.set(dms);
                            }
                        });
                    }
                }
                TouchResponse::CONSUMED
            })
            // The divider between the top (server-related) cluster and the
            // bottom (DM + Settings) row — Discord's rail separator.
            view(style = divider_style) {}
            // --- Bottom row: Direct Messages (mock) + Settings -------------
            // Real Discord's bottom rail row is DM + Settings only.
            view(style = circle_style.clone(), test_id = "rail-direct-messages") {
                icon(MAIL).color(|| Color::from(TEXT_MUTED)).size(20.0)
            }
            // Direct Messages (mock): toggle the full-screen `dm_screen` overlay.
            // No per-open state to reset — `dm_active` / `dm_sent` / `dm_revision`
            // are owned in `app_shell` scope and persist across toggles, so a
            // re-open keeps the previously-selected conversation and any DMs
            // sent this session (the same persistence as the channel `sent` log).
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    dm_open.set(!dm_open.get());
                }
                TouchResponse::CONSUMED
            })
            view(style = circle_style.clone(), test_id = "rail-settings") {
                icon(SETTINGS).color(|| Color::from(TEXT_MUTED)).size(20.0)
            }
            // Settings: opens the full-screen Notification Settings overlay
            // (the same toggle idiom the user-panel's settings gear uses —
            // `notif_mode` is the same signal that one controls).
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    settings_open.set(true);
                }
                TouchResponse::CONSUMED
            })
            // (server_folder_group/server_template_card/TemplateGalleryGrid/
            // server_tag_chips/FeaturedServerBanner used to be spliced directly
            // into this 72px icon rail -- a real usability regression, it
            // pushed the real rail controls (add-server/DM/settings) below the
            // fold. Moved into the Discover Servers overlay instead, alongside
            // the other discovery cards that were already re-homed there for
            // the same reason -- see fa4bf410 and discover_servers_modal.rs.)
        }
    };

    RailParts { rail_style, server_rows, create_server_section, live_section, join_el, create_el, discover_el, cc_el, rc_el, dc_el, invite_el, ss_el, rail_actions }
}
