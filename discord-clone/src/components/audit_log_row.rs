//! `audit_log_row` — a single Discord server-audit-log row for the Discord
//! clone. Renders one audit event — an actor/action/target line (e.g. "Alex
//! banned Sam") plus a muted relative timestamp on the same line — fed by
//! the `text` and `timestamp` `String` props (hardcoded defaults `"Alex
//! kicked Sam"` / `"2 hours ago"`, so the component also compiles standalone).
//!
//! It is wired into the LIVE `ServerSettingsPanel` (app.rs `server_settings`
//! section) as a short, static "AUDIT LOG" section: a small set of mock log
//! lines rendered through this row, one per event. It is a deliberate, fixed
//! list — NOT a per-row widget spliced into every server/member row (the
//! graveyard anti-pattern).
//!
//! Idioms, each verified against this crate's `components/activity_card.rs` /
//! `components/edited_label.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the row is a single `view` (`flex_direction: Row`, `align_items:
//!   Center`, `gap`) holding the two `text` leaves as inline `ui!` children
//!   — no `Vec<Element>` built outside the macro.
//! - `text` / `timestamp` arrive as `String` props (the `#[component]`
//!   inline-props idiom — the macro rewrites them to `Reactive<String>`),
//!   read once into locals at build so the static text bodies are baked.

use crate::theme;
use crate::style_helpers::{col, px};
use runtime_core::{component, ui, AlignItems, Element, FlexDirection, StyleRules};

// Discord dark palette — kept local so the component is self-contained.

/// A single audit-log row. Renders the `text` event line with a muted
/// `timestamp` next to it on the same line. No backend.
#[component]
#[allow(non_snake_case)]
pub fn AuditLogRow(
    #[prop(default = "Alex kicked Sam")] text: String,
    #[prop(default = "2 hours ago")] timestamp: String,
) -> Element {
    // Static props (never `.set`): read once into owned locals so the text
    // bodies are baked as static nodes (the `InviteLinkDisplay` idiom).
    let event = text.get();
    let when = timestamp.get();

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(8.0)),
            ..Default::default()
        }) {
            // The readable event line, on the row's left.
            text(style = StyleRules {
                font_size: Some(px(14.0)),
                color: Some(col(theme::TEXT_STRONG)),
                flex_grow: Some(runtime_core::Tokenized::Literal(1.0)),
                ..Default::default()
            }) { event.to_string() }

            // The muted timestamp, right-aligned on the same line.
            text(style = StyleRules {
                font_size: Some(px(12.0)),
                color: Some(col(theme::TEXT_MUTED)),
                flex_shrink: Some(runtime_core::Tokenized::Literal(0.0)),
                ..Default::default()
            }) { when.to_string() }
        }
    }
}
