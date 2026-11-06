//! `app_shell` section, split out of `app.rs`. Reads the shared state
//! through `ShellState`; returns the elements the final layout places.

use crate::theme;
use super::*;

pub(super) struct RailParts {
    pub(super) rail_style: StyleRules,
    pub(super) rail_top: Element,
    pub(super) rail_list: Element,
}

pub(super) fn build(s: &ShellState) -> RailParts {
    let ShellState { active_live_server, create_busy, create_error, create_name, create_open, friends_snap, gateway, live_dm_active, live_dm_mode, live_dms, live_servers, rail_unread, .. } = s.clone();
    // Owned copy for the tile handlers: the rail lambdas must be `'static`,
    // so they capture this clone (never the `s` reference) and hand it to
    // `enter_live_server` when a tile is tapped.
    let shell = s.clone();
    // --- server rail -----------------------------------------------------
    let rail_style = StyleRules {
        width: Some(px(72.0)),
        background: Some(col(BG_RAIL)),
        flex_direction: Some(FlexDirection::Column),
        align_items: Some(AlignItems::Center),
        gap: Some(px(10.0)),
        padding_top: Some(px(12.0)),
        // The DM button stays put; only `rail_list` below it scrolls.
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };

    // The 48px circle behind every rail action (DM, Create, Join).
    let circle_style = StyleRules {
        width: Some(px(48.0)),
        height: Some(px(48.0)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        background: Some(col(BG_SIDEBAR)),
        ..rounded(24.0)
    };
    // Discord's rail separator: a thin 32px line between sections.
    let divider_style = StyleRules {
        width: Some(px(32.0)),
        height: Some(px(1.0)),
        background: Some(col(BORDER)),
        ..Default::default()
    };

    // --- Top of the rail: Direct Messages ----------------------------------
    // Discord's rail leads with DMs, above a thin divider, before the server
    // tiles. The tile shows the same left-edge white pill as a server tile
    // when the DM view is open (`live_dm_mode`), so the selection signal
    // looks identical across the rail.
    let dm_list = live_dms.clone();
    let friends = friends_snap.clone();
    // The connection indicator sits between the DM button and the divider:
    // nothing while connected; a small yellow dot while reconnecting
    // (`Connecting`) and a red one while `Closed` / `Error`.
    let conn_indicator = switch(
        move || format!("{:?}", gateway.connection_status()),
        move |status: &String| {
            // Nothing while connected (`Open`) or before the first connect
            // reaches a steady state; a small dot only for trouble:
            // yellow while reconnecting, red while Closed / Error.
            let color = match status.as_str() {
                "Connecting" => Some(theme::WARNING),
                "Open" => None,
                // Closed and Error share the red accent.
                _ => Some(theme::DANGER),
            };
            if let Some(color) = color {
                ui! {
                    view(style = StyleRules {
                        width: Some(px(8.0)),
                        height: Some(px(8.0)),
                        background: Some(col(color)),
                        ..rounded(4.0)
                    }, test_id = "conn-dot") {}
                }
            } else {
                ui! {
                    view(style = StyleRules {
                        height: Some(px(0.0)),
                        ..Default::default()
                    }) {}
                }
            }
        },
    );
    let rail_top = ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Center),
            gap: Some(px(10.0)),
            ..Default::default()
        }) {
            view(style = StyleRules {
                width: Some(px(48.0)),
                height: Some(px(48.0)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                background: Some(col(BG_SIDEBAR)),
                // Positioning ancestor for the selected pill below.
                position: Some(Position::Relative),
                ..rounded(24.0)
            }, test_id = "rail-direct-messages") {
                // The DM selection pill on the far-left rail edge — the same
                // white pill server tiles show when selected (the rail is
                // 72px, the tile 48px centered, so left: -12 is flush).
                view(style = move || StyleRules {
                    position: Some(Position::Absolute),
                    left: Some(px(-12.0)),
                    top: Some(px(14.0)),
                    width: Some(px(4.0)),
                    height: Some(px(20.0)),
                    background: Some(col(PILL)),
                    opacity: Some(Tokenized::Literal(if live_dm_mode.get() { 1.0 } else { 0.0 })),
                    ..rounded(2.0)
                }) {}
                // The cassette: grey while DMs are closed, the chosen app
                // icon in colour while they're open (read on each toggle, so
                // an Appearance change shows the next time).
                { switch(
                    move || live_dm_mode.get(),
                    |open: &bool| ui! {
                        image(
                            src = if *open {
                                crate::app_icon::current().data_url()
                            } else {
                                crate::app_icon::rail_mono_url()
                            },
                            alt = "Direct Messages".to_string(),
                            style = StyleRules {
                                width: Some(px(32.0)),
                                height: Some(px(32.0)),
                                flex_shrink: Some(grow(0.0)),
                                ..Default::default()
                            },
                        )
                    },
                ) }
            }
            // Direct Messages. Every account is signed in, so this row always
            // drives the live DM screen (the demo DM screen is gone).
            //
            // Live: every open re-fetches the conversation list and the friend
            // list (the "Find or start a conversation" search reads it), and
            // records the open conversation as the last view so a refresh
            // re-opens it. Per-open state (`live_dm_active`, the fetched
            // history) lives in `app_shell` scope and survives a close.
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    let now_open = !live_dm_mode.get();
                    live_dm_mode.set(now_open);
                    if now_open {
                        if let Some(dm_id) = live_dm_active.get() {
                            crate::auth::save_last_view_dm(dm_id);
                        }
                        // Anchored on the app shell: the DM list and friends
                        // snapshot live there, and this handler toggles the
                        // overlay whose rebuild would drop its own node.
                        runtime_core::spawn_then_in(
                            &crate::app::shell_alive(),
                            async move {
                                (crate::backend::list_dms().await, crate::backend::list_friends().await)
                            },
                            move |dms_and_friends| {
                                let (dms, friends_list) = dms_and_friends;
                                if let Ok(dms) = dms {
                                    dm_list.set(dms);
                                }
                                if let Ok(f) = friends_list {
                                    friends.set(crate::friends_view::snapshot(f));
                                }
                            },
                        );
                    }
                }
                TouchResponse::CONSUMED
            })
            conn_indicator
            view(style = divider_style) {}
        }
    };

    // Live (backend) servers — the tiles under the divider. Clicking a row
    // enters live mode: it sets `active_live_server` (so the channel sidebar
    // swaps to the live branch) and fetches that server's real channels into
    // `live_channels`. The active row is highlighted reactively (its key is
    // `live_servers`, and the render closure also reads
    // `active_live_server` reactively).
    let live_section = switch(
        move || live_servers.get(),
        move |servers: &Vec<crate::servers::ServerSummary>| {
            let row_style = StyleRules {
                width: Some(px(48.0)),
                height: Some(px(48.0)),
                // Positioning ancestor for the active-server pill.
                position: Some(Position::Relative),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                background: Some(col(BG_SIDEBAR)),
                ..rounded(16.0)
            };
            let initial_style = StyleRules {
                font_size: Some(px(18.0)),
                font_weight: Some(FontWeight::Bold),
                color: Some(col(TEXT_CHANNEL)),
                ..Default::default()
            };
            // The generated keyed loop body moves its captures on each
            // iteration, so hoist owned copies to sit outside it.
            let shell_c = shell.clone();
            let unread_c = rail_unread.clone();
            let active_c = active_live_server.clone();
            let rows_keyed = ui! {
                for s in servers.iter(), key = s.id {
                    { rail_server_tile(s, row_style.clone(), initial_style.clone(), shell_c.clone(), unread_c.clone(), active_c.clone()) }
                }
            };
            ui! {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Column),
                    align_items: Some(AlignItems::Center),
                    gap: Some(px(10.0)),
                    ..Default::default()
                }) {
                    rows_keyed
                }
            }
        },
    );

    let rail_actions = {
        let plus_create_open = create_open.clone();
        let plus_name = create_name.clone();
        let plus_error = create_error.clone();
        let plus_busy = create_busy.clone();
        ui! {
            view(style = circle_style.clone(), test_id = "rail-create-server") {
                icon(PLUS).color(|| theme::live(theme::SUCCESS)).size(20.0)
            }
            // Add a Server: reset the per-open state (name, inline error,
            // busy) and open the create modal — the same reset-on-open
            // `create_server_modal`'s docs describe.
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    plus_name.set(String::new());
                    plus_error.set(String::new());
                    plus_busy.set(false);
                    plus_create_open.set(true);
                }
                TouchResponse::CONSUMED
            })
        }
    };

    // Servers and the add button scroll under the fixed DM button, as in
    // Discord. The column keeps the rail's 72px so the selection pill at
    // left -12 still sits on the rail edge.
    let rail_list = ui! {
        view(style = StyleRules {
            flex_grow: Some(grow(1.0)),
            width: Some(px(72.0)),
            overflow: Some(Overflow::Hidden),
            ..Default::default()
        }) {
            scroll_view(style = StyleRules {
                flex_grow: Some(grow(1.0)),
                scrollbar: Some(runtime_core::ScrollbarVisibility::Hidden),
                ..Default::default()
            }) {
                view(style = StyleRules {
                    width: Some(px(72.0)),
                    flex_direction: Some(FlexDirection::Column),
                    align_items: Some(AlignItems::Center),
                    gap: Some(px(10.0)),
                    padding_bottom: Some(px(12.0)),
                    ..Default::default()
                }) {
                    live_section
                    rail_actions
                }
            }
        }
    };

    RailParts { rail_style, rail_top, rail_list }
}

