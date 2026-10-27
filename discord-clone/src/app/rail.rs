//! `app_shell` section, split out of `app.rs`. Reads the shared state
//! through `ShellState`; returns the elements the final layout places.

use crate::theme;
use super::*;

pub(super) struct RailParts {
    pub(super) rail_style: StyleRules,
    pub(super) server_rows: Vec<Element>,
    pub(super) create_server_section: Element,
    pub(super) live_section: Element,
    pub(super) rail_actions: Element,
}

pub(super) fn build(s: &ShellState) -> RailParts {
    let ShellState { active_channel, active_live_channel, active_live_server, active_server, create_busy, create_error, create_name, create_open, discover_open, dm_mode, dynamic_servers, friends_snap, gateway, is_real_user, join_busy, join_code, join_error, join_open, join_result, joined_voice, live_ban_confirm, live_channels, live_dm_active, live_dm_mode, live_dms, live_kick_confirm, live_member_error, live_members, live_servers, live_timeout_confirm, live_unread_rev, notif_mode, rail_unread, server_rail, .. } = s.clone();
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
                icon(PLUS).color(|| Color::from(theme::SUCCESS)).size(20.0)
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
            // The divider between the top (server-related) cluster and the
            // bottom (DM + Settings) row — Discord's rail separator.
            view(style = divider_style) {}
            // --- Bottom row: Direct Messages + Settings --------------------
            // Real Discord's bottom rail row is DM + Settings only.
            view(style = circle_style.clone(), test_id = "rail-direct-messages") {
                icon(MAIL).color(|| Color::from(TEXT_MUTED)).size(20.0)
            }
            // Direct Messages. A signed-in account gets its REAL DMs (the live
            // DM screen); a guest has no backend identity, so it keeps the
            // demo DM screen (`dm_screen`).
            //
            // Live: every open re-fetches the conversation list and the friend
            // list (the "Find or start a conversation" search reads it), and
            // records the open conversation as the last view so a refresh
            // re-opens it. Per-open state (`live_dm_active`, the fetched
            // history) lives in `app_shell` scope and survives a close.
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    if !is_real_user {
                        dm_open.set(!dm_open.get());
                        return TouchResponse::CONSUMED;
                    }
                    let now_open = !live_dm_mode.get();
                    live_dm_mode.set(now_open);
                    if now_open {
                        if let Some(dm_id) = live_dm_active.get() {
                            crate::auth::save_last_view_dm(dm_id);
                        }
                        let list = live_dms.clone();
                        let friends = friends_snap.clone();
                        runtime_core::driver::spawn_async(async move {
                            if let Ok(dms) = crate::dms::list_dms().await {
                                list.set(dms);
                            }
                            if let Ok(f) = crate::friends::list_friends().await {
                                friends.set(crate::friends_view::snapshot(f));
                            }
                        });
                    }
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

    RailParts { rail_style, server_rows, create_server_section, live_section, rail_actions }
}
