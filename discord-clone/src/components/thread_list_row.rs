//! `thread_list_row` — a single Discord channel-thread row for the Discord
//! clone. Renders one hardcoded local thread — name "Bug reports", reply
//! count `12` — as one compact line: a small thread icon, the thread name,
//! and a muted reply-count numeral, all side by side.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `audit_log_row` /
//! `activity_card` / `server_event_card` ship in).
//!
//! Idioms, each verified against this crate's `components/audit_log_row.rs` /
//! `components/message_rating.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the row is a single `view` (`flex_direction: Row`, `align_items:
//!   Center`, `gap`) holding the three `text` leaves (icon, name, count) as
//!   inline `ui!` children — no `Vec<Element>` built outside the macro;
//! - the thread "icon" is a static string glyph (the thread spool "🧵"),
//!   baked to `TextSource::Static` — no reactive machinery, the same
//!   static-glyph approach `message_rating` uses for its ↑ / ↓ arrows.

use crate::theme;
use crate::style_helpers::{col, px};
use runtime_core::{component, ui, AlignItems, Element, FlexDirection, StyleRules};

// Discord dark palette — kept local so the component is self-contained.

// The one hardcoded local thread — no backend fetch.
const THREAD_NAME: &str = "Bug reports";
const REPLY_COUNT: i32 = 12;

/// A single thread row. Renders the hardcoded local thread as one compact
/// line — a small thread icon, the thread name, and a muted reply count —
/// all side by side. No props, no backend.
#[component]
pub fn ThreadListRow() -> Element {
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(8.0)),
            ..Default::default()
        }) {
            // The small thread icon — a static glyph, so it bakes to
            // `TextSource::Static` (no reactive machinery).
            text(style = StyleRules {
                font_size: Some(px(14.0)),
                ..Default::default()
            }) { "🧵".to_string() }

            // The thread name, on the same line.
            text(style = StyleRules {
                font_size: Some(px(14.0)),
                color: Some(col(theme::TEXT_STRONG)),
                ..Default::default()
            }) { THREAD_NAME.to_string() }

            // The muted reply count, next to the name on the same line.
            text(style = StyleRules {
                font_size: Some(px(12.0)),
                color: Some(col(theme::TEXT_MUTED)),
                ..Default::default()
            }) { REPLY_COUNT.to_string() }
        }
    }
}
