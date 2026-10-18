//! `settings_suite` — the full-screen user-settings screen opened by the gear,
//! replacing the lone `notification_settings::NotificationSettings` screen.
//!
//! It is a three-tab suite (the `settings_tab_bar` idiom, but bound to the real
//! tabs "Notifications" / "Account" / "Appearance"):
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

use crate::style_helpers::{borderless_input, col, px};
use std::rc::Rc;
use runtime_core::{
    component, signal, switch, ui, AlignItems, Element, FlexDirection, JustifyContent, Position,
    Signal, StyleRules, TouchPhase, TouchResponse,
};

// Discord dark palette (kept local so the component is self-contained).
const BG_CARD: &str = "#2b2d31"; // card surface
const BG_ROW: &str = "#313338"; // elevated surface (rows, tab bar)
const BG_BAR: &str = "#313338"; // tab bar background
const BG_TAB_ACTIVE: &str = "#5865f2"; // active tab fill (Discord blurple)
const BG_TAB_INACTIVE: &str = "#3a3d44"; // inactive tab ghost fill (a hair above the bar so every tab shares the same pill box)
const TEXT: &str = "#f2f3f5"; // primary text
const TEXT_MUTED: &str = "#949ba4"; // muted text
const LABEL: &str = "#dbdee1"; // option label
const TITLE: &str = "#f2f3f5"; // heading
const DOT_ON: &str = "#5865f2"; // selected radio glyph
const DOT_OFF: &str = "#80848e"; // unselected radio glyph
const ERROR: &str = "#f23f43"; // error line (Discord danger red)
const OK: &str = "#23a559"; // success line (Discord green)

// Accent presets (Discord blurple default + a spread for the picker).
const ACCENTS: [&str; 5] = ["#5865f2", "#23a559", "#f23f43", "#f0b232", "#eb459e"];

/// The three settings tabs, in order (the tab-bar index keys).
const TABS: [&str; 3] = ["Notifications", "Account", "Appearance"];

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
            background: Some(col(BG_ROW)),
            border_top_left_radius: Some(px(6.0)),
            border_top_right_radius: Some(px(6.0)),
            border_bottom_left_radius: Some(px(6.0)),
            border_bottom_right_radius: Some(px(6.0)),
            ..Default::default()
        }) {
            if selected.get() == idx {
                text(style = StyleRules {
                    font_size: Some(px(16.0)),
                    color: Some(col(DOT_ON)),
                    ..Default::default()
                }) { "◉".to_string() }
            } else {
                text(style = StyleRules {
                    font_size: Some(px(16.0)),
                    color: Some(col(DOT_OFF)),
                    ..Default::default()
                }) { "○".to_string() }
            }
            text(style = StyleRules {
                font_size: Some(px(14.0)),
                color: Some(col(LABEL)),
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
                color: Some(col(TITLE)),
                ..Default::default()
            }) { "Message notifications".to_string() }
            all
            mentions
            nothing
            text(style = StyleRules {
                font_size: Some(px(12.0)),
                color: Some(col(TEXT_MUTED)),
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
fn account_tab() -> Element {
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
        background: Some(col("#1e1f22")),
        border_top_left_radius: Some(px(6.0)),
        border_top_right_radius: Some(px(6.0)),
        border_bottom_left_radius: Some(px(6.0)),
        border_bottom_right_radius: Some(px(6.0)),
        font_size: Some(px(14.0)),
        padding_left: Some(px(10.0)),
        padding_right: Some(px(10.0)),
        color: Some(col(TEXT)),
        ..borderless_input()
    };

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
                color: Some(col(TITLE)),
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
                    height: Some(px(28.0)),
                    padding_left: Some(px(14.0)),
                    padding_right: Some(px(14.0)),
                    background: Some(col(BG_TAB_ACTIVE)),
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
                color: Some(col(TITLE)),
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
                    height: Some(px(28.0)),
                    padding_left: Some(px(14.0)),
                    padding_right: Some(px(14.0)),
                    background: Some(col(BG_TAB_ACTIVE)),
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
                        color: Some(col(OK)),
                        ..Default::default()
                    }) { move || msg.get().to_string() }
                } else {
                    text(style = StyleRules {
                        font_size: Some(px(13.0)),
                        color: Some(col(ERROR)),
                        ..Default::default()
                    }) { move || msg.get().to_string() }
                }
            }
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
                background: Some(col(hex)),
                border_top_left_radius: Some(px(8.0)),
                border_top_right_radius: Some(px(8.0)),
                border_bottom_left_radius: Some(px(8.0)),
                border_bottom_right_radius: Some(px(8.0)),
                border_top_width: Some(runtime_core::Tokenized::Literal(if on { 3.0 } else { 1.0 })),
                border_right_width: Some(runtime_core::Tokenized::Literal(if on { 3.0 } else { 1.0 })),
                border_bottom_width: Some(runtime_core::Tokenized::Literal(if on { 3.0 } else { 1.0 })),
                border_left_width: Some(runtime_core::Tokenized::Literal(if on { 3.0 } else { 1.0 })),
                border_top_color: Some(col(if on { TEXT } else { DOT_OFF })),
                border_right_color: Some(col(if on { TEXT } else { DOT_OFF })),
                border_bottom_color: Some(col(if on { TEXT } else { DOT_OFF })),
                border_left_color: Some(col(if on { TEXT } else { DOT_OFF })),
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
fn appearance_tab(accent: Signal<String>) -> Element {
    let accent_chk = accent.clone();
    let on_pick = Rc::new(move |hex: String| {
        accent_chk.set(hex.clone());
        crate::notify::save_accent(&hex);
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
                color: Some(col(TITLE)),
                ..Default::default()
            }) { "Accent color".to_string() }
            text(style = StyleRules {
                font_size: Some(px(12.0)),
                color: Some(col(TEXT_MUTED)),
                ..Default::default()
            }) { "Picks the highlight color used across the app. A dark/light theme toggle is a follow-up."
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
#[allow(non_snake_case)]
pub fn UserSettings(notif_mode: Signal<u32>) -> Element {
    let active = signal(0usize);
    // The accent is owned here (the gateway doesn't read it) and seeded from
    // storage (default Discord blurple). `Signal` is `Copy`, so each `move`
    // closure in the `switch` below captures its own copy.
    let accent = signal(crate::notify::load_accent().unwrap_or_else(|| "#5865f2".to_string()));

    // The active panel: `switch` keyed on `active` mounts exactly one tab's
    // panel, rebuilding it when the tab changes. (Not `if/else if` over
    // pre-built `Element`s — that form lowers to an `Fn` closure that would
    // move each owned `Element` out on every reactive re-evaluation, which
    // Rust forbids; `switch`'s render closure rebuilds fresh, so no
    // move-out-of-capture.) `notif_mode` is the SHARED notification level
    // (owned by `app_shell`, read by the gateway effect on every inbound
    // message) — `Signal` is `Copy`, so the arm captures its own copy and the
    // picker writes the same level the gateway reads.
    let panel = switch(
        move || active.get(),
        move |tab: &usize| {
            match *tab {
                0 => notifications_tab(notif_mode),
                1 => account_tab(),
                _ => appearance_tab(accent),
            }
        },
    );

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            ..Default::default()
        }) {
            // The tab bar (its own reactive active-pill styling). A `#[component]`
            // is called by tag INSIDE `ui!` — `UserTabBar(active = …)` — not as
            // a bare fn call (the macro generates the `BuildElement` glue).
            UserTabBar(active = active.clone())

            // The active panel: pre-built above with `let panel = switch(…)` so the
            // `ui!` macro doesn't have to parse a trailing-closure `switch(…)` as a
            // child (the app.rs auth form uses the same `let … = switch(…)` +
            // bare-identifier-splat idiom, see `form` at ~line 2812). The panel
            // rebuilds when `active` changes — `switch`'s key closure reads it.
            panel
        }
    }
}

