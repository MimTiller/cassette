//! `settings_suite` — the full-screen User Settings screen opened by the gear
//! (and by "Profile"), laid out like Discord's: a section sidebar on the left
//! (My Account, Profile, Appearance, Notifications, Keybinds, then Log Out), the
//! selected section in a centred column, and a close button with "ESC" on the
//! right (Escape closes it too, see `app::popups`).
//!
//! Sections:
//!
//! - **Notifications** — the same three-level picker ("All messages" /
//!   "Mentions only" / "Nothing") the old screen had, now WIRE-D to the
//!   `crate::notify` engine: the level is persisted (localStorage) and read on
//!   every inbound gateway message, and picking a notifying level also requests
//!   the browser `Notification` permission so browser toasts start working.
//! - **Account** — change display name and password, both calling the REAL auth
//!   backend (`crate::auth::change_username` / `change_password`); the session
//!   token is preserved, so no re-login is needed.
//! - **Appearance** — an accent-color picker (preset swatches). A true
//!   dark/light theme toggle is not available to the component library in this
//!   pass (`StyleRules` has no theme/filter field), so the accent option is the
//!   shipped fallback per the feature spec.
//!
//! Privacy & Safety, Voice & Video, and Accessibility are deliberate follow-ups
//! (out of scope here).
//!
//! Idioms are the crate's: plain `StyleRules` via `style_helpers::px`/`col`, a
//! reactive `if active.get() == n` per tab that lowers to a `when`, the
//! `view` + `.on_touch` click pattern guarded on `TouchPhase::Ended`,
//! `text_input(value = …, on_change = …)` for the fields, and `button(label = …,
//! on_click = …)` for the actions. `Signal` is `Copy`, so `move` closures
//! capture copies of the same signals and stay `Fn`.

use crate::theme;
use crate::style_helpers::{borderless_input, col, pct, px};
use std::rc::Rc;
use runtime_core::{
    component, signal, switch, ui, AlignItems, Element, FlexDirection, JustifyContent, Position,
    Signal, StyleRules, TouchPhase, TouchResponse,
};

// Discord dark palette (kept local so the component is self-contained).

// Accent presets (Discord blurple default + a spread for the picker).
const ACCENTS: [&str; 6] = ["#5865f2", "#9b6bff", "#23a559", "#f23f43", "#f0b232", "#eb459e"];

/// The three settings tabs, in order (the tab-bar index keys).
/// Sidebar entries in order: (group heading, or "" to continue the group; label).
const SECTIONS: [(&str, &str); 5] = [
    ("USER SETTINGS", "My Account"),
    ("", "Profile"),
    ("APP SETTINGS", "Appearance"),
    ("", "Notifications"),
    ("", "Keybinds"),
];

/// The Profile section's index, for openers that land on it.
pub const SECTION_PROFILE: usize = 1;

/// Build one radio-style option row (the `notification_settings` shape): a dot
/// glyph (filled when `selected == idx`) and the label; pressing it sets
/// `selected` to `idx`, deselecting the others.
fn option_row(label: &str, idx: u32, selected: Signal<u32>, on_pick: Rc<dyn Fn(u32)>) -> Element {
    ui! {
        view(style = StyleRules {
            position: Some(Position::Relative),
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(10.0)),
            padding_top: Some(px(10.0)),
            padding_bottom: Some(px(10.0)),
            padding_left: Some(px(12.0)),
            padding_right: Some(px(12.0)),
            background: Some(col(theme::BG_PRIMARY)),
            border_top_left_radius: Some(px(6.0)),
            border_top_right_radius: Some(px(6.0)),
            border_bottom_left_radius: Some(px(6.0)),
            border_bottom_right_radius: Some(px(6.0)),
            ..Default::default()
        }) {
            if selected.get() == idx {
                text(style = StyleRules {
                    font_size: Some(px(16.0)),
                    color: Some(col(theme::ACCENT)),
                    ..Default::default()
                }) { "◉".to_string() }
            } else {
                text(style = StyleRules {
                    font_size: Some(px(16.0)),
                    color: Some(col(theme::TEXT_FAINT)),
                    ..Default::default()
                }) { "○".to_string() }
            }
            text(style = StyleRules {
                font_size: Some(px(14.0)),
                color: Some(col(theme::TEXT_NORMAL)),
                ..Default::default()
            }) { label.to_string() }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                selected.set(idx);
                on_pick(idx);
            }
            TouchResponse::CONSUMED
        })
    }
}

