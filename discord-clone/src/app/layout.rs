//! Panel widths: the resize handle and the responsive sidebar / member panel sizing.
//! Split out of `app.rs`; `use super::*` keeps app.rs's imports and shared
//! colors/helpers in scope.

use super::*;

/// A thin vertical drag handle that resizes a fixed-width column by feeding
/// horizontal pans back into its `width` signal. It sits in the top-level
/// row flush against the column's inner edge, between that column and the
/// growing main view. `sign` is `+1` when dragging right should *grow* the
/// column (the channel sidebar, whose handle rides its right edge) and `-1`
/// for the member list (handle on its left edge, so dragging right shrinks
/// it). `id` is the robot `test_id` anchor (kept distinct per instance so the
/// two handles don't collide in the automation registry).
///
/// The pan callback is `Fn` (not `FnMut`), so the "width at drag start" is
/// stashed in a `Cell` — that gives the callback interior mutability without
/// requiring `FnMut`. `PanEvent::Moved.delta` is *cumulative* from `Began`
/// (window space, not view-local), so it's applied directly as an offset from
/// the stashed start rather than accumulated event-to-event (accumulating
/// onto an already-moving value would oscillate — see the recognizer docs).
pub(super) fn resize_handle(width: Signal<f32>, sign: f32, id: &'static str) -> Element {
    let hovered = signal(false);
    let start = Cell::new(0.0f32);
    // Transparent at rest (invisible, but still hit-testable — the default
    // `PointerEvents::Auto` doesn't skip empty-area hit-testing), a subtle
    // fill once hovered so the affordance is discoverable. `ColResize` gives
    // the east-west resize cursor over the strip.
    let handle_style = move || StyleRules {
        width: Some(px(6.0)),
        background: Some(col(if hovered.get() { BG_HOVER } else { "transparent" })),
        cursor: Some(Cursor::ColResize),
        // A fixed grab strip: never shrinks below 6px in the flex row, so it
        // stays reliably draggable even when the window is narrow.
        flex_shrink: Some(grow(0.0)),
        flex_direction: Some(FlexDirection::Column),
        ..Default::default()
    };
    ui! {
        view(style = handle_style, test_id = id) {}
            .on_touch({
                let h = pan(
                    PanRecognizer::default(),
                    move |ev: &PanEvent| {
                        match ev {
                            PanEvent::Began { .. } => {
                                start.set(width.get());
                            }
                            PanEvent::Moved { delta, .. } => {
                                let next = (start.get() + sign * delta.x).clamp(180.0, 400.0);
                                width.set(next);
                            }
                            // Width stays wherever it was left on release.
                            PanEvent::Ended { .. } | PanEvent::Cancelled => {}
                        }
                    },
                );
                move |ev: &TouchEvent| h(ev)
            })
            .on_hover(move |on| hovered.set(on))
    }
}

// --- responsive sidebar widths (window-resize driven) ----------------------
// The two side columns keep the user's manual drag choice in `sidebar_width` /
// `member_panel_width` (the previous task's drag handles write straight to
// those signals). But when the *window* itself narrows — a live
// `viewport_size()` change, not a drag — the fixed columns stop fitting and
// the layout used to just clip. These helpers fold the viewport into the
// *rendered* width without ever writing back to the user's signal, so
// widening the window back up past the collapse threshold automatically
// restores the manual width (we only read it here — we never overwrite it, so
// a user's dragged choice is never fought).
//
// Calling these inside the reactive style closures (`sidebar_style`,
// `member_panel`'s `panel_style`) is what subscribes the layout to the
// viewport: each closure re-runs on a real resize and re-resolves its `width`.
// Same idiom whiteboard's `chrome.rs` popover uses for a viewport-driven
// height — no hand-rolled resize detection, just the framework's
// `viewport_size()` signal.
//
// Thresholds (CSS px of total window width). The main message column is the
// star and gets a hard floor of `MAIN_MIN`; the two panels surrender space
// first, in Discord's own order — the member list collapses, then the channel
// sidebar shrinks toward its 180px drag-handle minimum. `RAIL` is the fixed
// server-rail width; `HANDLE` is each 6px drag handle.
pub(super) const RAIL: f32 = 72.0;
pub(super) const HANDLE: f32 = 6.0;
/// The minimum width the main message column is guaranteed. The two side
/// panels are what gets squeezed below this (the message column is the star).
pub(super) const MAIN_MIN: f32 = 520.0;
/// The channel sidebar's hard floor — the same 180 the drag handle clamps to,
/// so a narrow window never shrinks it past what a manual drag can reach.
pub(super) const SIDEBAR_MIN: f32 = 180.0;

