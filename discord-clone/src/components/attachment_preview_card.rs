//! `attachment_preview_card` — a self-contained "attachment preview" card for
//! the Discord clone. It shows two hardcoded local values — a filename
//! (`"screenshot.png"`) and a size (`"2.4 MB"`) — next to a generic file icon,
//! with a small "x" remove button that flips a local `removed` boolean signal
//! (hardcoded `false` at start). While `removed` is `true` the card renders
//! nothing.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `welcome_banner` /
//! `edited_label` / `server_event_card` ship in).
//!
//! Idioms, each verified against this crate's `components/welcome_banner.rs`
//! (a card with a "x" dismiss that renders nothing once dismissed) and
//! `components/emoji_upload_slot.rs` (the `view` + `.on_touch` tap pattern):
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the card-vs-nothing swap is a node-level reactive
//!   `if !removed.get() { … }` inside `ui!` — the `if` condition reads a
//!   signal (`.get()`), so it lowers to the `when` primitive (mounts the card
//!   while not removed, unmounts it the moment the "x" is tapped). `StyleRules`
//!   is not `Copy`, so every style is written INLINE (created fresh per
//!   evaluation, never captured across the `when` boundary). A top-level `if`
//!   with no wrapping node is a legal `ui!` tree (the grammar is
//!   `ui! := node*`), so the fragment root holds — the same shape
//!   `welcome_banner` / `edited_label` use.
//! - the generic file icon is a `view` carrying a Unicode document glyph
//!   (the `icon` primitive isn't used in the `ui!` macro here — the app renders
//!   glyphs as text, e.g. the voice-row speaker in `app.rs`), so the card stays
//!   dependency-free;
//! - the "x" remove button is a `view` + `.on_touch(|ev| …)` box with the
//!   `TouchPhase::Ended` guard — the app's standard click pattern (the
//!   runtime's `button` helper takes a fixed `label` prop, which doesn't fit a
//!   small glyph tap target).

use crate::theme;
use crate::style_helpers::{col, px};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent,
    Position, StyleRules, Tokenized, TouchPhase, TouchResponse,
};

// Discord dark palette — kept local so the component is self-contained.

// The two hardcoded local values — no backend fetch, just literals.
const FILENAME: &str = "screenshot.png";
const SIZE: &str = "2.4 MB";

/// The attachment preview card. Renders the hardcoded filename and size next to
/// a generic file icon; a small "x" button sets the local `removed` signal, and
/// once it is set the card renders nothing. No props, no backend.
#[component]
#[allow(dead_code)] // declared but not wired into any screen yet
pub fn AttachmentPreviewCard() -> Element {
    // Whether the attachment has been removed. Local only; `false` at start.
    let removed = signal(false);

    ui! {
        // Reactive branch: `if !removed.get() {…}` lowers to a `when` that
        // mounts the card while not removed and unmounts it the moment the
        // "x" is tapped. A top-level `if` with no wrapping node is a legal
        // `ui!` tree (the grammar is `ui! := node*`), so the fragment root
        // holds — the same shape `welcome_banner` / `edited_label` use.
        if !removed.get() {
            view(style = StyleRules {
                width: Some(px(320.0)),
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(10.0)),
                padding_top: Some(px(12.0)),
                padding_bottom: Some(px(12.0)),
                padding_left: Some(px(14.0)),
                padding_right: Some(px(14.0)),
                background: Some(col(theme::BG_SECONDARY)),
                border_top_left_radius: Some(px(8.0)),
                border_top_right_radius: Some(px(8.0)),
                border_bottom_left_radius: Some(px(8.0)),
                border_bottom_right_radius: Some(px(8.0)),
                ..Default::default()
            }) {
                // The generic file icon: a small rounded tile with a Unicode
                // document glyph. The `icon` primitive isn't used in the `ui!`
                // macro here, so the glyph stays dependency-free (the same
                // text-glyph idiom as the voice-row speaker in `app.rs`).
                view(style = StyleRules {
                    width: Some(px(36.0)),
                    height: Some(px(36.0)),
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    background: Some(col(theme::BUTTON_SECONDARY)),
                    border_top_left_radius: Some(px(6.0)),
                    border_top_right_radius: Some(px(6.0)),
                    border_bottom_left_radius: Some(px(6.0)),
                    border_bottom_right_radius: Some(px(6.0)),
                    ..Default::default()
                }) {
                    text(style = StyleRules {
                        font_size: Some(px(18.0)),
                        ..Default::default()
                    }) { "📄".to_string() }
                }

                // The filename + size column: takes the spare width so the
                // "x" stays pinned to the right edge.
                view(style = StyleRules {
                    flex_grow: Some(Tokenized::Literal(1.0)),
                    flex_direction: Some(FlexDirection::Column),
                    align_items: Some(AlignItems::FlexStart),
                    ..Default::default()
                }) {
                    text(style = StyleRules {
                        font_size: Some(px(14.0)),
                        font_weight: Some(FontWeight::SemiBold),
                        color: Some(col(theme::TEXT_STRONG)),
                        ..Default::default()
                    }) { FILENAME.to_string() }
                    text(style = StyleRules {
                        font_size: Some(px(12.0)),
                        color: Some(col(theme::TEXT_MUTED)),
                        ..Default::default()
                    }) { SIZE.to_string() }
                }

                // The remove "x": a 24x24 tap target. `.on_touch` sets
                // `removed` on `TouchPhase::Ended` — the app's standard click
                // pattern. Style built inline (nothing captured across the
                // `when` boundary).
                view(style = StyleRules {
                    width: Some(px(24.0)),
                    height: Some(px(24.0)),
                    position: Some(Position::Relative),
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    border_top_left_radius: Some(px(6.0)),
                    border_top_right_radius: Some(px(6.0)),
                    border_bottom_left_radius: Some(px(6.0)),
                    border_bottom_right_radius: Some(px(6.0)),
                    border_top_width: Some(Tokenized::Literal(1.0)),
                    border_right_width: Some(Tokenized::Literal(1.0)),
                    border_bottom_width: Some(Tokenized::Literal(1.0)),
                    border_left_width: Some(Tokenized::Literal(1.0)),
                    border_top_color: Some(col(theme::BUTTON_SECONDARY)),
                    border_right_color: Some(col(theme::BUTTON_SECONDARY)),
                    border_bottom_color: Some(col(theme::BUTTON_SECONDARY)),
                    border_left_color: Some(col(theme::BUTTON_SECONDARY)),
                    ..Default::default()
                }) {
                    text(style = StyleRules {
                        font_size: Some(px(14.0)),
                        color: Some(col(theme::TEXT_MUTED)),
                        ..Default::default()
                    }) { "x".to_string() }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        removed.set(true);
                    }
                    TouchResponse::CONSUMED
                })
            }
        }
    }
}
