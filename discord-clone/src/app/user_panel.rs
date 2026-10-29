//! The bottom-left user panel and the voice channel section.
//! Split out of `app.rs`; `use super::*` keeps app.rs's imports and shared
//! colors/helpers in scope.

use crate::theme;
use super::*;

/// The bottom-left user profile section, as Discord has it: a single row —
/// avatar (with status dot) and name/status on the left, and the mic-mute,
/// deafen, and settings-gear icon buttons on the right. Identity comes from
/// the data model's `USER` constant; the live presence status comes from the
/// `user_status` signal, so the strip can show Idle / DND without touching
/// the data.
///
/// Clicking the avatar opens the status menu (`status_menu`): Online, Idle,
/// Do Not Disturb, Invisible, and Edit Profile, which flips `profile_mode`
/// to show User Settings at its Profile section. Log Out and the keyboard shortcuts are not repeated here — both
/// are rows in the User Settings sidebar the gear opens ("Log Out" under the
/// divider; "Keybinds" among the APP SETTINGS sections).
///
/// While the user is in a voice channel, the name + status line becomes a
/// voice-connection line: a green "Voice Connected" (Discord's actual text),
/// the channel name beneath it, and a **Leave** button that disconnects.
/// The whole strip is wrapped in a `switch` keyed on `(status, voice_name?)`
/// so any of those changes re-renders the strip (the key is value-deduped,
/// like every other `switch` here).
pub(super) fn user_panel(
    user_status: Signal<Status>,
    joined_voice: Signal<Option<(u32, u32)>>,
    profile_mode: Signal<bool>,
    notif_mode: Signal<bool>,
    muted: Signal<bool>,
    deafened: Signal<bool>,
    status_menu: Signal<bool>,
    gateway: server::UseSocket<discord_server::server_settings::WsServerMsg, discord_server::server_settings::WsClientMsg>,
) -> Element {
    switch(
        move || (user_status.get(), voice_name_for(joined_voice.get())),
        move |(status, voice_name): &(Status, Option<String>)| {
            let user = USER;
            // Real signed-in identity: when genuinely logged in (not guest), the
            // backend's username was stored in `CURRENT_USERNAME` at login and
            // outlives this screen. Guest mode leaves it empty, so the strip
            // falls back to the mock `USER` identity ("Nova") there. The
            // shown name follows display-name changes (see `shown`, below).
            let real_name = crate::auth::current_username();
            let username = signal(real_name.clone());
            // When the signed-in account has an uploaded avatar (shared
            // client registry, fed by login / `list_members`), the profile
            // strip's chip draws the image instead of the initial. Read
            // before `real_name` moves into the display-name branch below.
            let avatar_img = crate::auth::avatar_for(real_name.as_str());
            let display_name = if !real_name.is_empty() {
                crate::auth::shown_name(&real_name)
            } else {
                user.name.to_string()
            };
            let voice_name = voice_name.clone();
            let panel_style = StyleRules {
                // The status menu floats above the panel.
                position: Some(Position::Relative),
                padding_top: Some(px(8.0)),
                padding_bottom: Some(px(8.0)),
                padding_left: Some(px(8.0)),
                padding_right: Some(px(8.0)),
                gap: Some(px(8.0)),
                flex_direction: Some(FlexDirection::Column),
                background: Some(col(USER_BAR)),
                ..Default::default()
            };
            let row_style = StyleRules {
                min_height: Some(px(32.0)),
                gap: Some(px(8.0)),
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                ..Default::default()
            };
            let avatar_style = StyleRules {
                width: Some(px(32.0)),
                height: Some(px(32.0)),
                position: Some(Position::Relative),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                background: Some(col(user.color)),
                // Fixed 32px circle: never squeeze it when the row is tight.
                flex_shrink: Some(grow(0.0)),
                ..rounded(16.0)
            };
            let initial_style = StyleRules {
                font_size: Some(px(14.0)),
                font_weight: Some(FontWeight::Bold),
                color: Some(col(TEXT)),
                ..Default::default()
            };
            let dot_style = StyleRules {
                position: Some(Position::Absolute),
                right: Some(px(0.0)),
                bottom: Some(px(0.0)),
                width: Some(px(11.0)),
                height: Some(px(11.0)),
                background: Some(col(status.color())),
                border_top_width: Some(border_width(2.0)),
                border_right_width: Some(border_width(2.0)),
                border_bottom_width: Some(border_width(2.0)),
                border_left_width: Some(border_width(2.0)),
                border_top_color: Some(col(USER_BAR)),
                border_right_color: Some(col(USER_BAR)),
                border_bottom_color: Some(col(USER_BAR)),
                border_left_color: Some(col(USER_BAR)),
                ..rounded(6.0)
            };
            let text_stack = StyleRules {
                flex_direction: Some(FlexDirection::Column),
                flex_grow: Some(grow(1.0)),
                // Only this column absorbs row shrinkage (the avatar and the
                // icon buttons are fixed): the name must keep ~40px so "you"
                // renders in full instead of clipping to "yo".
                min_width: Some(px(40.0)),
                overflow: Some(Overflow::Hidden),
                ..Default::default()
            };
            let name_style = StyleRules {
                font_size: Some(px(14.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(TEXT)),
                ..Default::default()
            };
            let initial = display_name.chars().next().unwrap_or(' ').to_string();
            let name = display_name;

            // In a voice channel? Replace the name/status pair with the
            // Discord-style connection line: green "Voice Connected" + the
            // channel name, and a Leave button.
            let second_line = match voice_name {
                Some(ref channel) => {
                    let vc_style = StyleRules {
                        font_size: Some(px(10.0)),
                        font_weight: Some(FontWeight::SemiBold),
                        color: Some(col(theme::SUCCESS)),
                        ..Default::default()
                    };
                    let ch_style = StyleRules {
                        font_size: Some(px(11.0)),
                        color: Some(col(TEXT_MUTED)),
                        overflow: Some(Overflow::Hidden),
                        ..Default::default()
                    };
                    ui! {
                        text(style = vc_style) { "Voice Connected".to_string() }
                        text(style = ch_style) { channel.clone() }
                    }
                }
                None => {
                    let status_style = StyleRules {
                        font_size: Some(px(12.0)),
                        color: Some(col(TEXT_MUTED)),
                        ..Default::default()
                    };
                    let status = status.name().to_string();
                    ui! {
                        text(style = status_style) { status }
                    }
                }
            };

            // The Leave button only exists while connected. Built as a
            // view + on_touch — the app's standard click pattern (the
            // runtime's `button` helper takes a fixed `label` prop, which
            // doesn't fit a styled pill; a view gives full style control).
            let leave_btn = match voice_name {
                Some(_) => {
                    let leave_style = StyleRules {
                        height: Some(px(24.0)),
                        padding_left: Some(px(6.0)),
                        padding_right: Some(px(6.0)),
                        background: Some(col(BG_SIDEBAR)),
                        color: Some(col(TEXT)),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        font_size: Some(px(11.0)),
                        font_weight: Some(FontWeight::SemiBold),
                        flex_shrink: Some(grow(0.0)),
                        ..rounded(6.0)
                    };
                    // `None` clears the join; the sidebar `switch` key
                    // includes `joined_voice`, so the voice rows rebuild too.
                    let leave = joined_voice.clone();
                    let leave_text_style = StyleRules {
                        color: Some(col(TEXT)),
                        font_size: Some(px(11.0)),
                        font_weight: Some(FontWeight::SemiBold),
                        ..Default::default()
                    };
                    Some(ui! {
                        view(style = leave_style) {
                            text(style = leave_text_style) { "Leave".to_string() }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                leave.set(None);
                            }
                            TouchResponse::CONSUMED
                        })
                    })
                }
                None => None,
            };

            // Click the avatar to open the status menu. The touch target is
            // the avatar only (not the whole row) so the Leave button keeps
            // its own region while connected.
            let menu_toggle = status_menu;
            // When the signed-in account has an uploaded avatar, the image
            // sits under the status dot (same 32px circle, cover-cropped);
            // the initial-circle branch renders exactly as before.
            let avatar = if let Some(url) = avatar_img {
                ui! {
                    view(style = StyleRules {
                        width: Some(px(32.0)),
                        height: Some(px(32.0)),
                        position: Some(Position::Relative),
                        flex_shrink: Some(grow(0.0)),
                        ..rounded(16.0)
                    }) {
                        image(
                            src = crate::auth::media_url(url.clone()),
                            alt = "Your avatar",
                            style = StyleRules {
                                width: Some(px(32.0)),
                                height: Some(px(32.0)),
                                flex_shrink: Some(grow(0.0)),
                                object_fit: Some(ObjectFit::Cover),
                                ..rounded(16.0)
                            },
                        )
                        view(style = dot_style) {}
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            menu_toggle.set(!menu_toggle.get());
                        }
                        TouchResponse::CONSUMED
                    })
                }
            } else {
                ui! {
                    view(style = avatar_style) {
                        text(style = initial_style) { initial }
                        view(style = dot_style) {}
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            menu_toggle.set(!menu_toggle.get());
                        }
                        TouchResponse::CONSUMED
                    })
                }
            };
            // Mic / deafen / settings icon buttons (Discord's user-panel
            // voice actions). The signals are created in `app_shell` and
            // owned here — pure local UI state, no backend call. The mic
            // swaps to a slashed glyph when muted; Lucide has no
            // headphones-off, so deafened colors the icon red instead.
            // 18px icons: the single row must fit the avatar, the name
            // column and the three icon buttons in the 224px the 240px
            // sidebar leaves after the panel's 8px side padding.
            let icon_btn_style = StyleRules {
                width: Some(px(18.0)),
                height: Some(px(18.0)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                flex_shrink: Some(grow(0.0)),
                ..Default::default()
            };
            let muted_state = muted.clone();
            let mic_btn = ui! {
                view(style = icon_btn_style.clone(), test_id = "user-mic") {
                    if muted_state.get() {
                        icon(MIC_OFF).color(|| theme::live(TEXT_MUTED)).size(15.0)
                    } else {
                        icon(MIC).color(|| theme::live(TEXT_MUTED)).size(15.0)
                    }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        muted_state.set(!muted_state.get());
                    }
                    TouchResponse::CONSUMED
                })
            };
            let deafened_state = deafened.clone();
            let deafen_btn = ui! {
                view(style = icon_btn_style.clone(), test_id = "user-deafen") {
                    if deafened_state.get() {
                        icon(HEADPHONES).color(|| theme::live(theme::DANGER)).size(15.0)
                    } else {
                        icon(HEADPHONES).color(|| theme::live(TEXT_MUTED)).size(15.0)
                    }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        deafened_state.set(!deafened_state.get());
                    }
                    TouchResponse::CONSUMED
                })
            };
            // Settings gear: opens the full-screen User Settings overlay
            // (the same build-once/toggle idiom as before). Log Out and the
            // Keybinds section live in that screen's sidebar, so the panel
            // itself carries only the three voice affordances.
            let notif_open = notif_mode.clone();
            let settings_btn = ui! {
                view(style = icon_btn_style.clone(), test_id = "user-settings") {
                    icon(SETTINGS).color(|| theme::live(TEXT_MUTED)).size(15.0)
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        notif_open.set(true);
                    }
                    TouchResponse::CONSUMED
                })
            };
            // One row (Discord's user panel): avatar + name/status pushed
            // left by the growing text column, the three icon buttons at the
            // right; the Leave button (voice only) sits just before them.
            let menu = status_menu_view(status_menu, user_status, profile_mode, gateway.clone());
            ui! {
                view(style = panel_style) {
                    menu
                    view(style = row_style) {
                        avatar
                        view(style = text_stack) {
                            text(style = name_style) {
                                move || {
                                    crate::mentions::revision();
                                    let u = username.get();
                                    if u.is_empty() { name.clone() } else { crate::auth::shown_name(&u) }
                                }
                            }
                            second_line
                        }
                        if let Some(btn) = leave_btn {
                            btn
                        }
                        mic_btn
                        deafen_btn
                        settings_btn
                    }
                }
            }
        },
    )
}

