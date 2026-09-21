//! `user_settings` — Discord's User Settings overlay, opened from the user
//! panel's gear button. It mounts over the app shell (a full-screen dimmed
//! backdrop, the panel itself on the right) so the rest of the app stays
//! alive underneath; closing it unmounts this subtree and returns to the
//! channels.
//!
//! Real, persisted-per-session state: display name, status, and a set of
//! notification toggles (each backed by a signal, so flipping one re-renders
//! just that row). Status is the app's live `user_status` signal, so a
//! change here also regroups the user in the member list. Display name and
//! toggles live here (the data module holds only static identity — the
//! live name is UI state, like the composer).

use std::rc::Rc;

use runtime_core::{
    AlignItems, Element, FlexDirection, FontWeight, JustifyContent, Overflow, Position, Signal,
    StyleRules, TouchPhase, TouchResponse,
    signal, ui,
};
use runtime_shared::scheduling::after_ms_detached;

use crate::data::{Status, USER};
use crate::style_helpers::{col, pct, px};

// The same Discord dark palette as `app.rs`. Kept local (rather than a
// shared module) so the overlay stays self-contained; the values mirror
// `app.rs` exactly so the two trees agree pixel-for-pixel.
const BG_RAIL: &str = "#1e1f22";
const BG_SIDEBAR: &str = "#2b2d31";
const BG_MAIN: &str = "#313338";
const BG_ACTIVE: &str = "#404249";
const TEXT: &str = "#f2f3f5";
const TEXT_MUTED: &str = "#949ba4";
const BORDER: &str = "#26272b";
const COMPOSER: &str = "#383a40";
const BUTTON: &str = "#5865f2";

fn grow(v: f32) -> runtime_core::Tokenized<f32> {
    runtime_core::Tokenized::Literal(v)
}

fn border_width(px: f32) -> runtime_core::Tokenized<f32> {
    runtime_core::Tokenized::Literal(px)
}

/// One toggle row in a settings section: a label (and optional muted hint)
/// on the left, a pill toggle on the right. `on` re-runs reactively when the
/// toggle flips.
fn toggle_row(
    label: &str,
    hint: &str,
    on: Signal<bool>,
    on_change: impl Fn(bool) + 'static,
) -> Element {
    let label_style = StyleRules {
        font_size: Some(px(15.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT)),
        flex_grow: Some(grow(1.0)),
        ..Default::default()
    };
    let hint_style = StyleRules {
        font_size: Some(px(12.0)),
        color: Some(col(TEXT_MUTED)),
        ..Default::default()
    };
    let on_change_style = move || StyleRules {
        width: Some(px(36.0)),
        height: Some(px(20.0)),
        padding_left: Some(px(2.0)),
        padding_right: Some(px(2.0)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(if on.get() {
            JustifyContent::FlexEnd
        } else {
            JustifyContent::FlexStart
        }),
        background: Some(col(if on.get() {
            BUTTON
        } else {
            BG_ACTIVE
        })),
        border_top_left_radius: Some(px(10.0)),
        border_top_right_radius: Some(px(10.0)),
        border_bottom_left_radius: Some(px(10.0)),
        border_bottom_right_radius: Some(px(10.0)),
        ..Default::default()
    };
    let knob_style = StyleRules {
        width: Some(px(16.0)),
        height: Some(px(16.0)),
        background: Some(col(TEXT)),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        ..Default::default()
    };
    let label_text = label.to_string();
    let hint_text = hint.to_string();
    // Hoist the emptiness check to a plain `bool` ABOVE the `ui!` block. An
    // `if` whose condition contains a method call (`.is_empty()`) is lowered
    // to the reactive `when()` path (an `Fn` closure that cannot move its
    // captures); a bare `bool` instead takes the static `FnOnce` arm, which
    // may move `hint_style` / `hint_text` into the branch.
    let show_hint = !hint_text.is_empty();
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(12.0)),
            padding_top: Some(px(8.0)),
            padding_bottom: Some(px(8.0)),
            ..Default::default()
        }) {
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Column),
                flex_grow: Some(grow(1.0)),
                gap: Some(px(2.0)),
                ..Default::default()
            }) {
                text(style = label_style) { label_text }
                if show_hint {
                    text(style = hint_style) { hint_text }
                }
            }
            view(style = on_change_style) {
                view(style = knob_style) {}
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    on_change(!on.get());
                }
                TouchResponse::CONSUMED
            })
        }
    }
}

/// A settings section header: a small uppercase label, Discord's "Settings"
/// category headers.
fn section_header(label: &str) -> Element {
    let style = StyleRules {
        padding_top: Some(px(16.0)),
        padding_bottom: Some(px(4.0)),
        font_size: Some(px(11.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT_MUTED)),
        ..Default::default()
    };
    ui! { text(style = style) { label.to_string() } }
}

