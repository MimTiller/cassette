//! `create_server_modal` — the "Add a Server" (create) modal for the server
//! rail.
//!
//! A centered card over a dimmed full-screen backdrop (the same build-once/
//! toggle idiom as `join_modal.rs`: when `open` is false the wrapper collapses
//! to zero size; when true it fills the window). It has a single state: a
//! `text_input` for the server name above a **Create** button. A **Cancel**
//! button (or tapping the backdrop) closes the modal without creating
//! anything.
//!
//! The `open` / `name` signals are owned by the caller (`app.rs`), which
//! resets `name` on open so a re-open always starts fresh. The `on_create`
//! callback is called with the trimmed name when Create is tapped with a
//! non-empty name; it is the caller's responsibility to create the server and
//! close the modal.
//!
//! Idioms: the button `.on_touch` chains are built as `let` bindings (the
//! `close_btn` idiom in `invite_modal.rs`) so the method chain lives outside
//! the `ui!` child position. `Signal` is `Copy`, so every `move` closure
//! captures a copy and stays `Fn`.
use std::rc::Rc;

use runtime_core::{
    ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent,
    Overflow, Position, Signal, StyleRules, Tokenized, TouchPhase, TouchResponse,
};

use crate::style_helpers::{col, pct, px};
use crate::server_icon_upload_placeholder::ServerIconUploadPlaceholder;
use crate::server_region_selector::ServerRegionSelector;

// Discord dark palette (kept local so the component is self-contained).
const CARD: &str = "#2b2d31";
const INPUT_BG: &str = "#1e1f22";
const BTN_BG: &str = "#5865f2";
const BTN_CANCEL: &str = "#4e5058";
const TEXT: &str = "#f2f3f5";
const TEXT_CHANNEL: &str = "#c8cacf";
const TEXT_MUTED: &str = "#949ba4";

pub fn create_server_modal(
    open: Signal<bool>,
    name: Signal<String>,
    on_create: Rc<dyn Fn(String)>,
) -> Element {
    let close = open;
    let n = name;
    let create = on_create.clone();

    // Shared card/field/button styles (one static pass).
    let card_style = StyleRules {
        width: Some(px(360.0)),
        padding_top: Some(px(20.0)),
        padding_bottom: Some(px(20.0)),
        padding_left: Some(px(20.0)),
        padding_right: Some(px(20.0)),
        background: Some(col(CARD)),
        border_top_left_radius: Some(px(12.0)),
        border_top_right_radius: Some(px(12.0)),
        border_bottom_left_radius: Some(px(12.0)),
        border_bottom_right_radius: Some(px(12.0)),
        flex_direction: Some(FlexDirection::Column),
        ..Default::default()
    };
    let title_style = StyleRules {
        font_size: Some(px(18.0)),
        font_weight: Some(FontWeight::Bold),
        color: Some(col(TEXT)),
        ..Default::default()
    };
    let sub_style = StyleRules {
        font_size: Some(px(13.0)),
        color: Some(col(TEXT_MUTED)),
        margin_top: Some(px(4.0)),
        ..Default::default()
    };
    let input_style = StyleRules {
        height: Some(px(40.0)),
        margin_top: Some(px(16.0)),
        padding_left: Some(px(12.0)),
        padding_right: Some(px(12.0)),
        background: Some(col(INPUT_BG)),
        color: Some(col(TEXT_CHANNEL)),
        font_size: Some(px(15.0)),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        ..Default::default()
    };
    let btn_row_style = StyleRules {
        margin_top: Some(px(14.0)),
        flex_direction: Some(FlexDirection::Row),
        gap: Some(px(8.0)),
        ..Default::default()
    };
    let create_btn_style = StyleRules {
        height: Some(px(40.0)),
        flex_grow: Some(Tokenized::Literal(1.0)),
        background: Some(col(BTN_BG)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        ..Default::default()
    };
    let cancel_btn_style = StyleRules {
        height: Some(px(40.0)),
        flex_grow: Some(Tokenized::Literal(1.0)),
        background: Some(col(BTN_CANCEL)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        ..Default::default()
    };
    let btn_label_style = StyleRules {
        font_size: Some(px(15.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT)),
        ..Default::default()
    };

    // Create handler: gate on non-empty trimmed name, then call the callback.
    let on_create = move || {
        let raw = n.get();
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return;
        }
        create(trimmed.to_string());
    };

    let create_btn = ui! {
        view(style = create_btn_style.clone(), test_id = "create-server-create") {
            text(style = btn_label_style.clone()) { "Create".to_string() }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                on_create();
            }
            TouchResponse::CONSUMED
        })
    };
    let cancel_btn = ui! {
        view(style = cancel_btn_style.clone(), test_id = "create-server-cancel") {
            text(style = btn_label_style.clone()) { "Cancel".to_string() }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                close.set(false);
            }
            TouchResponse::CONSUMED
        })
    };

    let card = ui! {
        view(style = card_style.clone()) {
            text(style = title_style.clone()) { "Add a Server".to_string() }
            text(style = sub_style.clone()) { "Enter a name for your new server".to_string() }
            text_input(
                value = name.clone(),
                placeholder = "Server name",
                style = input_style.clone(),
                test_id = "create-server-name-input",
            )
            view(style = btn_row_style.clone()) {
                { create_btn }
                { cancel_btn }
            }
            ServerIconUploadPlaceholder()
            ServerRegionSelector()
        }
        .on_touch(move |_ev| TouchResponse::CONSUMED)
    };

    // Full-screen backdrop (same idiom as join_modal.rs).
    let open_chk = open;
    let wrap_style = move || StyleRules {
        position: Some(Position::Absolute),
        left: Some(px(0.0)),
        top: Some(px(0.0)),
        width: Some(if open_chk.get() { pct(100.0) } else { px(0.0) }),
        height: Some(if open_chk.get() { pct(100.0) } else { px(0.0) }),
        background: Some(col("rgba(0, 0, 0, 0.6)")),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    ui! {
        view(style = wrap_style) {
            { card }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                close.set(false);
            }
            TouchResponse::CONSUMED
        })
    }
}