/// The **Notifications** tab: the three-level picker, now wired. Selecting a
/// notifying level (0 or 1) persists it and requests browser permission;
/// selecting "Nothing" (2) just persists.
fn notifications_tab(selected: Signal<u32>) -> Element {
    let sel0 = selected.clone();
    let sel1 = selected.clone();
    let sel2 = selected.clone();

    // On picking a notifying level: persist + request browser permission
    // (async, fire-and-forget). "Nothing" just persists (no permission need).
    let on_pick = Rc::new(move |idx: u32| {
        crate::notify::save_mode(idx);
        if idx < crate::notify::NOTHING {
            runtime_core::driver::spawn_async(async move {
                crate::notify::request_permission().await;
            });
        }
    });

    let all = option_row("All messages", crate::notify::ALL_MESSAGES, sel0, on_pick.clone());
    let mentions = option_row("Mentions only", crate::notify::MENTIONS_ONLY, sel1, on_pick.clone());
    let nothing = option_row("Nothing", crate::notify::NOTHING, sel2, on_pick);

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            gap: Some(px(8.0)),
            padding_top: Some(px(16.0)),
            padding_bottom: Some(px(16.0)),
            padding_left: Some(px(16.0)),
            padding_right: Some(px(16.0)),
            ..Default::default()
        }) {
            text(style = StyleRules {
                font_size: Some(px(14.0)),
                color: Some(col(theme::TEXT_STRONG)),
                ..Default::default()
            }) { "Message notifications".to_string() }
            all
            mentions
            nothing
            text(style = StyleRules {
                font_size: Some(px(12.0)),
                color: Some(col(theme::TEXT_MUTED)),
                ..Default::default()
            }) {
                "A toast appears for matches while the channel is not open; browser notifications fire once you grant permission."
                    .to_string()
            }
        }
    }
}

