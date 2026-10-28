//! Jumping to a message (from search results): open its channel, page older
//! history until the message is loaded, then scroll to it and highlight it
//! for a moment. The row of the target message binds [`row_ref`] and draws
//! the highlight while [`target`] names it.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use runtime_core::{after_ms_detached, Ref, ScrollViewHandle, Signal, ViewHandle};

use super::message_view::LiveMsg;

thread_local! {
    static TARGET: Cell<Option<i64>> = const { Cell::new(None) };
    static ROW: Ref<ViewHandle> = Ref::<ViewHandle>::new();
    static LIST: RefCell<Option<(Ref<ScrollViewHandle>, Ref<ViewHandle>)>> = const { RefCell::new(None) };
}

/// Register the message list's scroll view and its content column.
pub(super) fn install(scroll: Ref<ScrollViewHandle>, content: Ref<ViewHandle>) {
    LIST.with(|l| *l.borrow_mut() = Some((scroll, content)));
}

/// The message being jumped to, if any.
pub(super) fn target() -> Option<i64> {
    TARGET.with(Cell::get)
}

/// The ref the target message's row binds.
pub(super) fn row_ref() -> Ref<ViewHandle> {
    ROW.with(|r| *r)
}

/// Scroll so the target row sits near the top. `false` while the row isn't
/// laid out yet.
fn scroll_to_target() -> bool {
    let row = ROW.with(|r| r.with(|h| h.absolute_frame()).flatten());
    LIST.with(|l| {
        let Some((scroll, content)) = *l.borrow() else {
            return false;
        };
        match (row, content.with(|h| h.absolute_frame()).flatten()) {
            (Some(row), Some(top)) => {
                scroll.with(|h| h.scroll_to(0.0, (row.y - top.y - 80.0).max(0.0)));
                true
            }
            _ => false,
        }
    })
}

/// What a jump needs from the app shell.
pub(super) struct Jump {
    pub(super) channel_id: i64,
    pub(super) message_id: i64,
    pub(super) live_messages: Rc<RefCell<HashMap<i64, Vec<LiveMsg>>>>,
    pub(super) has_more_earlier: Signal<bool>,
    pub(super) load_busy: Signal<bool>,
    pub(super) live_revision: Signal<u32>,
    /// Asks for the page of history older than the given message id.
    pub(super) load_older: Rc<dyn Fn(i64)>,
}

/// Start jumping; the caller has already opened the channel.
pub(super) fn start(jump: Jump) {
    TARGET.with(|t| t.set(Some(jump.message_id)));
    step(Rc::new(jump), 0);
}

/// One poll: done when the message is loaded, else ask for older history
/// (one page at a time) and look again shortly. Gives up after ~15 s.
fn step(jump: Rc<Jump>, attempt: u32) {
    if target() != Some(jump.message_id) {
        return;
    }
    if attempt >= 75 {
        TARGET.with(|t| t.set(None));
        return;
    }
    let (loaded, oldest) = {
        let map = jump.live_messages.borrow();
        let msgs = map.get(&jump.channel_id);
        (
            msgs.is_some_and(|v| v.iter().any(|m| m.0 == jump.message_id)),
            msgs.and_then(|v| v.first().map(|m| m.0)),
        )
    };
    if loaded {
        // Redraw so the row picks up the highlight and the ref.
        jump.live_revision.set(jump.live_revision.get() + 1);
        scroll_when_drawn(jump, 0);
        return;
    }
    // The first look leaves time for the channel's latest page to arrive.
    if attempt > 0 && !jump.load_busy.get() && jump.has_more_earlier.get() {
        if let Some(oldest) = oldest {
            jump.load_busy.set(true);
            (jump.load_older)(oldest);
        }
    }
    after_ms_detached(200, move || step(jump, attempt + 1));
}

fn scroll_when_drawn(jump: Rc<Jump>, attempt: u32) {
    if scroll_to_target() {
        // Fade the highlight after a moment.
        after_ms_detached(2_500, move || {
            if target() == Some(jump.message_id) {
                TARGET.with(|t| t.set(None));
                jump.live_revision.set(jump.live_revision.get() + 1);
            }
        });
    } else if attempt < 20 {
        after_ms_detached(100, move || scroll_when_drawn(jump, attempt + 1));
    }
}
