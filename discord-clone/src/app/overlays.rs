//! `app_shell` section, split out of `app.rs`. Reads the shared state
//! through `ShellState`; returns the elements the final layout places.

use crate::theme;
use super::*;

pub(super) struct OverlaysParts {
    pub(super) profile_region: Element,
    pub(super) notif_region: Element,
    pub(super) shortcuts_region: Element,
    pub(super) live_dm_region: Element,
    pub(super) moderation_region: Element,
    pub(super) member_profile_el: Element,
    pub(super) live_invite_region: Element,
    pub(super) bug_reports_region: Element,
}

pub(super) fn build(s: &ShellState) -> OverlaysParts {
    let ShellState { active_live_server, active_server, authed, blocked_users, bug_reports_mode,
        friends_add_error, friends_add_username, friends_entry, friends_open, friends_snap, friends_tab, gateway, live_dm_active, live_dm_messages, live_dm_mode, live_dm_new_id, live_dm_revision, live_dms, live_invite_code, live_invite_error, live_invite_mode, live_members, live_presence, live_presence_rev, live_revision, moderation_mode, notif_level, notif_mode, on_dm_message, profile_mode, selected_member, shortcuts_mode, .. } = s.clone();

    // User Settings: one full-window screen (see `settings_suite`), shown
    // while `notif_mode` is on. "Profile" (`profile_mode`) opens it at the
    // Profile section. Log Out signs out as the user panel's button did.
    let settings_tab: Signal<usize> = signal(0);
    let profile_region = switch(
        move || profile_mode.get(),
        move |open: &bool| {
            if *open {
                after_ms_detached(0, move || {
                    profile_mode.set(false);
                    settings_tab.set(crate::settings_suite::SECTION_PROFILE);
                    notif_mode.set(true);
                });
            }
            ui! { view(style = StyleRules { width: Some(px(0.0)), height: Some(px(0.0)), ..Default::default() }) {} }
        },
    );
    let on_close: Rc<dyn Fn()> = Rc::new(move || notif_mode.set(false));
    let on_logout: Rc<dyn Fn()> = Rc::new(move || {
        crate::auth::clear_session();
        crate::auth::request_logout();
        *super::SESSION_NOTE.lock().unwrap_or_else(|p| p.into_inner()) = None;
        notif_mode.set(false);
        authed.set(false);
    });
    let notif_open_chk = notif_mode.clone();
    let notif_wrap = move || StyleRules {
        position: Some(Position::Absolute),
        left: Some(px(0.0)),
        top: Some(px(0.0)),
        width: Some(if notif_open_chk.get() { pct(100.0) } else { px(0.0) }),
        height: Some(if notif_open_chk.get() { pct(100.0) } else { px(0.0) }),
        background: Some(col(BG_MAIN)),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    let notif_region = ui! {
        view(style = notif_wrap) {
            UserSettings(
                notif_mode = notif_level.clone(),
                blocked = blocked_users,
                live_rev = live_revision,
                active = settings_tab,
                on_close = Some(on_close),
                on_logout = Some(on_logout),
            )
        }
    };

    // Keyboard Shortcuts overlay (same shape as the two above): built once,
    // collapsed to zero size while `shortcuts_mode` is off, fills the window
    // when on. The panel's own ✕ flips `shortcuts_mode` back off (it receives
    // a clone of the signal as its `open` flag); the user panel's '?' icon
    // opens it.
    let shortcuts_el = keyboard_shortcuts_overlay(shortcuts_mode.clone());
    let shortcuts_close = shortcuts_mode.clone();
    let shortcuts_header = ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::SpaceBetween),
            padding_top: Some(px(16.0)),
            padding_bottom: Some(px(16.0)),
            padding_left: Some(px(16.0)),
            padding_right: Some(px(16.0)),
            ..Default::default()
        }) {
            text(style = StyleRules {
                font_size: Some(px(18.0)),
                font_weight: Some(FontWeight::Bold),
                color: Some(col(TEXT)),
                ..Default::default()
            }) { "Keyboard Shortcuts".to_string() }
            view(style = StyleRules {
                height: Some(px(26.0)),
                padding_left: Some(px(12.0)),
                padding_right: Some(px(12.0)),
                background: Some(col(BG_SIDEBAR)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                border_top_left_radius: Some(px(6.0)),
                border_top_right_radius: Some(px(6.0)),
                border_bottom_left_radius: Some(px(6.0)),
                border_bottom_right_radius: Some(px(6.0)),
                ..Default::default()
            }) {
                text(style = StyleRules {
                    color: Some(col(TEXT)),
                    font_size: Some(px(12.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    ..Default::default()
                }) { "Done".to_string() }
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    shortcuts_close.set(false);
                }
                TouchResponse::CONSUMED
            })
        }
    };
    let shortcuts_open_chk = shortcuts_mode.clone();
    let shortcuts_wrap = move || StyleRules {
        position: Some(Position::Absolute),
        left: Some(px(0.0)),
        top: Some(px(0.0)),
        width: Some(if shortcuts_open_chk.get() { pct(100.0) } else { px(0.0) }),
        height: Some(if shortcuts_open_chk.get() { pct(100.0) } else { px(0.0) }),
        background: Some(col(theme::OVERLAY)),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    let shortcuts_region = ui! {
        view(style = shortcuts_wrap) {
            shortcuts_header
            view(style = StyleRules { flex_grow: Some(grow(1.0)), background: Some(col(BG_MAIN)), overflow: Some(Overflow::Hidden), ..Default::default() }) {
                scroll_view(style = StyleRules { flex_grow: Some(grow(1.0)), ..Default::default() }) { shortcuts_el }
            }
        }
    };
    // Direct Messages are LIVE only — the mock DM screen (dm_screen) and its
    // header/wrap region were deleted with it; see `live_dm_region` below.

    // Live (backend) Direct Messages screen: same full-screen, paint-last idiom
    // as the mock `dm_region` above, but a separate, independent entry point.
    // Its three data inputs (`live_dms` / `live_dm_active` / `live_dm_messages`
    // + `live_dm_revision`) are owned in `app_shell` scope, so the open
    // conversation and its fetched history survive a close/re-open. A "Done"
    // pill in the header flips `live_dm_mode` back off to close it.
    // Friends panel (live): the factory the live DM screen's main area calls
    // while `friends_open` is on (`Element` is not `Clone`, hence a factory —
    // all state is signals owned in app_shell scope, so a rebuild is cheap and
    // state-stable). Reads the app_shell live-presence map for the Online tab.
    let friends_panel = Rc::new(
        move || crate::friends_view::friends_screen(
            friends_snap.clone(),
            friends_tab.clone(),
            friends_add_username.clone(),
            friends_add_error.clone(),
            live_presence.clone(),
            live_presence_rev.clone(),
            on_dm_message.clone(),
        ),
    );
    let live_dm_el = crate::live_dm_view::live_dm_screen(
        live_dms,
        live_dm_active,
        live_dm_messages,
        live_dm_revision,
        live_dm_new_id,
        friends_snap,
        live_members,
        gateway.incoming(),
        friends_entry,
        friends_open,
        friends_panel,
    );
    let live_dm_close = live_dm_mode.clone();
    let live_dm_header = ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::SpaceBetween),
            padding_top: Some(px(16.0)),
            padding_bottom: Some(px(16.0)),
            padding_left: Some(px(16.0)),
            padding_right: Some(px(16.0)),
            border_bottom_width: Some(border_width(1.0)),
            border_bottom_color: Some(col(BORDER)),
            background: Some(col(BG_MAIN)),
            ..Default::default()
        }) {
            text(style = StyleRules {
                font_size: Some(px(18.0)),
                font_weight: Some(FontWeight::Bold),
                color: Some(col(TEXT)),
                ..Default::default()
            }) { "Direct Messages".to_string() }
            view(style = StyleRules {
                height: Some(px(26.0)),
                padding_left: Some(px(12.0)),
                padding_right: Some(px(12.0)),
                background: Some(col(BG_SIDEBAR)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                border_top_left_radius: Some(px(6.0)),
                border_top_right_radius: Some(px(6.0)),
                border_bottom_left_radius: Some(px(6.0)),
                border_bottom_right_radius: Some(px(6.0)),
                ..Default::default()
            }) {
                text(style = StyleRules {
                    color: Some(col(TEXT)),
                    font_size: Some(px(12.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    ..Default::default()
                }) { "Done".to_string() }
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    live_dm_close.set(false);
                }
                TouchResponse::CONSUMED
            })
        }
    };
    let live_dm_open_chk = live_dm_mode.clone();
    let live_dm_wrap = move || StyleRules {
        position: Some(Position::Absolute),
        left: Some(px(0.0)),
        top: Some(px(0.0)),
        width: Some(if live_dm_open_chk.get() { pct(100.0) } else { px(0.0) }),
        height: Some(if live_dm_open_chk.get() { pct(100.0) } else { px(0.0) }),
        background: Some(col(theme::OVERLAY)),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    let live_dm_region = ui! {
        view(style = live_dm_wrap) {
            live_dm_header
            view(style = StyleRules { flex_grow: Some(grow(1.0)), background: Some(col(BG_MAIN)), overflow: Some(Overflow::Hidden), ..Default::default() }) { live_dm_el }
        }
    };


    // Server Moderation panel (mock): the same full-screen overlay idiom as the
    // Profile Settings overlay above — built once, shown/hidden by
    // `moderation_mode`, closed by the "Done" pill in its header. The content
    // stacks the eight moderation components, each with a section label; every
    // row is hardcoded mock data, no backend call.
    let moderation_close = moderation_mode.clone();
    let section_label_style = section_label();
    let moderation_header = ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::SpaceBetween),
            padding_top: Some(px(16.0)),
            padding_bottom: Some(px(16.0)),
            padding_left: Some(px(16.0)),
            padding_right: Some(px(16.0)),
            ..Default::default()
        }) {
            text(style = StyleRules {
                font_size: Some(px(18.0)),
                font_weight: Some(FontWeight::Bold),
                color: Some(col(TEXT)),
                ..Default::default()
            }) { "Server Moderation".to_string() }
            view(style = StyleRules {
                height: Some(px(26.0)),
                padding_left: Some(px(12.0)),
                padding_right: Some(px(12.0)),
                background: Some(col(BG_SIDEBAR)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                border_top_left_radius: Some(px(6.0)),
                border_top_right_radius: Some(px(6.0)),
                border_bottom_left_radius: Some(px(6.0)),
                border_bottom_right_radius: Some(px(6.0)),
                ..Default::default()
            }) {
                text(style = StyleRules {
                    color: Some(col(TEXT)),
                    font_size: Some(px(12.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    ..Default::default()
                }) { "Done".to_string() }
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    moderation_close.set(false);
                }
                TouchResponse::CONSUMED
            })
        }
    };
    let moderation_open_chk = moderation_mode.clone();
    let moderation_wrap = move || StyleRules {
        position: Some(Position::Absolute),
        left: Some(px(0.0)),
        top: Some(px(0.0)),
        width: Some(if moderation_open_chk.get() { pct(100.0) } else { px(0.0) }),
        height: Some(if moderation_open_chk.get() { pct(100.0) } else { px(0.0) }),
        background: Some(col(theme::OVERLAY)),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    // "Invite People" panel (mock, local only): generate a code via
    // `invite::generate_invite_code`, build its link via `invite::invite_link`,
    // render both through `InviteLinkDisplay`, with the `InviteExpirySelector`
    // alongside. Sits at the top of the Server Moderation panel. No backend.
    let invite_code = crate::invite::generate_invite_code(active_server.get());
    let invite_url = crate::invite::invite_link(&invite_code);
    let invite_expiry = signal(0usize);
    // The picker's chosen duration (seconds) — local to this graveyard demo.
    let timeout_duration = signal(60i64);
    let invite_panel = ui! {
        text(style = section_label_style.clone()) { "Invite People".to_string() }
        // `InviteLinkDisplay` formats `discord.gg/{code}` internally, so it
        // takes the raw code; the full shareable URL from `invite_link` is
        // shown on its own line below.
        InviteLinkDisplay(code = invite_code.clone())
        text(style = StyleRules {
            font_size: Some(px(13.0)),
            color: Some(col(TEXT_MUTED)),
            padding_left: Some(px(16.0)),
            padding_right: Some(px(16.0)),
            ..Default::default()
        }) { invite_url }
        InviteExpirySelector(value = invite_expiry)
    };
    let moderation_el = ui! {
        invite_panel
        text(style = section_label_style.clone()) { "Banned members".to_string() }
        BanListRow()
        BanListRow()
        BanReasonInput()
        text(style = section_label_style.clone()) { "Timeout duration".to_string() }
        TimeoutDurationPicker(duration = timeout_duration)
        text(style = section_label_style.clone()) { "Member roles".to_string() }
        MemberRoleList()
        MemberRoleList()
        text(style = section_label_style.clone()) { "Blocked users".to_string() }
        BlockedUserRow()
        BlockedUserRow()
        text(style = section_label_style.clone()) { "Kick a member".to_string() }
        KickConfirmDialog()
        text(style = section_label_style.clone()) { "Delete a message".to_string() }
        DeleteMessageConfirm()
        text(style = section_label_style.clone()) { "Transfer ownership".to_string() }
        TransferOwnershipConfirm()
    };
    let moderation_region = ui! {
        view(style = moderation_wrap) {
            moderation_header
            view(style = StyleRules { flex_grow: Some(grow(1.0)), background: Some(col(BG_MAIN)), overflow: Some(Overflow::Hidden), ..Default::default() }) {
                scroll_view(style = StyleRules { flex_grow: Some(grow(1.0)), ..Default::default() }) { moderation_el }
            }
        }
    };

    // Member profile overlay: built once, shown/hidden by `selected_member`.
    // Full-screen and painted last so it sits above the member list; collapses
    // to zero size while no member is selected (the same toggle idiom as the
    // other overlay regions above).
    let member_profile_el = member_profile_panel(selected_member.clone());

    // --- Live "Invite People" overlay (real backend) ----------------------
    // The live channel header's `USER_PLUS` action opens this. Unlike the mock
    // invite panel above (which only generates a deterministic code), this
    // mints a REAL invite via `invite::create_invite` against `active_live_server`
    // (the only context with a real backend `server_id`) and shows the resulting
    // code in `InviteLinkDisplay`, whose Copy button now writes the link to the
    // system clipboard. `live_invite_expiry` is the expiry the user picked
    // (index 0/1/2 = 30 min / 1 day / never, per `InviteExpirySelector`); the
    // Create tap converts it to an `expires_at` epoch-seconds `Option` for the
    // backend. The content `switch`es on `live_invite_code` so the view flips
    // from the create form to the link display the moment a code is minted.
    // Same full-screen, paint-last idiom as `moderation_region`; inert outside
    // live mode (no entry point + `active_live_server` is None on a mock server).
    let live_invite_close = live_invite_mode.clone();
    let live_invite_header = ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::SpaceBetween),
            padding_top: Some(px(16.0)),
            padding_bottom: Some(px(16.0)),
            padding_left: Some(px(16.0)),
            padding_right: Some(px(16.0)),
            ..Default::default()
        }) {
            text(style = StyleRules {
                font_size: Some(px(18.0)),
                font_weight: Some(FontWeight::Bold),
                color: Some(col(TEXT)),
                ..Default::default()
            }) { "Invite People".to_string() }
            view(style = StyleRules {
                height: Some(px(26.0)),
                padding_left: Some(px(12.0)),
                padding_right: Some(px(12.0)),
                background: Some(col(BG_SIDEBAR)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                border_top_left_radius: Some(px(6.0)),
                border_top_right_radius: Some(px(6.0)),
                border_bottom_left_radius: Some(px(6.0)),
                border_bottom_right_radius: Some(px(6.0)),
                ..Default::default()
            }) {
                text(style = StyleRules {
                    color: Some(col(TEXT)),
                    font_size: Some(px(12.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    ..Default::default()
                }) { "Done".to_string() }
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    live_invite_close.set(false);
                }
                TouchResponse::CONSUMED
            })
        }
    };
    let live_invite_open_chk = live_invite_mode.clone();
    let live_invite_wrap = move || StyleRules {
        position: Some(Position::Absolute),
        left: Some(px(0.0)),
        top: Some(px(0.0)),
        width: Some(if live_invite_open_chk.get() { pct(100.0) } else { px(0.0) }),
        height: Some(if live_invite_open_chk.get() { pct(100.0) } else { px(0.0) }),
        background: Some(col(theme::OVERLAY)),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    // Local expiry picker for this flow (separate from the mock panel's
    // `invite_expiry` signal); the Create tap reads it at submit time.
    let live_invite_expiry = signal(0usize);
    // The "Create" action: read the live server id + chosen expiry, compute the
    // `expires_at` epoch-seconds (`None` = never expires), then spawn the RPC
    // and write the resulting code back to `live_invite_code` (which flips the
    // content view). On rejection, the backend message lands in
    // `active_live_server.get()` is `Option<i64>`; `None` (no live server —
    // shouldn't happen, the entry point only exists in live mode) reports the
    // rejection rather than panicking. The closure captures only `Copy` signal
    // handles (the spawned async block clones the ones it needs to outlive the
    // `Fn`), so the closure itself is `Copy` and can be passed into `ui!` across
    // `switch` re-renders (the same idiom as `on_submit_click`).
    let live_invite_create = move || {
        let sid = match active_live_server.get() {
            Some(s) => s,
            None => {
                live_invite_error.set("No live server selected.".to_string());
                return;
            }
        };
        let exp = live_invite_expiry.get();
        let code = live_invite_code.clone();
        let err = live_invite_error.clone();
        runtime_core::driver::spawn_async(async move {
            let expires_at: Option<i64> = match exp {
                0 => Some(runtime_shared::time::epoch_millis() / 1000 + 30 * 60),
                1 => Some(runtime_shared::time::epoch_millis() / 1000 + 24 * 60 * 60),
                _ => None,
            };
            match crate::invite::create_invite(sid, expires_at, None).await {
                Ok(inv) => code.set(Some(inv.code)),
                Err(msg) => err.set(msg),
            }
        });
    };
    // The content `switch`es on `(code, error)`: with a code it shows the link;
    // otherwise it shows the create form, plus the backend rejection (if the
    // last Create failed) under it. Keying on both means the view also rebuilds
    // when an error lands (a failed submit sets `error` while `code` stays None).
    let live_invite_content = switch(
        move || (live_invite_code.get(), live_invite_error.get()),
        move |state: &(Option<String>, String)| {
            let (code, err) = (&state.0, state.1.clone());
            if let Some(c) = code {
                let code = c.clone();
                ui! {
                    text(style = section_label_style.clone()) { "Your invite link".to_string() }
                    InviteLinkDisplay(code = code)
                }
            } else {
                // The rejection, shown only while non-empty (a successful
                // Create clears it by setting `code`, which rebuilds the view).
                let err_style = StyleRules {
                    margin_top: Some(px(8.0)),
                    margin_left: Some(px(16.0)),
                    margin_right: Some(px(16.0)),
                    font_size: Some(px(13.0)),
                    color: Some(col(TEXT_MUTED)),
                    ..Default::default()
                };
                let err_el = if err.is_empty() {
                    None
                } else {
                    Some(ui! { text(style = err_style) { err } })
                };
                ui! {
                    InviteExpirySelector(value = live_invite_expiry)
                    if let Some(el) = err_el {
                        el
                    }
                    button(
                        label = "Create invite".to_string(),
                        on_click = live_invite_create,
                        style = StyleRules {
                            height: Some(px(40.0)),
                            margin_top: Some(px(16.0)),
                            margin_left: Some(px(16.0)),
                            margin_right: Some(px(16.0)),
                            background: Some(col(BUTTON)),
                            color: Some(col(TEXT)),
                            font_size: Some(px(15.0)),
                            font_weight: Some(FontWeight::SemiBold),
                            border_top_left_radius: Some(px(8.0)),
                            border_top_right_radius: Some(px(8.0)),
                            border_bottom_left_radius: Some(px(8.0)),
                            border_bottom_right_radius: Some(px(8.0)),
                            align_items: Some(AlignItems::Center),
                            justify_content: Some(JustifyContent::Center),
                            ..Default::default()
                        },
                    )
                }
            }
        },
    );
    let live_invite_region = ui! {
        view(style = live_invite_wrap) {
            live_invite_header
            view(style = StyleRules { flex_grow: Some(grow(1.0)), background: Some(col(BG_MAIN)), overflow: Some(Overflow::Hidden), ..Default::default() }) {
                scroll_view(style = StyleRules { flex_grow: Some(grow(1.0)), ..Default::default() }) { live_invite_content }
            }
        }
    };

    // --- Bug reports overlay -----------------------------------------------
    // The `ThreadListRow` "Bug reports" row in the channel sidebar flips
    // `bug_reports_mode`; this full-screen overlay lists a handful of
    // hardcoded placeholder reports (mock-only, no backend call). Same
    // paint-last / collapse-to-zero idiom as `moderation_region`.
    let bug_reports_close = bug_reports_mode.clone();
    let bug_reports_header = ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::SpaceBetween),
            padding_top: Some(px(16.0)),
            padding_bottom: Some(px(16.0)),
            padding_left: Some(px(16.0)),
            padding_right: Some(px(16.0)),
            ..Default::default()
        }) {
            text(style = StyleRules {
                font_size: Some(px(18.0)),
                font_weight: Some(FontWeight::Bold),
                color: Some(col(TEXT)),
                ..Default::default()
            }) { "Bug reports".to_string() }
            view(style = StyleRules {
                height: Some(px(26.0)),
                padding_left: Some(px(12.0)),
                padding_right: Some(px(12.0)),
                background: Some(col(BG_SIDEBAR)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                border_top_left_radius: Some(px(6.0)),
                border_top_right_radius: Some(px(6.0)),
                border_bottom_left_radius: Some(px(6.0)),
                border_bottom_right_radius: Some(px(6.0)),
                ..Default::default()
            }) {
                text(style = StyleRules {
                    color: Some(col(TEXT)),
                    font_size: Some(px(12.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    ..Default::default()
                }) { "Done".to_string() }
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    bug_reports_close.set(false);
                }
                TouchResponse::CONSUMED
            })
        }
    };
    // (title, reporter, status) — static placeholder reports.
    let bug_report_rows: Vec<(&str, &str, &str)> = vec![
        ("Messages fail to load on slow connections", "nova", "Open"),
        ("Voice channel drops after ~5 minutes", "kito", "Open"),
        ("Emoji picker does not close on outside tap", "mira", "In review"),
        ("Sidebar channel names truncate too early", "juno", "In review"),
        ("DM list loses scroll position on refresh", "sami", "Resolved"),
    ];
    let bug_reports_el = ui! {
        for (title, reporter, status) in &bug_report_rows {
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Column),
                margin_left: Some(px(16.0)),
                margin_right: Some(px(16.0)),
                margin_top: Some(px(8.0)),
                padding_top: Some(px(10.0)),
                padding_bottom: Some(px(10.0)),
                padding_left: Some(px(12.0)),
                padding_right: Some(px(12.0)),
                background: Some(col(BG_SIDEBAR)),
                border_top_left_radius: Some(px(6.0)),
                border_top_right_radius: Some(px(6.0)),
                border_bottom_left_radius: Some(px(6.0)),
                border_bottom_right_radius: Some(px(6.0)),
                ..Default::default()
            }) {
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { (*title).to_string() }
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Row),
                    justify_content: Some(JustifyContent::SpaceBetween),
                    margin_top: Some(px(4.0)),
                    ..Default::default()
                }) {
                    text(style = StyleRules {
                        font_size: Some(px(12.0)),
                        color: Some(col(TEXT_MUTED)),
                        ..Default::default()
                    }) { (format!("reported by {}", reporter)).to_string() }
                    text(style = StyleRules {
                        font_size: Some(px(12.0)),
                        color: Some(col(TEXT_MUTED)),
                        ..Default::default()
                    }) { (*status).to_string() }
                }
            }
        }
    };
    let bug_reports_open_chk = bug_reports_mode.clone();
    let bug_reports_wrap = move || StyleRules {
        position: Some(Position::Absolute),
        left: Some(px(0.0)),
        top: Some(px(0.0)),
        width: Some(if bug_reports_open_chk.get() { pct(100.0) } else { px(0.0) }),
        height: Some(if bug_reports_open_chk.get() { pct(100.0) } else { px(0.0) }),
        background: Some(col(theme::OVERLAY)),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    let bug_reports_region = ui! {
        view(style = bug_reports_wrap) {
            bug_reports_header
            view(style = StyleRules { flex_grow: Some(grow(1.0)), background: Some(col(BG_MAIN)), overflow: Some(Overflow::Hidden), ..Default::default() }) {
                scroll_view(style = StyleRules { flex_grow: Some(grow(1.0)), ..Default::default() }) { bug_reports_el }
            }
        }
    };

    OverlaysParts { profile_region, notif_region, shortcuts_region, live_dm_region, moderation_region, member_profile_el, live_invite_region, bug_reports_region }
}