/// The **Account** tab: change display name (handle) and password, both against
/// the real auth backend. Local field signals + a shared message line
/// (error red / success green) surface the backend's result inline.
fn account_tab(blocked: Signal<Vec<crate::blocks::BlockedUser>>, live_rev: Signal<u32>) -> Element {
    let name = signal(String::new());
    let cur_pw = signal(String::new());
    let new_pw = signal(String::new());
    let msg = signal(String::new());
    let msg_kind = signal(0u32); // 0 = idle, 1 = error, 2 = ok

    // Change display name: spawn the RPC, surface the result inline.
    // A bare `move || {}` closure (its three `Signal` captures are `Copy`, so it
    // is itself `Copy` and satisfies `IntoAction`'s `F: Fn() + 'static` bound).
    // The `ui!` macro's `on_click = <closure>` sugar coerces it to an `Action`.
    // (Not `Rc<dyn Fn()>` — `IntoAction` is not impl'd for `Rc`.)
    let do_name = {
        let name = name.clone();
        let msg = msg.clone();
        let kind = msg_kind.clone();
        move || {
            let handle = name.get();
            if handle.trim().is_empty() {
                msg.set("A display name is required.".to_string());
                kind.set(1);
                return;
            }
            msg.set(String::new());
            runtime_core::driver::spawn_async(async move {
                match crate::auth::change_username(&handle).await {
                    Ok(new_name) => {
                        msg.set(format!("Display name set to {new_name}."));
                        kind.set(2);
                    }
                    Err(e) => {
                        msg.set(e);
                        kind.set(1);
                    }
                }
            });
        }
    };

    // Change password: the current one is verified server-side before the new
    // one is applied; the session token is preserved.
    let do_pw = {
        let cur = cur_pw.clone();
        let new = new_pw.clone();
        let msg = msg.clone();
        let kind = msg_kind.clone();
        move || {
            let cur_v = cur.get();
            let new_v = new.get();
            if cur_v.trim().is_empty() || new_v.trim().is_empty() {
                msg.set("Current and new password are required.".to_string());
                kind.set(1);
                return;
            }
            msg.set(String::new());
            runtime_core::driver::spawn_async(async move {
                match crate::auth::change_password(&cur_v, &new_v).await {
                    Ok(()) => {
                        msg.set("Password changed.".to_string());
                        kind.set(2);
                    }
                    Err(e) => {
                        msg.set(e);
                        kind.set(1);
                    }
                }
            });
        }
    };

    let on_name_change = {
        let name = name.clone();
        move |s: String| name.set(s)
    };
    let on_cur_change = {
        let cur = cur_pw.clone();
        move |s: String| cur.set(s)
    };
    let on_new_change = {
        let new = new_pw.clone();
        move |s: String| new.set(s)
    };

    // `borderless_input()` zeroes the four border widths so the `<input>`
    // UA-default bevel (the light 3D bottom/right edge) doesn't leak as a
    // partial stroke — the same idiom every other `text_input` in the app uses.
    let input_style = StyleRules {
        height: Some(px(32.0)),
        background: Some(col(theme::BG_TERTIARY)),
        border_top_left_radius: Some(px(6.0)),
        border_top_right_radius: Some(px(6.0)),
        border_bottom_left_radius: Some(px(6.0)),
        border_bottom_right_radius: Some(px(6.0)),
        font_size: Some(px(14.0)),
        padding_left: Some(px(10.0)),
        padding_right: Some(px(10.0)),
        color: Some(col(theme::TEXT_STRONG)),
        ..borderless_input()
    };

    let blocked_list = switch(
        move || blocked.get(),
        move |list: &Vec<crate::blocks::BlockedUser>| {
            if list.is_empty() {
                return ui! {
                    text(style = StyleRules { font_size: Some(px(13.0)), color: Some(col(theme::TEXT_MUTED)), ..Default::default() }) {
                        "You haven't blocked anyone.".to_string()
                    }
                };
            }
            let rows: Vec<Element> = list.iter().map(|b| {
                let user_id = b.user_id;
                ui! {
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        gap: Some(px(10.0)),
                        padding_top: Some(px(6.0)),
                        padding_bottom: Some(px(6.0)),
                        ..Default::default()
                    }) {
                        { crate::avatar_chip::avatar_chip(b.username.as_str(), theme::ACCENT, 28.0, 14.0) }
                        text(style = StyleRules { flex_grow: Some(runtime_core::Tokenized::Literal(1.0)), font_size: Some(px(14.0)), color: Some(col(theme::TEXT_STRONG)), ..Default::default() }) { b.username.clone() }
                        view(style = StyleRules {
                            height: Some(px(28.0)),
                            padding_left: Some(px(12.0)),
                            padding_right: Some(px(12.0)),
                            align_items: Some(AlignItems::Center),
                            justify_content: Some(JustifyContent::Center),
                            background: Some(col(theme::BUTTON_SECONDARY)),
                            border_top_left_radius: Some(px(4.0)),
                            border_top_right_radius: Some(px(4.0)),
                            border_bottom_left_radius: Some(px(4.0)),
                            border_bottom_right_radius: Some(px(4.0)),
                            ..Default::default()
                        }) {
                            text(style = StyleRules { font_size: Some(px(13.0)), color: Some(col(theme::TEXT_STRONG)), ..Default::default() }) { "Unblock".to_string() }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                crate::blocks::set_blocked(user_id, false, blocked, live_rev, || {});
                            }
                            TouchResponse::CONSUMED
                        })
                    }
                }
            }).collect();
            ui! { view(style = StyleRules { flex_direction: Some(FlexDirection::Column), ..Default::default() }) { rows } }
        },
    );

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            gap: Some(px(18.0)),
            padding_top: Some(px(16.0)),
            padding_bottom: Some(px(16.0)),
            padding_left: Some(px(16.0)),
            padding_right: Some(px(16.0)),
            ..Default::default()
        }) {
            // ── Display name ──
            text(style = StyleRules {
                font_size: Some(px(14.0)),
                color: Some(col(theme::TEXT_STRONG)),
                ..Default::default()
            }) { "Display name".to_string() }
            text_input(
                value = name.clone(),
                on_change = on_name_change,
                placeholder = "New display name",
                style = input_style.clone(),
            )
            button(
                label = "Save name".to_string(),
                on_click = do_name,
                style = StyleRules {
                    color: Some(col(theme::TEXT_ON_ACCENT)),
                    align_self: Some(runtime_core::AlignSelf::FlexStart),
                    height: Some(px(28.0)),
                    padding_left: Some(px(14.0)),
                    padding_right: Some(px(14.0)),
                    background: Some(col(theme::ACCENT)),
                    border_top_left_radius: Some(px(6.0)),
                    border_top_right_radius: Some(px(6.0)),
                    border_bottom_left_radius: Some(px(6.0)),
                    border_bottom_right_radius: Some(px(6.0)),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    flex_direction: Some(FlexDirection::Row),
                    ..Default::default()
                }
            )

            // ── Password ──
            text(style = StyleRules {
                font_size: Some(px(14.0)),
                color: Some(col(theme::TEXT_STRONG)),
                ..Default::default()
            }) { "Change password".to_string() }
            text_input(
                value = cur_pw.clone(),
                on_change = on_cur_change,
                placeholder = "Current password",
                style = input_style.clone(),
            )
            text_input(
                value = new_pw.clone(),
                on_change = on_new_change,
                placeholder = "New password",
                style = input_style.clone(),
            )
            button(
                label = "Update password".to_string(),
                on_click = do_pw,
                style = StyleRules {
                    color: Some(col(theme::TEXT_ON_ACCENT)),
                    align_self: Some(runtime_core::AlignSelf::FlexStart),
                    height: Some(px(28.0)),
                    padding_left: Some(px(14.0)),
                    padding_right: Some(px(14.0)),
                    background: Some(col(theme::ACCENT)),
                    border_top_left_radius: Some(px(6.0)),
                    border_top_right_radius: Some(px(6.0)),
                    border_bottom_left_radius: Some(px(6.0)),
                    border_bottom_right_radius: Some(px(6.0)),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    flex_direction: Some(FlexDirection::Row),
                    ..Default::default()
                }
            )

            // ── Shared result line ── (reactive: reads `msg`/`msg_kind`)
            if !msg.get().is_empty() {
                if msg_kind.get() == 2 {
                    text(style = StyleRules {
                        font_size: Some(px(13.0)),
                        color: Some(col(theme::SUCCESS)),
                        ..Default::default()
                    }) { move || msg.get().to_string() }
                } else {
                    text(style = StyleRules {
                        font_size: Some(px(13.0)),
                        color: Some(col(theme::DANGER)),
                        ..Default::default()
                    }) { move || msg.get().to_string() }
                }
            }

            // ── Blocked users ── (the account's real block list; Unblock
            // takes effect at once, and their messages show again)
            text(style = StyleRules {
                font_size: Some(px(12.0)),
                font_weight: Some(runtime_core::FontWeight::Bold),
                color: Some(col(theme::TEXT_MUTED)),
                margin_top: Some(px(24.0)),
                ..Default::default()
            }) { "BLOCKED USERS".to_string() }
            { blocked_list }

            // ── Sessions ── (revokes every login of this account, this
            // device's included)
            text(style = StyleRules {
                font_size: Some(px(12.0)),
                font_weight: Some(runtime_core::FontWeight::Bold),
                color: Some(col(theme::TEXT_MUTED)),
                margin_top: Some(px(24.0)),
                ..Default::default()
            }) { "SESSIONS".to_string() }
            text(style = StyleRules {
                font_size: Some(px(13.0)),
                color: Some(col(theme::TEXT_MUTED)),
                margin_top: Some(px(6.0)),
                margin_bottom: Some(px(8.0)),
                ..Default::default()
            }) { "Signs this account out everywhere it is logged in, including here.".to_string() }
            view(style = StyleRules {
                height: Some(px(32.0)),
                padding_left: Some(px(16.0)),
                padding_right: Some(px(16.0)),
                align_self: Some(runtime_core::AlignSelf::FlexStart),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                background: Some(col(theme::DANGER_STRONG)),
                border_top_left_radius: Some(px(4.0)),
                border_top_right_radius: Some(px(4.0)),
                border_bottom_left_radius: Some(px(4.0)),
                border_bottom_right_radius: Some(px(4.0)),
                ..Default::default()
            }, test_id = "logout-everywhere") {
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    font_weight: Some(runtime_core::FontWeight::SemiBold),
                    color: Some(col(theme::TEXT_ON_ACCENT)),
                    ..Default::default()
                }) { "Log out of all devices".to_string() }
            }
            .on_touch(|ev| {
                if ev.phase == TouchPhase::Ended {
                    crate::auth::request_logout_everywhere();
                }
                TouchResponse::CONSUMED
            })
        }
    }
}