/// One status option row in the Status section: a colored dot, the status
/// name, and a check when selected. `selected` re-runs reactively when
/// `user_status` changes; tapping sets it.
fn status_row(
    status: Status,
    user_status: Signal<Status>,
) -> Element {
    let name = status.name().to_string();
    let dot_style = move || StyleRules {
        width: Some(px(12.0)),
        height: Some(px(12.0)),
        background: Some(col(status.color())),
        border_top_left_radius: Some(px(6.0)),
        border_top_right_radius: Some(px(6.0)),
        border_bottom_left_radius: Some(px(6.0)),
        border_bottom_right_radius: Some(px(6.0)),
        ..Default::default()
    };
    let label_style = move || StyleRules {
        font_size: Some(px(15.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT)),
        flex_grow: Some(grow(1.0)),
        ..Default::default()
    };
    let check_style = move || StyleRules {
        font_size: Some(px(15.0)),
        color: Some(col(BUTTON)),
        ..Default::default()
    };
    ui! {
        view(style = move || StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(10.0)),
            padding_top: Some(px(6.0)),
            padding_bottom: Some(px(6.0)),
            padding_left: Some(px(8.0)),
            padding_right: Some(px(8.0)),
            border_top_left_radius: Some(px(6.0)),
            border_top_right_radius: Some(px(6.0)),
            border_bottom_left_radius: Some(px(6.0)),
            border_bottom_right_radius: Some(px(6.0)),
            background: Some(col(if user_status.get() == status {
                BG_ACTIVE
            } else {
                "transparent"
            })),
            ..Default::default()
        }) {
            view(style = dot_style) {}
            text(style = label_style) { name }
            if user_status.get() == status {
                text(style = check_style) { "✓".to_string() }
            }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                user_status.set(status);
            }
            TouchResponse::CONSUMED
        })
    }
}