/// Show `st` as the user's status; Do Not Disturb also silences
/// notifications.
pub(super) fn set_status(user_status: Signal<Status>, st: Status) {
    crate::notify::set_do_not_disturb(st == Status::Dnd);
    user_status.set(st);
}

/// The status menu the avatar opens (Discord's status popout): the four
/// statuses, each with its dot, then Edit Profile. It floats above the user
/// panel at the popup level, so the click-outside layer closes it.
fn status_menu_view(
    open: Signal<bool>,
    user_status: Signal<Status>,
    profile_mode: Signal<bool>,
    gateway: server::UseSocket<discord_server::server_settings::WsServerMsg, discord_server::server_settings::WsClientMsg>,
) -> Element {
    switch(
        move || open.get(),
        move |shown: &bool| {
            if !*shown {
                return ui! {
                    view(style = StyleRules {
                        width: Some(px(0.0)),
                        height: Some(px(0.0)),
                        ..Default::default()
                    }) {}
                };
            }
            let current = user_status.peek();
            let row = |st: Status| {
                let gateway = gateway.clone();
                status_menu_row(st.name(), Some(st.color()), st.hint(), st == current, move || {
                    open.set(false);
                    set_status(user_status, st);
                    gateway.send(discord_server::server_settings::WsClientMsg::SetStatus {
                        status: st.wire().to_string(),
                    });
                })
            };
            ui! {
                view(style = StyleRules {
                    position: Some(Position::Absolute),
                    left: Some(px(8.0)),
                    right: Some(px(8.0)),
                    bottom: Some(px(56.0)),
                    z_index: Some(super::popups::POPUP_Z),
                    flex_direction: Some(FlexDirection::Column),
                    gap: Some(px(2.0)),
                    padding_top: Some(px(6.0)),
                    padding_bottom: Some(px(6.0)),
                    padding_left: Some(px(6.0)),
                    padding_right: Some(px(6.0)),
                    background: Some(col(theme::BG_FLOATING)),
                    ..rounded(8.0)
                }, test_id = "status-menu") {
                    { row(Status::Online) }
                    { row(Status::Idle) }
                    { row(Status::Dnd) }
                    { row(Status::Invisible) }
                    view(style = StyleRules {
                        height: Some(px(1.0)),
                        margin_top: Some(px(4.0)),
                        margin_bottom: Some(px(4.0)),
                        background: Some(col(theme::DIVIDER)),
                        ..Default::default()
                    }) {}
                    {
                        status_menu_row("Edit Profile", None, None, false, move || {
                            open.set(false);
                            profile_mode.set(true);
                        })
                    }
                }
            }
        },
    )
}

