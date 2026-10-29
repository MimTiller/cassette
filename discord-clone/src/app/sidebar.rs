//! `app_shell` section, split out of `app.rs`. Reads the shared state
//! through `ShellState`; returns the elements the final layout places.

use crate::theme;
use super::*;

pub(super) struct SidebarParts {
    pub(super) sidebar: Element,
}

pub(super) fn build(s: &ShellState) -> SidebarParts {
    let ShellState { active_live_channel, active_live_server, cc_busy, cc_err, cc_kind, cc_name, cc_open, collapsed, dc_busy, dc_cid, dc_err, dc_name, dc_open, deafened, gateway, invite_created, invite_err, invite_expiry, invite_mode, joined_voice, live_at_bottom, live_chan_menu, live_channels, live_collapsed, live_members, live_mention_rev, live_scroll_handle, live_servers, live_unread, live_unread_rev, member_panel_width, muted, notif_mode, profile_mode, rc_busy, rc_cid, rc_err, rc_name, rc_open, server_menu_open, sidebar_width, ss_add_role, ss_add_user_id, ss_busy, ss_err, ss_icon, ss_name, ss_open, ss_owner, user_status, .. } = s.clone();
    // --- channel sidebar -------------------------------------------------
    // Width is driven by the `sidebar_width` signal (a thin drag handle on the
    // sidebar's right edge resizes it). A rules *closure* so the width
    // re-resolves as the signal changes — the same reactive idiom the channel
    // rows use. Captures only a `Copy` `Signal`, so the closure itself is
    // `Clone`, which the `switch` render below relies on (`sidebar_style.clone()`).
    let sidebar_style = move || {
        let (s, _m) = effective_panel_widths(sidebar_width, member_panel_width);
        let phone = mobile::is_phone();
        StyleRules {
            width: (!phone).then(|| px(s)),
            flex_grow: phone.then(|| grow(1.0)),
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
    // The `collapsed` map is `Rc<RefCell<..>>` (not `Copy`, since `RefCell`
    // isn't `Copy`). The key and render closures are both `move`, so each
    // captures its own clone; the shared `RefCell` inside keeps them in sync.
    let collapsed_key = collapsed.clone();
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
                // Collapsed live categories ride the same key (high bit set so
                // they never collide with a mock category id).
                v.extend(live_collapsed.get().iter().map(|id| (0x8000_0000 | *id as u32, true)));
                v
            };
            (
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
        move |(_voice, _live_server, _live_fp, _mention_rev, _unread_rev, _cat_collapsed, _chan_menu): &(
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
            // [Feature 16] A signed-in account with no live channel selected
            // renders the onboarding hint (below) instead of any mock render.
            if _live_server.is_none() {
                // The profile bar still shows here — a real (authed) user's
                // bottom-left sidebar bar is present for the login -> fetch
                // race and the zero-server state, not just once a live server
                // is entered. Without it this onboarding hint is the only thing
                // in the sidebar, leaving it without a profile bar. `user_panel` renders the presence
                // status dot ON the avatar, coexisting with the gateway badge.
                let user = user_panel(
                    user_status,
                    joined_voice,
                    user_profile_mode,
                    user_notif_mode,
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
            let sidebar_style = sidebar_style.clone();
            let sidebar_header_style = sidebar_header_style.clone();
            let cat_style = cat_style.clone();

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
                    // Gap between the channel-type icon and the name.
                    gap: Some(px(6.0)),
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
                    let chan_actions = |cid: i64, nm: Rc<String>, u_mentions: i64, hovered: Signal<bool>, row_ref: Ref<ViewHandle>| -> Element {
                        // `nm` is an `Rc` (`Clone`, not `Copy`): each `move` tap
                        // handler captures its own clone, and reads the name via
                        // `(*nm)` without ever moving the `String` out of this
                        // `Fn` render closure (E0507).
                        let nm_rename = nm.clone();
                        let nm_delete = nm.clone();
                        // The unread badge, fed by the REAL per-channel
                        // `unread_counts` data: a red count pill showing the
                        // mention count only when the channel has an unread
                        // @mention (a plain unread has no pill here — its
                        // unread pill sits at the row's left edge instead).
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
                            // Everyone: mark the whole channel read (message id
                            // 0 = up to its newest message, so it works for a
                            // channel whose messages were never loaded) and
                            // copy its id. The badge clears optimistically.
                            let unread = live_unread_map.clone();
                            items.push((
                                "Mark As Read".to_string(),
                                false,
                                Rc::new(move || {
                                    unread.borrow_mut().insert(cid, (0, 0));
                                    live_unread_rev.set(live_unread_rev.get() + 1);
                                    runtime_core::driver::spawn_async(async move {
                                        let _ = crate::servers::mark_read(cid, 0).await;
                                    });
                                }),
                            ));
                            items.push((
                                "Copy Channel ID".to_string(),
                                false,
                                Rc::new(move || {
                                    runtime_core::driver::spawn_async(async move {
                                        let _ = clipboard::set_text(&cid.to_string()).await;
                                    });
                                }),
                            ));
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
                                                icon(SETTINGS).color(|| theme::live(TEXT_MUTED)).size(14.0)
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
                    // Discord's order: channels outside any category first,
                    // then each category's header followed by its own
                    // channels (left out while the category is collapsed).
                    let is_cat = |c: &&crate::servers::ChannelSummary| c.kind == "category";
                    let cat_ids: Vec<i64> = live.iter().filter(is_cat).map(|c| c.id).collect();
                    let collapsed_cats = live_collapsed.get();
                    let mut ordered: Vec<&crate::servers::ChannelSummary> = live
                        .iter()
                        .filter(|c| !is_cat(c) && !c.parent_channel_id.is_some_and(|p| cat_ids.contains(&p)))
                        .collect();
                    for cat in live.iter().filter(is_cat) {
                        ordered.push(cat);
                        if !collapsed_cats.contains(&cat.id) {
                            ordered.extend(live.iter().filter(|c| !is_cat(c) && c.parent_channel_id == Some(cat.id)));
                        }
                    }
                    let live_rows: Vec<Element> = ordered
                        .into_iter()
                        .map(|c| {
                            if c.kind == "category" {
                                return live_category_header(c.id, c.name.clone(), live_collapsed);
                            }
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
                            // Reactive, per-row — the mock path's rows work the
                            // same way: background fills with the active colour
                            // when selected, and the glyph/label go semi-bold
                            // (label also brightens) — nothing changes when no
                            // live channel is active, so the unselected rows
                            // render exactly as before.
                            // Hover + selection state minted here (before the
                            // style closures below) so every closure reads the
                            // same signals the row's handlers set.
                            let hovered = signal(false);
                            let row_ref: Ref<ViewHandle> = Ref::<ViewHandle>::new();
                            // Unread with no mention shows Discord's unread
                            // pill: a small white sliver at the very left edge
                            // of the row (a mention keeps the red count pill on
                            // the right instead, inside `chan_actions`).
                            let unread_pill = ui! {
                                switch(
                                    move || u_count > 0 && u_mentions == 0,
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
                                                view(style = StyleRules {
                                                    position: Some(Position::Absolute),
                                                    // Row starts 8px inside the
                                                    // sidebar — pull the pill to
                                                    // the sidebar's left edge.
                                                    left: Some(px(-8.0)),
                                                    top: Some(px(13.0)),
                                                    width: Some(px(4.0)),
                                                    height: Some(px(8.0)),
                                                    background: Some(col(PILL)),
                                                    border_top_left_radius: Some(px(2.0)),
                                                    border_top_right_radius: Some(px(2.0)),
                                                    border_bottom_left_radius: Some(px(2.0)),
                                                    border_bottom_right_radius: Some(px(2.0)),
                                                    ..Default::default()
                                                }) {}
                                            }
                                        }
                                    },
                                )
                            };
                            let row_style = move || StyleRules {
                                background: Some(col(if live_sel.get() == Some(cid) {
                                    BG_ACTIVE
                                } else if hovered.get() {
                                    BG_HOVER
                                } else {
                                    "transparent"
                                })),
                                ..live_row_style()
                            };
                            let _glyph_style = move || StyleRules {
                                font_weight: Some(if live_sel.get() == Some(cid) {
                                    FontWeight::SemiBold
                                } else {
                                    FontWeight::Normal
                                }),
                                ..live_glyph_style()
                            };
                            let label_style = move || StyleRules {
                                font_weight: Some(if live_sel.get() == Some(cid) || u_count > 0 {
                                    FontWeight::SemiBold
                                } else {
                                    FontWeight::Normal
                                }),
                                // Inactive names sit in muted grey; a hovered
                                // row brightens them, the open channel and any
                                // channel with real unread messages go white.
                                color: Some(col(if live_sel.get() == Some(cid) || u_count > 0 {
                                    TEXT
                                } else if hovered.get() {
                                    TEXT_CHANNEL
                                } else {
                                    TEXT_MUTED
                                })),
                                // Names clamp to one line so long ones end
                                // "…" instead of pushing the action cluster.
                                flex_shrink: Some(grow(1.0)),
                                max_lines: Some(1u32),
                                ..live_label_style()
                            };
                            // The right-side action cluster for this row (mention
                            // pill + hover-gated gear + the gear's Rename/Delete
                            // menu), built once per row. `chan_actions` is
                            // `Copy` (all `Copy` captures), so calling it here
                            // does not move it out of the `switch` render
                            // closure.
                            let actions =
                                chan_actions(cid, Rc::new(name.clone()), u_mentions, hovered, row_ref);
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
                                            .color(|| theme::live(TEXT_MUTED))
                                            .size(18.0)
                                        text(style = label_style) { name }
                                        { actions }
                                        { unread_pill }
                                    }
                                    .on_touch(move |ev| {
                                        // Right-click: the channel menu (a
                                        // secondary press only ever delivers
                                        // `Began`).
                                        if ev.phase == TouchPhase::Began
                                            && runtime_shared::touch::pointer_button().opens_context_menu()
                                        {
                                            chan_menu.set(Some(cid));
                                            return TouchResponse::CONSUMED;
                                        }
                                        if ev.phase == TouchPhase::Ended {
                                            live_sel.set(Some(cid));
                                            mobile::show(mobile::Pane::Chat);
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
                                            .color(|| theme::live(TEXT_MUTED))
                                            .size(18.0)
                                        text(style = label_style) { name }
                                        { actions }
                                        { unread_pill }
                                    }
                                    .bind(row_ref)
                                    .on_hover(move |on| hovered.set(on))
                                    .on_touch(move |ev| {
                                        if ev.phase == TouchPhase::Began
                                            && runtime_shared::touch::pointer_button().opens_context_menu()
                                        {
                                            chan_menu.set(Some(cid));
                                        }
                                        TouchResponse::CONSUMED
                                    })
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
                    // The header's server-name label: clamped to one line
                    // ("…" tails it when it doesn't fit), and `max_lines`
                    // makes it shrink below its natural width so the chevron
                    // next to it keeps its room.
                    let header_label_style = move || StyleRules {
                        font_size: Some(px(15.0)),
                        font_weight: Some(FontWeight::SemiBold),
                        color: Some(col(TEXT)),
                        max_lines: Some(1u32),
                        ..Default::default()
                    };
                    // Hovering the name+chevron row previews the click that
                    // opens the server dropdown — same hover fill as rows.
                    let header_hovered = signal(false);
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
                        user_profile_mode,
                        user_notif_mode,
                        muted,
                        deafened,
                    );
                    // The header: the server's name and a chevron; a click
                    // toggles the server dropdown below it.
                    let header_name = move || {
                        let sid = active_live_server.get();
                        live_servers
                            .get()
                            .into_iter()
                            .find(|s| Some(s.id) == sid)
                            .map(|s| s.name)
                            .unwrap_or_default()
                    };
                    let server_menu = server_dropdown(
                        server_menu_open,
                        active_live_server,
                        live_servers,
                        live_members,
                        live_channels,
                        active_live_channel,
                        Rc::new(move || {
                            invite_expiry.set(0);
                            invite_created.set(None);
                            invite_err.set(String::new());
                            invite_mode.set(true);
                        }),
                        Rc::new(move || {
                            if let Some(active_id) = active_live_server.get() {
                                if let Some(s) = live_servers.get().into_iter().find(|s| s.id == active_id) {
                                    ss_name.set(s.name.clone());
                                    ss_icon.set(s.icon_url.clone().unwrap_or_default());
                                }
                            }
                            ss_owner.set(String::new());
                            ss_add_user_id.set(String::new());
                            ss_add_role.set("member".to_string());
                            ss_busy.set(false);
                            ss_err.set(String::new());
                            ss_open.set(true);
                        }),
                        Rc::new(move |kind: &str| {
                            cc_name.set(String::new());
                            cc_kind.set(kind.to_string());
                            cc_busy.set(false);
                            cc_err.set(String::new());
                            cc_open.set(true);
                        }),
                    );
                    ui! {
                        view(style = sidebar_style.clone()) {
                            view(style = StyleRules { position: Some(Position::Relative), ..sidebar_header_style.clone() }) {
                                view(style = move || StyleRules {
                                    flex_direction: Some(FlexDirection::Row),
                                    align_items: Some(AlignItems::Center),
                                    gap: Some(px(6.0)),
                                    // The name+chevron row spans the header's
                                    // full width so it is the whole click target
                                    // for the dropdown.
                                    flex_grow: Some(runtime_core::Tokenized::Literal(1.0)),
                                    overflow: Some(runtime_core::Overflow::Hidden),
                                    background: Some(col(if header_hovered.get() { BG_HOVER } else { "transparent" })),
                                    border_top_left_radius: Some(px(6.0)),
                                    border_top_right_radius: Some(px(6.0)),
                                    border_bottom_left_radius: Some(px(6.0)),
                                    border_bottom_right_radius: Some(px(6.0)),
                                    ..Default::default()
                                }, test_id = "server-header") {
                                    text(style = header_label_style.clone()) { move || header_name() }
                                    icon(CHEVRON_DOWN).color(|| theme::live(TEXT_MUTED)).size(14.0)
                                }
                                .on_hover(move |on| header_hovered.set(on))
                                .on_touch(move |ev| {
                                    if ev.phase == TouchPhase::Ended {
                                        server_menu_open.set(!server_menu_open.get());
                                    }
                                    TouchResponse::CONSUMED
                                })
                                { server_menu }
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
        },
    );

    SidebarParts { sidebar }
}

/// A live channel category: a small uppercase name with a chevron. Clicking
/// it collapses or expands the category's channels (its id joins or leaves
/// `collapsed`, which the sidebar's key reads).
fn live_category_header(id: i64, name: String, collapsed: Signal<Vec<i64>>) -> Element {
    let is_collapsed = collapsed.get().contains(&id);
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(2.0)),
            padding_top: Some(px(16.0)),
            padding_bottom: Some(px(4.0)),
            padding_left: Some(px(8.0)),
            padding_right: Some(px(16.0)),
            ..Default::default()
        }, test_id = "live-category") {
            icon(if is_collapsed { CHEVRON_RIGHT } else { CHEVRON_DOWN })
                .color(|| theme::live(TEXT_MUTED))
                .size(12.0)
            text(style = StyleRules {
                font_size: Some(px(12.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(TEXT_MUTED)),
                letter_spacing: Some(Tokenized::Literal(0.3)),
                text_transform: Some(TextTransform::Uppercase),
                ..Default::default()
            }) { name }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                let mut ids = collapsed.get();
                match ids.iter().position(|c| *c == id) {
                    Some(i) => {
                        ids.remove(i);
                    }
                    None => ids.push(id),
                }
                collapsed.set(ids);
            }
            TouchResponse::CONSUMED
        })
    }
}

/// The live server dropdown under the sidebar header (Discord's server
/// menu): Invite People, Server Settings, Create Channel, Create Category,
/// and Leave Server (not for the owner; a second click confirms). `invite`,
/// `settings` and `create` open the existing modals with their per-open
/// resets; a press anywhere else closes it (the catcher in `user_popups`).
#[allow(clippy::too_many_arguments)]
fn server_dropdown(
    open: Signal<bool>,
    server: Signal<Option<i64>>,
    servers: Signal<Vec<crate::servers::ServerSummary>>,
    members: Signal<Vec<crate::moderation::MemberInfo>>,
    channels: Signal<Vec<crate::servers::ChannelSummary>>,
    channel: Signal<Option<i64>>,
    invite: Rc<dyn Fn()>,
    settings: Rc<dyn Fn()>,
    create: Rc<dyn Fn(&str)>,
) -> Element {
    let confirm_leave = signal(false);
    switch(
        move || (open.get(), confirm_leave.get()),
        move |(is_open, confirming): &(bool, bool)| {
            if !*is_open {
                return ui! { view(style = StyleRules { width: Some(px(0.0)), height: Some(px(0.0)), ..Default::default() }) {} };
            }
            let self_id = crate::auth::CURRENT_USER_ID.load(std::sync::atomic::Ordering::SeqCst);
            let my_rank = role_rank(acting_role(&members.get(), self_id));
            let is_owner = my_rank >= 3;
            // (label, red, close after, action); `None` is a divider.
            let mut items: Vec<Option<(String, bool, bool, Rc<dyn Fn()>)>> = Vec::new();
            items.push(Some(("Invite People".to_string(), false, true, invite.clone())));
            if my_rank >= 2 {
                items.push(Some(("Server Settings".to_string(), false, true, settings.clone())));
            }
            if my_rank >= 1 {
                let (c1, c2) = (create.clone(), create.clone());
                items.push(Some(("Create Channel".to_string(), false, true, Rc::new(move || (c1)("text")))));
                items.push(Some(("Create Category".to_string(), false, true, Rc::new(move || (c2)("category")))));
            }
            if !is_owner {
                items.push(None);
                let label = if *confirming { "Click again to leave" } else { "Leave Server" };
                items.push(Some((label.to_string(), true, *confirming, Rc::new(move || {
                    if !confirm_leave.get() {
                        confirm_leave.set(true);
                        return;
                    }
                    confirm_leave.set(false);
                    let Some(sid) = server.get() else { return };
                    let request = async move {
                        let input = discord_server::server_settings::LeaveServerInput { server_id: sid };
                        discord_server::server_settings::leave_server(input).await.ok()?;
                        Some(crate::servers::list_servers().await.ok())
                    };
                    // The menu that started this closes, so anchor to the shell.
                    runtime_core::spawn_then_in(&crate::app::shell_alive(), request, move |left| {
                        if let Some(list) = left {
                            server.set(None);
                            channel.set(None);
                            channels.set(Vec::new());
                            if let Some(list) = list {
                                servers.set(list);
                            }
                        }
                    });
                }))));
            }
            let rows: Vec<Element> = items.into_iter().map(|item| match item {
                None => ui! {
                    view(style = StyleRules { height: Some(px(1.0)), margin_top: Some(px(4.0)), margin_bottom: Some(px(4.0)), margin_left: Some(px(8.0)), margin_right: Some(px(8.0)), background: Some(col(BORDER)), ..Default::default() }) {}
                },
                Some((label, red, close_after, action)) => {
                    let hovered = signal(false);
                    ui! {
                        view(style = move || StyleRules {
                            height: Some(px(32.0)),
                            padding_left: Some(px(8.0)),
                            padding_right: Some(px(8.0)),
                            margin_left: Some(px(6.0)),
                            margin_right: Some(px(6.0)),
                            flex_direction: Some(FlexDirection::Row),
                            align_items: Some(AlignItems::Center),
                            background: Some(col(if hovered.get() { if red { theme::DANGER_STRONG } else { theme::ACCENT_HOVER } } else { "transparent" })),
                            border_top_left_radius: Some(px(4.0)),
                            border_top_right_radius: Some(px(4.0)),
                            border_bottom_left_radius: Some(px(4.0)),
                            border_bottom_right_radius: Some(px(4.0)),
                            ..Default::default()
                        }) {
                            text(style = move || StyleRules {
                                font_size: Some(px(14.0)),
                                font_weight: Some(FontWeight::SemiBold),
                                color: Some(col(if red && !hovered.get() { theme::DANGER } else { TEXT })),
                                ..Default::default()
                            }) { label.clone() }
                        }
                        .on_hover(move |on| hovered.set(on))
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                (action)();
                                if close_after || !red {
                                    open.set(false);
                                }
                            }
                            TouchResponse::CONSUMED
                        })
                    }
                }
            }).collect();
            ui! {
                view(style = StyleRules {
                    position: Some(Position::Absolute),
                    left: Some(px(8.0)),
                    top: Some(px(50.0)),
                    width: Some(px(220.0)),
                    z_index: Some(super::popups::POPUP_Z),
                    flex_direction: Some(FlexDirection::Column),
                    padding_top: Some(px(6.0)),
                    padding_bottom: Some(px(6.0)),
                    background: Some(col(theme::BG_FLOATING)),
                    border_top_left_radius: Some(px(6.0)),
                    border_top_right_radius: Some(px(6.0)),
                    border_bottom_left_radius: Some(px(6.0)),
                    border_bottom_right_radius: Some(px(6.0)),
                    ..Default::default()
                }) { rows }
                .on_touch(|_| TouchResponse::CONSUMED)
            }
        },
    )
}
