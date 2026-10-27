//! One popup at a time, closed by Escape or a click outside.
//!
//! Every popup and dialog keeps its own open/closed signal; this module knows
//! all of them and adds the shared behaviour:
//!
//! - **One at a time.** When a popup opens while another is open, the older
//!   one closes.
//! - **Click outside.** While a dropdown-style popup (the composer's emoji
//!   and GIF pickers, a message's "⋯" menu, the channel menu, the server
//!   dropdown, a member's menu) is open, a transparent full-window layer sits
//!   under it and any press on that layer closes it. Those popups carry
//!   `POPUP_Z` so they paint above the layer. Overlays with their own
//!   backdrop (the reaction picker, the pinned panel, the profile card) close
//!   themselves on an outside press.
//! - **Escape** closes the open popup, or else the open dialog or settings
//!   screen.
//!
//! A message's "⋯" menu is per row, so which row's menu is open lives in one
//! shared signal ([`msg_menu`]) and each row gets a [`MenuHandle`] onto it.

use super::*;

/// z-index for dropdown-style popups, above the click-outside layer.
pub(super) const POPUP_Z: i32 = 60;

thread_local! {
    static MSG_MENU: Cell<Option<Signal<Option<MessageKey>>>> = const { Cell::new(None) };
}

/// The shared "which message's ⋯ menu is open" signal (set up by [`build`]).
pub(super) fn msg_menu() -> Signal<Option<MessageKey>> {
    MSG_MENU.with(|m| m.get()).expect("popups::build runs before any message row")
}

/// One message row's "⋯" menu, opened and closed through [`msg_menu`].
#[derive(Clone, Copy)]
pub(super) struct MenuHandle {
    pub(super) key: MessageKey,
}

impl MenuHandle {
    pub(super) fn get(&self) -> bool {
        msg_menu().get() == Some(self.key)
    }

    pub(super) fn set(&self, open: bool) {
        let menu = msg_menu();
        if open {
            menu.set(Some(self.key));
        } else if menu.get() == Some(self.key) {
            menu.set(None);
        }
    }
}

/// A popup or dialog: whether it is open, and how to close it.
#[derive(Clone)]
struct Entry {
    open: Rc<dyn Fn() -> bool>,
    close: Rc<dyn Fn()>,
}

fn flag(s: Signal<bool>) -> Entry {
    Entry { open: Rc::new(move || s.get()), close: Rc::new(move || s.set(false)) }
}

fn opt<T: Clone + PartialEq + 'static>(s: Signal<Option<T>>) -> Entry {
    Entry { open: Rc::new(move || s.get().is_some()), close: Rc::new(move || s.set(None)) }
}

pub(super) struct PopupsParts {
    /// The click-outside layer; placed in the root after the main content.
    pub(super) catcher_el: Element,
    /// Closes the older popup when another opens (renders nothing).
    pub(super) exclusive_el: Element,
}

pub(super) fn build(s: &ShellState) -> PopupsParts {
    let ShellState {
        bug_reports_mode, cc_open, create_open, dc_open, discover_open, emoji_open, gif_open, invite_mode,
        join_open, live_chan_menu, live_invite_mode, live_member_menu, moderation_mode, msg_menu, notif_mode,
        picker, pinned_panel_open, profile_mode, rc_open, server_menu_open, shortcuts_mode, ss_open,
        user_popup, ..
    } = s.clone();
    MSG_MENU.with(|m| m.set(Some(msg_menu)));

    // Dropdown-style popups: closed by the click-outside layer.
    let dropdowns: Rc<Vec<Entry>> = Rc::new(vec![
        flag(emoji_open),
        flag(gif_open),
        opt(msg_menu),
        opt(live_chan_menu),
        flag(server_menu_open),
        opt(live_member_menu),
    ]);
    // Every popup (one open at a time; Escape closes it first).
    let popups: Rc<Vec<Entry>> = Rc::new(
        dropdowns.iter().cloned().chain([opt(picker), flag(pinned_panel_open), opt(user_popup)]).collect(),
    );
    // Dialogs and full-screen panes Escape closes when no popup is open.
    let dialogs: Rc<Vec<Entry>> = Rc::new(vec![
        flag(cc_open),
        flag(rc_open),
        flag(dc_open),
        flag(ss_open),
        flag(create_open),
        flag(join_open),
        flag(discover_open),
        flag(invite_mode),
        flag(live_invite_mode),
        flag(shortcuts_mode),
        flag(moderation_mode),
        flag(bug_reports_mode),
        flag(profile_mode),
        flag(notif_mode),
    ]);

    // Escape: the open popup, else the open dialog.
    {
        let (popups, dialogs) = (popups.clone(), dialogs.clone());
        runtime_core::on_key(move |e| {
            if e.phase != runtime_core::KeyPhase::Down || e.key != "Escape" {
                return KeyOutcome::Default;
            }
            for set in [&popups, &dialogs] {
                if let Some(entry) = set.iter().find(|e| (e.open)()) {
                    (entry.close)();
                    return KeyOutcome::PreventDefault;
                }
            }
            KeyOutcome::Default
        });
    }

    // One at a time: a popup that just opened closes the others. The close
    // is deferred out of the render pass.
    let exclusive_el = {
        let popups = popups.clone();
        let prev: Rc<RefCell<Vec<bool>>> = Rc::new(RefCell::new(Vec::new()));
        let states = {
            let popups = popups.clone();
            move || popups.iter().map(|e| (e.open)()).collect::<Vec<bool>>()
        };
        switch(states, move |now: &Vec<bool>| {
            let before = prev.replace(now.clone());
            let opened: Vec<usize> = (0..now.len()).filter(|&i| now[i] && !before.get(i).copied().unwrap_or(false)).collect();
            if let Some(&newest) = opened.last() {
                let popups = popups.clone();
                after_ms_detached(0, move || {
                    for (i, e) in popups.iter().enumerate() {
                        if i != newest && (e.open)() {
                            (e.close)();
                        }
                    }
                });
            }
            ui! { view(style = StyleRules { width: Some(px(0.0)), height: Some(px(0.0)), ..Default::default() }) {} }
        })
    };

    let catcher_el = {
        let any_open = {
            let dropdowns = dropdowns.clone();
            move || dropdowns.iter().any(|e| (e.open)())
        };
        switch(any_open, move |open: &bool| {
            if !*open {
                return ui! { view(style = StyleRules { width: Some(px(0.0)), height: Some(px(0.0)), ..Default::default() }) {} };
            }
            let dropdowns = dropdowns.clone();
            ui! {
                view(style = StyleRules {
                    position: Some(Position::Absolute),
                    left: Some(px(0.0)),
                    top: Some(px(0.0)),
                    width: Some(pct(100.0)),
                    height: Some(pct(100.0)),
                    z_index: Some(POPUP_Z - 1),
                    ..Default::default()
                }) {}
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Began {
                        for e in dropdowns.iter() {
                            if (e.open)() {
                                (e.close)();
                            }
                        }
                    }
                    TouchResponse::CONSUMED
                })
            }
        })
    };

    PopupsParts { catcher_el, exclusive_el }
}
