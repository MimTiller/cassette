//! `spoiler_tag` — an inline spoiler for the Discord clone. It renders the
//! text from its `text` prop (hardcoded default `"the twist was all along"`,
//! so the component also compiles standalone) with the text visible
//! underneath a dark, blurred-out overlay by default. Clicking it toggles a
//! local `revealed` boolean signal (hardcoded `false` at start); while it is
//! `false` the overlay covers the text so it reads as a blurred / blacked-out
//! block, and while it is `true` the overlay unmounts, revealing the real
//! text.
//!
//! It is wired into `app.rs` `message_body`: a message body is split on
//! Discord's `||…||` spoiler markers, and each marked run renders as one
//! `SpoilerTag` (the plain runs stay inline text). Content-dependent — only
//! the one seed message carrying a `||…||` marker shows a spoiler; the rest
//! render a single plain run. A deliberate single instance, not a per-row
//! widget.
//!
//! Idioms, each verified against this crate's siblings:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `pct` / `col`
//!   constructors (one static pass, no live style engine);
//! - the "overlay on by default, off when revealed" part is a reactive
//!   `if revealed.get() { … }` inside `ui!` — an `if` whose condition reads a
//!   signal (`.get()`) lowers to the `when` primitive (mounts while true,
//!   unmounts on false). The overlay is the *only* thing gated by the signal,
//!   so it is present at start (hidden) and unmounts the moment `revealed`
//!   flips — the same reactive-`if` `jump_to_bottom_button` uses for its pill;
//! - there is no content-blur in `StyleRules` (the runtime can't blur text,
//!   only a translucent `background` fill per the style docs), so the "blurred
//!   out" look is approximated with a near-opaque dark `OVERLAY` fill at
//!   `opacity: 0.92` laid over the text — a soft dark veil that hides it the
//!   way a blur would;
//! - the "cover the whole text" part is a fill-absolute overlay child:
//!   `position: Absolute` + `left/top: 0` + `width/height: pct(100.0)` (the
//!   exact fill idiom `app.rs` uses for its full-window profile overlay), with
//!   `pointer_events: None` so the veil is purely visual and never swallows
//!   the click;
//! - the click is the root `view` + `.on_touch(|ev| …)` chain with the
//!   `TouchPhase::Ended` guard — the app's standard click pattern
//!   (`jump_to_bottom_button` / `message_rating` / `emoji_upload_slot`). The
//!   root is the whole tag, so one handler covers the text *and* the overlay
//!   (the overlay's touches still reach the root handler — the same reason
//!   `emoji_upload_slot`'s single root handler covers its 20 absolute dash
//!   children). The handler is a `move` closure capturing a `Copy` of the
//!   `revealed` signal, so it stays `Fn` (callable on every press) and merely
//!   toggles the local state — no backend call.

use crate::theme;
use crate::style_helpers::{col, pct, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, JustifyContent, Overflow,
    PointerEvents, Position, StyleRules, Tokenized, TouchPhase, TouchResponse,
};

// Palette, kept local so the component is self-contained.
const RADIUS: f32 = 6.0;

/// The inline spoiler tag. Renders the `text` prop (the spoiler body the
/// caller extracts from a message's `||…||` marker) under a dark blurred-out
/// overlay by default; clicking toggles the local `revealed` signal, which
/// unmounts the overlay and reveals the real text (clicking again toggles it
/// back). `text` arrives as a `String` prop with a hardcoded default so the
/// component still compiles standalone; no backend.
#[component]
pub fn SpoilerTag(
    #[prop(default = "the twist was all along")] text: String,
) -> Element {
    // The spoiler body, read once from the prop. The prop is never `.set`, so
    // it is a static value (the `RoleBadge` / `ChannelTopicBanner` idiom).
    let body = text.get();

    // Whether the spoiler has been revealed. Local only — starts `false`, so
    // the tag ships covered.
    let revealed = signal(false);

    ui! {
        // The tag host: a `Position::Relative` anchor so the fill-absolute
        // overlay child sizes to it. Row-aligned + center-justified so the
        // (inline-flow) text node sizes to its own content and the overlay's
        // `pct(100.0)` fills exactly that box — never a hardcoded width, which
        // would under- or over-cover the text.
        view(style = StyleRules {
            position: Some(Position::Relative),
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::Center),
            overflow: Some(Overflow::Hidden),
            ..Default::default()
        }) {
            // The real text. Read from the `text` prop into a static body; a
            // `body.clone()` (owned `String`, borrowed — never moves the
            // non-`Copy` local out of the reactive branch's `Fn` render
            // closure), so it bakes to `TextSource::Static` (no reactive
            // machinery) — always mounted, only ever covered by the overlay
            // above.
            text(style = StyleRules {
                font_size: Some(px(15.0)),
                color: Some(col(theme::TEXT_STRONG)),
                padding_left: Some(px(8.0)),
                padding_right: Some(px(8.0)),
                padding_top: Some(px(4.0)),
                padding_bottom: Some(px(4.0)),
                border_top_left_radius: Some(px(RADIUS)),
                border_top_right_radius: Some(px(RADIUS)),
                border_bottom_left_radius: Some(px(RADIUS)),
                border_bottom_right_radius: Some(px(RADIUS)),
                ..Default::default()
            }) { body.clone() }

            // Reactive branch: `if revealed.get() {…}` lowers to a `when` whose
            // render is an `Fn` closure. `StyleRules` is not `Copy`, so the
            // overlay style is written INLINE (created fresh per evaluation,
            // never captured across the `when` boundary) — the same shape
            // `jump_to_bottom_button` uses for its pill. While `revealed` is
            // `false` (the start state) the overlay mounts and covers the
            // text; when it flips `true` the overlay unmounts, revealing it.
            if !revealed.get() {
                view(style = StyleRules {
                    position: Some(Position::Absolute),
                    left: Some(px(0.0)),
                    top: Some(px(0.0)),
                    width: Some(pct(100.0)),
                    height: Some(pct(100.0)),
                    background: Some(col(theme::BG_TERTIARY)),
                    opacity: Some(Tokenized::Literal(0.92)),
                    pointer_events: Some(PointerEvents::None),
                    border_top_left_radius: Some(px(RADIUS)),
                    border_top_right_radius: Some(px(RADIUS)),
                    border_bottom_left_radius: Some(px(RADIUS)),
                    border_bottom_right_radius: Some(px(RADIUS)),
                    ..Default::default()
                })
            }
        }
        .on_touch(move |ev| {
            // The reveal affordance: toggle the local signal on press.
            // `revealed` is `Copy`, so the `move` closure captures a copy of
            // the same underlying signal (the one the reactive `if` above
            // reads) and remains `Fn`. No backend call — this only drives the
            // local state. Touches on the overlay reach this root handler
            // (it is the root of the whole tag), so the veil never needs its
            // own handler.
            if ev.phase == TouchPhase::Ended {
                revealed.set(!revealed.get());
            }
            TouchResponse::CONSUMED
        })
    }
}
