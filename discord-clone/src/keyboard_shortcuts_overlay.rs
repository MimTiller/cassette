//! `keyboard_shortcuts_overlay` -- a standalone "Keyboard Shortcuts" overlay
//! for the Discord clone. A dimmed full-screen backdrop with a centered
//! panel that lists a hardcoded set of fake keyboard shortcuts (a label and
//! its key-combo text). The close button toggles a local `Signal<bool>` open
//! flag, so tapping it hides the whole overlay. No backend call, no wiring
//! into any other screen yet: declared in `lib.rs` so it compiles as part of
//! the crate, but referenced by nothing.
//!
//! Idioms follow the crate's other overlays (for example
//! `profile_settings`): a plain `Signal<bool>` for the open state, `StyleRules`
//! built with the `style_helpers` `px` / `col` / `pct` constructors, an
//! on_touch` handler that fires on `TouchPhase::Ended`, and a `ui!` body.

use runtime_core::{
    AlignItems, Element, FlexDirection, FontWeight, JustifyContent, Overflow, Position, Signal,
    StyleRules, TouchPhase, TouchResponse,
    signal, ui,
};

use crate::style_helpers::{col, pct, px};

// The same Discord dark palette as `profile_settings.rs`. Kept local so the
// overlay stays self-contained; the values mirror it exactly so the two
// trees agree pixel-for-pixel.
const BG_SIDEBAR: &str = "#2b2d31";
const BG_ACTIVE: &str = "#404249";
const TEXT: &str = "#f2f3f5";
const TEXT_MUTED: &str = "#949ba4";
const BORDER: &str = "#26272b";

fn border_width(v: f32) -> runtime_core::Tokenized<f32> {
    runtime_core::Tokenized::Literal(v)
}

/// One hardcoded shortcut: a human label and its key-combo text. Kept as a
/// small tuple struct so the list below reads as data, and so each row can
/// be rendered without a `#[component]` (this file is referenced by
/// nothing, so there is no call site that needs the `ui!` struct-literal
/// form).
struct Shortcut {
    label: &'static str,
    combo: &'static str,
}

/// The fake shortcut list. Deliberately placeholder data -- a handful of
/// plausible Discord-style shortcuts so the panel looks populated.
const SHORTCUTS: &[Shortcut] = &[
    Shortcut { label: "Open chat history", combo: "Ctrl + H" },
    Shortcut { label: "Mute/unmute microphone", combo: "Shift + M" },
    Shortcut { label: "Open user settings", combo: "Ctrl + ," },
    Shortcut { label: "Go to unread messages", combo: "Ctrl + U" },
    Shortcut { label: "Open keyboard shortcuts", combo: "?" },
];

/// One row of the list: the shortcut label on the left, the key combo on the
/// right (rendered as a small pill).
fn shortcut_row(s: &Shortcut) -> Element {
    let label_style = StyleRules {
        font_size: Some(px(15.0)),
        color: Some(col(TEXT)),
        flex_grow: Some(runtime_core::Tokenized::Literal(1.0)),
        ..Default::default()
    };
    let combo_style = StyleRules {
        font_size: Some(px(12.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT_MUTED)),
        padding_left: Some(px(8.0)),
        padding_right: Some(px(8.0)),
        padding_top: Some(px(2.0)),
        padding_bottom: Some(px(2.0)),
        background: Some(col(BG_ACTIVE)),
        border_top_left_radius: Some(px(4.0)),
        border_top_right_radius: Some(px(4.0)),
        border_bottom_left_radius: Some(px(4.0)),
        border_bottom_right_radius: Some(px(4.0)),
        ..Default::default()
    };
    let combo_text = s.combo.to_string();
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            padding_top: Some(px(10.0)),
            padding_bottom: Some(px(10.0)),
            ..Default::default()
        }) {
            text(style = label_style) { s.label.to_string() }
            text(style = combo_style) { combo_text }
        }
    }
}

/// The Keyboard Shortcuts overlay. On first mount it is open (`signal(true)`);
/// the close button toggles the local open flag, which hides the entire
/// backdrop+panel subtree. Nothing external drives it.
pub fn keyboard_shortcuts_overlay() -> Element {
    let open: Signal<bool> = signal(true);

    // Backdrop: dims whatever is behind the panel. Tapping it does NOT close
    // the overlay -- only the close button does (matches the request).
    let backdrop_style = StyleRules {
        position: Some(Position::Absolute),
        left: Some(pct(0.0)),
        top: Some(pct(0.0)),
        width: Some(pct(100.0)),
        height: Some(pct(100.0)),
        background: Some(col("rgba(0, 0, 0, 0.6)")),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        ..Default::default()
    };

    let close = open.clone();
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

    let title_style = StyleRules {
        font_size: Some(px(16.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT)),
        flex_grow: Some(runtime_core::Tokenized::Literal(1.0)),
        ..Default::default()
    };

    // The panel: centered, fixed width, scrollable content.
    let panel_style = StyleRules {
        width: Some(px(380.0)),
        background: Some(col(BG_SIDEBAR)),
        border_top_left_radius: Some(px(10.0)),
        border_top_right_radius: Some(px(10.0)),
        border_bottom_left_radius: Some(px(10.0)),
        border_bottom_right_radius: Some(px(10.0)),
        border_top_width: Some(border_width(1.0)),
        border_right_width: Some(border_width(1.0)),
        border_bottom_width: Some(border_width(1.0)),
        border_left_width: Some(border_width(1.0)),
        border_top_color: Some(col(BORDER)),
        border_right_color: Some(col(BORDER)),
        border_bottom_color: Some(col(BORDER)),
        border_left_color: Some(col(BORDER)),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        padding_top: Some(px(16.0)),
        padding_left: Some(px(20.0)),
        padding_right: Some(px(20.0)),
        padding_bottom: Some(px(16.0)),
        ..Default::default()
    };

    // Build the whole panel inline inside the `if` so the macro constructs it
    // fresh on every evaluation (the `ui!` conditional lowers to an `Fn`
    // closure that must not *move* non-`Clone` captures like `Element`;
    // `StyleRules` and `Signal` are `Clone`, so cloning them is fine).
    ui! {
        if open.get() {
            view(style = backdrop_style.clone()) {
                view(style = panel_style.clone()) {
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        padding_bottom: Some(px(14.0)),
                        gap: Some(px(8.0)),
                        border_bottom_width: Some(border_width(1.0)),
                        border_bottom_color: Some(col(BORDER)),
                        margin_bottom: Some(px(8.0)),
                        ..Default::default()
                    }) {
                        text(style = title_style.clone()) { "Keyboard Shortcuts".to_string() }
                        view(style = close_btn_style.clone()) {
                            text(style = close_text_style.clone()) { "✕".to_string() }
                        }
                        .on_touch(move |_ev| {
                            if _ev.phase == TouchPhase::Ended {
                                close.set(!close.get());
                            }
                            TouchResponse::CONSUMED
                        })
                    }
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Column),
                        gap: Some(px(2.0)),
                        ..Default::default()
                    }) {
                        for s in SHORTCUTS { shortcut_row(s) }
                    }
                }
            }
        }
    }
}