/// The three-tab bar (the `settings_tab_bar` shape, bound to `active`). Tapping
/// a tab sets `active` to that tab's index; the active tab renders highlighted
/// (blurple fill + white bold label). `active` is a `Signal` prop (stays a
/// `Signal` in the body — the macro does not wrap `Signal`/`ReadSignal`/
/// `WriteSignal` props), so the `move` closures read `active.get()` reactively.
#[component]
#[allow(non_snake_case)]
pub fn UserTabBar(active: Signal<usize>) -> Element {
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(4.0)),
            background: Some(col(BG_BAR)),
            padding_top: Some(px(6.0)),
            padding_bottom: Some(px(6.0)),
            // Match the tab pills to the panel body's content margin (the tab
            // views below use padding_left 16) so the active pill never
            // overhangs to the left of the content it labels.
            padding_left: Some(px(16.0)),
            padding_right: Some(px(16.0)),
            border_top_left_radius: Some(px(8.0)),
            border_top_right_radius: Some(px(8.0)),
            border_bottom_left_radius: Some(px(8.0)),
            border_bottom_right_radius: Some(px(8.0)),
            margin_bottom: Some(px(12.0)),
            ..Default::default()
        }) {
            for (i, label) in TABS.iter().enumerate() {
                // Reactive pill: `on = i == active.get()` flips fill + weight.
                view(style = move || {
                    let on = i == active.get();
                    StyleRules {
                        height: Some(px(32.0)),
                        padding_left: Some(px(16.0)),
                        padding_right: Some(px(16.0)),
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        background: Some(col(if on { BG_TAB_ACTIVE } else { BG_TAB_INACTIVE })),
                        border_top_left_radius: Some(px(6.0)),
                        border_top_right_radius: Some(px(6.0)),
                        border_bottom_left_radius: Some(px(6.0)),
                        border_bottom_right_radius: Some(px(6.0)),
                        ..Default::default()
                    }
                }) {
                    text(style = move || {
                        let on = i == active.get();
                        StyleRules {
                            font_size: Some(px(14.0)),
                            font_weight: Some(if on {
                                runtime_core::FontWeight::SemiBold
                            } else {
                                runtime_core::FontWeight::Normal
                            }),
                            color: Some(col(if on { "#ffffff" } else { TEXT_MUTED })),
                            ..Default::default()
                        }
                    }) { label.to_string() }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        active.set(i);
                    }
                    TouchResponse::CONSUMED
                })
            }
        }
    }
}
