//! `pinned_messages_panel` — the Discord clone's "Pinned messages" panel.
//!
//! A full-screen, paint-last overlay (the same idiom as the Server Moderation
//! and Bug reports overlays: `Position::Absolute` at `0/0`, sized to the
//! viewport while `open` is true and collapsed to zero while false) listing a
//! channel's REAL pinned messages with a per-row Unpin button.
//!
//! All data is caller-owned signals so the header pin badge and the list stay
//! in sync with the backend:
//! - the caller (the live channel header's pin icon) sets `open` true and
//!   spawns `crate::pins::list_pinned(channel_id)` to fill `messages`;
//! - each Unpin button calls `crate::pins::unpin_message(message_id)`, then on
//!   success writes the backend's fresh `pinned_count` into `count` (the header
//!   badge updates from it with no second round trip), removes the row from
//!   `messages`, and bumps `revision` so the list rebuilds;
//! - a rejected Unpin (e.g. a plain member who lacks "Manage Messages") is
//!   surfaced in `error` and `revision` is bumped to re-render it.
//!
//! The list rebuilds against `(open, revision)` via `switch` — the same
//! rebuild-trigger idiom `live_dm_revision` / `live_revision` use — so a
//! `Signal<Vec<..>>` (whose value is not a stable `switch` key) still re-renders
//! on every mutation.
//!
//! Idioms, each verified against `components/pin_confirm_toast.rs` / `app.rs`
//! (the `moderation_region` / `bug_reports_region` overlays):
//! - show/hide is a reactive branch driven by the `switch` key (mounts while
//!   open, unmounts the moment `open` is cleared);
//! - every clickable node is a `view(...)` builder with `.on_touch` chained on
//!   the builder inside `ui!` (`Element` has no `on_touch`), guarded by
//!   `TouchPhase::Ended` — the app's standard click pattern;
//! - the row list is a `Vec<Element>` built by a data-driven map with a
//!   per-row Unpin closure (the same shape `app.rs`'s `live_edit_row` loop uses
//!   for its per-row edit/delete closures) — a `ui!` `for` cannot attach a
//!   distinct captured closure per row.

use crate::theme;
use icons_lucide::{PIN, PIN_OFF};
use runtime_core::icon;
use crate::pins::{PinnedMessage, unpin_message};
use crate::style_helpers::{col, px, pct, rounded};
use runtime_core::{
    switch, ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent, Overflow,
    Position, Signal, StyleRules, TouchPhase, TouchResponse,
};

// Discord dark palette — kept local so the component is self-contained.

