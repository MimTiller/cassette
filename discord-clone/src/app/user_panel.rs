//! The bottom-left user panel and the voice channel section.
//! Split out of `app.rs`; `use super::*` keeps app.rs's imports and shared
//! colors/helpers in scope.

use crate::theme;
use super::*;

/// A voice-channel row in the sidebar: a speaker icon (`VOLUME_2`) + name,
/// with the connected members listed beneath it in smaller, indented rows.
///
/// Interactive: clicking the row **joins** the channel (Discord's click-to-
/// join). While joined, the user's row appears in the member list beneath
/// the name and the profile strip turns into a voice-connection bar. Clicking
/// a joined row again leaves. Join state is the app-level `joined_voice`
/// signal — this function's output is only valid for `server_id` (the signal
/// is keyed by server), and it's rebuilt whenever that signal changes (the
/// sidebar `switch`'s key includes it).
pub(super) fn voice_section(
    cat: &Category,
    server_id: u32,
    active_server: Signal<u32>,
    joined_voice: Signal<Option<(u32, u32)>>,
    user_status: Signal<Status>,
) -> Vec<Element> {
    let mut out: Vec<Element> = Vec::new();
    if cat.voice.is_empty() {
        return out;
    }
    for v in cat.voice {
        // Voice rows get the same compact pill + hover treatment as text
        // rows (Discord treats voice rows identically here).
        let hovered = signal(false);
        let joined = joined_voice.clone();
        let row_style = move || StyleRules {
            height: Some(px(34.0)),
            margin_left: Some(px(8.0)),
            margin_right: Some(px(8.0)),
            padding_left: Some(px(12.0)),
            padding_right: Some(px(8.0)),
            background: Some(col(if hovered.get() {
                BG_HOVER
            } else {
                "transparent"
            })),
            border_top_left_radius: Some(px(8.0)),
            border_top_right_radius: Some(px(8.0)),
            border_bottom_left_radius: Some(px(8.0)),
            border_bottom_right_radius: Some(px(8.0)),
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(6.0)),
            ..Default::default()
        };
        let v_label_style = move || StyleRules {
            font_size: Some(px(15.0)),
            font_weight: Some(if joined.get() == Some((server_id, v.id)) {
                FontWeight::SemiBold
            } else {
                FontWeight::Normal
            }),
            color: Some(col(if joined.get() == Some((server_id, v.id)) {
                TEXT
            } else {
                TEXT_CHANNEL
            })),
            ..Default::default()
        };
        let label = v.name;
        let row = ui! {
            view(style = row_style, test_id = voice_test_id(v.id)) {
                icon(VOLUME_2)
                    .color(|| theme::live(TEXT_MUTED))
                    .size(14.0)
                text(style = v_label_style) { label }
                if joined.get() == Some((server_id, v.id)) {
                    ConnectionQualityIndicator()
                    ScreenShareIndicator()
                    LatencyBadge()
                }
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    // Join/leave toggle, Discord-style: only one channel at a
                    // time, and a voice join always belongs to the server you
                    //'re looking at. Switching servers clears the join
                    // (handled in the rail handler).
                    let is_here = active_server.get() == server_id;
                    match joined.get() {
                        Some((sv, _)) if sv == server_id => {
                            // Already in a voice channel on this server: a
                            // click leaves it (even on a different row).
                            joined.set(None);
                        }
                        None if is_here => joined.set(Some((server_id, v.id))),
                        // Joined on a different server: switching to this
                        // server already cleared the join — but if it
                        // somehow survived, don't double-join.
                        _ => {}
                    }
                }
                TouchResponse::CONSUMED
            })
            .on_hover(move |on| hovered.set(on))
        };
        out.push(row);
        // Voice controls (join trigger + volume + soundboard) — a compact
        // stack that appears only while the user is joined to this channel,
        // sitting just beneath the row. All local/mock, no backend call.
        if joined_voice.get() == Some((server_id, v.id)) {
            out.push(ui! {
                view(style = StyleRules {
                    padding_top: Some(px(4.0)),
                    padding_bottom: Some(px(4.0)),
                    padding_left: Some(px(44.0)),
                    padding_right: Some(px(8.0)),
                    flex_direction: Some(FlexDirection::Column),
                    gap: Some(px(8.0)),
                    ..Default::default()
                }) {
                    VoiceJoinButton()
                    VolumeSlider()
                    SoundboardButton()
                    MicSelector()
                }
            });
        }
        // "VOICE — N CONNECTED" section header, shown only while joined — the
        // count is the real connected set rendered beneath (the channel's
        // speakers plus your own row), not a hardcoded mock. Matches the
        // member-panel group-label "LABEL — N" idiom.
        if joined_voice.get() == Some((server_id, v.id)) {
            let connected = v.speakers.len() + 1;
            let hdr_style = StyleRules {
                padding_top: Some(px(4.0)),
                padding_left: Some(px(12.0)),
                padding_right: Some(px(8.0)),
                ..Default::default()
            };
            let hdr_text_style = StyleRules {
                font_size: Some(px(11.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(TEXT_MUTED)),
                ..Default::default()
            };
            out.push(ui! {
                view(style = hdr_style) {
                    text(style = hdr_text_style) { format!("VOICE — {} CONNECTED", connected) }
                }
            });
        }
        // Connected members, Discord's voice layout (avatar + name, indented).
        for sp in v.speakers {
            let row_style = StyleRules {
                padding_top: Some(px(2.0)),
                padding_bottom: Some(px(2.0)),
                padding_left: Some(px(44.0)),
                padding_right: Some(px(8.0)),
                gap: Some(px(8.0)),
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                ..Default::default()
            };
            let av_style = StyleRules {
                width: Some(px(20.0)),
                height: Some(px(20.0)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                background: Some(col(sp.color)),
                border_top_left_radius: Some(px(10.0)),
                border_top_right_radius: Some(px(10.0)),
                border_bottom_left_radius: Some(px(10.0)),
                border_bottom_right_radius: Some(px(10.0)),
                ..Default::default()
            };
            let av_text = StyleRules {
                font_size: Some(px(9.0)),
                font_weight: Some(FontWeight::Bold),
                color: Some(col(TEXT)),
                ..Default::default()
            };
            let name_style = StyleRules {
                font_size: Some(px(13.0)),
                color: Some(col(TEXT_MUTED)),
                ..Default::default()
            };
            let initial = sp.name.chars().next().unwrap_or(' ').to_string();
            out.push(ui! {
                view(style = row_style) {
                    view(style = av_style) {
                        text(style = av_text) { initial }
                    }
                    text(style = name_style) { sp.name.to_string() }
                }
            });
        }
        // While joined, the user's own row appears beneath the seed members —
        // Discord shows you in the channel you're in. Built as a plain row
        // (no `member_row` — the dot ring is a separate concern and this one
        // reuses the sidebar's 20px avatar scale).
        if joined_voice.get() == Some((server_id, v.id)) {
            let you = USER;
            let ustatus = user_status.get();
            let row_style = StyleRules {
                padding_top: Some(px(2.0)),
                padding_bottom: Some(px(2.0)),
                padding_left: Some(px(44.0)),
                padding_right: Some(px(8.0)),
                gap: Some(px(8.0)),
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                ..Default::default()
            };
            let av_style = StyleRules {
                width: Some(px(20.0)),
                height: Some(px(20.0)),
                position: Some(Position::Relative),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                background: Some(col(you.color)),
                border_top_left_radius: Some(px(10.0)),
                border_top_right_radius: Some(px(10.0)),
                border_bottom_left_radius: Some(px(10.0)),
                border_bottom_right_radius: Some(px(10.0)),
                ..Default::default()
            };
            let av_text = StyleRules {
                font_size: Some(px(9.0)),
                font_weight: Some(FontWeight::Bold),
                color: Some(col(TEXT)),
                ..Default::default()
            };
            let dot_style = StyleRules {
                position: Some(Position::Absolute),
                right: Some(px(0.0)),
                bottom: Some(px(0.0)),
                width: Some(px(8.0)),
                height: Some(px(8.0)),
                background: Some(col(ustatus.color())),
                border_top_width: Some(border_width(1.5)),
                border_right_width: Some(border_width(1.5)),
                border_bottom_width: Some(border_width(1.5)),
                border_left_width: Some(border_width(1.5)),
                border_top_color: Some(col(BG_SIDEBAR)),
                border_right_color: Some(col(BG_SIDEBAR)),
                border_bottom_color: Some(col(BG_SIDEBAR)),
                border_left_color: Some(col(BG_SIDEBAR)),
                border_top_left_radius: Some(px(4.0)),
                border_top_right_radius: Some(px(4.0)),
                border_bottom_left_radius: Some(px(4.0)),
                border_bottom_right_radius: Some(px(4.0)),
                ..Default::default()
            };
            let name_style = StyleRules {
                font_size: Some(px(13.0)),
                color: Some(col(TEXT)),
                ..Default::default()
            };
            let initial = you.name.chars().next().unwrap_or(' ').to_string();
            out.push(ui! {
                view(style = row_style) {
                    view(style = av_style) {
                        text(style = av_text) { initial }
                        view(style = dot_style) {}
                    }
                    text(style = name_style) { you.name.to_string() }
                }
            });
        }
    }
    out
}

