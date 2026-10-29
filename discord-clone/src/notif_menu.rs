//! The notification settings dialog for one server or channel: mute it for
//! a while (or until turned back on), and pick what notifies — all
//! messages, only @mentions, nothing, or the default. Opened from the
//! server menu and a channel's menu.

use std::cell::Cell;

use discord_server::notification_settings::{Level, NotificationSetting, Scope, MUTED_UNTIL_TURNED_OFF};
use runtime_core::{
    signal, switch, ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent, Overflow, Position, Signal,
    StyleRules, TouchPhase, TouchResponse,
};

use crate::style_helpers::{col, pct, px, rounded};
use crate::theme;

/// What the dialog is open for: the scope, its id and a display name.
type Target = Option<(Scope, i64, String)>;

thread_local! {
    static OPEN: Cell<Option<Signal<Target>>> = const { Cell::new(None) };
}

/// Set up the dialog's state (once, from `app_shell`).
pub(crate) fn install() {
    OPEN.with(|o| o.set(Some(signal(None))));
}

/// The open/closed signal, for Escape (see `app::popups`).
pub(crate) fn open_signal() -> Signal<Target> {
    OPEN.with(|o| o.get()).expect("notif_menu::install runs in app_shell")
}

/// Open the dialog for a server or channel.
pub(crate) fn open_for(scope: Scope, id: i64, name: String) {
    open_signal().set(Some((scope, id, name)));
}

const MUTES: [(&str, i64); 5] = [
    ("For 15 minutes", 15 * 60_000),
    ("For 1 hour", 60 * 60_000),
    ("For 8 hours", 8 * 60 * 60_000),
    ("For 24 hours", 24 * 60 * 60_000),
    ("Until I turn it back on", 0),
];

/// "Muted for 37 more minutes" and the like.
fn mute_note(until: i64) -> String {
    if until == MUTED_UNTIL_TURNED_OFF {
        return "Muted until you turn it back on".to_string();
    }
    let mins = ((until - runtime_shared::time::epoch_millis()) / 60_000).max(1);
    if mins < 90 {
        format!("Muted for {mins} more minute{}", if mins == 1 { "" } else { "s" })
    } else {
        format!("Muted for about {} more hours", (mins + 30) / 60)
    }
}

fn option_row(label: String, picked: bool, on_tap: impl Fn() + 'static) -> Element {
    let hovered = signal(false);
    let mark = if picked { "✓" } else { "" }.to_string();
    ui! {
        view(style = move || StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            height: Some(px(34.0)),
            padding_left: Some(px(10.0)),
            padding_right: Some(px(10.0)),
            background: Some(col(if hovered.get() { theme::BG_HOVER } else { "transparent" })),
            ..rounded(4.0)
        }) {
            text(style = StyleRules {
                font_size: Some(px(14.0)),
                color: Some(col(theme::TEXT_NORMAL)),
                flex_grow: Some(runtime_core::Tokenized::Literal(1.0)),
                ..Default::default()
            }) { label }
            text(style = StyleRules {
                font_size: Some(px(14.0)),
                color: Some(col(theme::ACCENT)),
                ..Default::default()
            }) { mark }
        }
        .on_hover(move |on| hovered.set(on))
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                on_tap();
            }
            TouchResponse::CONSUMED
        })
    }
}

fn heading(label: &str) -> Element {
    let label = label.to_string();
    ui! {
        text(style = StyleRules {
            font_size: Some(px(12.0)),
            font_weight: Some(FontWeight::Bold),
            color: Some(col(theme::TEXT_MUTED)),
            margin_top: Some(px(10.0)),
            margin_bottom: Some(px(2.0)),
            ..Default::default()
        }) { label }
    }
}

