//! `message_action_bar` — the hover action bar for a Discord message. Shown at
//! the top-right of a message row while the row is hovered (the row owns the
//! `visible` signal and feeds it from its own `on_hover`), it renders a compact
//! pill of three icon buttons — react, reply, more — each wired to a callback
//! the caller supplies:
//!
//! - **react** → `on_react`: opens the SAME add-reaction picker the
//!   `reaction_area` strip already renders (the caller points the shared
//!   `picker` signal at this message). There is deliberately no second,
//!   per-bar reaction UI — the always-visible add-reaction button in
//!   `reaction_area` is the other entry point into that one picker.
//! - **reply** → `on_reply`: the caller populates the composer's reply context
//!   (author + snippet), which the `reply_preview_bar` strip reads.
//! - **more** → `on_more`: the caller opens this message's small overflow menu
//!   (the "Pin message" entry lives there; see `app.rs`).
//!
//! The bar is `Position::Absolute` at `top: 0 / right: 0`, so — placed inside
//! the row's `Position::Relative` wrapper (`app.rs`) — it floats over the row's
//! top-right corner, Discord-style, without affecting the row's layout. It is a
//! child of that wrapper, so while the pointer rests on the bar the row's
//! `on_hover` keeps `visible` true (the bar stays up while you use it, and
//! disappears when the pointer leaves the row).
//!
//! Idioms, each verified against this crate's `app.rs` (`emoji_popover` /
//! `create_server_modal` / `reaction_picker_overlay`):
//! - the show/hide is the `switch` primitive (`switch(|| visible.get(), …)`):
//!   its branch closure is *plain Rust*, so the three `Rc` callbacks are
//!   cloned there (branch-local, owned per evaluation) and moved into the
//!   button `on_touch(move)` handlers — a `move` handler can only own a
//!   branch-local handle, never a captured variable;
//! - each icon button is a `view` + `.on_touch(|ev| …)` chain with the
//!   `TouchPhase::Ended` guard — the app's standard click pattern (the
//!   runtime's `button` helper takes a fixed `label` prop, which doesn't fit a
//!   glyph-only square). `Position::Relative` gives each button a definite box
//!   for hit-testing;
//! - the glyphs are static string bodies (`.to_string()`), so they bake to
//!   static `TextSource` with no reactive machinery; every style is written
//!   INLINE in `style = …` (created fresh per evaluation).
use crate::theme;
use std::rc::Rc;

use crate::style_helpers::{col, px};
use runtime_core::{
    switch, ui, AlignItems, Element, FlexDirection, JustifyContent, Position, Signal, StyleRules,
    TouchPhase, TouchResponse,
};

// Discord dark palette (kept local so the component is self-contained).

/// The message action bar. Renders the react / reply / more pill while `visible`
/// is set; each button fires its callback. Positioned absolutely at the row's
/// top-right by the caller's wrapper (`app.rs`).
pub fn message_action_bar(
    visible: Signal<bool>,
    on_react: Rc<dyn Fn()>,
    on_reply: Rc<dyn Fn()>,
    on_more: Rc<dyn Fn()>,
) -> Element {
    switch(
        move || visible.get(),
        move |show: &bool| {
            // Collapsed while not hovered: a zero-size view so the branch
            // unmounts the pill without affecting the row's layout (the
            // `emoji_popover` idiom).
            if !show {
                return ui! {
                    view(style = StyleRules {
                        width: Some(px(0.0)),
                        height: Some(px(0.0)),
                        ..Default::default()
                    }) {}
                };
            }
            // Clone each callback here (branch-local, owned per evaluation) so
            // its `move` handler can own it — a captured variable of this `Fn`
            // closure could not be moved out.
            let react = on_react.clone();
            let reply = on_reply.clone();
            let more = on_more.clone();

            // The action bar: a horizontal row of the three icon buttons,
            // centered, on the elevated bar surface with rounded corners — the
            // shape Discord draws on a message's top-right on hover. Absolute
            // at top-right so it floats over the row (see file header).
            ui! {
                view(style = StyleRules {
                    position: Some(Position::Absolute),
                    top: Some(px(0.0)),
                    right: Some(px(12.0)),
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    gap: Some(px(2.0)),
                    padding_left: Some(px(4.0)),
                    padding_right: Some(px(4.0)),
                    padding_top: Some(px(2.0)),
                    padding_bottom: Some(px(2.0)),
                    background: Some(col(theme::BG_SECONDARY)),
                    border_top_left_radius: Some(px(8.0)),
                    border_top_right_radius: Some(px(8.0)),
                    border_bottom_left_radius: Some(px(8.0)),
                    border_bottom_right_radius: Some(px(8.0)),
                    ..Default::default()
                }) {
                    // React button — opens the shared add-reaction picker (the
                    // caller points `picker` at this message). No second UI here.
                    view(style = StyleRules {
                        position: Some(Position::Relative),
                        width: Some(px(32.0)),
                        height: Some(px(32.0)),
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        border_top_left_radius: Some(px(4.0)),
                        border_top_right_radius: Some(px(4.0)),
                        border_bottom_left_radius: Some(px(4.0)),
                        border_bottom_right_radius: Some(px(4.0)),
                        ..Default::default()
                    }) {
                        text(style = StyleRules {
                            font_size: Some(px(16.0)),
                            color: Some(col(theme::INTERACTIVE)),
                            ..Default::default()
                        }) { "😊".to_string() }
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            crate::emoji_picker::set_anchor(ev.window_position.x, ev.window_position.y);
                            react();
                        }
                        TouchResponse::CONSUMED
                    })

                    // Reply button — populates the composer's reply context.
                    view(style = StyleRules {
                        position: Some(Position::Relative),
                        width: Some(px(32.0)),
                        height: Some(px(32.0)),
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        border_top_left_radius: Some(px(4.0)),
                        border_top_right_radius: Some(px(4.0)),
                        border_bottom_left_radius: Some(px(4.0)),
                        border_bottom_right_radius: Some(px(4.0)),
                        ..Default::default()
                    }) {
                        text(style = StyleRules {
                            font_size: Some(px(16.0)),
                            color: Some(col(theme::INTERACTIVE)),
                            ..Default::default()
                        }) { "↩".to_string() }
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            reply();
                        }
                        TouchResponse::CONSUMED
                    })

                    // More button — opens this message's overflow menu.
                    view(style = StyleRules {
                        position: Some(Position::Relative),
                        width: Some(px(32.0)),
                        height: Some(px(32.0)),
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        border_top_left_radius: Some(px(4.0)),
                        border_top_right_radius: Some(px(4.0)),
                        border_bottom_left_radius: Some(px(4.0)),
                        border_bottom_right_radius: Some(px(4.0)),
                        ..Default::default()
                    }) {
                        text(style = StyleRules {
                            font_size: Some(px(16.0)),
                            color: Some(col(theme::INTERACTIVE)),
                            ..Default::default()
                        }) { "⋯".to_string() }
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            more();
                        }
                        TouchResponse::CONSUMED
                    })
                }
            }
        },
    )
}