/// The bottom-left user profile section: avatar (with status dot), name, and
/// status line. Identity comes from the data model's `USER` constant; the
/// live presence status comes from the `user_status` signal, so the strip can
/// show Idle / DND without touching the data.
///
/// While the user is in a voice channel, the name + status line becomes a
/// voice-connection line: a green "Voice Connected" (Discord's actual text),
/// the channel name beneath it, and a **Leave** button that disconnects.
/// The whole strip is wrapped in a `switch` keyed on `(status, voice_name?)`
/// so any of those changes re-renders the strip (the key is value-deduped,
/// like every other `switch` here).
///
/// The strip is two rows (the 240px sidebar can't fit the identity, the
/// Profile/Log Out pills, and the voice-action icons on one line): row 1 is
/// the avatar + name/status, row 2 is a control row — the mic-mute, deafen,
/// settings-gear, and keyboard-shortcuts ('?') icon buttons (Discord's
/// user-panel actions) on the left, the Leave button (voice only) and the
/// Profile / Log Out pills on the right.
pub(super) fn user_panel(
    user_status: Signal<Status>,
    joined_voice: Signal<Option<(u32, u32)>>,
    authed: Signal<bool>,
    profile_mode: Signal<bool>,
    notif_mode: Signal<bool>,
    shortcuts_mode: Signal<bool>,
    muted: Signal<bool>,
    deafened: Signal<bool>,
) -> Element {
    switch(
        move || (user_status.get(), voice_name_for(joined_voice.get())),
        move |(status, voice_name): &(Status, Option<String>)| {
            let user = USER;
            // Real signed-in identity: when genuinely logged in (not guest), the
            // backend's username was stored in `CURRENT_USERNAME` at login and
            // outlives this screen. Guest mode leaves it empty, so the strip
            // falls back to the mock `USER` identity ("Nova") there. The value
            // is set before `authed` flips (login completes first), so a plain
            // read here is stable for the whole mount — no reactivity needed.
            let real_name = crate::auth::current_username();
            // When the signed-in account has an uploaded avatar (shared
            // client registry, fed by login / `list_members`), the profile
            // strip's chip draws the image instead of the initial. Read
            // before `real_name` moves into the display-name branch below.
            let avatar_img = crate::auth::avatar_for(real_name.as_str());
            let display_name = if !real_name.is_empty() {
                real_name
            } else {
                user.name.to_string()
            };
            let voice_name = voice_name.clone();
            // Two rows (see the fn doc): identity on top, the icon-button
            // control row beneath.
            let panel_style = StyleRules {
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
                border_top_left_radius: Some(px(16.0)),
                border_top_right_radius: Some(px(16.0)),
                border_bottom_left_radius: Some(px(16.0)),
                border_bottom_right_radius: Some(px(16.0)),
                ..Default::default()
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
                border_top_left_radius: Some(px(6.0)),
                border_top_right_radius: Some(px(6.0)),
                border_bottom_left_radius: Some(px(6.0)),
                border_bottom_right_radius: Some(px(6.0)),
                ..Default::default()
            };
            let text_stack = StyleRules {
                flex_direction: Some(FlexDirection::Column),
                flex_grow: Some(grow(1.0)),
                // Only this column absorbs row shrinkage (the avatar is
                // fixed): the name must keep ~40px so "you" renders in full
                // instead of clipping to "yo".
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
                        border_top_left_radius: Some(px(6.0)),
                        border_top_right_radius: Some(px(6.0)),
                        border_bottom_left_radius: Some(px(6.0)),
                        border_bottom_right_radius: Some(px(6.0)),
                        ..Default::default()
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

            // Click the avatar to cycle the live status (Discord's
            // avatar-click status menu, simplified to a one-click cycle:
            // Online → Idle → DND → Online). The touch target is the avatar
            // only, not the whole row — the Leave button lives in the same
            // row while connected, and its own `on_touch` owns that region.
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
                        border_top_left_radius: Some(px(16.0)),
                        border_top_right_radius: Some(px(16.0)),
                        border_bottom_left_radius: Some(px(16.0)),
                        border_bottom_right_radius: Some(px(16.0)),
                        ..Default::default()
                    }) {
                        image(
                            src = crate::auth::media_url(url.clone()),
                            alt = "Your avatar",
                            style = StyleRules {
                                width: Some(px(32.0)),
                                height: Some(px(32.0)),
                                flex_shrink: Some(grow(0.0)),
                                object_fit: Some(ObjectFit::Cover),
                                border_top_left_radius: Some(px(16.0)),
                                border_top_right_radius: Some(px(16.0)),
                                border_bottom_left_radius: Some(px(16.0)),
                                border_bottom_right_radius: Some(px(16.0)),
                                ..Default::default()
                            },
                        )
                        view(style = dot_style) {}
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            let touch = user_status;
                            touch.set(touch.get().next());
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
                            let touch = user_status;
                            touch.set(touch.get().next());
                        }
                        TouchResponse::CONSUMED
                    })
                }
            };
            // Log Out: flips the shared `authed` signal back to false, which
            // unmounts this tree and re-mounts the login screen (the `switch`
            // key in `app()` re-evaluates on it). Same view+on_touch pill the
            // Leave button uses, so it fits the row's styling.
            // Compact pill: 6px padding + 11px type. This row now also holds
            // the mic/deafen/settings icons, so the old 8px padding (which
            // only had to fit avatar + name + 2 pills) no longer fits — the
            // tight 6px is part of the width budget in
            // `controls_row_fits_224px_sidebar`.
            let logout_style = StyleRules {
                height: Some(px(24.0)),
                padding_left: Some(px(6.0)),
                padding_right: Some(px(6.0)),
                background: Some(col(BG_SIDEBAR)),
                color: Some(col(TEXT_MUTED)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                font_size: Some(px(11.0)),
                font_weight: Some(FontWeight::SemiBold),
                // The row must shrink the name column, never the pills:
                // a squashed pill is what made "Log Out" wrap to two lines.
                flex_shrink: Some(grow(0.0)),
                border_top_left_radius: Some(px(6.0)),
                border_top_right_radius: Some(px(6.0)),
                border_bottom_left_radius: Some(px(6.0)),
                border_bottom_right_radius: Some(px(6.0)),
                ..Default::default()
            };
            let logout = authed.clone();
            let logout_text_style = StyleRules {
                color: Some(col(TEXT_MUTED)),
                font_size: Some(px(11.0)),
                font_weight: Some(FontWeight::SemiBold),
                ..Default::default()
            };
            let logout_btn = ui! {
                view(style = logout_style, test_id = "logout") {
                    text(style = logout_text_style) { "Log Out".to_string() }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        // Drop the persisted session so a refresh after logout
                        // doesn't silently restore it, and clear the one-shot
                        // note so this (a real logout) shows a clean form — no
                        // spurious "session expired".
                        crate::auth::clear_session();
                        // Ask the backend to clear the httpOnly session cookie
                        // too — both halves are needed: storage keys gone AND
                        // cookie gone, or a reload would replay the dead
                        // session.
                        crate::auth::request_logout();
                        *SESSION_NOTE
                            .lock()
                            .unwrap_or_else(|p| p.into_inner()) = None;
                        logout.set(false);
                    }
                    TouchResponse::CONSUMED
                })
            };
            // Profile Settings entry: a "Profile" pill (styled like Log Out)
            // that opens the full-screen `profile_settings` screen. `app_shell`
            // owns `profile_mode` and renders the overlay when it flips on.
            let profile_open = profile_mode.clone();
            // Compact like Log Out (6px padding): the three pills plus the
            // three icon buttons must all fit the 224px control row — see
            // `controls_row_fits_224px_sidebar`.
            let profile_btn = ui! {
                view(style = StyleRules {
                    height: Some(px(24.0)),
                    padding_left: Some(px(6.0)),
                    padding_right: Some(px(6.0)),
                    background: Some(col(BG_SIDEBAR)),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    font_size: Some(px(11.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    flex_shrink: Some(grow(0.0)),
                    border_top_left_radius: Some(px(6.0)),
                    border_top_right_radius: Some(px(6.0)),
                    border_bottom_left_radius: Some(px(6.0)),
                    border_bottom_right_radius: Some(px(6.0)),
                    ..Default::default()
                }, test_id = "profile") {
                    text(style = StyleRules {
                        color: Some(col(TEXT_MUTED)),
                        font_size: Some(px(11.0)),
                        font_weight: Some(FontWeight::SemiBold),
                        ..Default::default()
                    }) { "Profile".to_string() }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        profile_open.set(true);
                    }
                    TouchResponse::CONSUMED
                })
            };
            // Mic / deafen / settings icon buttons (Discord's user-panel
            // voice actions). The signals are created in `app_shell` and
            // owned here — pure local UI state, no backend call. The mic
            // swaps to a slashed glyph when muted; Lucide has no
            // headphones-off, so deafened colors the icon red instead.
            // 18px (not 20) + a 6px row gap: the control row must fit all
            // three icons AND the Profile / Log Out / Leave pills in the
            // 224px the 240px sidebar leaves after the panel's 8px side
            // padding — see `controls_row_fits_224px_sidebar` for the
            // arithmetic. 20px icons + 8px gaps overflowed it and clipped
            // Log Out (sidebar `Overflow::Hidden`), which was the regression.
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
            // Settings gear: opens the full-screen Notification Settings
            // overlay (the same build-once/toggle idiom as the Profile pill).
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
            // Keyboard shortcuts: a '?' icon next to the gear opens the
            // full-screen shortcuts overlay (same idiom as the gear).
            let shortcuts_open = shortcuts_mode.clone();
            let shortcuts_btn = ui! {
                view(style = icon_btn_style.clone(), test_id = "user-shortcuts") {
                    icon(CIRCLE_HELP).color(|| theme::live(TEXT_MUTED)).size(15.0)
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        shortcuts_open.set(true);
                    }
                    TouchResponse::CONSUMED
                })
            };
            // Idle toggle: a `MOON` icon that flips the user's live presence
            // status to `Status::Idle` (and cycles on to clear it when already
            // idle). It is the status/voice control the profile strip's avatar
            // click implements, exposed as a one-tap icon here with the mic and
            // deafen. It reads/writes the SAME `user_status` signal the avatar
            // uses, so toggling it regroups the user in the member list and
            // updates the strip's status line. Driven through `idle_timer_badge`
            // (the component's correct home is this control row, not the
            // message list).
            let idle_status = user_status.clone();
            let idle_btn = IdleTimerBadge(idle_status);
            // Row 2: the icon buttons left, the pills (plus Leave, voice only)
            // pushed to the right. Its own gap (6px, not the identity row's
            // 8px) is deliberate — the extra 2px per gap is part of what lets
            // all three icons plus up to three pills fit the 224px; the
            // identity row above keeps the roomier 8px.
            let controls_row_style = StyleRules {
                min_height: Some(px(24.0)),
                gap: Some(px(6.0)),
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                ..Default::default()
            };
            let controls_row = ui! {
                view(style = controls_row_style) {
                    mic_btn
                    deafen_btn
                    settings_btn
                    shortcuts_btn
                    idle_btn
                    view(style = StyleRules { flex_grow: Some(grow(1.0)), ..Default::default() }) {}
                    if let Some(btn) = leave_btn {
                        btn
                    }
                    profile_btn
                    logout_btn
                }
            };
            ui! {
                view(style = panel_style) {
                    view(style = row_style) {
                        avatar
                        view(style = text_stack) {
                            text(style = name_style) { name }
                            second_line
                        }
                    }
                    controls_row
                }
            }
        },
    )
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
