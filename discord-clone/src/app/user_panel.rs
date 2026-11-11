//! The bottom-left user panel and the voice channel section.
//! Split out of `app.rs`; `use super::*` keeps app.rs's imports and shared
//! colors/helpers in scope.

use crate::avatar_chip::AvatarFrame;
use crate::theme;
use super::*;

/// The bottom-left user profile section, as Discord has it: a single row —
/// avatar (with status dot) and name/status on the left, and the mic-mute,
/// deafen, and settings-gear icon buttons on the right. Identity comes from
/// the signed-in account (`crate::auth::current_username`); the live presence
/// status comes from the
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
/// and a **Leave** button that disconnects.
/// The whole strip is wrapped in a `switch` keyed on `(status, joined_voice?)`
/// so any of those changes re-renders the strip (the key is value-deduped,
/// like every other `switch` here).
pub(super) fn user_panel(
    user_status: Signal<Status>,
    joined_voice: Signal<Option<(u32, u32)>>,
    profile_mode: Signal<bool>,
    notif_mode: Signal<bool>,
    muted: Signal<bool>,
    deafened: Signal<bool>,
    camera: Signal<bool>,
    screen_share: Signal<bool>,
    voice_error: Signal<Option<(i64, String)>>,
    status_menu: Signal<bool>,
    gateway: crate::backend::Gateway,
) -> Element {
    switch(
        move || (user_status.get(), joined_voice.get()),
        move |(status, joined): &(Status, Option<(u32, u32)>)| {
            // Real signed-in identity: when genuinely logged in (not guest), the
            // backend's username was stored in `CURRENT_USERNAME` at login and
            // outlives this screen. Guest mode leaves it empty, so the strip
            // shows no name and a theme-tinted avatar chip.
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
                String::new()
            };
            let voice_connected = joined.is_some();
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
                // A phone's panel sits in the narrow 72px rail: stacked as a
                // column, the avatar, the readable name/status line and the
                // icon buttons each get their own line; on a desktop sidebar
                // the row has room for all of them side by side.
                flex_direction: Some(if crate::app::mobile::is_phone() { FlexDirection::Column } else { FlexDirection::Row }),
                align_items: Some(AlignItems::Center),
                ..Default::default()
            };
            let avatar_style = StyleRules {
                width: Some(px(32.0)),
                height: Some(px(32.0)),
                position: Some(Position::Relative),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                background: Some(col(theme::ACCENT)),
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
            // The shared 10px presence dot, ring cut out in the panel's own
            // fill — the one convention used on every avatar surface.
            let dot = crate::avatar_chip::presence_dot(status.color(), USER_BAR);
            let text_stack = move || StyleRules {
                flex_direction: Some(FlexDirection::Column),
                // A phone's panel is the narrow 72px rail: nothing absorbs the
                // row, so the avatar, the name/status stack and each icon
                // button wrap onto their own lines inside it. The stack keeps
                // its natural width — the status line ("Online") stays
                // readable and the name clamps to one line.
                flex_grow: Some(grow(if crate::app::mobile::is_phone() { 0.0 } else { 1.0 })),
                overflow: Some(Overflow::Hidden),
                ..Default::default()
            };
            // On a phone the row is the whole window less the rail: an
            // unshrunk name would push the three icon buttons off the
            // right edge. `max_lines` clamps the label to one line AND
            // lets it shrink below its natural width (the header's
            // server-name idiom), so the buttons keep their tap size.
            let name_style = move || StyleRules {
                font_size: Some(px(14.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(TEXT)),
                max_lines: crate::app::mobile::is_phone().then_some(1u32),
                ..Default::default()
            };
            let initial = display_name.chars().next().unwrap_or(' ').to_string();
            let name = display_name;

            // In a voice channel? Replace the name/status pair with the
            // Discord-style connection line: green "Voice Connected" + the
            // channel name, and a Leave button.
            let second_line = if voice_connected {
                let vc_style = StyleRules {
                    font_size: Some(px(10.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    color: Some(col(theme::SUCCESS)),
                    ..Default::default()
                };
                ui! {
                    text(style = vc_style) { "Voice Connected".to_string() }
                }
            } else {
                    let status_style = StyleRules {
                        font_size: Some(px(12.0)),
                        color: Some(col(TEXT_MUTED)),
                        ..Default::default()
                    };
                    let status = status.name().to_string();
                    ui! {
                        text(style = status_style) { status }
                    }
                };

            // The Leave button only exists while connected. Built as a
            // view + on_touch — the app's standard click pattern (the
            // runtime's `button` helper takes a fixed `label` prop, which
            // doesn't fit a styled pill; a view gives full style control).
            let leave_btn = if voice_connected {
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
            } else {
                None
            };

            // Click the avatar to open the status menu. The touch target is
            // the avatar only (not the whole row) so the Leave button keeps
            // its own region while connected.
            let menu_toggle = status_menu;
            // When the signed-in account has an uploaded avatar, the image
            // takes the place of the initial circle (same 32px circle,
            // cover-cropped). Either one sits under the status dot.
            let chip = if let Some(url) = avatar_img {
                ui! {
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
                }
            } else {
                ui! {
                    view(style = avatar_style) {
                        text(style = initial_style) { initial }
                    }
                }
            };
            let avatar = ui! {
                view(style = StyleRules {
                    width: Some(px(32.0)),
                    height: Some(px(32.0)),
                    position: Some(Position::Relative),
                    flex_shrink: Some(grow(0.0)),
                    ..rounded(16.0)
                }, test_id = "user-panel-avatar") {
                    AvatarFrame(name = username.get(), size = 32.0_f32) { chip }
                    { dot }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        menu_toggle.set(!menu_toggle.get());
                    }
                    TouchResponse::CONSUMED
                })
            };
            // Mic / Video / Share Screen / deafen / settings icon buttons
            // (Discord's user-panel voice actions). The signals are created
            // in `app_shell` and owned here — they post through the seam
            // when connected. The mic swaps to a slashed glyph when muted;
            // Lucide has no headphones-off, so deafened colors the icon red
            // instead, and Video / Share Screen light in the accent tone
            // while on.
            // 18px icons: the single row must fit the avatar, the name
            // column and five icon buttons in the 224px the 240px
            // sidebar leaves after the panel's 8px side padding.
            let icon_btn_style = move || StyleRules {
                // 18px icons fit the desktop sidebar's 224px row (avatar +
                // name + five buttons); a phone's sidebar is the whole
                // window, so the buttons grow to a comfortable tap size.
                width: Some(px(if crate::app::mobile::is_phone() { 24.0 } else { 18.0 })),
                height: Some(px(if crate::app::mobile::is_phone() { 24.0 } else { 18.0 })),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                flex_shrink: Some(grow(0.0)),
                ..Default::default()
            };
            // The mute/deafen flags live on the server: a browser's own LiveKit
            // toggle is invisible to peers, so the tap posts the pair through
            // the seam (which also drives the client's LiveKit room) and the
            // icons follow the answer. While not in a voice channel there is
            // nothing to post, so the tap just flips the icon locally.
            let muted_state = muted.clone();
            let joined_for_flags = joined_voice.clone();
            let err_for_flags = voice_error.clone();
            let cam_for_mic = camera.clone();
            let share_for_mic = screen_share.clone();
            let mic_btn = ui! {
                view(style = icon_btn_style.clone(), test_id = "user-mic") {
                    if muted_state.get() {
                        icon(MIC_OFF).color(|| theme::live(theme::DANGER)).size(15.0)
                    } else {
                        icon(MIC).color(|| theme::live(TEXT_MUTED)).size(15.0)
                    }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        let m2 = !muted_state.get();
                        match joined_for_flags.get() {
                            Some((_, ch)) => {
                                runtime_core::spawn_then_in(
                                    &crate::app::shell_alive(),
                                    crate::backend::set_voice_flags(
                                        ch as i64,
                                        m2,
                                        deafened.get(),
                                        cam_for_mic.get(),
                                        share_for_mic.get(),
                                    ),
                                    move |result| {
                                        match result {
                                            Ok(_) => {
                                                muted_state.set(m2);
                                                err_for_flags.set(None);
                                            }
                                            Err(msg) => err_for_flags.set(Some((ch as i64, msg))),
                                        }
                                    },
                                );
                            }
                            None => muted_state.set(m2),
                        }
                    }
                    TouchResponse::CONSUMED
                })
            };
            let deafened_state = deafened.clone();
            let cam_for_deafen = camera.clone();
            let share_for_deafen = screen_share.clone();
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
                        let d2 = !deafened_state.get();
                        match joined_for_flags.get() {
                            Some((_, ch)) => {
                                runtime_core::spawn_then_in(
                                    &crate::app::shell_alive(),
                                    crate::backend::set_voice_flags(
                                        ch as i64,
                                        muted.get(),
                                        d2,
                                        cam_for_deafen.get(),
                                        share_for_deafen.get(),
                                    ),
                                    move |result| {
                                        match result {
                                            Ok(_) => {
                                                deafened_state.set(d2);
                                                err_for_flags.set(None);
                                            }
                                            Err(msg) => err_for_flags.set(Some((ch as i64, msg))),
                                        }
                                    },
                                );
                            }
                            None => deafened_state.set(d2),
                        }
                    }
                    TouchResponse::CONSUMED
                })
            };
            // The Video and Share Screen buttons (Discord's camera and
            // screen-share toggles in the voice bar). Publishing video is
            // server-gated like audio — the seam's refusal paints in the
            // same error line. The icons light in ACCENT while on and show
            // their off glyph while off.
            let camera_state = camera.clone();
            let cam_btn = ui! {
                view(style = icon_btn_style.clone(), test_id = "user-camera") {
                    if camera_state.get() {
                        icon(VIDEO).color(|| theme::live(theme::ACCENT)).size(15.0)
                    } else {
                        icon(VIDEO_OFF).color(|| theme::live(TEXT_MUTED)).size(15.0)
                    }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        let c2 = !camera_state.get();
                        match joined_for_flags.get() {
                            Some((_, ch)) => {
                                runtime_core::spawn_then_in(
                                    &crate::app::shell_alive(),
                                    crate::backend::set_voice_flags(
                                        ch as i64,
                                        muted.get(),
                                        deafened.get(),
                                        c2,
                                        share_for_deafen.get(),
                                    ),
                                    move |result| {
                                        match result {
                                            Ok(_) => {
                                                camera_state.set(c2);
                                                err_for_flags.set(None);
                                            }
                                            Err(msg) => err_for_flags.set(Some((ch as i64, msg))),
                                        }
                                    },
                                );
                            }
                            None => camera_state.set(c2),
                        }
                    }
                    TouchResponse::CONSUMED
                })
            };
            let share_state = screen_share.clone();
            let cam_for_share = camera.clone();
            let share_btn = ui! {
                view(style = icon_btn_style.clone(), test_id = "user-share") {
                    if share_state.get() {
                        icon(SCREEN_SHARE).color(|| theme::live(theme::ACCENT)).size(15.0)
                    } else {
                        icon(SCREEN_SHARE).color(|| theme::live(theme::TEXT_FAINT)).size(15.0)
                    }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        let s2 = !share_state.get();
                        match joined_for_flags.get() {
                            Some((_, ch)) => {
                                runtime_core::spawn_then_in(
                                    &crate::app::shell_alive(),
                                    crate::backend::set_voice_flags(
                                        ch as i64,
                                        muted.get(),
                                        deafened.get(),
                                        cam_for_share.get(),
                                        s2,
                                    ),
                                    move |result| {
                                        match result {
                                            Ok(_) => {
                                                share_state.set(s2);
                                                err_for_flags.set(None);
                                            }
                                            Err(msg) => err_for_flags.set(Some((ch as i64, msg))),
                                        }
                                    },
                                );
                            }
                            None => share_state.set(s2),
                        }
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
                        // A phone shows one settings screen at a time: the
                        // gear re-opens it on its section list, so the Log
                        // Out row is reachable even after a test (or a user)
                        // left a section open.
                        crate::app::mobile::show_settings_nav(true);
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
                        cam_btn
                        share_btn
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
    gateway: crate::backend::Gateway,
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
                let test_id: &'static str = match st {
                    Status::Online => "status-row-online",
                    Status::Idle => "status-row-idle",
                    Status::Dnd => "status-row-dnd",
                    Status::Invisible => "status-row-invisible",
                };
                status_menu_row(st.name(), Some(st.color()), st.hint(), st == current, test_id, move || {
                    open.set(false);
                    set_status(user_status, st);
                    gateway.set_status(st.wire().to_string());
                })
            };
            ui! {
                view(style = StyleRules {
                    position: Some(Position::Absolute),
                    left: Some(px(8.0)),
                    right: Some(px(8.0)),
                    bottom: Some(px(56.0)),
                    z_index: Some(super::popups::POPUP_Z),
                    gap: Some(px(2.0)),
                    padding_top: Some(px(6.0)),
                    padding_bottom: Some(px(6.0)),
                    padding_left: Some(px(6.0)),
                    padding_right: Some(px(6.0)),
                    ..crate::style_helpers::popover_panel()
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
                        status_menu_row("Edit Profile", None, None, false, "status-row-edit-profile", move || {
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
/// optional hint line beneath it. `selected` marks the current status;
/// `test_id` names the row for the robot suite.
fn status_menu_row(
    label: &'static str,
    dot: Option<&'static str>,
    hint: Option<&'static str>,
    selected: bool,
    test_id: &'static str,
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
        }, test_id = test_id) {
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