/// One server tile on the rail: its icon (or initial letter) and the active-
/// server selection pill, with a tap that enters live mode for this server.
/// Keyed by the server id in the caller's loop, so the pill and the tap
/// handler stay on the right tile when the server list changes.
fn rail_server_tile(
    s: &discord_server::servers::ServerSummary,
    base: StyleRules,
    initial_style: StyleRules,
    shell: ShellState,
    unread_buf: Rc<RefCell<HashMap<i64, (i64, i64)>>>,
    active_live: Signal<Option<i64>>,
) -> Element {
    let initial = s.name.chars().next().unwrap_or('?').to_string();
    // The uploaded icon fills the 48px tile with the tile's own
    // rounding; servers without one keep the initial letter.
    let face = match s.icon_url.clone().filter(|u| !u.is_empty()) {
        Some(url) => ui! {
            image(
                src = crate::auth::media_url(url),
                alt = s.name.clone(),
                test_id = "rail-server-icon",
                style = StyleRules {
                    width: Some(px(48.0)),
                    height: Some(px(48.0)),
                    flex_shrink: Some(grow(0.0)),
                    object_fit: Some(ObjectFit::Cover),
                    ..rounded(16.0)
                },
            )
        },
        None => ui! { text(style = initial_style.clone()) { initial } },
    };
    let sid = s.id;
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
                opacity: Some(Tokenized::Literal(if active_live.get() == Some(sid) {
                    1.0
                } else {
                    0.0
                })),
                ..rounded(2.0)
            }) {}
            { face }
        }
        // Enter live mode: select this real server and pull its
        // channels/roster/unread via the shared enter-live helper
        // (same one the Create and Join modals call). The rail
        // displays unread from `rail_unread`.
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                enter_live_server(shell.clone(), sid, unread_buf.clone())
            }
            TouchResponse::CONSUMED
        })
    }
}