/// One row of the status menu: an optional status dot, the label, and an
/// optional hint line beneath it. `selected` marks the current status.
fn status_menu_row(
    label: &'static str,
    dot: Option<&'static str>,
    hint: Option<&'static str>,
    selected: bool,
    on_pick: impl Fn() + 'static,
) -> Element {
    let dot_el = dot.map(|c| {
        ui! {
            view(style = StyleRules {
                width: Some(px(10.0)),
                height: Some(px(10.0)),
                flex_shrink: Some(grow(0.0)),
                background: Some(col(c)),
                ..rounded(5.0)
            }) {}
        }
    });
    let hint_el = hint.map(|h| {
        ui! {
            text(style = StyleRules {
                font_size: Some(px(12.0)),
                color: Some(col(TEXT_MUTED)),
                ..Default::default()
            }) { h.to_string() }
        }
    });
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(10.0)),
            padding_top: Some(px(6.0)),
            padding_bottom: Some(px(6.0)),
            padding_left: Some(px(8.0)),
            padding_right: Some(px(8.0)),
            background: selected.then(|| col(theme::BG_SELECTED)),
            ..rounded(4.0)
        }) {
            if let Some(d) = dot_el {
                d
            }
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Column),
                ..Default::default()
            }) {
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    font_weight: Some(FontWeight::Medium),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { label.to_string() }
                if let Some(h) = hint_el {
                    h
                }
            }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                on_pick();
            }
            TouchResponse::CONSUMED
        })
    }
}

/// The name of the voice channel the user is currently joined to, resolved
/// from the `(server_id, voice_id)` pair in `joined_voice`. Used to build the
/// `user_panel` switch key (value-deduped — `String` compares by content, so
/// two joins of the same channel on different servers would be equal, which
/// is exactly the "same visible state, no rebuild" behavior we want).
pub(super) fn voice_name_for(joined: Option<(u32, u32)>) -> Option<String> {
    let (server_id, voice_id) = joined?;
    let server = SERVERS.iter().find(|s| s.id == server_id)?;
    Some(server.find_voice_channel(voice_id)?.name.to_string())
}