/// One accent swatch: a rounded `view` filled with `hex`, with a ring on it
/// while it is the selected accent. `hex` is `&'static str` (Copy), so BOTH the
/// reactive `style` closure and the `on_touch` handler can capture a copy
/// without a double-move (the non-Copy-`String` form that would move it twice).
fn accent_swatch(
    hex: &'static str,
    accent: Signal<String>,
    on_pick: Rc<dyn Fn(String)>,
) -> Element {
    ui! {
        view(style = move || {
            let on = accent.get() == hex;
            StyleRules {
                width: Some(px(34.0)),
                height: Some(px(34.0)),
                background: Some(theme::fixed(hex)),
                border_top_left_radius: Some(px(8.0)),
                border_top_right_radius: Some(px(8.0)),
                border_bottom_left_radius: Some(px(8.0)),
                border_bottom_right_radius: Some(px(8.0)),
                border_top_width: Some(runtime_core::Tokenized::Literal(if on { 3.0 } else { 1.0 })),
                border_right_width: Some(runtime_core::Tokenized::Literal(if on { 3.0 } else { 1.0 })),
                border_bottom_width: Some(runtime_core::Tokenized::Literal(if on { 3.0 } else { 1.0 })),
                border_left_width: Some(runtime_core::Tokenized::Literal(if on { 3.0 } else { 1.0 })),
                border_top_color: Some(col(if on { theme::TEXT_STRONG } else { theme::TEXT_FAINT })),
                border_right_color: Some(col(if on { theme::TEXT_STRONG } else { theme::TEXT_FAINT })),
                border_bottom_color: Some(col(if on { theme::TEXT_STRONG } else { theme::TEXT_FAINT })),
                border_left_color: Some(col(if on { theme::TEXT_STRONG } else { theme::TEXT_FAINT })),
                ..Default::default()
            }
        })
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                on_pick(hex.to_string());
            }
            TouchResponse::CONSUMED
        })
    }
}

