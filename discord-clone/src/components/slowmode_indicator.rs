#![allow(dead_code)]
//! `slowmode_indicator` — a "slowmode" timer indicator for the Discord clone.
//! Renders a small clock glyph (🕓) beside a static "Slowmode: {n}s" label —
//! the compact notice Discord shows while a channel's slowmode cooldown is
//! active. The cooldown seconds arrive as an inline `secs` prop; when the
//! value is `0` the whole indicator renders **nothing at all** (no icon, no
//! pill, no empty node).
//!
//! Wired into the channel header's composer-area column in `app.rs`, where the
//! active channel's `slowmode_secs` (`Option<u32>`, see `crate::data`) is
//! resolved and passed as `slowmode_secs.unwrap_or(0)` — so the pill shows only
//! for slowmode-enabled channels (a couple of the mock channels set it) and
//! renders nothing for the rest. Gated per-channel, not unconditional.
//!
//! Idioms, each verified against this crate's
//! `components/pin_count_badge.rs` / `components/jump_to_bottom_button.rs`
//! / `components/dm_view.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the "render nothing when the value is 0" part is a STATIC
//!   `if secs > 0 { … }` wrapping the WHOLE pill, with no `else`. Because the
//!   condition is call-free (once `secs` is bound to a plain `u32` local), the
//!   macro lowers it through `emit_plain_if`'s `Ctx::Child` arm to a plain
//!   Rust `if` whose taken branch flattens into its flat siblings and whose
//!   missing `else` contributes *nothing* — so a zero value mounts no node at
//!   all. (The same shape `pin_count_badge` uses for its
//!   `if PIN_COUNT > 0 { … }` count badge.)
//! - the clock glyph and the label are both static values baked to
//!   `TextSource::Static` — the glyph is a string literal, the label is a
//!   `format!(…)` bound above the `ui!` block (the exact idiom
//!   `pin_count_badge.rs` uses for `count_label`). No signals anywhere here;
//!   the per-channel gating is done at the call site by passing `0` or a real
//!   cooldown.

use crate::theme;
use crate::style_helpers::{col, px};
use runtime_core::{
    component, ui, AlignItems, Element, FlexDirection, FontWeight, StyleRules,
};

// Discord dark palette (kept local so the component is self-contained).

/// The slowmode timer indicator. A small pill with a clock glyph and the
/// "Slowmode: {n}s" label. The cooldown seconds arrive as a prop (`secs`), so
/// a call site can gate the whole indicator on real per-channel state: pass
/// `0` (e.g. a channel's `slowmode_secs.unwrap_or(0)`) and the component
/// renders *nothing* — no icon, no pill, no empty node — exactly the way the
/// old hardcoded `SLOWMODE_SECONDS == 0` path did. Passing a nonzero value
/// mounts the pill with that channel's actual cooldown. This is what lets the
/// channel header show the indicator only for slowmode-enabled channels.
#[component]
pub fn SlowmodeIndicator(#[prop(default = 0)] secs: u32) -> Element {
    // `secs` is a data param, so the macro wraps it `Reactive<u32>`; read it
    // into a plain `u32` once at build. The label is a one-shot `format!` of
    // that value (a static text body — the same `pin_count_badge` shape for
    // `count_label`). No signals: the gating happens at the call site, which
    // passes either `0` (render nothing) or the channel's real cooldown.
    let secs = secs.get();
    let label = format!("Slowmode: {}s", secs);

    ui! {
        // Static branch: `secs > 0` is call-free once bound to the `u32` local
        // above, so the macro lowers it through `emit_plain_if`'s `Ctx::Child`
        // arm to a plain Rust `if`. The pill flattens into flat siblings when
        // true; the missing `else` contributes nothing, so a zero value mounts
        // no node at all. `StyleRules` is not `Copy`, so the styles are
        // written INLINE (`style = …`), created fresh per evaluation — the
        // same shape `pin_count_badge` uses.
        if secs > 0 {
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(4.0)),
                padding_left: Some(px(7.0)),
                padding_right: Some(px(7.0)),
                padding_top: Some(px(3.0)),
                padding_bottom: Some(px(3.0)),
                background: Some(col(theme::BG_SECONDARY)),
                border_top_left_radius: Some(px(8.0)),
                border_top_right_radius: Some(px(8.0)),
                border_bottom_left_radius: Some(px(8.0)),
                border_bottom_right_radius: Some(px(8.0)),
                ..Default::default()
            }) {
                // The clock glyph — a static string literal, so it bakes to
                // `TextSource::Static` (no reactive machinery).
                text(style = StyleRules {
                    font_size: Some(px(13.0)),
                    color: Some(col(theme::TEXT_MUTED)),
                    ..Default::default()
                }) { "🕓".to_string() }
                // The label — the static `label` bound above, so it bakes to
                // `TextSource::Static` too.
                text(style = StyleRules {
                    font_size: Some(px(12.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    color: Some(col(theme::TEXT_MUTED)),
                    ..Default::default()
                }) { label }
            }
        }
    }
}