/// The User Settings overlay. `on_close` is called when the backdrop or the
/// X is tapped (unmounting this subtree returns to the app shell). `user_status`
/// is the app's live presence signal so the Status section edits it directly.
pub fn user_settings(
    user_status: Signal<Status>,
    on_close: impl Fn() + 'static,
) -> Element {
    // Session-local display name, seeded from the static identity. The
    // data module holds no live name — this is UI state, like the composer.
    let display_name: Signal<String> = signal(USER.name.to_string());
    // Notification toggles, one signal each so a flip re-renders only its row.
    let msg_notify: Signal<bool> = signal(true);
    let mention_notify: Signal<bool> = signal(true);
    let push_notify: Signal<bool> = signal(true);
    let sound_notify: Signal<bool> = signal(true);
    let auto_read: Signal<bool> = signal(true);
    let read_receipts: Signal<bool> = signal(false);
    // A transient "Saved" toast that appears on display-name change and
    // fades out on its own. `Signal<bool>` so the toast subtree is reactive.
    let saved: Signal<bool> = signal(false);

    let on_name_change = {
        let saved = saved.clone();
        move |_s: String| {
            saved.set(true);
            after_ms_detached(1600, move || saved.set(false));
        }
    };

    // Backdrop: dims the app behind the panel. Tapping it closes the
    // overlay (the panel itself sits above and stops the touch).
    let backdrop_style = StyleRules {
        position: Some(Position::Absolute),
        left: Some(pct(0.0)),
        top: Some(pct(0.0)),
        width: Some(pct(100.0)),
        height: Some(pct(100.0)),
        background: Some(col("rgba(0, 0, 0, 0.6)")),
        ..Default::default()
    };

    // `on_close` is used by two different `on_touch` handlers (the X button
    // and the backdrop); an `on_touch` closure is `Fn` (it can't move its
    // captures). Share the callback through an `Rc` and clone the cheap
    // handle per use site.
    let on_close = Rc::new(on_close);
    let close = on_close.clone();
    let close_btn_style = StyleRules {
        width: Some(px(28.0)),
        height: Some(px(28.0)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        color: Some(col(TEXT_MUTED)),
        font_size: Some(px(16.0)),
        border_top_left_radius: Some(px(6.0)),
        border_top_right_radius: Some(px(6.0)),
        border_bottom_left_radius: Some(px(6.0)),
        border_bottom_right_radius: Some(px(6.0)),
        ..Default::default()
    };
    let close_text_style = StyleRules {
        color: Some(col(TEXT_MUTED)),
        font_size: Some(px(16.0)),
        ..Default::default()
    };
    let close_btn = ui! {
        view(style = close_btn_style) {
            text(style = close_text_style) { "✕".to_string() }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                close();
            }
            TouchResponse::CONSUMED
        })
    };

    // Title bar: "User Settings" + close.
    let title_style = StyleRules {
        font_size: Some(px(16.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT)),
        flex_grow: Some(grow(1.0)),
        ..Default::default()
    };
    let title_bar = ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            padding_top: Some(px(14.0)),
            padding_bottom: Some(px(14.0)),
            padding_left: Some(px(20.0)),
            padding_right: Some(px(20.0)),
            gap: Some(px(8.0)),
            border_bottom_width: Some(border_width(1.0)),
            border_bottom_color: Some(col(BORDER)),
            ..Default::default()
        }) {
            text(style = title_style) { "User Settings".to_string() }
            close_btn
        }
    };

    // My Account section: avatar + name input + (transient) "Saved" toast.
    let name_input_style = StyleRules {
        flex_grow: Some(grow(1.0)),
        height: Some(px(40.0)),
        padding_left: Some(px(12.0)),
        padding_right: Some(px(12.0)),
        background: Some(col(COMPOSER)),
        color: Some(col(TEXT)),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        ..Default::default()
    };
    let saved_style = StyleRules {
        font_size: Some(px(12.0)),
        color: Some(col("#23a559")),
        font_weight: Some(FontWeight::SemiBold),
        ..Default::default()
    };

    let avatar_style = StyleRules {
        width: Some(px(72.0)),
        height: Some(px(72.0)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        background: Some(col(USER.color)),
        color: Some(col(TEXT)),
        font_size: Some(px(30.0)),
        font_weight: Some(FontWeight::Bold),
        border_top_left_radius: Some(px(36.0)),
        border_top_right_radius: Some(px(36.0)),
        border_bottom_left_radius: Some(px(36.0)),
        border_bottom_right_radius: Some(px(36.0)),
        ..Default::default()
    };
    let avatar_text = USER.name.chars().next().map(|c| c.to_ascii_uppercase()).unwrap_or('?').to_string();

    let my_account = ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            padding_top: Some(px(16.0)),
            padding_bottom: Some(px(12.0)),
            ..Default::default()
        }) {
            section_header("My Account")
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(16.0)),
                padding_top: Some(px(8.0)),
                ..Default::default()
            }) {
                text(style = avatar_style) { avatar_text }
                text_input(
                    value = display_name,
                    on_change = on_name_change,
                    placeholder = "Display name",
                    style = name_input_style.clone(),
                )
                if saved.get() {
                    text(style = saved_style.clone()) { "Saved".to_string() }
                }
            }
        }
    };

    // Status section: the four presence options.
    let status_section = ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            ..Default::default()
        }) {
            section_header("Status")
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Column),
                gap: Some(px(2.0)),
                ..Default::default()
            }) {
                status_row(Status::Online, user_status.clone())
                status_row(Status::Idle, user_status.clone())
                status_row(Status::DoNotDisturb, user_status.clone())
                status_row(Status::Offline, user_status.clone())
            }
        }
    };

    // Notifications section: toggle rows.
    let notifications = ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            ..Default::default()
        }) {
            section_header("Notifications")
            toggle_row("Mention messages", "When someone @mentions you", mention_notify.clone(), move |b| mention_notify.set(b))
            toggle_row("Direct messages", "When someone DMs you", msg_notify.clone(), move |b| msg_notify.set(b))
            toggle_row("Push notifications", "Notify your devices", push_notify.clone(), move |b| push_notify.set(b))
            toggle_row("Message sounds", "Play a sound for new messages", sound_notify.clone(), move |b| sound_notify.set(b))
            toggle_row("Mark channels read", "Auto-read when you scroll", auto_read.clone(), move |b| auto_read.set(b))
            toggle_row("Read receipts", "Show when you've read a DM", read_receipts.clone(), move |b| read_receipts.set(b))
        }
    };

    // The panel: right-anchored, full height, 380px wide, scrollable content.
    let panel_style = StyleRules {
        position: Some(Position::Absolute),
        right: Some(px(0.0)),
        top: Some(px(0.0)),
        width: Some(px(380.0)),
        height: Some(pct(100.0)),
        background: Some(col(BG_SIDEBAR)),
        border_left_width: Some(border_width(1.0)),
        border_left_color: Some(col(BORDER)),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };

    let content = ui! {
        view(style = StyleRules {
            padding_left: Some(px(20.0)),
            padding_right: Some(px(20.0)),
            padding_bottom: Some(px(24.0)),
            ..Default::default()
        }) {
            my_account
            status_section
            notifications
        }
    };

    let close2 = on_close;
    ui! {
        view(style = backdrop_style) {
            view(style = panel_style) {
                title_bar
                scroll_view(style = StyleRules {
                    flex_grow: Some(grow(1.0)),
                    overflow: Some(Overflow::Hidden),
                    ..Default::default()
                }) { content }
            }
            .on_touch(move |ev| {
                // Swallow touches landing on the panel so the backdrop's
                // close handler (parent) never fires for in-panel taps.
                TouchResponse::CONSUMED
            })
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                close2();
            }
            TouchResponse::CONSUMED
        })
    }
}