/// The **Appearance** tab: an accent-color picker. Tapping a swatch sets the
/// shared `accent` signal (owned by `UserSettings`) and persists it. A true
/// dark/light toggle is not supported by the component library yet.
/// One theme choice: a small preview of the theme's own colours (always
/// shown in that theme, whatever is active) and its name; the active one
/// has an accent outline.
fn theme_card(name: theme::ThemeName, active: Signal<theme::ThemeName>, on_pick: Rc<dyn Fn(theme::ThemeName)>) -> Element {
    let bar = |w: f32, c: &str| ui! {
        view(style = StyleRules {
            width: Some(px(w)),
            height: Some(px(6.0)),
            background: Some(theme::fixed(c)),
            border_top_left_radius: Some(px(3.0)),
            border_top_right_radius: Some(px(3.0)),
            border_bottom_left_radius: Some(px(3.0)),
            border_bottom_right_radius: Some(px(3.0)),
            ..Default::default()
        }) {}
    };
    let preview = ui! {
        view(style = StyleRules {
            width: Some(px(132.0)),
            height: Some(px(84.0)),
            flex_direction: Some(FlexDirection::Row),
            background: Some(theme::fixed(name.value(theme::BG_PRIMARY))),
            border_top_left_radius: Some(px(6.0)),
            border_top_right_radius: Some(px(6.0)),
            border_bottom_left_radius: Some(px(6.0)),
            border_bottom_right_radius: Some(px(6.0)),
            overflow: Some(runtime_core::Overflow::Hidden),
            ..Default::default()
        }) {
            view(style = StyleRules { width: Some(px(14.0)), background: Some(theme::fixed(name.value(theme::BG_TERTIARY))), ..Default::default() }) {}
            view(style = StyleRules { width: Some(px(30.0)), background: Some(theme::fixed(name.value(theme::BG_SECONDARY))), ..Default::default() }) {}
            view(style = StyleRules {
                flex_grow: Some(runtime_core::Tokenized::Literal(1.0)),
                flex_direction: Some(FlexDirection::Column),
                gap: Some(px(7.0)),
                padding_top: Some(px(12.0)),
                padding_left: Some(px(8.0)),
                ..Default::default()
            }) {
                { bar(52.0, name.value(theme::TEXT_STRONG)) }
                { bar(70.0, name.value(theme::TEXT_MUTED)) }
                { bar(40.0, name.value(theme::ACCENT)) }
            }
        }
    };
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Center),
            gap: Some(px(8.0)),
            ..Default::default()
        }) {
            view(style = move || {
                let on = active.get() == name;
                let w = runtime_core::Tokenized::Literal(2.0);
                let c = col(if on { theme::ACCENT } else { theme::DIVIDER });
                StyleRules {
                    padding_top: Some(px(3.0)),
                    padding_bottom: Some(px(3.0)),
                    padding_left: Some(px(3.0)),
                    padding_right: Some(px(3.0)),
                    border_top_width: Some(w.clone()),
                    border_right_width: Some(w.clone()),
                    border_bottom_width: Some(w.clone()),
                    border_left_width: Some(w),
                    border_top_color: Some(c.clone()),
                    border_right_color: Some(c.clone()),
                    border_bottom_color: Some(c.clone()),
                    border_left_color: Some(c),
                    border_top_left_radius: Some(px(9.0)),
                    border_top_right_radius: Some(px(9.0)),
                    border_bottom_left_radius: Some(px(9.0)),
                    border_bottom_right_radius: Some(px(9.0)),
                    ..Default::default()
                }
            }) { preview }
            text(style = StyleRules { font_size: Some(px(14.0)), color: Some(col(theme::TEXT_NORMAL)), ..Default::default() }) {
                name.label().to_string()
            }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                on_pick(name);
            }
            TouchResponse::CONSUMED
        })
    }
}

