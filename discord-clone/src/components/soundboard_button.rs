//! `soundboard_button` — a self-contained soundboard trigger for the Discord
//! clone. It renders a single `button` whose label is a hardcoded local sound
//! name (`"Airhorn"`) and that carries a local `playing` boolean signal
//! (hardcoded `false` at start). A click flips `playing` to `true` — which
//! re-styles the button as "now playing" (a highlight background + a `▶`
//! glyph prepended to the label) — and schedules a one-shot timer that flips
//! `playing` back to `false` after a brief moment, so the button reverts to
//! its normal look on its own. No real audio is played and there is no
//! backend call.
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape `verified_badge` /
//! `idle_timer_badge` / `translate_toggle` ship in).
//!
//! Idioms, each verified against this crate's `components/translate_toggle.rs`
//! / `components/jump_to_bottom_button.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the button is the `button(label, on_click)` primitive; `on_click` is a
//!   plain `move` closure prebuilt in a `let` and passed by variable name
//!   (exactly the `button` in `translate_toggle` / `idle_timer_badge`).
//!   `Signal` is `Copy`, so the `move` closure captures a copy of the signal
//!   and stays `Fn` (callable on every press);
//! - the "styled differently while `playing`" part is a node-level reactive
//!   `if playing.get() { … } else { … }` inside `ui!`. An `if` whose condition
//!   reads a signal (`.get()`) lowers to the `when` primitive (mounts the
//!   chosen branch, unmounts the other on toggle) — the same reactive `if`
//!   `translate_toggle` uses for its original/translated swap. Because
//!   `StyleRules` is not `Copy`, both styles are written INLINE in `style = …`
//!   (created fresh per evaluation, never captured across the `when` boundary);
//!   the `on_press` closure is `Copy` (it captures only the `Copy` signal and a
//!   `Copy` inner timer closure), so BOTH branches can capture it by copy.
//! - the brief revert is `after_ms_detached` — the framework's documented
//!   choice for event handlers that run outside any component body ("imperative,
//!   off-scope call sites — event handlers"). It owns the timer in a runtime
//!   registry (no `ScheduledTask` to keep alive, nothing to cancel), so the
//!   click closure stays a plain `Fn` with no handle plumbing.

use crate::theme;
use crate::style_helpers::{col, px};
use runtime_core::{
    after_ms_detached, component, signal, ui, AlignItems, Element, FlexDirection, JustifyContent,
    StyleRules,
};

// The one hardcoded local sound this button triggers — no props, no backend.
const SOUND: &str = "Airhorn";

// How long the "now playing" highlight lingers before the button reverts.
const PLAYING_MS: i32 = 600;

// Discord palette (kept local so the component is self-contained).

/// A soundboard trigger. Renders one `button` labelled with the hardcoded
/// local sound name. While the local `playing` signal is `false` the button
/// shows the plain sound name on the blurple accent; a click sets `playing`
/// to `true`, swapping the button to its "now playing" look (green highlight +
/// a `▶` glyph) for a brief moment, after which a timer flips `playing` back
/// to `false` and the button returns to its normal style.
#[component]
#[allow(non_snake_case)]
pub fn SoundboardButton() -> Element {
    let playing = signal(false);

    // Press: light the "now playing" state and schedule its brief revert.
    // `playing` is `Copy`, so the outer `move` closure captures a copy of the
    // signal (the one the reactive `if` below also reads) and the inner timer
    // closure captures its own copy. Both closures are `Copy`/`Fn`.
    let on_press = move || {
        playing.set(true);
        after_ms_detached(PLAYING_MS, move || playing.set(false));
    };

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::Center),
            padding_top: Some(px(20.0)),
            padding_bottom: Some(px(20.0)),
            padding_left: Some(px(20.0)),
            padding_right: Some(px(20.0)),
            ..Default::default()
        }) {
            // Reactive branch: `if playing.get() {…} else {…}` lowers to a
            // `when` (mounts the chosen branch, unmounts the other on toggle).
            // The styles are INLINE (created fresh per evaluation) because
            // `StyleRules` is not `Copy`; `on_press` IS `Copy`, so both
            // branches capture it by copy. No real audio — only the local
            // signal flip and its brief timer-driven revert.
            if playing.get() {
                button(
                    label = format!("▶ {}", SOUND),
                    on_click = on_press,
                    style = StyleRules {
                        height: Some(px(34.0)),
                        padding_left: Some(px(18.0)),
                        padding_right: Some(px(18.0)),
                        background: Some(col(theme::SUCCESS)),
                        border_top_left_radius: Some(px(6.0)),
                        border_top_right_radius: Some(px(6.0)),
                        border_bottom_left_radius: Some(px(6.0)),
                        border_bottom_right_radius: Some(px(6.0)),
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        ..Default::default()
                    }
                )
            } else {
                button(
                    label = SOUND.to_string(),
                    on_click = on_press,
                    style = StyleRules {
                        height: Some(px(34.0)),
                        padding_left: Some(px(18.0)),
                        padding_right: Some(px(18.0)),
                        background: Some(col(theme::ACCENT)),
                        border_top_left_radius: Some(px(6.0)),
                        border_top_right_radius: Some(px(6.0)),
                        border_bottom_left_radius: Some(px(6.0)),
                        border_bottom_right_radius: Some(px(6.0)),
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        ..Default::default()
                    }
                )
            }
        }
    }
}
