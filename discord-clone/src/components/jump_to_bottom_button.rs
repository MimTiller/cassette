//! `jump_to_bottom_button` — a "jump to bottom" control for the Discord clone.
//!
//! The caller owns the real "the reader has scrolled up away from the newest
//! messages" state and passes it in as a `Signal<bool>` (`visible`); clicking
//! the pill invokes `on_jump`, which the caller wires to the message list's
//! `ScrollViewHandle::scroll_to`. This component renders a small floating pill
//! with a down-arrow glyph and a "Jump to bottom" label while `visible` is set,
//! and nothing at all while it is unset — it no longer hardcodes its own
//! visibility, so the pill only appears when the reader is actually away from
//! the bottom.
//!
//! It is a plain `fn -> Element` (not `#[component]`) because one of its inputs
//! is an `Rc<dyn Fn()>` callback, which the component macro's struct-literal
//! dispatch (relying on a `Default` props base) cannot represent. Callers assign
//! the returned `Element` to a `let` and splat the identifier as a child — the
//! same shape `create_server_modal` / `modal` use.
//!
//! Idioms, each verified against this crate's `app.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the "only render when `visible`" part is a reactive
//!   `if visible.get() { … }` inside `ui!` — an `if` whose condition reads a
//!   signal (`.get()`) lowers to the `when` primitive (mounts while true,
//!   unmounts on false), with no `else` branch;
//! - the pill is a `view` + `.on_touch(|ev| …)` chain with the
//!   `TouchPhase::Ended` guard — the app's standard click pattern (the
//!   runtime's `button` helper takes a fixed `label` prop, which doesn't fit
//!   a styled floating pill);
//! - "floating" is `position: Absolute` with `bottom` / `right` insets (the
//!   `StyleRules` position fields), anchored to the message column's
//!   `Position::Relative` viewport wrapper so it floats over the visible
//!   message area's bottom-right corner rather than scrolling with content.

use crate::theme;
use std::rc::Rc;

use crate::style_helpers::{col, px};
use runtime_core::{
    ui, AlignItems, Element, FlexDirection, FontWeight, Position, Signal, StyleRules, TouchPhase,
    TouchResponse,
};

// Discord palette (same constants `app.rs` uses for the blurple button and
// near-white text) — kept local so the component is self-contained.

/// The floating "jump to bottom" pill. Renders a down-arrow + "Jump to bottom"
/// label while `visible` is set; clicking it invokes `on_jump`. While `visible`
/// is unset, the pill renders nothing.
#[allow(non_snake_case)]
pub fn JumpToBottomButton(visible: Signal<bool>, on_jump: Rc<dyn Fn()>) -> Element {
    ui! {
        // Reactive branch: `if visible.get() {…}` lowers to a `when` whose
        // render is an `Fn` closure. `StyleRules` is not `Copy`, so both text
        // styles are written INLINE in `style = …` (created fresh per
        // evaluation, never captured across the `when` boundary). No `else`
        // branch: while unset, the button renders nothing.
        if visible.get() {
            view(style = StyleRules {
                position: Some(Position::Absolute),
                bottom: Some(px(16.0)),
                right: Some(px(16.0)),
                padding_left: Some(px(10.0)),
                padding_right: Some(px(10.0)),
                padding_top: Some(px(6.0)),
                padding_bottom: Some(px(6.0)),
                background: Some(col(theme::ACCENT)),
                color: Some(col(theme::TEXT_STRONG)),
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(6.0)),
                font_size: Some(px(12.0)),
                font_weight: Some(FontWeight::SemiBold),
                border_top_left_radius: Some(px(16.0)),
                border_top_right_radius: Some(px(16.0)),
                border_bottom_left_radius: Some(px(16.0)),
                border_bottom_right_radius: Some(px(16.0)),
                ..Default::default()
            }) {
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    color: Some(col(theme::TEXT_STRONG)),
                    ..Default::default()
                }) { "↓".to_string() }
                text(style = StyleRules {
                    font_size: Some(px(12.0)),
                    color: Some(col(theme::TEXT_STRONG)),
                    ..Default::default()
                }) { "Jump to bottom".to_string() }
            }
            .on_touch({
                // `on_jump` is an `Rc<dyn Fn()>` captured (by reference) by the
                // `when` render body, which is an `Fn` closure — so it can't be
                // moved into the `on_touch` handler by value. Clone the `Rc` into
                // a handle the handler owns.
                let on_jump = on_jump.clone();
                move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        on_jump();
                    }
                    TouchResponse::CONSUMED
                }
            })
        }
    }
}