fn appearance_tab(accent: Signal<String>) -> Element {
    // The theme: picking one saves it, recolours the app and brings the
    // theme's own accent back.
    let active = signal(theme::current());
    let on_theme: Rc<dyn Fn(theme::ThemeName)> = Rc::new(move |name: theme::ThemeName| {
        active.set(name);
        crate::notify::save_theme(name);
        theme::set_theme_name(name);
        accent.set(name.value(theme::ACCENT).to_string());
    });
    let theme_cards: Vec<Element> = theme::ThemeName::ALL.iter().map(|t| theme_card(*t, active, on_theme.clone())).collect();
    let accent_chk = accent.clone();
    let on_pick = Rc::new(move |hex: String| {
        accent_chk.set(hex.clone());
        crate::notify::save_accent(&hex);
        crate::theme::set_accent(&hex);
    });

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            gap: Some(px(12.0)),
            padding_top: Some(px(16.0)),
            padding_bottom: Some(px(16.0)),
            padding_left: Some(px(16.0)),
            padding_right: Some(px(16.0)),
            ..Default::default()
        }) {
            text(style = StyleRules {
                font_size: Some(px(14.0)),
                color: Some(col(theme::TEXT_STRONG)),
                ..Default::default()
            }) { "Theme".to_string() }
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                flex_wrap: Some(runtime_core::FlexWrap::Wrap),
                gap: Some(px(16.0)),
                margin_bottom: Some(px(12.0)),
                ..Default::default()
            }) { theme_cards }
            text(style = StyleRules {
                font_size: Some(px(14.0)),
                color: Some(col(theme::TEXT_STRONG)),
                ..Default::default()
            }) { "Accent color".to_string() }
            text(style = StyleRules {
                font_size: Some(px(12.0)),
                color: Some(col(theme::TEXT_MUTED)),
                ..Default::default()
            }) { "The highlight colour for buttons, selection and mentions. Picking a theme resets it to that theme's own."
                .to_string() }
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                gap: Some(px(10.0)),
                ..Default::default()
            }) {
                for hex in ACCENTS.iter() {
                    accent_swatch(*hex, accent.clone(), on_pick.clone())
                }
            }
        }
    }
}

/// The full-screen user-settings screen (the gear's target). Owns the tab state
/// (0..=2) and the accent (seeded from storage), then composes the tab bar over
/// the active panel. The notification level is NOT owned here — it is passed in
/// as a shared `Signal` (owned by `app_shell`), because the GATEWAY effect reads
/// that same level on every inbound message; a single source of truth means a
/// picker change is live for the next message without a re-open.
#[component]
pub fn UserSettings(
    notif_mode: Signal<u32>,
    blocked: Signal<Vec<crate::blocks::BlockedUser>>,
    live_rev: Signal<u32>,
    active: Signal<usize>,
    on_close: Option<Rc<dyn Fn()>>,
    on_logout: Option<Rc<dyn Fn()>>,
) -> Element {
    // The accent is owned here (the gateway doesn't read it) and seeded from
    // storage (default Discord blurple).
    let accent = signal(crate::notify::load_accent().unwrap_or_else(|| theme::current().value(theme::ACCENT).to_string()));

    // The selected section, rebuilt when `active` changes. `notif_mode` is
    // the SHARED notification level the gateway reads on every message.
    let panel = switch(
        move || active.get(),
        move |tab: &usize| {
            let body = match *tab {
                0 => account_tab(blocked, live_rev),
                1 => crate::profile_settings::ProfileSettings(),
                2 => appearance_tab(accent),
                3 => notifications_tab(notif_mode),
                _ => crate::keyboard_shortcuts_overlay::shortcuts_list(),
            };
            let title = SECTIONS.get(*tab).map(|(_, l)| *l).unwrap_or_default().to_string();
            ui! {
                view(style = StyleRules { flex_direction: Some(FlexDirection::Column), ..Default::default() }) {
                    text(style = StyleRules {
                        font_size: Some(px(20.0)),
                        font_weight: Some(runtime_core::FontWeight::SemiBold),
                        color: Some(col(theme::TEXT_STRONG)),
                        margin_bottom: Some(px(20.0)),
                        ..Default::default()
                    }) { title }
                    { body }
                }
            }
        },
    );

    // Sidebar rows: group headings, the sections, a divider, Log Out.
    let mut nav: Vec<Element> = Vec::new();
    for (i, (group, label)) in SECTIONS.iter().enumerate() {
        if !group.is_empty() {
            let group = group.to_string();
            nav.push(nav_heading(move || group.clone(), i == 0));
        }
        nav.push(nav_row(label, false, active, i, None));
    }
    nav.push(nav_divider());
    nav.push(nav_row("Log Out", true, active, usize::MAX, on_logout));
    settings_layout(nav, panel, on_close)
}

