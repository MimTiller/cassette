//! `load_earlier_button` — a "Load earlier messages" scrollback affordance for
//! the Discord clone.
//!
//! The caller owns the real paging state (the backend's `History { before_id }`
//! cursor + the in-flight guard) and passes in an `Rc<dyn Fn()>` (`on_load`) it
//! invokes when the pill is tapped. This component renders a small pill with an
//! up-arrow glyph and a "Load earlier messages" label — it is the mirror image
//! of `jump_to_bottom_button` (which renders a down-arrow at the bottom).
//!
//! Visibility is NOT owned here: the caller mounts/unmounts this element with a
//! reactive `switch` on "is the reader at the top / following the live tail?"
//! (`live_at_bottom`), so the pill appears at the top of the scrollback while
//! the reader reads upward and vanishes once they scroll to the live tail.
//!
//! It is a plain `fn -> Element` (not `#[component]`) because one of its inputs
//! is an `Rc<dyn Fn()>` callback, which the component macro's struct-literal
//! dispatch (relying on a `Default` props base) cannot represent. Callers
//! assign the returned `Element` to a `let` and splat the identifier as a child
//! — the same shape `jump_to_bottom_button` / `send_failed_indicator` use.
//!
//! Idioms, each verified against this crate's `components/jump_to_bottom_button.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the pill is a `view` + `.on_touch(|ev| …)` chain with the
//!   `TouchPhase::Ended` guard — the app's standard click pattern (the runtime's
//!   `button` helper takes a fixed `label` prop, which doesn't fit a styled
//!   pill);
//! - the "in-flight" guard the caller passes renders a muted label while a
//!   page is loading so a double-tap can't fire a second `History` request.

use crate::theme;
use std::rc::Rc;

use crate::style_helpers::{col, px, rounded};
use runtime_core::{
    ui, AlignItems, AlignSelf, Element, FlexDirection, FontWeight, Signal, StyleRules,
    TouchPhase, TouchResponse,
};

// Discord palette (same constants `app.rs` uses for the blurple button and
// near-white text) — kept local so the component is self-contained.

/// The "Load earlier messages" scrollback pill. Renders an up-arrow + label.
/// `busy` is `true` while a history page is in flight (the label goes muted and
/// the tap is ignored so a double-tap can't fire a second request).
#[allow(non_snake_case)]
pub fn load_earlier_button(busy: Signal<bool>, on_load: Rc<dyn Fn()>) -> Element {
    ui! {
        view(style = StyleRules {
            align_self: Some(AlignSelf::Center),
            margin_top: Some(px(8.0)),
            margin_bottom: Some(px(8.0)),
            padding_left: Some(px(12.0)),
            padding_right: Some(px(12.0)),
            padding_top: Some(px(6.0)),
            padding_bottom: Some(px(6.0)),
            background: Some(col(theme::ACCENT)),
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(6.0)),
            font_size: Some(px(12.0)),
            font_weight: Some(FontWeight::SemiBold),
            ..rounded(16.0)
        }) {
            text(style = StyleRules {
                font_size: Some(px(14.0)),
                color: Some(col(theme::TEXT_STRONG)),
                ..Default::default()
            }) { "↑".to_string() }
            text(style = move || StyleRules {
                font_size: Some(px(12.0)),
                // Muted while a page is in flight ("Loading…"), bright otherwise.
                color: Some(col(if busy.get() { theme::TEXT_FAINT } else { theme::TEXT_STRONG })),
                ..Default::default()
            }) { move || (if busy.get() { "Loading…" } else { "Load earlier messages" }).to_string() }
        }
        .on_touch({
            // `on_load` is an `Rc<dyn Fn()>` captured (by reference) by this
            // `Fn` closure — clone the `Rc` into a handle the handler owns, the
            // same shape `jump_to_bottom_button` uses for its `on_jump`.
            let on_load = on_load.clone();
            let busy = busy.clone();
            move |ev| {
                if ev.phase == TouchPhase::Ended && !busy.get() {
                    on_load();
                }
                TouchResponse::CONSUMED
            }
        })
    }
}
