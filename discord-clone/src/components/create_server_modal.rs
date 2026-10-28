//! `create_server_modal` — the "Add a Server" (create) modal for the server
//! rail.
//!
//! A centered card over a dimmed full-screen backdrop (the same build-once/
//! toggle idiom as `join_modal.rs`: when `open` is false the wrapper collapses
//! to zero size; when true it fills the window). A `text_input` for the server
//! name above a **Create** button. A **Cancel** button (or tapping the
//! backdrop) closes the modal without creating anything.
//!
//! The `open` / `name` / `error` / `busy` signals are owned by the caller
//! (`app.rs`), which resets `name` / `error` / `busy` on open so a re-open
//! always starts fresh. The `on_create` callback is called with the trimmed
//! name (possibly empty) when Create is tapped — the modal itself imposes no
//! empty-name gate; the backend is the single source of truth for that (its
//! own wording, "server name must not be empty"). When the backend rejects,
//! the caller sets `error`, which is shown inline below the input while the
//! modal stays open (the name field remains editable so it can be corrected
//! and resubmitted); on success the caller closes the modal. `busy` guards
//! against a double-tap firing two RPCs and drives the button's
//! "Creating…" pending label. `on_join` is called when the secondary
//! "Have an invite already? Join a Server" link is tapped: this modal closes
//! and the caller opens the Join modal (with its usual per-open resets).
//!
//! Idioms: the button `.on_touch` chains are built as `let` bindings (the
//! `close_btn` idiom in `invite_modal.rs`) so the method chain lives outside
//! the `ui!` child position. The inline `error` line is the reactive
//! `if !…is_empty() { text { move || … } }` idiom from `app.rs`. `Signal` is
//! `Copy`, so every `move` closure captures a copy and stays `Fn`.
use crate::theme;
use std::rc::Rc;

use runtime_core::{
    ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent,
    Overflow, Position, Signal, StyleRules, Tokenized, TouchPhase, TouchResponse,
};

use crate::style_helpers::{borderless_input, col, pct, px};
use crate::server_icon_upload_placeholder::ServerIconUploadPlaceholder;
use crate::server_region_selector::ServerRegionSelector;

// Discord dark palette (kept local so the component is self-contained).

pub fn create_server_modal(
    open: Signal<bool>,
    name: Signal<String>,
    error: Signal<String>,
    busy: Signal<bool>,
    on_create: Rc<dyn Fn(String)>,
    on_join: Rc<dyn Fn()>,
) -> Element {
    let close = open;
    let n = name;
    let e = error;
    let b = busy;
    let create = on_create.clone();
    let join = on_join.clone();

    // Shared card/field/button styles (one static pass).
    let card_style = StyleRules {
        width: Some(px(360.0)),
        padding_top: Some(px(20.0)),
        padding_bottom: Some(px(20.0)),
        padding_left: Some(px(20.0)),
        padding_right: Some(px(20.0)),
        background: Some(col(theme::BG_SECONDARY)),
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
        color: Some(col(theme::TEXT_STRONG)),
        ..Default::default()
    };
    let sub_style = StyleRules {
        font_size: Some(px(13.0)),
        color: Some(col(theme::TEXT_MUTED)),
        margin_top: Some(px(4.0)),
        ..Default::default()
    };
    let input_style = StyleRules {
        height: Some(px(40.0)),
        margin_top: Some(px(16.0)),
        padding_left: Some(px(12.0)),
        padding_right: Some(px(12.0)),
        background: Some(col(theme::BG_TERTIARY)),
        color: Some(col(theme::TEXT_NORMAL)),
        font_size: Some(px(15.0)),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        ..borderless_input()
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
        background: Some(col(theme::ACCENT)),
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
        background: Some(col(theme::BUTTON_SECONDARY)),
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
        color: Some(col(theme::TEXT_STRONG)),
        ..Default::default()
    };
    let err_style = StyleRules {
        font_size: Some(px(13.0)),
        color: Some(col(theme::DANGER)),
        margin_top: Some(px(12.0)),
        ..Default::default()
    };

    // Create handler: the only client-side gate is the in-flight `busy` guard
    // (a fast double-tap would otherwise fire two RPCs and mint two servers).
    // The empty-name check is the backend's (its own wording, "server name
    // must not be empty"), so we do NOT short-circuit on an empty field — we
    // hand the trimmed name to the caller and surface the backend's rejection
    // in the inline `error` slot below. `Signal` is `Copy`, so every captured
    // copy stays `Fn`.
    let on_create = move || {
        if b.get() {
            return;
        }
        let raw = n.get();
        let trimmed = raw.trim().to_string();
        b.set(true);
        e.set(String::new());
        create(trimmed);
    };
    // The `name` signal is the input's source of truth. `value = name` is the
    // signal→widget leg only: keystrokes arrive via `on_change` (per the
    // `input_with_submit.rs` recipe — "without this, the signal never updates
    // and `get()` is always empty"). Without this callback the `name` signal
    // stayed empty while the DOM field showed the typed text, so Create always
    // handed the backend an empty name and every submit was rejected with
    // "server name must not be empty". `Signal` is `Copy`, so this closure
    // captures a copy of `n` and stays `Fn` on each keystroke.
    let on_name_change = move |s: String| n.set(s);

    let create_btn = ui! {
        view(style = create_btn_style.clone(), test_id = "create-server-create") {
            text(style = btn_label_style.clone()) { move || (if b.get() { "Creating…" } else { "Create" }).to_string() }
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
    // Secondary link: Discord's create card offers "Have an invite already?
    // Join a Server" next to Create. Tapping it closes THIS modal and hands
    // the caller the Join flow (`on_join` opens the join modal with its
    // usual per-open resets).
    let join_btn_style = StyleRules {
        height: Some(px(32.0)),
        margin_top: Some(px(10.0)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        background: Some(col(theme::BG_TERTIARY)),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        ..Default::default()
    };
    let join_btn = ui! {
        view(style = join_btn_style.clone(), test_id = "create-server-join-link") {
            text(style = btn_label_style.clone()) { "Have an invite already? Join a Server".to_string() }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                close.set(false);
                join();
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
                on_change = on_name_change,
                placeholder = "Server name",
                style = input_style.clone(),
                test_id = "create-server-name-input",
            )
            // Add-a-Server flow extras (mock/local, no backend): a server-icon
            // upload placeholder and a region selector. Both are self-contained
            // components; they sit between the name field and the action row
            // the way Discord's create-server form stacks icon + region above
            // the Create button.
            view(style = StyleRules {
                margin_top: Some(px(16.0)),
                width: Some(pct(100.0)),
                flex_direction: Some(FlexDirection::Column),
                gap: Some(px(12.0)),
                ..Default::default()
            }) {
                ServerIconUploadPlaceholder()
                ServerRegionSelector()
            }
            // Inline backend rejection (the backend's own wording — e.g. an
            // empty name). Shown only when a Create attempt was rejected; the
            // name field above stays editable so it can be corrected and
            // resubmitted. The same reactive-inline idiom as the per-row
            // backend error in app.rs (kick/role-change).
            if !e.get().is_empty() {
                text(style = err_style.clone()) { move || e.get().clone() }
            }
            view(style = btn_row_style.clone()) {
                { create_btn }
                { cancel_btn }
            }
            { join_btn }
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
        background: Some(col(theme::OVERLAY)),
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
