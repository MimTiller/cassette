//! `server_folder_group` — a Discord-style server-folder row for the server
//! rail: a small stacked-servers glyph + the folder name in one clickable
//! header row, and (while the local `expanded` signal is set) a hardcoded
//! list of exactly two fake server names stacked below it.
//!
//! Everything is hardcoded local state — no backend call, no props, no
//! wiring into any screen yet. Declared in `lib.rs` so it compiles as part
//! of the crate, but referenced by nothing (the same shape `profile_settings`
//! ships in).
//!
//! Idioms, each verified against this crate's `app.rs` / `profile_settings.rs`:
//! styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//! constructors; the header row is a `view` with an `.on_touch(|ev| …)`
//! chain toggling the `expanded` signal on `TouchPhase::Ended` (returning
//! `TouchResponse::CONSUMED`); the server list is a plain
//! `if expanded.get() { … }` inside `ui!`, which lowers to the `when`
//! primitive (mounts while true, unmounts on false). The `for` loop is
//! lowered to a `repeat` whose per-item builder is an `Fn` closure, so the
//! per-row styles are inlined as `StyleRules` literals rather than shared
//! `let` bindings (a captured, non-`Copy` binding would be moved on the
//! first iteration).

use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, FontWeight, StyleRules, TouchPhase,
    TouchResponse,
};

// Discord dark palette (kept local so the component is self-contained).
const BG_RAIL: &str = "#1e1f22";
const BG_HOVER: &str = "#35373c";
const TEXT_MUTED: &str = "#949ba4";
const TEXT: &str = "#f2f3f5";

/// The folder label — hardcoded for now.
const FOLDER_NAME: &str = "Work";

/// The two fake servers inside the folder — hardcoded for now.
const SERVERS: [&str; 2] = ["Design Guild", "Rust Community"];

/// A server-folder group. The header row (stacked-servers glyph + folder
/// name) toggles the local `expanded` signal on click; while it is set, the
/// two hardcoded server names render below it, otherwise the list is
/// unmounted.
#[component]
pub fn server_folder_group() -> Element {
    let expanded = signal(false);

    let header_style = StyleRules {
        height: Some(px(34.0)),
        margin_left: Some(px(8.0)),
        margin_right: Some(px(8.0)),
        padding_left: Some(px(12.0)),
        padding_right: Some(px(8.0)),
        background: Some(col(BG_HOVER)),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        gap: Some(px(8.0)),
        ..Default::default()
    };

    let icon_style = StyleRules {
        font_size: Some(px(18.0)),
        color: Some(col(TEXT_MUTED)),
        ..Default::default()
    };

    let name_style = StyleRules {
        font_size: Some(px(14.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT_MUTED)),
        ..Default::default()
    };

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Stretch),
            gap: Some(px(4.0)),
            padding_top: Some(px(6.0)),
            background: Some(col(BG_RAIL)),
            ..Default::default()
        }) {
            view(style = header_style) {
                text(style = icon_style) { "🗂" }
                text(style = name_style) { FOLDER_NAME.to_string() }
            }.on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    expanded.set(!expanded.get());
                }
                TouchResponse::CONSUMED
            })
            if expanded.get() {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Column),
                    align_items: Some(AlignItems::Stretch),
                    ..Default::default()
                }) {
                    for name in SERVERS.iter() {
                        view(style = StyleRules {
                            height: Some(px(32.0)),
                            margin_left: Some(px(24.0)),
                            margin_right: Some(px(8.0)),
                            padding_left: Some(px(12.0)),
                            padding_right: Some(px(8.0)),
                            flex_direction: Some(FlexDirection::Row),
                            align_items: Some(AlignItems::Center),
                            gap: Some(px(8.0)),
                            ..Default::default()
                        }) {
                            text(style = StyleRules {
                                font_size: Some(px(14.0)),
                                color: Some(col(TEXT)),
                                ..Default::default()
                            }) { name.to_string() }
                        }
                    }
                }
            }
        }
    }
}
