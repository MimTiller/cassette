//! `audit_log_row` — a single Discord server-audit-log row for the Discord
//! clone. Renders one hardcoded local audit event — actor "Alex", action
//! "kicked", target "Sam" — as one readable line ("Alex kicked Sam"), with a
//! muted hardcoded timestamp ("2 hours ago") next to it on the same line.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `activity_card` /
//! `server_event_card` / `server_discovery_card` ship in).
//!
//! Idioms, each verified against this crate's `components/activity_card.rs` /
//! `components/edited_label.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the row is a single `view` (`flex_direction: Row`, `align_items:
//!   Center`, `gap`) holding the two `text` leaves as inline `ui!` children
//!   — no `Vec<Element>` built outside the macro.

use crate::style_helpers::{col, px};
use runtime_core::{component, ui, AlignItems, Element, FlexDirection, StyleRules};

// Discord dark palette — kept local so the component is self-contained.
const TEXT: &str = "#f2f3f5"; // primary text
const TEXT_MUTED: &str = "#949ba4"; // muted timestamp text

// The one hardcoded local audit event — no backend fetch.
const ACTOR: &str = "Alex";
const ACTION: &str = "kicked";
const TARGET: &str = "Sam";
const TIMESTAMP: &str = "2 hours ago";

/// A single audit-log row. Renders the hardcoded local event as one readable
/// line — "Alex kicked Sam" — with a muted hardcoded timestamp next to it on
/// the same line. No props, no backend.
#[component]
pub fn AuditLogRow() -> Element {
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(8.0)),
            ..Default::default()
        }) {
            // The readable event line: "<actor> <action> <target>".
            text(style = StyleRules {
                font_size: Some(px(14.0)),
                color: Some(col(TEXT)),
                ..Default::default()
            }) { format!("{ACTOR} {ACTION} {TARGET}") }

            // The muted timestamp, next to the event text on the same line.
            text(style = StyleRules {
                font_size: Some(px(12.0)),
                color: Some(col(TEXT_MUTED)),
                ..Default::default()
            }) { TIMESTAMP.to_string() }
        }
    }
}