/// A sidebar group heading ("USER SETTINGS", a server's name). `first` drops
/// the space above it.
pub(crate) fn nav_heading(label: impl Fn() -> String + 'static, first: bool) -> Element {
    ui! {
        text(style = StyleRules {
            font_size: Some(px(12.0)),
            font_weight: Some(runtime_core::FontWeight::Bold),
            color: Some(col(theme::TEXT_MUTED)),
            padding_left: Some(px(10.0)),
            padding_top: Some(px(if first { 0.0 } else { 16.0 })),
            padding_bottom: Some(px(6.0)),
            ..Default::default()
        }) { move || label() }
    }
}

/// The thin rule between sidebar groups.
pub(crate) fn nav_divider() -> Element {
    ui! {
        view(style = StyleRules {
            height: Some(px(1.0)),
            background: Some(col(theme::DIVIDER)),
            margin_top: Some(px(8.0)),
            margin_bottom: Some(px(8.0)),
            margin_left: Some(px(10.0)),
            margin_right: Some(px(10.0)),
            ..Default::default()
        }) {}
    }
}

/// The full-screen settings layout shared by User Settings and Server
/// Settings: `nav` in a sidebar on the left (right-aligned in its half like
/// Discord's), `panel` in a centred scrolling column, and a close button with
/// "ESC" on the right.
pub(crate) fn settings_layout(nav: Vec<Element>, panel: Element, close: Option<Rc<dyn Fn()>>) -> Element {
    ui! {
        view(style = StyleRules {
            width: Some(pct(100.0)),
            height: Some(pct(100.0)),
            flex_direction: Some(FlexDirection::Row),
            ..Default::default()
        }) {
            // Left: the sidebar, right-aligned in its half like Discord's.
            // On a phone it fills the window until a section opens.
            view(style = move || {
                let hidden = crate::app::mobile::settings_hidden(true);
                StyleRules {
                    flex_grow: Some(runtime_core::Tokenized::Literal(if hidden { 0.0 } else { 1.0 })),
                    flex_shrink: Some(runtime_core::Tokenized::Literal(0.0)),
                    flex_basis: Some(px(if crate::app::mobile::is_phone() { 0.0 } else { 218.0 })),
                    width: hidden.then(|| px(0.0)),
                    flex_direction: Some(FlexDirection::Row),
                    justify_content: Some(if crate::app::mobile::is_phone() { JustifyContent::FlexStart } else { JustifyContent::FlexEnd }),
                    background: Some(col(theme::BG_SECONDARY)),
                    overflow: Some(runtime_core::Overflow::Hidden),
                    ..Default::default()
                }
            }) {
                scroll_view(style = move || StyleRules {
                    width: (!crate::app::mobile::is_phone()).then(|| px(218.0)),
                    flex_grow: crate::app::mobile::is_phone().then(|| runtime_core::Tokenized::Literal(1.0)),
                    scrollbar: Some(runtime_core::ScrollbarVisibility::Hidden),
                    ..Default::default()
                }) {
                    view(style = move || StyleRules {
                        flex_direction: Some(FlexDirection::Column),
                        gap: Some(px(2.0)),
                        padding_top: Some(px(if crate::app::mobile::is_phone() { 16.0 } else { 60.0 })),
                        padding_bottom: Some(px(60.0)),
                        padding_left: Some(px(6.0)),
                        padding_right: Some(px(6.0)),
                        ..Default::default()
                    }) { nav }
                }
            }
            // Right: the section in a centred column, and the close button.
            // On a phone, while the section list shows, only the close
            // button column stays visible here.
            view(style = move || {
                let hidden = crate::app::mobile::settings_hidden(false);
                StyleRules {
                    flex_grow: Some(runtime_core::Tokenized::Literal(if hidden { 0.0 } else { 1.0 })),
                    flex_shrink: Some(runtime_core::Tokenized::Literal(1.0)),
                    flex_basis: (!hidden).then(|| px(if crate::app::mobile::is_phone() { 0.0 } else { 800.0 })),
                    flex_direction: Some(FlexDirection::Row),
                    background: Some(col(theme::BG_PRIMARY)),
                    ..Default::default()
                }
            }) {
                scroll_view(style = move || StyleRules {
                    flex_grow: Some(runtime_core::Tokenized::Literal(if crate::app::mobile::settings_hidden(false) { 0.0 } else { 1.0 })),
                    width: crate::app::mobile::settings_hidden(false).then(|| px(0.0)),
                    ..Default::default()
                }) {
                    view(style = move || {
                        let pad = if crate::app::mobile::is_phone() { 16.0 } else { 40.0 };
                        StyleRules {
                            flex_direction: Some(FlexDirection::Column),
                            max_width: Some(px(740.0)),
                            padding_top: Some(px(if crate::app::mobile::is_phone() { 16.0 } else { 60.0 })),
                            padding_bottom: Some(px(80.0)),
                            padding_left: Some(px(pad)),
                            padding_right: Some(px(pad)),
                            ..Default::default()
                        }
                    }) {
                        { crate::app::mobile::back_button("settings-back", || crate::app::mobile::show_settings_nav(true)) }
                        { panel }
                    }
                }
                view(style = StyleRules {
                    width: Some(px(72.0)),
                    flex_shrink: Some(runtime_core::Tokenized::Literal(0.0)),
                    padding_top: Some(px(60.0)),
                    flex_direction: Some(FlexDirection::Column),
                    align_items: Some(AlignItems::Center),
                    gap: Some(px(6.0)),
                    ..Default::default()
                }) {
                    view(style = StyleRules {
                        width: Some(px(36.0)),
                        height: Some(px(36.0)),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        border_top_width: Some(runtime_core::Tokenized::Literal(2.0)),
                        border_right_width: Some(runtime_core::Tokenized::Literal(2.0)),
                        border_bottom_width: Some(runtime_core::Tokenized::Literal(2.0)),
                        border_left_width: Some(runtime_core::Tokenized::Literal(2.0)),
                        border_top_color: Some(col(theme::TEXT_MUTED)),
                        border_right_color: Some(col(theme::TEXT_MUTED)),
                        border_bottom_color: Some(col(theme::TEXT_MUTED)),
                        border_left_color: Some(col(theme::TEXT_MUTED)),
                        border_top_left_radius: Some(px(18.0)),
                        border_top_right_radius: Some(px(18.0)),
                        border_bottom_left_radius: Some(px(18.0)),
                        border_bottom_right_radius: Some(px(18.0)),
                        ..Default::default()
                    }, test_id = "settings-close") {
                        text(style = StyleRules { font_size: Some(px(16.0)), color: Some(col(theme::TEXT_MUTED)), ..Default::default() }) { "✕".to_string() }
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            if let Some(c) = &close {
                                // The next settings screen opens on its list.
                                crate::app::mobile::show_settings_nav(true);
                                c();
                            }
                        }
                        TouchResponse::CONSUMED
                    })
                    text(style = StyleRules {
                        font_size: Some(px(13.0)),
                        font_weight: Some(runtime_core::FontWeight::SemiBold),
                        color: Some(col(theme::TEXT_MUTED)),
                        ..Default::default()
                    }) { "ESC".to_string() }
                }
            }
        }
    }
}