/// `(rendered sidebar width, rendered member width)` for the current viewport.
/// Keeps the main message column at a hard floor of `MAIN_MIN` and squeezes
/// the two side panels to make room, in Discord's own order: the member list
/// sheds its space first (all the way down to `0` = fully collapsed) before the
/// channel sidebar is ever shrunk (toward the 180px handle floor). When the
/// viewport is wide enough to hold both panels at their chosen sizes, both are
/// returned exactly as the user set them — so widening the window back up
/// past the squeeze automatically restores a manual drag choice for free. The
/// manual signals are only ever *read* here, never written, so a drag choice is
/// never fought. Reading `viewport_size()` inside the reactive style closures
/// that call this is what makes the layout re-fire on every real resize (the
/// framework's own signal — no hand-rolled resize detection).
/// `(rendered sidebar width, rendered member width)` for the given viewport
/// width and the user's manual widths. Keeps the main message column at a hard
/// floor of `MAIN_MIN` and squeezes the two side panels to make room, in
/// Discord's own order: the member list sheds its space first (all the way
/// down to `0` = fully collapsed) before the channel sidebar is ever shrunk
/// (toward the 180px handle floor). When the viewport is wide enough to hold
/// both panels at their chosen sizes, both are returned exactly as the user set
/// them — so widening the window back up past the squeeze automatically
/// restores a manual drag choice for free. `sidebar`/`member` are the user's
/// manual widths; they are only ever *read*, never written, so a drag choice is
/// never fought.
///
/// Pure (no signal reads) so it can be unit-tested against exact pixel
/// budgets; `vw` is the value `viewport_size()` reports. The caller (the
/// reactive style closures) supplies that live value.
pub(super) fn effective_panel_widths_for(vw: f32, sidebar: f32, member: f32) -> (f32, f32) {
    let s = sidebar.max(SIDEBAR_MIN);
    let m = member.max(0.0);
    // Before the backend reports a real size (vw == 0) there is nothing to be
    // responsive about — keep the manual widths (the normal desktop sizes) so
    // the first paint is the same as the pre-resize behaviour, no collapse
    // flash.
    let budget = (vw - RAIL - HANDLE * 2.0 - MAIN_MIN).max(0.0);
    if vw <= 0.0 || budget >= s + m {
        return (s, m);
    }
    // Not enough room for both at their chosen sizes: shed the excess, member
    // first. The member panel absorbs the overflow down to 0; only what it
    // can't absorb then pulls the sidebar down, but never below its floor.
    let shed = (s + m) - budget;
    let m_out = m - m.min(shed);
    let s_shed = (shed - m.min(shed)).min((s - SIDEBAR_MIN).max(0.0));
    let s_out = (s - s_shed).max(SIDEBAR_MIN);
    (s_out, m_out)
}

/// Thin signal-reading wrapper over [`effective_panel_widths_for`]: reads the
/// live `viewport_size()` and the user's manual `sidebar_width` /
/// `member_panel_width` signals, then folds the viewport into the rendered
/// widths. Called *inside* the reactive style closures (see `sidebar_style`
/// and `member_panel`'s `panel_style`), which is what subscribes the layout to
/// the framework's `viewport_size()` signal — every real window resize re-runs
/// the closure and re-resolves the width (no hand-rolled resize detection).
pub(super) fn effective_panel_widths(sidebar: Signal<f32>, member: Signal<f32>) -> (f32, f32) {
    let vw = viewport_size().get().width;
    effective_panel_widths_for(vw, sidebar.get(), member.get())
}

#[cfg(test)]
mod responsive_width_tests {
    use super::*;

    /// Both panels at their manual 240px fit comfortably in a 1400px window
    /// → returned unchanged (the restore case: widening back up past the
    /// squeeze keeps the user's manual choice).
    #[test]
    fn wide_viewport_keeps_manual_widths() {
        assert_eq!(effective_panel_widths_for(1400.0, 240.0, 240.0), (240.0, 240.0));
        // A user who manually dragged the panels wider is also left alone.
        assert_eq!(effective_panel_widths_for(1900.0, 380.0, 380.0), (380.0, 380.0));
    }

    /// At 1000px the 480px of panels + 72 rail + 12 handles + 520 main floor =
    /// 1084 > 1000, so the excess (84) is shed from the member panel first:
    /// member 240 -> 156, sidebar stays 240.
    #[test]
    fn mid_viewport_shrinks_member_first() {
        let (s, m) = effective_panel_widths_for(1000.0, 240.0, 240.0);
        assert_eq!(s, 240.0, "channel sidebar untouched while member can absorb");
        assert_eq!(m, 156.0, "member panel sheds the 84px overflow first");
    }

    /// At 820px the budget for panels is 820 - 72 - 12 - 520 = 216. The 480px
    /// of panels overflows by 264: the member panel collapses entirely
    /// (240 -> 0) absorbing 240 of it, and the sidebar sheds the remaining 24,
    /// down to 216 — never below its 180 floor. (At 860 the overflow is only
    /// 224, so the member panel alone absorbs it down to 16 and the sidebar is
    /// untouched — the collapse boundary is right around here.)
    #[test]
    fn narrow_viewport_collapses_member_then_shrinks_sidebar() {
        let (s, m) = effective_panel_widths_for(820.0, 240.0, 240.0);
        assert_eq!(m, 0.0, "member panel fully collapsed first");
        assert_eq!(s, 216.0, "sidebar sheds only what the member couldn't absorb");
    }

    /// Below the point where even sidebar=180 fits, the sidebar pins at its
    /// 180px drag-handle floor and the member panel stays fully collapsed —
    /// it is never pushed below 0, and the sidebar never below 180.
    #[test]
    fn very_narrow_viewport_floors_sidebar_and_collapses_member() {
        let (s, m) = effective_panel_widths_for(760.0, 240.0, 240.0);
        assert_eq!(s, 180.0, "sidebar held at the 180px drag-handle floor");
        assert_eq!(m, 0.0, "member panel collapsed to 0, not negative");
    }

    /// Before any backend reports a viewport size (width 0) the helper keeps
    /// the manual widths — no first-paint collapse flash.
    #[test]
    fn zero_viewport_keeps_manual_widths() {
        assert_eq!(effective_panel_widths_for(0.0, 240.0, 240.0), (240.0, 240.0));
    }
}
