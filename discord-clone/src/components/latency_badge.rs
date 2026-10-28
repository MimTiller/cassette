//! `latency_badge` — a self-contained connection-latency badge for the Discord
//! clone. Renders a compact pill whose label is a hardcoded local millisecond
//! value formatted as `"{n}ms"`; the label's COLOR varies by the value's range
//! (the familiar green/amber/red health coding): green under 50ms, amber
//! 50–150ms, red above 150ms.
//!
//! Everything is hardcoded local data — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same shape
//! `pin_count_badge` / `connection_quality_indicator` / `verified_badge`
//! ship in). The hardcoded value is `42`, so the badge renders in the GREEN
//! range; change `LATENCY_MS` to `120` or `300` to exercise the amber / red
//! branches.
//!
//! Idioms, each verified against this crate's
//! `components/pin_count_badge.rs` / `components/connection_quality_indicator.rs`
//! / `components/send_failed_indicator.rs`:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - there is NO signal anywhere, so the label is a STATIC value: a
//!   `format!("{}ms", LATENCY_MS)` bound above the `ui!` block, which bakes to
//!   `TextSource::Static` (the exact idiom `pin_count_badge` uses for its
//!   `count_label`). No reactive `when`, no `.get()` reads;
//! - the range→color mapping is a pure, call-free helper `latency_color` over
//!   the `LATENCY_MS` const, so it is constant-folded at compile time — the
//!   label color is fixed for the build, not recomputed reactively;
//! - `StyleRules` is not `Copy`, so the per-node styles are written INLINE in
//!   `style = …` (the crate's universal style idiom), never shared as a
//!   `let` binding.

use crate::theme;
use crate::style_helpers::{col, px};
use runtime_core::{component, ui, AlignItems, Element, FlexDirection, FontWeight, StyleRules};

// Discord dark palette (kept local so the component is self-contained).

// Latency health colors — the green/amber/red coding Discord uses for
// connection quality. Green matches `connection_quality_indicator`'s solid
// bar fill; amber matches `send_failed_indicator`'s warning glyph; red is
// Discord's danger tone.

// The hardcoded local latency in milliseconds. No backend fetch — this is a
// self-contained badge, so a fixed value stands in for a live measurement.
// `42` is deliberately in the GREEN range, per spec.
const LATENCY_MS: i32 = 42;

/// The range→color mapping: green under 50, amber 50–150, red above 150.
/// Pure and call-free over a `const`, so `latency_color(LATENCY_MS)` is
/// constant-folded at compile time.
fn latency_color(ms: i32) -> &'static str {
    if ms < 50 {
        theme::SUCCESS
    } else if ms <= 150 {
        theme::WARNING
    } else {
        theme::DANGER
    }
}

/// The latency badge. A small pill whose `{n}ms` label is colored by the
/// hardcoded value's range (green / amber / red).
#[component]
pub fn LatencyBadge() -> Element {
    // Static: `LATENCY_MS` is a `const`, so this `format!` is a one-shot value,
    // not a reactive read. Bound above the `ui!` block so the text bakes to
    // `TextSource::Static` (the `pin_count_badge` `count_label` idiom).
    let latency_label = format!("{}ms", LATENCY_MS);

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            padding_left: Some(px(8.0)),
            padding_right: Some(px(8.0)),
            padding_top: Some(px(3.0)),
            padding_bottom: Some(px(3.0)),
            background: Some(col(theme::BG_SECONDARY)),
            border_top_left_radius: Some(px(8.0)),
            border_top_right_radius: Some(px(8.0)),
            border_bottom_left_radius: Some(px(8.0)),
            border_bottom_right_radius: Some(px(8.0)),
            ..Default::default()
        }) {
            // The `{n}ms` label. Its color comes from the compile-time range
            // check `latency_color(LATENCY_MS)` (green for the hardcoded `42`).
            // Inline `style = …` because `StyleRules` is not `Copy`.
            text(style = StyleRules {
                font_size: Some(px(13.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(latency_color(LATENCY_MS))),
                ..Default::default()
            }) { latency_label }
        }
    }
}