/// One sidebar row: a section (selects `index`), or with `danger` the Log
/// Out row (runs `action`). Hover lightens it; the selected section is
/// filled.
pub(crate) fn nav_row(label: &str, danger: bool, active: Signal<usize>, index: usize, action: Option<Rc<dyn Fn()>>) -> Element {
    let hovered = signal(false);
    let label = label.to_string();
    ui! {
        view(style = move || {
            let on = active.get() == index;
            StyleRules {
                height: Some(px(32.0)),
                padding_left: Some(px(10.0)),
                padding_right: Some(px(10.0)),
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                background: Some(col(if on { theme::BG_SELECTED } else if hovered.get() { theme::BG_HOVER } else { "transparent" })),
                border_top_left_radius: Some(px(4.0)),
                border_top_right_radius: Some(px(4.0)),
                border_bottom_left_radius: Some(px(4.0)),
                border_bottom_right_radius: Some(px(4.0)),
                ..Default::default()
            }
        }) {
            text(style = move || {
                let on = active.get() == index;
                StyleRules {
                    font_size: Some(px(15.0)),
                    font_weight: Some(runtime_core::FontWeight::Medium),
                    color: Some(col(if danger { theme::DANGER } else if on || hovered.get() { theme::TEXT_STRONG } else { theme::TEXT_MUTED })),
                    ..Default::default()
                }
            }) { label.clone() }
        }
        .on_hover(move |on| hovered.set(on))
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                match &action {
                    Some(a) => a(),
                    None => {
                        active.set(index);
                        crate::app::mobile::show_settings_nav(false);
                    }
                }
            }
            TouchResponse::CONSUMED
        })
    }
}