/// The "Pinned messages" full-screen panel. Renders nothing (zero size) while
/// `open` is false; while true it shows a header (title + close) and the
/// channel's real pinned rows, each with its own Unpin button.
pub fn pinned_messages_panel(
    open: Signal<bool>,
    messages: Signal<Vec<PinnedMessage>>,
    error: Signal<String>,
    count: Signal<i64>,
    revision: Signal<u32>,
) -> Element {
    switch(
        move || (open.get(), revision.get()),
        move |(o, _r): &(bool, u32)| {
            if !*o {
                return ui! {
                    view(style = StyleRules {
                        width: Some(px(0.0)),
                        height: Some(px(0.0)),
                        ..Default::default()
                    }) {}
                };
            }
            let msgs = messages.get();
            let err = error.get();
            let count_val = count.get();
            let open_c = open;
            let count_c = count;
            let rev_c = revision;
            let err_c = error;
            let msgs_c = messages;

            // Header: title + close ("x") that clears `open`.
            let header = ui! {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::SpaceBetween),
                    padding_top: Some(px(16.0)),
                    padding_bottom: Some(px(16.0)),
                    padding_left: Some(px(20.0)),
                    padding_right: Some(px(20.0)),
                    border_bottom_width: Some(runtime_core::Tokenized::Literal(1.0)),
                    border_bottom_color: Some(col(theme::BG_TERTIARY)),
                    ..Default::default()
                }) {
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        gap: Some(px(8.0)),
                        ..Default::default()
                    }) {
                        icon(PIN).color(|| theme::live(theme::DANGER)).size(18.0)
                        text(style = StyleRules {
                            font_size: Some(px(18.0)),
                            font_weight: Some(FontWeight::Bold),
                            color: Some(col(theme::TEXT_STRONG)),
                            ..Default::default()
                        }) { "Pinned messages".to_string() }
                        if count_val > 0 {
                            text(style = StyleRules {
                                font_size: Some(px(13.0)),
                                color: Some(col(theme::TEXT_MUTED)),
                                ..Default::default()
                            }) { format!("({count_val})") }
                        }
                    }
                    view(style = StyleRules {
                        width: Some(px(28.0)),
                        height: Some(px(28.0)),
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        border_top_width: Some(runtime_core::Tokenized::Literal(1.0)),
                        border_right_width: Some(runtime_core::Tokenized::Literal(1.0)),
                        border_bottom_width: Some(runtime_core::Tokenized::Literal(1.0)),
                        border_left_width: Some(runtime_core::Tokenized::Literal(1.0)),
                        border_top_color: Some(col(theme::TEXT_MUTED)),
                        border_right_color: Some(col(theme::TEXT_MUTED)),
                        border_bottom_color: Some(col(theme::TEXT_MUTED)),
                        border_left_color: Some(col(theme::TEXT_MUTED)),
                        ..rounded(6.0)
                    }) {
                        text(style = StyleRules {
                            font_size: Some(px(14.0)),
                            color: Some(col(theme::TEXT_MUTED)),
                            ..Default::default()
                        }) { "x".to_string() }
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            open_c.set(false);
                        }
                        TouchResponse::CONSUMED
                    })
                }
            };

            // Body: a scroll column of rows. Data-driven `Vec<Element>` with a
            // per-row Unpin closure (the `live_edit_row` loop idiom — a `ui!`
            // `for` cannot attach a distinct captured closure per row).
            let body = build_body(msgs, err, count_c, rev_c, err_c, msgs_c);

            ui! {
                view(style = StyleRules {
                    position: Some(Position::Absolute),
                    left: Some(px(0.0)),
                    top: Some(px(0.0)),
                    width: Some(pct(100.0)),
                    height: Some(pct(100.0)),
                    background: Some(col(theme::BG_PRIMARY)),
                    flex_direction: Some(FlexDirection::Column),
                    overflow: Some(Overflow::Hidden),
                    ..Default::default()
                }) {
                    { header }
                    view(style = StyleRules {
                        flex_grow: Some(runtime_core::Tokenized::Literal(1.0)),
                        overflow: Some(Overflow::Hidden),
                        ..Default::default()
                    }) {
                        scroll_view(style = StyleRules {
                            flex_grow: Some(runtime_core::Tokenized::Literal(1.0)),
                            ..Default::default()
                        }) { { body } }
                    }
                }
            }
        },
    )
}

/// The panel's scroll body: an empty state, an error state, or one row per
/// pinned message with its Unpin button.
fn build_body(
    msgs: Vec<PinnedMessage>,
    err: String,
    count: Signal<i64>,
    revision: Signal<u32>,
    error: Signal<String>,
    messages: Signal<Vec<PinnedMessage>>,
) -> Element {
    if !err.is_empty() {
        return ui! {
            view(style = StyleRules {
                padding_top: Some(px(20.0)),
                padding_left: Some(px(20.0)),
                padding_right: Some(px(20.0)),
                ..Default::default()
            }) {
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    color: Some(col(theme::DANGER)),
                    ..Default::default()
                }) { err.clone() }
            }
        };
    }
    if msgs.is_empty() {
        return ui! {
            view(style = StyleRules {
                padding_top: Some(px(20.0)),
                padding_left: Some(px(20.0)),
                padding_right: Some(px(20.0)),
                ..Default::default()
            }) {
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    color: Some(col(theme::TEXT_MUTED)),
                    ..Default::default()
                }) { "No pinned messages yet.".to_string() }
            }
        };
    }
    let rows: Vec<Element> = msgs
        .iter()
        .map(|m| pin_row(m, count, revision, error, messages))
        .collect();
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            gap: Some(px(8.0)),
            padding_top: Some(px(16.0)),
            padding_bottom: Some(px(16.0)),
            padding_left: Some(px(20.0)),
            padding_right: Some(px(20.0)),
            ..Default::default()
        }) { { rows } }
    }
}

