//! `app_shell` section, split out of `app.rs`. Reads the shared state
//! through `ShellState`; returns the elements the final layout places.

use super::*;

pub(super) struct SidebarParts {
    pub(super) sidebar: Element,
}

pub(super) fn build(s: &ShellState) -> SidebarParts {
    let ShellState { active_channel, active_live_channel, active_live_server, active_server, authed, bug_reports_mode, cc_busy, cc_err, cc_kind, cc_name, cc_open, collapsed, dc_busy, dc_cid, dc_err, dc_name, dc_open, deafened, gateway, invite_created, invite_err, invite_expiry, invite_mode, is_real_user, joined_voice, live_at_bottom, live_chan_menu, live_channels, live_members, live_mention_rev, live_scroll_handle, live_servers, live_unread, live_unread_rev, member_panel_width, muted, notif_mode, profile_mode, rc_busy, rc_cid, rc_err, rc_name, rc_open, shortcuts_mode, sidebar_width, ss_add_role, ss_add_user_id, ss_busy, ss_err, ss_icon, ss_name, ss_open, ss_owner, user_status, .. } = s.clone();
    // --- channel sidebar -------------------------------------------------
    // Width is driven by the `sidebar_width` signal (a thin drag handle on the
    // sidebar's right edge resizes it). A rules *closure* so the width
    // re-resolves as the signal changes — the same reactive idiom the channel
    // rows use. Captures only a `Copy` `Signal`, so the closure itself is
    // `Clone`, which the `switch` render below relies on (`sidebar_style.clone()`).
    let sidebar_style = move || {
        let (s, _m) = effective_panel_widths(sidebar_width, member_panel_width);
        StyleRules {
            width: Some(px(s)),
            // flex_shrink:0 so the flex row never shrinks the sidebar below
            // the viewport-responsive width computed above — the main column
            // (flex_grow:1) absorbs any residual slack instead.
            flex_shrink: Some(grow(0.0)),
            background: Some(col(BG_SIDEBAR)),
            border_right_width: Some(border_width(1.0)),
            border_right_color: Some(col(BORDER)),
            flex_direction: Some(FlexDirection::Column),
            overflow: Some(Overflow::Hidden),
            ..Default::default()
        }
    };
    let sidebar_header_style = StyleRules {
        padding_top: Some(px(14.0)),
        padding_bottom: Some(px(14.0)),
        padding_left: Some(px(16.0)),
        padding_right: Some(px(16.0)),
        border_bottom_width: Some(border_width(1.0)),
        border_bottom_color: Some(col(BORDER)),
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        gap: Some(px(6.0)),
        ..Default::default()
    };
    let sidebar_title_style = StyleRules {
        font_size: Some(px(16.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT)),
        ..Default::default()
    };
    let chevron_style = StyleRules {
        font_size: Some(px(11.0)),
        color: Some(col(TEXT_MUTED)),
        ..Default::default()
    };
    // Category section header: the small uppercase "TEXT CHANNELS" label.
    let cat_style = StyleRules {
        padding_top: Some(px(14.0)),
        padding_bottom: Some(px(4.0)),
        padding_left: Some(px(16.0)),
        padding_right: Some(px(16.0)),
        ..section_label()
    };

    // Profile panel toggle, cloned so the `move` switch closure below can
    // own this copy while the overlay (built later) keeps the original.
    let user_profile_mode = profile_mode.clone();
    // Notification panel toggle, cloned the same way.
    let user_notif_mode = notif_mode.clone();
    // Keyboard shortcuts toggle, cloned the same way.
    let user_shortcuts_mode = shortcuts_mode.clone();
    // The `collapsed` map is `Rc<RefCell<..>>` (not `Copy`, since `RefCell`
    // isn't `Copy`). The key and render closures are both `move`, so each
    // captures its own clone; the shared `RefCell` inside keeps them in sync.
    let collapsed_key = collapsed.clone();
    let collapsed_render = collapsed.clone();
    let sidebar = switch(
        move || {
            // Fingerprint (id, name, kind) so the key is `PartialEq` even
            // though `ChannelSummary` itself isn't; it changes when a live
            // server's channels are fetched, forcing the live branch below to
            // rebuild and show them.
            let live_fp: Vec<(i64, String, String)> = live_channels
                .get()
                .iter()
                .map(|c| (c.id, c.name.clone(), c.kind.clone()))
                .collect();
            // Collapse fingerprint: (category id, collapsed) pairs, ordered by
            // id so it's stable. Read from the shared `collapsed` map — when a
            // CategoryHeader flips its signal, this changes and the row loop
            // re-runs to re-gate each category's rows on the new flag.
            let cat_collapsed: Vec<(u32, bool)> = {
                let mut v: Vec<(u32, bool)> = collapsed_key
                    .borrow()
                    .iter()
                    .map(|(id, sig)| (*id, sig.get()))
                    .collect();
                v.sort_by_key(|(id, _)| *id);
                v
            };
            (
                active_server.get(),
                active_channel.get(),
                joined_voice.get(),
                active_live_server.get(),
                live_fp,
                live_mention_rev.get(),
                live_unread_rev.get(),
                // Per-category collapse flags, so the row loop re-runs (and
                // re-gates each category's rows) when a CategoryHeader toggles.
                cat_collapsed,
                // Which channel row's management popup is open — in the key so
                // a gear tap rebuilds the rows and mounts/unmounts the portal.
                live_chan_menu.get(),
            )
        },
        move |(server_id, _channel_id, _voice, _live_server, _live_fp, _mention_rev, _unread_rev, _cat_collapsed, _chan_menu): &(
            u32,
            u32,
            Option<(u32, u32)>,
            Option<i64>,
            Vec<(i64, String, String)>,
            u32,
            u32,
            Vec<(u32, bool)>,
            Option<i64>,
        )| {
            // `switch`'s render is `Fn` — clone the captured styles so the
            // closure borrows them rather than moving them out (whiteboard idiom).
            let sidebar_style = sidebar_style.clone();
            // [Feature 16] A real account with no live channel selected must
            // never fall through to the mock render: its default `active_server`
            // / `active_channel` point at the demo `SERVERS[0]`, and the mock
            // branch below would render those demo identities (the "Nova"
            // confusion). During the login -> server-fetch race (and for a real
            // account with zero servers) there is no live channel, so show an
            // onboarding hint instead — the live auto-enter effect settles the
            // steady state once the first server resolves.
            if is_real_user && _live_server.is_none() {
                // The profile bar still shows here — a real (authed) user's
                // bottom-left sidebar bar is present for the login -> fetch
                // race and the zero-server state, not just once a live server
                // is entered. Without it this onboarding hint is the only thing
                // in the sidebar, leaving the same "missing profile bar" the
                // live branch used to have. `user_panel` renders the presence
                // status dot ON the avatar, coexisting with the gateway badge.
                let user = user_panel(
                    user_status,
                    joined_voice,
                    authed,
                    user_profile_mode,
                    user_notif_mode,
                    user_shortcuts_mode,
                    muted,
                    deafened,
                );
                return ui! {
                    view(style = sidebar_style.clone()) {
                        text(style = cat_style.clone()) {
                            "No server selected — create or join one to get started."
                                .to_string()
                        }
                        user
                    }
                };
            }
            let sidebar_header_style = sidebar_header_style.clone();
            let sidebar_title_style = sidebar_title_style.clone();
            let chevron_style = chevron_style.clone();
            let cat_style = cat_style.clone();
            let bug_reports_open = bug_reports_mode.clone();
            let channel = active_channel;

            // Live (backend) mode: build the live channel rows up front
            // (harmless when not in live mode — `live_channels` is empty).
            let live = live_channels.get();
                // Static row style mirroring the mock channel rows' geometry
                // (34px pill, inset 8px) so live rows line up with the mock
                // ones. `row_style` is a `Fn` closure, so it can be captured
                // by the per-row `ui!` below without being moved.
                let live_row_style = move || StyleRules {
                    height: Some(px(34.0)),
                    margin_left: Some(px(8.0)),
                    margin_right: Some(px(8.0)),
                    padding_left: Some(px(12.0)),
                    padding_right: Some(px(8.0)),
                    background: Some(col("transparent")),
                    border_top_left_radius: Some(px(8.0)),
                    border_top_right_radius: Some(px(8.0)),
                    border_bottom_left_radius: Some(px(8.0)),
                    border_bottom_right_radius: Some(px(8.0)),
                    // Positioning ancestor for the absolutely-positioned mention
                    // badge on the row's right edge (the mock rows do the same).
                    position: Some(Position::Relative),
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    ..Default::default()
                };
                let live_glyph_style = move || StyleRules {
                    width: Some(px(20.0)),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    flex_shrink: Some(grow(0.0)),
                    font_size: Some(px(15.0)),
                    color: Some(col(TEXT_MUTED)),
                    ..Default::default()
                };
                let live_label_style = move || StyleRules {
                    flex_grow: Some(grow(1.0)),
                    flex_shrink: Some(grow(0.0)),
                    font_size: Some(px(15.0)),
                    color: Some(col(TEXT_CHANNEL)),
                    ..Default::default()
                };
                if _live_server.is_some() {
                    // Build the live channel rows here, inside the live arm, so
                    // they're a local of this block. Splatting a captured outer
                    // `Vec<Element>` out of the inner `ui!` closure would be a
                    // move out of an `Fn` closure (`Element` is not `Clone`).
                    let live_sel = active_live_channel.clone();
                    // The live server id — read by the channel-row select
                    // handler to key the persisted last view (server+channel).
                    let live_sid = active_live_server.clone();
                    let gw = gateway.clone();
                    let live_unread_map = live_unread.clone();
                    // Scroll-back state for the live list, captured for the
                    // channel-select handler: selecting a channel re-arms the
                    // "follow the live tail" mode (the list starts at the
                    // bottom) and clears the scroll handle the old pane had
                    // captured, so the fresh pane's `on_handle` callback can
                    // store its own.
                    let live_at_bottom_row = live_at_bottom.clone();
                    let live_scroll_handle_row = live_scroll_handle.clone();
                    // The channel-row management-menu state (which hovered row's
                    // Rename/Delete popup is open), captured for `chan_actions`
                    // and the per-row hover handlers.
                    let chan_menu = live_chan_menu.clone();
                    // The roster (already fetched when a live server entered) —
                    // read here ONLY to pick which menu rows RENDER (the
                    // backend re-checks on commit). Plain read of the acting
                    // id is fine at this build site: the sidebar rebuilds on
                    // live-server/channel changes and on login (`authed`).
                    let live_mbrs = live_members.clone();
                    let chan_self_id =
                        crate::auth::CURRENT_USER_ID.load(std::sync::atomic::Ordering::SeqCst);
                    // Shared style for the live channel row action icon buttons
                    // (hover gear) and the new-channel "+" header action. A
                    // rules *closure* (like `live_row_style` below) so the
                    // gear's nested `move` render closures can capture it and
                    // re-build it on every re-render (StyleRules is not `Copy`)
                    // without the E0507 "move out of Fn" the per-use `.clone()`
                    // hits when the style itself is a plain local.
                    let chan_btn_style = move || StyleRules {
                        width: Some(px(22.0)),
                        height: Some(px(22.0)),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        border_top_left_radius: Some(px(6.0)),
                        border_top_right_radius: Some(px(6.0)),
                        border_bottom_left_radius: Some(px(6.0)),
                        border_bottom_right_radius: Some(px(6.0)),
                        flex_shrink: Some(runtime_core::Tokenized::Literal(0.0)),
                        ..Default::default()
                    };
                    // The per-row name is shared into both `move` tap handlers via
                    // an `Rc<String>` (`Clone`), so neither handler moves a `String`
                    // out of this `Fn` closure (E0507) — the established idiom for
                    // non-`Copy` data in `'static` handlers here.
                    let chan_actions = |cid: i64, nm: Rc<String>, u_count: i64, u_mentions: i64, hovered: Signal<bool>, row_ref: Ref<ViewHandle>| -> Element {
                        // `nm` is an `Rc` (`Clone`, not `Copy`): each `move` tap
                        // handler captures its own clone, and reads the name via
                        // `(*nm)` without ever moving the `String` out of this
                        // `Fn` render closure (E0507).
                        let nm_rename = nm.clone();
                        let nm_delete = nm.clone();
                        // The unread badge, fed by the REAL per-channel
                        // `unread_counts` data: a red count pill whenever
                        // `u_count > 0` (the same pill shape the mock rows and
                        // this row used for mentions). When the channel also
                        // has an unread @mention, the pill shows the
                        // mention count — the exact distinction the mock
                        // sidebar makes for "has an unread @mention".
                        // Mirror the mock sidebar's badge treatment exactly
                        // (see the mock `total_mentions`/`total_unread` branch
                        // above): a red numbered pill for an unread @mention
                        // (showing the mention count), a small plain white dot
                        // for unread with no mention, and nothing for a read
                        // channel. The live row's data is the real
                        // `unread_counts` pair, not the mock `mentions` field.
                        let badge = if u_mentions > 0 {
                            Some(ui! {
                                view(style = StyleRules {
                                    min_width: Some(px(16.0)),
                                    height: Some(px(16.0)),
                                    background: Some(col(BADGE)),
                                    border_top_left_radius: Some(px(8.0)),
                                    border_top_right_radius: Some(px(8.0)),
                                    border_bottom_left_radius: Some(px(8.0)),
                                    border_bottom_right_radius: Some(px(8.0)),
                                    align_items: Some(AlignItems::Center),
                                    justify_content: Some(JustifyContent::Center),
                                    padding_left: Some(px(4.0)),
                                    padding_right: Some(px(4.0)),
                                    flex_shrink: Some(runtime_core::Tokenized::Literal(0.0)),
                                    ..Default::default()
                                }) {
                                    text(style = StyleRules {
                                        font_size: Some(px(11.0)),
                                        font_weight: Some(FontWeight::Bold),
                                        color: Some(col(TEXT)),
                                        ..Default::default()
                                    }) { u_mentions.to_string() }
                                }
                            })
                        } else if u_count > 0 {
                            // Plain dot: unread but no mention — Discord shows a
                            // small white indicator instead of the red badge.
                            Some(ui! {
                                view(style = StyleRules {
                                    width: Some(px(10.0)),
                                    height: Some(px(10.0)),
                                    background: Some(col(TEXT)),
                                    border_top_left_radius: Some(px(5.0)),
                                    border_top_right_radius: Some(px(5.0)),
                                    border_bottom_left_radius: Some(px(5.0)),
                                    border_bottom_right_radius: Some(px(5.0)),
                                    flex_shrink: Some(runtime_core::Tokenized::Literal(0.0)),
                                    ..Default::default()
                                }) {}
                            })
                        } else {
                            None
                        };
                        // The ALWAYS-VISIBLE pencil + trash icons are gone: a
                        // single settings GEAR renders only while the row is
                        // hovered (per-row `hovered`, the `message_action_bar`
                        // idiom), and only for users whose own roster role
                        // clears the backend bars — the menu below still arms
                        // the existing rename/delete modals (permission checks
                        // and their confirm/commit steps stay the backend's).
                        let my_role = live_mbrs
                            .get()
                            .iter()
                            .find(|m| m.user_id == chan_self_id)
                            .map(|m| role_rank(m.role.clone()))
                            .unwrap_or(-1);
                        let menu_items: Vec<(String, bool, Rc<dyn Fn()>)> = {
                            let mut items: Vec<(String, bool, Rc<dyn Fn()>)> =
                                Vec::<(String, bool, Rc<dyn Fn()>)>::new();
                            if my_role >= 1 {
                                items.push((
                                    "Rename".to_string(),
                                    false,
                                    Rc::new(move || {
                                        rc_cid.set(Some(cid));
                                        rc_name.set((*nm_rename).clone());
                                        rc_busy.set(false);
                                        rc_err.set(String::new());
                                        rc_open.set(true);
                                    }),
                                ));
                            }
                            if my_role >= 2 {
                                items.push((
                                    "Delete".to_string(),
                                    true,
                                    Rc::new(move || {
                                        dc_cid.set(Some(cid));
                                        dc_name.set((*nm_delete).clone());
                                        dc_busy.set(false);
                                        dc_err.set(String::new());
                                        dc_open.set(true);
                                    }),
                                ));
                            }
                            items
                        };
                        let has_menu = !menu_items.is_empty();
                        let menu = action_menu(chan_menu, cid, row_ref, menu_items);
                        // The gear: mounted only while hovered (and only when
                        // this row has any menu entry at all — `has_menu` is
                        // the Copy snapshot computed BEFORE `menu_items` is
                        // moved into `action_menu` above, so the gear's `Fn`
                        // render closure borrows a bool, never the Vec), and a
                        // tap opens this row's Rename/Delete menu.
                        let gear = ui! {
                            switch(
                                move || hovered.get() && has_menu,
                                move |on: &bool| {
                                    if !*on {
                                        ui! {
                                            view(style = StyleRules {
                                                width: Some(px(0.0)),
                                                height: Some(px(0.0)),
                                                ..Default::default()
                                            }) {}
                                        }
                                    } else {
                                        ui! {
                                            view(style = chan_btn_style(), test_id = "live-channel-gear") {
                                                icon(SETTINGS).color(|| Color::from(TEXT_MUTED)).size(14.0)
                                            }
                                            .on_touch(move |ev| {
                                                if ev.phase == TouchPhase::Ended {
                                                    chan_menu.set(Some(cid));
                                                }
                                                TouchResponse::CONSUMED
                                            })
                                        }
                                    }
                                },
                            )
                        };
                        ui! {
                            view(style = StyleRules {
                                position: Some(Position::Absolute),
                                top: Some(px(6.0)),
                                right: Some(px(8.0)),
                                flex_direction: Some(FlexDirection::Row),
                                align_items: Some(AlignItems::Center),
                                gap: Some(px(2.0)),
                                ..Default::default()
                            }) {
                                if let Some(b) = badge {
                                    { b }
                                }
                                { gear }
                                { menu }
                            }
                        }
                    };
                    let live_rows: Vec<Element> = live
                        .iter()
                        .map(|c| {
                            // '#' for text channels, a speaker icon for voice —
                            // the same icons the mock rows use below.
                            let name = c.name.clone();
                            let is_text = c.kind == "text";
                            let cid = c.id;
                            // `live_scroll_handle_row` is an `Rc` (not `Copy`); the
                            // row's `on_touch(move …)` closure moves it, so each
                            // row needs its own clone (the `nm: Rc<String>` idiom
                            // a few lines up) — the first row would otherwise
                            // move the single shared binding out of this `FnMut`
                            // and the second row hits E0507.
                            let live_scroll_handle_row = live_scroll_handle_row.clone();
                            // The REAL per-channel unread state from the backend
                            // `unread_counts` fetch (fetched when the live server
                            // was selected, bumped per new inbound frame, zeroed
                            // when the channel is opened). Drives the row's
                            // badge: red count pill when `count > 0` with an
                            // unread @mention, a plain dot when `count > 0`
                            // without one, nothing when fully read — the exact
                            // visual contract the mock rows use for
                            // `c.mentions` / `c.unread`.
                            let (u_count, u_mentions) = *live_unread_map
                                .borrow()
                                .get(&cid)
                                .unwrap_or(&(0, 0));
                            // Reactive, per-row — the mock path's rows work the
                            // same way: background fills with the active colour
                            // when selected, and the glyph/label go semi-bold
                            // (label also brightens) — nothing changes when no
                            // live channel is active, so the unselected rows
                            // render exactly as before.
                            let row_style = move || StyleRules {
                                background: Some(col(if live_sel.get() == Some(cid) {
                                    BG_ACTIVE
                                } else {
                                    "transparent"
                                })),
                                ..live_row_style()
                            };
                            let glyph_style = move || StyleRules {
                                font_weight: Some(if live_sel.get() == Some(cid) {
                                    FontWeight::SemiBold
                                } else {
                                    FontWeight::Normal
                                }),
                                ..live_glyph_style()
                            };
                            let label_style = move || StyleRules {
                                font_weight: Some(if live_sel.get() == Some(cid) {
                                    FontWeight::SemiBold
                                } else {
                                    FontWeight::Normal
                                }),
                                // A channel with real unread messages brightens
                                // its name (the mock rows do the same for
                                // `c.unread > 0`).
                                color: Some(col(if live_sel.get() == Some(cid) || u_count > 0 {
                                    TEXT
                                } else {
                                    TEXT_CHANNEL
                                })),
                                ..live_label_style()
                            };
                            // The right-side action cluster for this row (mention
                            // pill + hover-gated gear + the gear's Rename/Delete
                            // menu), built once per row. `chan_actions` is
                            // `Copy` (all `Copy` captures), so calling it here
                            // does not move it out of the `switch` render
                            // closure. `hovered` is minted PER ROW here (not
                            // threaded through the shared row builder — the
                            // per-row idiom `message_action_bar` uses) so each
                            // row's gear tracks its own hover, and `row_ref`
                            // anchors the menu portal to this row.
                            let hovered = signal(false);
                            let row_ref: Ref<ViewHandle> = Ref::<ViewHandle>::new();
                            let actions =
                                chan_actions(cid, Rc::new(name.clone()), u_count, u_mentions, hovered, row_ref);
                            let row = if is_text {
                                // Voice rows stay non-clickable for now — a later
                                // task handles voice selection. Text rows select
                                // the channel and request its history over the
                                // gateway (the echo/History frames land in
                                // `gateway.incoming()`). The right-side action
                                // cluster (mention pill + rename/delete buttons)
                                // is `chan_actions`, built per row above.
                                ui! {
                                    view(style = row_style) {
                                        icon(HASH)
                                            .color(|| Color::from(TEXT_MUTED))
                                            .size(14.0)
                                        text(style = label_style) { name }
                                        { actions }
                                    }
                                    .on_touch(move |ev| {
                                        if ev.phase == TouchPhase::Ended {
                                            live_sel.set(Some(cid));
                                            // Persist THIS channel (and its
                                            // server) as the account's last
                                            // view: the tap path is the
                                            // canonical "selection", so the
                                            // saved value always tracks where
                                            // the user last clicked — a
                                            // non-first server AND a non-first
                                            // channel.
                                            if let Some(sid) = live_sid.get() {
                                                crate::auth::save_last_view_channel(sid, cid);
                                            }
                                            // Re-arm the live list's scroll state for the
                                            // freshly opened channel: start "following the
                                            // live tail" (at the bottom) and drop the old
                                            // pane's scroll handle so the new pane's
                                            // `on_handle` callback can capture its own.
                                            live_at_bottom_row.set(true);
                                            *live_scroll_handle_row.borrow_mut() = None;
                                            gw.send(
                                                discord_server::server_settings::WsClientMsg::History {
                                                    // Initial load: the latest page (no
                                                    // scrollback cursor yet).
                                                    channel_id: cid,
                                                    // `before_id: None` = fetch the
                                                    // latest page (no before-cursor) —
                                                    // the initial load on channel select.
                                                    before_id: None,
                                                },
                                            );
                                        }
                                        TouchResponse::CONSUMED
                                    })
                                    .bind(row_ref)
                                    .on_hover(move |on| hovered.set(on))
                                }
                            } else {
                                ui! {
                                    view(style = row_style) {
                                        icon(VOLUME_2)
                                            .color(|| Color::from(TEXT_MUTED))
                                            .size(14.0)
                                        text(style = label_style) { name }
                                        { actions }
                                    }
                                    .bind(row_ref)
                                    .on_hover(move |on| hovered.set(on))
                                }
                            };
                            row
                        })
                        .collect();
                    // Resolve the channels content as a local `Element` *outside*
                    // the `ui!` below. An `if/else` written directly inside `ui!`
                    // wraps its branches in a reactivity closure that captures
                    // `live_rows`, and that closure is `Fn` (re-runs) so it can't
                    // move the captured `Vec<Element>` into `view { live_rows }`
                    // (`Element` is not `Clone`). The mock branch avoids this by
                    // splatting `rows` directly, never in a conditional.
                    let live_content: Element = if live_rows.is_empty() {
                        ui! {
                            text(style = cat_style.clone()) {
                                "No channels yet".to_string()
                            }
                        }
                    } else {
                        ui! {
                            view { live_rows }
                        }
                    };
                    // The "New Channel" "+" button on the live header's right
                    // edge: opens the create-channel modal, resetting its
                    // per-open state so a re-open starts fresh (the same
                    // reset-on-open idiom the rail's Join flow uses).
                    let add_btn = ui! {
                        view(style = chan_btn_style.clone(), test_id = "live-channel-add") {
                            icon(PLUS).color(|| Color::from(TEXT_MUTED)).size(14.0)
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                cc_name.set(String::new());
                                cc_kind.set("text".to_string());
                                cc_busy.set(false);
                                cc_err.set(String::new());
                                cc_open.set(true);
                            }
                            TouchResponse::CONSUMED
                        })
                    };
                    // The "Invite People" (USERS) icon on the live header's
                    // right edge (next to the gear / + buttons): opens the
                    // LIVE invite overlay. On open it resets the expiry picker
                    // to the default (30 min) and clears any previous mint /
                    // error, so a re-open starts fresh — the same
                    // reset-on-open idiom as the create-channel "+" above.
                    let invite_btn = ui! {
                        view(style = chan_btn_style.clone(), test_id = "live-invite") {
                            icon(USERS).color(|| Color::from(TEXT_MUTED)).size(14.0)
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                invite_expiry.set(0);
                                invite_created.set(None);
                                invite_err.set(String::new());
                                invite_mode.set(true);
                            }
                            TouchResponse::CONSUMED
                        })
                    };
                    // The "Server Settings" gear on the live header's right edge
                    // (next to the "+" button): opens the LIVE server-settings
                    // overlay (rename / icon / transfer ownership). On open it
                    // seeds the name + icon inputs from the active live server
                    // (looked up in `live_servers`) and resets the transfer /
                    // error / busy state, so a re-open starts fresh — the same
                    // reset-on-open idiom as the create-channel "+" above.
                    let gear_btn = ui! {
                        view(style = chan_btn_style.clone(), test_id = "live-server-settings") {
                            icon(SETTINGS).color(|| Color::from(TEXT_MUTED)).size(14.0)
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                if let Some(active_id) = active_live_server.get() {
                                    let cur = live_servers.get().into_iter().find(|s| s.id == active_id);
                                    if let Some(s) = cur {
                                        ss_name.set(s.name.clone());
                                        ss_icon.set(s.icon_url.clone().unwrap_or_default());
                                    }
                                }
                                ss_owner.set(String::new());
                                // Reset the add-member inputs too, so a re-open
                                // starts with an empty id and the default role.
                                ss_add_user_id.set(String::new());
                                ss_add_role.set("member".to_string());
                                ss_busy.set(false);
                                ss_err.set(String::new());
                                ss_open.set(true);
                            }
                            TouchResponse::CONSUMED
                        })
                    };
                    // The user profile bar is the bottom-left of the sidebar in
                    // BOTH branches (Discord convention). The mock branch builds
                    // it as `user` below; the live branch must do the same. Real
                    // logged-in users always take the live branch (they have a
                    // backend server), so omitting it here is the "missing
                    // bottom-left profile bar" bug: the header shows a live
                    // channel but the sidebar never renders the profile.
                    // `user_panel` renders the presence status dot ON the avatar
                    // (like real Discord), so it coexists with the gateway
                    // "Open/Closed" badge in the 72px rail — no separate text
                    // badge is needed in the sidebar slot.
                    let user = user_panel(
                        user_status,
                        joined_voice,
                        authed,
                        user_profile_mode,
                        user_notif_mode,
                        user_shortcuts_mode,
                        muted,
                        deafened,
                    );
                    ui! {
                        view(style = sidebar_style.clone()) {
                            view(style = sidebar_header_style.clone()) {
                                view(style = sidebar_title_style.clone()) {
                                    text(style = cat_style.clone()) {
                                        "SERVER".to_string()
                                    }
                                    text(style = chevron_style.clone()) {
                                        "⌃".to_string()
                                    }
                                }
                                view(style = StyleRules {
                                    flex_grow: Some(runtime_core::Tokenized::Literal(1.0)),
                                    ..Default::default()
                                }) {}
                                { invite_btn }
                                { gear_btn }
                                { add_btn }
                            }
                            // Channel list fills the available vertical space
                            // (flex_grow:1), pinning the user profile bar to
                            // the bottom of the sidebar — Discord convention,
                            // and the same layout the mock branch below uses
                            // (its `view(flex_grow){ scroll_view }` wraps `rows`
                            // before `user`). Without the grow, a short live
                            // channel list leaves the profile bar floating
                            // mid-sidebar instead of bottom-left.
                            view(style = StyleRules {
                                flex_grow: Some(runtime_core::Tokenized::Literal(1.0)),
                                overflow: Some(runtime_core::Overflow::Hidden),
                                ..Default::default()
                            }) {
                                live_content
                            }
                            user
                        }
                    }
                } else {
                    let server = SERVERS.iter().find(|x| x.id == *server_id).unwrap();
                    let mut rows: Vec<Element> = Vec::new();
            // Category sections, in data order: each section is a small
            // uppercase header, its text-channel rows, then its voice rows.
            for cat in server.categories {
                let cat_label = cat.title.to_string();
                // The category's per-category collapse signal (created up front
                // in `app_shell` scope, see the `collapsed` map). The header
                // toggles it; the row loop below re-runs when it flips (the
                // switch key fingerprints it) and re-gates the rows.
                let cat_collapsed_signal = collapsed_render
                    .borrow()
                    .get(&cat.id)
                    .cloned()
                    .unwrap_or_else(|| signal(false));
                rows.push(ui! {
                    CategoryHeader(
                        title = cat_label,
                        collapsed = cat_collapsed_signal,
                    )
                });
                if cat_collapsed_signal.get() {
                    continue; // rows for this category are collapsed
                }
                for c in cat.text {
                    let active = channel;
                    // Discord's channel rows are compact (~34px) with a
                    // rounded pill (inset 8px each side) that fills with the
                    // hover colour, and the active colour when selected — the
                    // row itself stays transparent otherwise.
                    let hovered = signal(false);
                    // Per-channel mute state, owned by the row (the same
                    // per-row `Signal` ownership as `hovered`). Starts unmuted;
                    // a press on the hover-revealed icon below flips it. It
                    // survives a sidebar rebuild within a selection (the
                    // `switch` key doesn't include it), which is the desired
                    // "my mute choice persists" behavior.
                    let muted = signal(false);
                    let row_style = move || {
                        let on = active.get() == c.id;
                        StyleRules {
                            height: Some(px(34.0)),
                            margin_left: Some(px(8.0)),
                            margin_right: Some(px(8.0)),
                            padding_left: Some(px(12.0)),
                            padding_right: Some(px(8.0)),
                            background: Some(col(if on {
                                BG_ACTIVE
                            } else if hovered.get() {
                                BG_HOVER
                            } else {
                                "transparent"
                            })),
                            border_top_left_radius: Some(px(8.0)),
                            border_top_right_radius: Some(px(8.0)),
                            border_bottom_left_radius: Some(px(8.0)),
                            border_bottom_right_radius: Some(px(8.0)),
                            // Positioning ancestor for the absolutely-positioned
                            // unread pill (default Static would anchor it to the
                            // sidebar instead of the row).
                            position: Some(Position::Relative),
                            flex_direction: Some(FlexDirection::Row),
                            align_items: Some(AlignItems::Center),
                            ..Default::default()
                        }
                    };
                    let label_style = move || StyleRules {
                        flex_grow: Some(grow(1.0)),
                        flex_shrink: Some(grow(0.0)),
                        font_size: Some(px(15.0)),
                        font_weight: Some(if active.get() == c.id {
                            FontWeight::SemiBold
                        } else {
                            FontWeight::Normal
                        }),
                        color: Some(col(if active.get() == c.id || c.unread > 0 {
                            TEXT
                        } else {
                            TEXT_CHANNEL
                        })),
                        ..Default::default()
                    };
                    let name = c.name.to_string();
                    rows.push(ui! {
                        view(style = row_style, test_id = channel_test_id(c.id)) {
                            icon(HASH)
                                .color(|| Color::from(TEXT_MUTED))
                                .size(14.0)
                            text(style = label_style) { name }
                            // Hover-revealed per-channel mute control. A `when`
                            // gated on `hovered || muted`: the icon appears only
                            // while the row is hovered, OR while the channel is
                            // already muted (the persistent red glyph is the
                            // mute affordance — Discord keeps a muted channel's
                            // marker visible, not just on hover). A press on it
                            // flips `muted`; the icon's own `on_touch` CONSUMEs
                            // so the row's channel-select `on_touch` (below) is
                            // not also fired. Deliberate per-row OPTION, not a
                            // persistent widget on every row (the graveyard
                            // anti-pattern).
                            if hovered.get() || muted.get() {
                                crate::mute_channel_toggle::MuteChannelToggle(muted)
                            }
                            // The unread pill on the row's right edge, vertically
                            // centered in the 34px row. Mock count per channel;
                            // the real value comes from the gateway later.
                            if c.mentions > 0 {
                                view(style = StyleRules {
                                    position: Some(Position::Absolute),
                                    top: Some(px(9.0)),
                                    right: Some(px(8.0)),
                                    min_width: Some(px(16.0)),
                                    height: Some(px(16.0)),
                                    background: Some(col(BADGE)),
                                    border_top_left_radius: Some(px(8.0)),
                                    border_top_right_radius: Some(px(8.0)),
                                    border_bottom_left_radius: Some(px(8.0)),
                                    border_bottom_right_radius: Some(px(8.0)),
                                    align_items: Some(AlignItems::Center),
                                    justify_content: Some(JustifyContent::Center),
                                    padding_left: Some(px(4.0)),
                                    padding_right: Some(px(4.0)),
                                    ..Default::default()
                                }) {
                                    text(style = StyleRules {
                                        font_size: Some(px(11.0)),
                                        font_weight: Some(FontWeight::Bold),
                                        color: Some(col(TEXT)),
                                        ..Default::default()
                                    }) { c.mentions.to_string() }
                                }
                            }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                active.set(c.id);
                            }
                            TouchResponse::CONSUMED
                        })
                        .on_hover(move |on| hovered.set(on))
                    });
                }
                rows.extend(voice_section(
                    cat,
                    server.id,
                    active_server.clone(),
                    joined_voice.clone(),
                    user_status.clone(),
                ));
            }
            let title = server.name.to_string();
            // Discord shows active threads as a persistent list near the bottom
            // of the sidebar (above the user bar), not interleaved with the
            // scrollable channel rows. Kept fixed (non-scrolling) so it stays
            // put regardless of channel-list scroll position.
            let thread_list_style = StyleRules {
                flex_direction: Some(FlexDirection::Column),
                padding_top: Some(px(8.0)),
                padding_left: Some(px(8.0)),
                padding_right: Some(px(8.0)),
                ..Default::default()
            };
            // The user profile is the bottom-left of the sidebar (Discord
            // convention). The member list is a separate right-side panel —
            // see `members_switch` further down — so it's not built here.
            let user = user_panel(user_status, joined_voice, authed, user_profile_mode, user_notif_mode, user_shortcuts_mode, muted, deafened);
            ui! {
                view(style = sidebar_style) {
                    view(style = sidebar_header_style) {
                        icon(CHEVRON_DOWN)
                            .color(|| Color::from(TEXT_MUTED))
                            .size(11.0)
                        text(style = sidebar_title_style) { title }
                    }
                    view(style = StyleRules {
                        flex_grow: Some(grow(1.0)),
                        overflow: Some(Overflow::Hidden),
                        ..Default::default()
                    }) {
                        scroll_view(style = StyleRules {
                            flex_grow: Some(grow(1.0)),
                            ..Default::default()
                        }) { rows }
                    }
                    // Mock thread list near the channel sidebar: a `ThreadListRow`
                    // above the user panel. Local mock data, no backend. Clicking
                    // it opens the "Bug reports" full-screen overlay (flips
                    // `bug_reports_mode`; the overlay's "Done" pill closes it).
                    view(style = thread_list_style) {
                        ThreadListRow()
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            bug_reports_open.set(!bug_reports_open.get());
                        }
                        TouchResponse::CONSUMED
                    })
                    user
                }
            }
            }
        },
    );

    SidebarParts { sidebar }
}