/// The dialog, built once by `app_shell`; collapsed while closed.
pub(crate) fn dialog() -> Element {
    let open = open_signal();
    let body = switch(
        move || (open.get(), crate::notif_prefs::revision()),
        move |(target, _rev): &(Target, u32)| {
            let Some((scope, id, name)) = target.clone() else {
                return ui! { view {} };
            };
            let current = crate::notif_prefs::get(scope, id);
            let save = move |s: NotificationSetting| crate::notif_prefs::set(s);
            let muted = crate::notif_prefs::muted(scope, id);

            let mut mute_rows: Vec<Element> = Vec::new();
            if muted {
                let note = mute_note(current.muted_until.unwrap_or(MUTED_UNTIL_TURNED_OFF));
                mute_rows.push(ui! {
                    text(style = StyleRules {
                        font_size: Some(px(13.0)),
                        color: Some(col(theme::TEXT_MUTED)),
                        padding_left: Some(px(10.0)),
                        ..Default::default()
                    }) { note }
                });
                mute_rows.push(option_row("Unmute".to_string(), false, move || {
                    save(NotificationSetting { muted_until: None, ..current });
                }));
            } else {
                for (label, ms) in MUTES {
                    mute_rows.push(option_row(label.to_string(), false, move || {
                        let until = if ms == 0 { MUTED_UNTIL_TURNED_OFF } else { runtime_shared::time::epoch_millis() + ms };
                        save(NotificationSetting { muted_until: Some(until), ..current });
                    }));
                }
            }
            let default_label = match scope {
                Scope::Server => "Use my default (User Settings)",
                Scope::Channel => "Use the server's setting",
            };
            let levels = [
                ("All messages", Some(Level::All)),
                ("Only @mentions", Some(Level::Mentions)),
                ("Nothing", Some(Level::Nothing)),
                (default_label, None),
            ];
            let level_rows: Vec<Element> = levels
                .into_iter()
                .map(|(label, level)| {
                    option_row(label.to_string(), current.level == level, move || {
                        save(NotificationSetting { level, ..current });
                    })
                })
                .collect();
            let title = match scope {
                Scope::Server => name,
                Scope::Channel => format!("#{name}"),
            };
            let mute_heading = match scope {
                Scope::Server => "MUTE SERVER",
                Scope::Channel => "MUTE CHANNEL",
            };
            ui! {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Column),
                    padding_top: Some(px(18.0)),
                    padding_bottom: Some(px(14.0)),
                    padding_left: Some(px(16.0)),
                    padding_right: Some(px(16.0)),
                    ..Default::default()
                }) {
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        ..Default::default()
                    }) {
                        view(style = StyleRules {
                            flex_direction: Some(FlexDirection::Column),
                            flex_grow: Some(runtime_core::Tokenized::Literal(1.0)),
                            ..Default::default()
                        }) {
                            text(style = StyleRules {
                                font_size: Some(px(18.0)),
                                font_weight: Some(FontWeight::SemiBold),
                                color: Some(col(theme::TEXT_STRONG)),
                                ..Default::default()
                            }) { "Notification Settings".to_string() }
                            text(style = StyleRules {
                                font_size: Some(px(12.0)),
                                color: Some(col(theme::TEXT_MUTED)),
                                ..Default::default()
                            }) { title }
                        }
                        view {
                            text(style = StyleRules { font_size: Some(px(18.0)), color: Some(col(theme::TEXT_MUTED)), ..Default::default() }) { "✕".to_string() }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                open.set(None);
                            }
                            TouchResponse::CONSUMED
                        })
                    }
                    { heading(mute_heading) }
                    { mute_rows }
                    { heading("NOTIFY ME ABOUT") }
                    { level_rows }
                }
            }
        },
    );
    let card = ui! {
        view(style = StyleRules {
            width: Some(px(crate::app::mobile::fit_width(380.0))),
            background: Some(col(theme::BG_PRIMARY)),
            overflow: Some(Overflow::Hidden),
            ..rounded(8.0)
        }) {
            { body }
        }
        .on_touch(|_| TouchResponse::CONSUMED)
    };
    ui! {
        view(style = move || {
            let shown = open.get().is_some();
            StyleRules {
                position: Some(Position::Absolute),
                left: Some(px(0.0)),
                top: Some(px(0.0)),
                width: Some(if shown { pct(100.0) } else { px(0.0) }),
                height: Some(if shown { pct(100.0) } else { px(0.0) }),
                background: Some(col(theme::OVERLAY)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                overflow: Some(Overflow::Hidden),
                ..Default::default()
            }
        }) {
            { card }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                open.set(None);
            }
            TouchResponse::CONSUMED
        })
    }
}