/// One pinned-message row: author + body, with an Unpin button on the right.
/// The Unpin button calls `unpin_message`, then writes the fresh `pinned_count`
/// into `count`, removes the row from the list, and bumps `revision` to rebuild.
fn pin_row(
    m: &PinnedMessage,
    count: Signal<i64>,
    revision: Signal<u32>,
    error: Signal<String>,
    messages: Signal<Vec<PinnedMessage>>,
) -> Element {
    let mid = m.message_id;
    let count_c = count;
    let rev_c = revision;
    let err_c = error;
    let msgs_c = messages;
    let on_unpin = move || {
        let mid2 = mid;
        let cc = count_c;
        let rc = rev_c;
        let ec = err_c;
        let mc = msgs_c;
        runtime_core::driver::spawn_async(async move {
            match unpin_message(mid2).await {
                Ok(res) => {
                    cc.set(res.pinned_count);
                    // Remove the just-unpinned row from the list (the
                    // `switch` key reads `revision`, so the bump rebuilds the
                    // body without the row) and bump to trigger that rebuild.
                    mc.set(mc.get().into_iter().filter(|x| x.message_id != mid2).collect());
                    rc.set(rc.get() + 1);
                }
                Err(e) => {
                    ec.set(e);
                    rc.set(rc.get() + 1);
                }
            }
        });
    };

    let author = m.author_username.clone();
    // A GIF body shows as "GIF" in a pinned row, not its media URL.
    let content = crate::reply_reference_strip::excerpt_text(m.content.as_str());
    let _edited = m.edited_at.is_some();
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::FlexStart),
            justify_content: Some(JustifyContent::SpaceBetween),
            padding_top: Some(px(10.0)),
            padding_bottom: Some(px(10.0)),
            padding_left: Some(px(14.0)),
            padding_right: Some(px(14.0)),
            background: Some(col(theme::BG_SECONDARY)),
            ..rounded(8.0)
        }) {
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Column),
                gap: Some(px(4.0)),
                flex_grow: Some(runtime_core::Tokenized::Literal(1.0)),
                margin_right: Some(px(12.0)),
                ..Default::default()
            }) {
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    color: Some(col(theme::TEXT_STRONG)),
                    ..Default::default()
                }) { crate::auth::shown_name(&author) }
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    color: Some(col(theme::TEXT_MUTED)),
                    ..Default::default()
                }) { content }
            }
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(6.0)),
                padding_top: Some(px(6.0)),
                padding_bottom: Some(px(6.0)),
                padding_left: Some(px(10.0)),
                padding_right: Some(px(10.0)),
                background: Some(col(theme::BG_SECONDARY)),
                border_top_width: Some(runtime_core::Tokenized::Literal(1.0)),
                border_right_width: Some(runtime_core::Tokenized::Literal(1.0)),
                border_bottom_width: Some(runtime_core::Tokenized::Literal(1.0)),
                border_left_width: Some(runtime_core::Tokenized::Literal(1.0)),
                border_top_color: Some(col(theme::DANGER)),
                border_right_color: Some(col(theme::DANGER)),
                border_bottom_color: Some(col(theme::DANGER)),
                border_left_color: Some(col(theme::DANGER)),
                ..rounded(8.0)
            }) {
                icon(PIN_OFF).color(|| theme::live(theme::DANGER)).size(14.0)
                text(style = StyleRules {
                    font_size: Some(px(12.0)),
                    color: Some(col(theme::DANGER)),
                    ..Default::default()
                }) { "Unpin".to_string() }
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    on_unpin();
                }
                TouchResponse::CONSUMED
            })
        }
    }
}
