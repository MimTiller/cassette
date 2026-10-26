//! `idle_timer_badge` — the "go idle / clear idle" control in the Discord
//! clone. It is an 18px icon button (the same shape as the user panel's
//! mic/deafen/settings/shortcuts icons) holding a Lucide `MOON` glyph.
//!
//! It is wired to the app's live presence status (`status: Signal<Status>`),
//! not to local state: the glyph is amber (`#f0b232`, Discord's idle dot
//! color) while the user is `Status::Idle`, muted otherwise. Tapping it
//! sets the status to `Status::Idle`; tapping it again while idle cycles
//! the status on with `Status::next()` (Idle → DoNotDisturb → Online → …),
//! which is the same one-click cycle the profile-strip avatar uses.
//!
//! This is the control's correct home — the user panel's status/voice icon
//! row (see `app.rs` `user_panel`) — NOT the message list. The previous
//! self-contained version (a local `idle` boolean + a "Toggle idle" pill) was
//! spliced into the message row during the graveyard-component rewire, which
//! rendered a "Toggle idle" button inline as if it were a chat message. That
//! was a UI-chrome bug; this rewrite takes the shared status signal so the
//! toggle actually flips the user's presence and it renders as a compact
//! control in the row with the other status controls.
//!
//! Idioms, each verified against this crate's `app.rs` (the `user_panel`
//! mic/deafen icon buttons) and `components/message_search_bar.rs`:
//! - the glyph is the `icon(MOON).color(|| …).size(15.0)` form — exactly the
//!   `icon(MIC)` / `icon(HEADPHONES)` shape in `user_panel`;
//! - the active state is a reactive `if status.get() == Status::Idle {…}
//!   else {…}` reading the signal `.get()` — an `if` whose condition reads a
//!   signal lowers to the `when` primitive (re-renders on the status
//!   change);
//! - the click is the pill's `.on_touch(|ev| …)` chain with the
//!   `TouchPhase::Ended` guard — the app's standard click pattern. The handler
//!   captures a `Copy` of the `status` signal, so it stays `Fn` and merely
//!   drives the shared state — no backend call.

use crate::data::Status;
use crate::style_helpers::px;
use icons_lucide::MOON;
use runtime_core::{
    component, icon, ui, AlignItems, Color, Element, JustifyContent, StyleRules,
    Tokenized, TouchPhase, TouchResponse,
};

// Discord dark palette (kept local so the component is self-contained).
const TEXT_MUTED: &str = "#949ba4"; // the inactive glyph (matches the row's icons)
const IDLE: &str = "#f0b232"; // the active idle glyph (Discord's idle dot color)

/// The idle control: an 18px icon button holding a `MOON` glyph. Amber while
/// the user is idle, muted otherwise; a tap goes idle (or cycles on, if
/// already idle), driven through the app's shared `status` signal.
#[component]
#[allow(non_snake_case)]
pub fn IdleTimerBadge(status: runtime_core::Signal<Status>) -> Element {
    // The idle glyph swaps to amber while `status` is `Idle`; a tap sets it to
    // `Idle`, or (already idle) advances it one step so a second tap clears
    // idle — the same cycle the profile-strip avatar uses. `status` is `Copy`,
    // so the `move` closure captures a copy of the same signal the reactive
    // `if` reads and remains `Fn`.
    let toggle = move || {
        let s = status;
        s.set(if s.get() == Status::Idle {
            s.get().next()
        } else {
            Status::Idle
        });
    };

    ui! {
        view(style = StyleRules {
            width: Some(px(18.0)),
            height: Some(px(18.0)),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::Center),
            flex_shrink: Some(Tokenized::Literal(0.0)),
            ..Default::default()
        }, test_id = "user-idle") {
            if status.get() == Status::Idle {
                icon(MOON).color(|| Color::from(IDLE)).size(15.0)
            } else {
                icon(MOON).color(|| Color::from(TEXT_MUTED)).size(15.0)
            }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                (toggle)();
            }
            TouchResponse::CONSUMED
        })
    }
}
