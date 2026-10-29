#![allow(dead_code)]
//! `channel_topic_banner` — a self-contained "channel topic" banner for the
//! Discord clone. It renders a hardcoded local topic string (default
//! "General discussion - be kind!") as a thin strip across the top, with a
//! small collapse button on the right that toggles a local `collapsed`
//! boolean signal to hide / show the banner's topic text.
//!
//! Everything is hardcoded local state — no backend call, no wiring into any
//! screen yet. The topic arrives as an inline prop with `#[prop(default = …)]`
//! so the component compiles standalone and can be dropped into a screen
//! without a call site supplying a value (the same shape
//! `server_stats_card` ships in). Declared in `lib.rs` so it compiles as part
//! of the crate, but referenced by nothing.
//!
//! Idioms, each verified against this crate's `components/featured_server_banner.rs`
//! / `components/welcome_banner.rs` / `components/setup_checklist_item.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the topic is an inline `String` prop (the macro wraps it
//!   `Reactive<String>`); because it is a hardcoded constant (never `.set`), it
//!   is read once at build into an owned `String` local and baked as a static
//!   text body (`topic_text.clone()` in the branch) — the same shape
//!   `setup_checklist_item` uses for its `label`, and it keeps the non-`Copy`
//!   reactive handle out of the branch's `Fn` render closure;
//! - the show/hide of the topic text is a node-level reactive
//!   `if !collapsed.get() { … }` inside `ui!`. The `if` condition reads a
//!   signal (`.get()`), so it lowers to the `when` primitive (mounts the topic
//!   text while not collapsed, unmounts it on collapse). Because `StyleRules`
//!   is not `Copy`, the branch style is written INLINE (created fresh per
//!   evaluation, never captured across the `when` boundary);
//! - the collapse button is a `view` + `.on_touch(|ev| …)` glyph box with the
//!   `TouchPhase::Ended` guard — the app's standard click pattern (the runtime
//!   `button` helper takes a fixed `label` prop, which doesn't fit a chevron
//!   that swaps on toggle). `Position::Relative` gives it a definite box for
//!   hit-testing. The toggle closure is a plain `move` closure `||
//!   collapsed.set(!collapsed.get())` prebuilt in a `let` and passed by
//!   variable name; `Signal` is `Copy`, so the `move` closure captures a copy
//!   of the same underlying signal (the one the reactive `if` below also reads)
//!   and remains `Fn` (callable on every press). The button glyph itself is a
//!   reactive `if collapsed.get() { "▸" } else { "▸"… }` swap so the chevron
//!   points the right way in each state (the button stays mounted in both
//!   states — only the topic text hides — so it can always be used to toggle
//!   back).

use crate::theme;
use crate::style_helpers::{col, px, rounded};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, JustifyContent, Position,
    StyleRules, TouchPhase, TouchResponse,
};

// Discord dark palette (kept local so the component is self-contained).

/// The channel-topic banner. Renders a thin top strip holding the (hardcoded)
/// topic text on the left and a small collapse button on the right. While the
/// local `collapsed` signal is `false` (the start state) the topic text shows;
/// pressing the button flips `collapsed`, hiding the topic text (the strip and
/// button stay so the banner can be re-expanded), and pressing it again flips
/// it back. No backend call — only the local signal toggle.
#[component]
pub fn ChannelTopicBanner(
    #[prop(default = "General discussion - be kind!")] topic: String,
) -> Element {
    let collapsed = signal(false);

    // The topic is a hardcoded constant (never `.set`), so read it into an
    // owned `String` once at build and bake it as a static text body. The
    // branch below uses `topic_text.clone()` (borrow, not move) so the
    // non-`Copy` value is never moved out of the reactive branch's `Fn` render
    // closure — the same shape `setup_checklist_item` uses for its `label`.
    let topic_text = topic.get().to_string();

    // Toggle: flip `collapsed` on every press. `collapsed` is `Copy`, so the
    // `move` closure captures a copy of the same underlying signal (the one the
    // reactive `if` below also reads) and remains `Fn`.
    let on_toggle = move || collapsed.set(!collapsed.get());

    ui! {
        // The thin top strip: a horizontal flex that always stays mounted
        // (the collapse button must remain reachable to re-expand), holding the
        // topic text on the left and the button on the right.
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(8.0)),
            padding_top: Some(px(6.0)),
            padding_bottom: Some(px(6.0)),
            padding_left: Some(px(16.0)),
            padding_right: Some(px(12.0)),
            background: Some(col(theme::BG_PRIMARY)),
            ..rounded(6.0)
        }) {
            // Reactive branch: `if !collapsed.get() {…}` lowers to a `when` that
            // mounts the topic text while not collapsed and unmounts it the
            // moment the button is pressed. No `else` — while `collapsed` is
            // set, the strip simply holds no text (the button keeps its place).
            // The style is INLINE (created fresh per evaluation) because
            // `StyleRules` is not `Copy`; the topic body is a static
            // `topic_text.clone()` (owned `String`, borrowed — never moves the
            // non-`Copy` local out of this branch's `Fn` closure).
            if !collapsed.get() {
                text(style = StyleRules {
                    flex_grow: Some(runtime_core::Tokenized::Literal(1.0)),
                    font_size: Some(px(13.0)),
                    color: Some(col(theme::TEXT_NORMAL)),
                    ..Default::default()
                }) { topic_text.clone() }
            }

            // The collapse button: a small 24x24 chevron pill. `Position::Relative`
            // gives it a definite box for hit-testing. The glyph swaps on the
            // `collapsed` signal — "▾" (pointing down) while expanded, "▸"
            // (pointing right) while collapsed — so it always signals the next
            // action. The glyph's style is INLINE for the same non-`Copy`
            // reason; `on_toggle` IS `Copy`, so both arms capture it by copy.
            view(style = StyleRules {
                position: Some(Position::Relative),
                width: Some(px(24.0)),
                height: Some(px(24.0)),
                background: Some(col(theme::BG_SECONDARY)),
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                ..rounded(6.0)
            }) {
                if collapsed.get() {
                    text(style = StyleRules {
                        font_size: Some(px(12.0)),
                        color: Some(col(theme::INTERACTIVE)),
                        ..Default::default()
                    }) { "▸".to_string() }
                } else {
                    text(style = StyleRules {
                        font_size: Some(px(12.0)),
                        color: Some(col(theme::INTERACTIVE)),
                        ..Default::default()
                    }) { "▾".to_string() }
                }
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    on_toggle();
                }
                TouchResponse::CONSUMED
            })
        }
    }
}
