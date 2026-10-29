//! Phone layout. Below [`PHONE_MAX`] the shell shows one pane at a time,
//! like Discord's mobile app: the server rail with the channel list (or the
//! DM list), or the open conversation. Opening a channel or a DM shows the
//! chat; the menu button in its header goes back to the list. The member
//! list becomes a slide-over from the right.

use std::cell::Cell;

use runtime_core::Signal;

use super::*;
use crate::theme;

/// Windows narrower than this (CSS px) get the one-pane phone layout.
pub(crate) const PHONE_MAX: f32 = 700.0;

/// Which pane a phone shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Pane {
    /// The server rail and the channel list (or the DM list).
    #[default]
    List,
    /// The open channel or DM.
    Chat,
}

thread_local! {
    static PANE: Cell<Option<Signal<Pane>>> = const { Cell::new(None) };
    /// Settings screens on a phone: true while the section list shows.
    static SETTINGS_NAV: Cell<Option<Signal<bool>>> = const { Cell::new(None) };
}

/// Install the shell's pane signals (done once by `app_shell`).
pub(super) fn install(pane: Signal<Pane>, settings_nav: Signal<bool>) {
    PANE.with(|p| p.set(Some(pane)));
    SETTINGS_NAV.with(|p| p.set(Some(settings_nav)));
}

/// True on a phone-width window. Reads `viewport_size()`, so a reactive
/// style closure that calls this re-runs on resize.
pub(crate) fn is_phone() -> bool {
    is_phone_width(viewport_size().get().width)
}

/// Pure form of [`is_phone`]; `0` (no size reported yet) is not a phone.
pub(crate) fn is_phone_width(width: f32) -> bool {
    width > 0.0 && width < PHONE_MAX
}

fn pane() -> Pane {
    PANE.with(|p| p.get()).map(|s| s.get()).unwrap_or_default()
}

/// Switch the phone pane (a no-op on wider windows, where both show).
pub(crate) fn show(p: Pane) {
    if let Some(s) = PANE.with(|c| c.get()) {
        s.set(p);
    }
}

/// True while the given pane is hidden: only on a phone, and only when the
/// other pane is showing. Reactive in both the viewport and the pane.
pub(crate) fn hidden(p: Pane) -> bool {
    is_phone() && pane() != p
}

/// On a phone, a settings screen shows its section list (`true`) or the
/// open section (`false`). Opening a section and the back button switch.
pub(crate) fn show_settings_nav(on: bool) {
    if let Some(s) = SETTINGS_NAV.with(|c| c.get()) {
        s.set(on);
    }
}

/// True while the settings section list (`nav`) or the open section
/// (`!nav`) is hidden: only on a phone, where one shows at a time.
pub(crate) fn settings_hidden(nav: bool) -> bool {
    is_phone() && SETTINGS_NAV.with(|c| c.get()).map(|s| s.get()).unwrap_or(true) != nav
}

/// The widest a popup or dialog may be: `desktop`, or the window less a
/// 12px margin each side on a narrower one. Read untracked, at build time.
pub(crate) fn fit_width(desktop: f32) -> f32 {
    let width = viewport_size().peek().width;
    if width > 0.0 { desktop.min(width - 24.0) } else { desktop }
}

/// The widest an inline image may be in a message row: `desktop`, or what
/// fits beside the avatar on a phone. Read untracked: rows are sized when
/// they are built, so resizing the window doesn't rebuild the message list.
pub(crate) fn media_max_width(desktop: f32) -> f32 {
    let width = viewport_size().peek().width;
    if is_phone_width(width) { desktop.min(width - 90.0) } else { desktop }
}

/// A header's "back" button, running `on_back`; renders nothing off a phone.
pub(crate) fn back_button(id: &'static str, on_back: fn()) -> Element {
    switch(is_phone, move |phone: &bool| {
        if !*phone {
            return ui! { view(style = StyleRules::default()) {} };
        }
        ui! {
            view(style = StyleRules {
                width: Some(px(32.0)),
                height: Some(px(32.0)),
                margin_right: Some(px(4.0)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                flex_shrink: Some(grow(0.0)),
                ..rounded(6.0)
            }, test_id = id) {
                icon(MENU).color(|| theme::live(TEXT_MUTED)).size(20.0)
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    on_back();
                }
                TouchResponse::CONSUMED
            })
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phone_width_threshold() {
        assert!(is_phone_width(PHONE_MAX - 1.0));
        assert!(!is_phone_width(0.0), "no size reported yet");
        assert!(is_phone_width(390.0));
        assert!(is_phone_width(699.0));
        assert!(!is_phone_width(700.0));
        assert!(!is_phone_width(1280.0));
    }
}
