//! `app()` — the Discord-style layout: a server rail, a channel sidebar, and a
//! main message view with a composer. State is a few signals (active server,
//! active channel, composer text) plus a per-session log of sent messages; the
//! tree is declarative around them.
//!
//! Idioms, each verified against a working example in `examples/`:
//! - styles are plain `StyleRules` (via `crate::style_helpers` `px` / `pct` /
//!   `col`) — one pass of static CSS, no live style engine;
//! - active-state highlight is a *reactive* style closure
//!   (`move || StyleRules { … }`), re-run on every signal change — the
//!   `IntoStyleProp for Fn()->StyleRules` dynamic path (`style_attach.rs`);
//! - clickable rows are the `view` builder's `.on_touch(|ev| …)` chain (the
//!   `ui!` macro's `view` tag is `style` + children only) — `whiteboard-demo`;
//! - the channel list, header, and message list are `switch(key, render)`
//!   keyed on `(server_id, channel_id)` so they rebuild when the selection
//!   changes — the same pattern `whiteboard-demo` uses to rebuild its canvas
//!   list (and `switch`'s render is `Fn`, so captured `StyleRules` are cloned
//!   inside the closure, exactly as whiteboard does);
//! - a precomputed `Element` / `Vec<Element>` splices in as a bare child —
//!   `whiteboard-demo` `scroll_view { rows }`.

use std::cell::RefCell;
use std::rc::Rc;

use runtime_core::{
    AlignItems, Element, FlexDirection, FontWeight, JustifyContent, KeyEvent, KeyOutcome, Overflow,
    Position, Signal, StyleRules, TextTransform, Tokenized, TouchPhase, TouchResponse, signal,
    switch, ui,
};

use crate::data::{SERVERS, Member, Status};
use crate::style_helpers::{col, pct, px};

// Discord dark palette (Blurple-flavoured).
const BG_RAIL: &str = "#1e1f22";
const BG_SIDEBAR: &str = "#2b2d31";
const BG_MAIN: &str = "#313338";
const BG_ACTIVE: &str = "#404249";
const TEXT: &str = "#f2f3f5";
const TEXT_MUTED: &str = "#949ba4";
const TEXT_CHANNEL: &str = "#c8cacf";
const BORDER: &str = "#26272b";
const COMPOSER: &str = "#383a40";
const BUTTON: &str = "#5865f2";

fn grow(v: f32) -> Tokenized<f32> {
    Tokenized::Literal(v)
}

fn border_width(px: f32) -> Tokenized<f32> {
    Tokenized::Literal(px)
}

/// A row's background: `active ? active_col : base_col` — the highlight.
fn active_bg(active: bool, active_col: &str, base_col: &str) -> Tokenized<runtime_core::Color> {
    col(if active { active_col } else { base_col })
}

/// One message row: avatar chip, author, body. Takes `&str`s so it serves both
/// the static seed data and runtime-authored messages (the strings are copied
/// into the node, so the returned `Element` is self-contained).
fn message_row(author: &str, color: &str, body: &str) -> Element {
    let row_style = StyleRules {
        padding_top: Some(px(6.0)),
        padding_bottom: Some(px(6.0)),
        padding_left: Some(px(16.0)),
        padding_right: Some(px(16.0)),
        gap: Some(px(10.0)),
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        ..Default::default()
    };
    let avatar_style = StyleRules {
        width: Some(px(28.0)),
        height: Some(px(28.0)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        background: Some(col(color)),
        border_top_left_radius: Some(px(14.0)),
        border_top_right_radius: Some(px(14.0)),
        border_bottom_left_radius: Some(px(14.0)),
        border_bottom_right_radius: Some(px(14.0)),
        ..Default::default()
    };
    let avatar_text_style = StyleRules {
        font_size: Some(px(13.0)),
        font_weight: Some(FontWeight::Bold),
        color: Some(col(TEXT)),
        ..Default::default()
    };
    let author_style = StyleRules {
        font_size: Some(px(15.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT)),
        ..Default::default()
    };
    let body_style = StyleRules {
        font_size: Some(px(15.0)),
        color: Some(col(TEXT_MUTED)),
        flex_grow: Some(grow(1.0)),
        ..Default::default()
    };
    let initial = author.chars().next().unwrap_or(' ').to_string();
    ui! {
        view(style = row_style) {
            view(style = avatar_style) {
                text(style = avatar_text_style) { initial }
            }
            text(style = author_style) { author.to_string() }
            text(style = body_style) { body.to_string() }
        }
    }
}

/// One member row: avatar chip with the status dot overlaid on its
/// bottom-right corner, then the name. Offline rows render dimmed,
/// Discord-style. Pure data in, self-contained `Element` out (like
/// `message_row`) so `member_panel` can splice them into a column.
fn member_row(m: &Member) -> Element {
    let dim = m.status == Status::Offline;
    let row_style = StyleRules {
        padding_top: Some(px(4.0)),
        padding_bottom: Some(px(4.0)),
        padding_left: Some(px(10.0)),
        padding_right: Some(px(12.0)),
        gap: Some(px(10.0)),
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        ..Default::default()
    };
    let avatar_style = StyleRules {
        width: Some(px(32.0)),
        height: Some(px(32.0)),
        position: Some(Position::Relative),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        background: Some(col(m.color)),
        border_top_left_radius: Some(px(16.0)),
        border_top_right_radius: Some(px(16.0)),
        border_bottom_left_radius: Some(px(16.0)),
        border_bottom_right_radius: Some(px(16.0)),
        ..Default::default()
    };
    let initial_style = StyleRules {
        font_size: Some(px(14.0)),
        font_weight: Some(FontWeight::Bold),
        color: Some(col(TEXT)),
        ..Default::default()
    };
    // The status dot: absolutely positioned at the avatar's bottom-right,
    // with a 2px ring matching the sidebar bg so it reads as "attached".
    let dot_style = StyleRules {
        position: Some(Position::Absolute),
        right: Some(px(0.0)),
        bottom: Some(px(0.0)),
        width: Some(px(11.0)),
        height: Some(px(11.0)),
        background: Some(col(m.status.color())),
        border_top_width: Some(border_width(2.0)),
        border_right_width: Some(border_width(2.0)),
        border_bottom_width: Some(border_width(2.0)),
        border_left_width: Some(border_width(2.0)),
        border_top_color: Some(col(BG_SIDEBAR)),
        border_right_color: Some(col(BG_SIDEBAR)),
        border_bottom_color: Some(col(BG_SIDEBAR)),
        border_left_color: Some(col(BG_SIDEBAR)),
        border_top_left_radius: Some(px(6.0)),
        border_top_right_radius: Some(px(6.0)),
        border_bottom_left_radius: Some(px(6.0)),
        border_bottom_right_radius: Some(px(6.0)),
        ..Default::default()
    };
    let name_style = StyleRules {
        font_size: Some(px(15.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(if dim { TEXT_MUTED } else { TEXT })),
        ..Default::default()
    };
    let initial = m.name.chars().next().unwrap_or(' ').to_string();
    ui! {
        view(style = row_style) {
            view(style = avatar_style) {
                text(style = initial_style) { initial }
                view(style = dot_style) {}
            }
            text(style = name_style) { m.name.to_string() }
        }
    }
}

/// The member-list panel for the right side of the window: a
/// "MEMBER COUNT — N" header (Discord's style), then members grouped by
/// status (online → idle → DND → offline). Pure data in, self-contained
/// `Element` out — spliced in as the right-most column for whichever server
/// is active (rebuilds when the active server changes).
fn member_panel(server: &crate::data::Server) -> Element {
    let mut body: Vec<Element> = Vec::new();
    // Header: "MEMBER COUNT — N" where N is the total members on this server.
    let header = format!("MEMBER COUNT — {}", server.members.len());
    let header_style = StyleRules {
        padding_top: Some(px(14.0)),
        padding_bottom: Some(px(6.0)),
        padding_left: Some(px(12.0)),
        font_size: Some(px(11.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT_MUTED)),
        text_transform: Some(TextTransform::Uppercase),
        ..Default::default()
    };
    body.push(ui! { text(style = header_style) { header } });

    // Status groups in Discord order: Online, Idle, DND, Offline.
    for status in [Status::Online, Status::Idle, Status::DoNotDisturb, Status::Offline] {
        let group: Vec<&Member> = server.members.iter().filter(|m| m.status == status).collect();
        if group.is_empty() {
            continue;
        }
        let group_label = format!("{} — {}", status.label(), group.len());
        let group_style = StyleRules {
            padding_top: Some(px(10.0)),
            padding_bottom: Some(px(2.0)),
            padding_left: Some(px(12.0)),
            font_size: Some(px(11.0)),
            font_weight: Some(FontWeight::SemiBold),
            color: Some(col(TEXT_MUTED)),
            text_transform: Some(TextTransform::Uppercase),
            ..Default::default()
        };
        body.push(ui! { text(style = group_style) { group_label } });
        for m in group.iter() {
            body.push(member_row(m));
        }
    }

    // Section wrapper: fixed-width right-side panel (Discord's member list
    // lives on the right). The left border separates it from the main view;
    // the panel owns its own bg.
    ui! {
        view(style = StyleRules {
            width: Some(px(240.0)),
            background: Some(col(BG_SIDEBAR)),
            border_left_width: Some(border_width(1.0)),
            border_left_color: Some(col(BORDER)),
            flex_direction: Some(FlexDirection::Column),
            overflow: Some(Overflow::Hidden),
            ..Default::default()
        }) {
            scroll_view(style = StyleRules {
                flex_grow: Some(grow(1.0)),
                ..Default::default()
            }) { body }
        }
    }
}

/// The bottom-left user profile section: avatar (with status dot), name, and
/// a muted "Online" status line. Static (same user across all servers) —
/// rendered once per sidebar rebuild.
fn user_panel() -> Element {
    let row_style = StyleRules {
        padding_top: Some(px(10.0)),
        padding_bottom: Some(px(10.0)),
        padding_left: Some(px(10.0)),
        padding_right: Some(px(10.0)),
        gap: Some(px(10.0)),
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        border_top_width: Some(border_width(1.0)),
        border_top_color: Some(col(BORDER)),
        background: Some(col(BG_SIDEBAR)),
        ..Default::default()
    };
    let avatar_style = StyleRules {
        width: Some(px(32.0)),
        height: Some(px(32.0)),
        position: Some(Position::Relative),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        background: Some(col(BUTTON)),
        border_top_left_radius: Some(px(16.0)),
        border_top_right_radius: Some(px(16.0)),
        border_bottom_left_radius: Some(px(16.0)),
        border_bottom_right_radius: Some(px(16.0)),
        ..Default::default()
    };
    let initial_style = StyleRules {
        font_size: Some(px(14.0)),
        font_weight: Some(FontWeight::Bold),
        color: Some(col(TEXT)),
        ..Default::default()
    };
    let dot_style = StyleRules {
        position: Some(Position::Absolute),
        right: Some(px(0.0)),
        bottom: Some(px(0.0)),
        width: Some(px(11.0)),
        height: Some(px(11.0)),
        background: Some(col(Status::Online.color())),
        border_top_width: Some(border_width(2.0)),
        border_right_width: Some(border_width(2.0)),
        border_bottom_width: Some(border_width(2.0)),
        border_left_width: Some(border_width(2.0)),
        border_top_color: Some(col(BG_SIDEBAR)),
        border_right_color: Some(col(BG_SIDEBAR)),
        border_bottom_color: Some(col(BG_SIDEBAR)),
        border_left_color: Some(col(BG_SIDEBAR)),
        border_top_left_radius: Some(px(6.0)),
        border_top_right_radius: Some(px(6.0)),
        border_bottom_left_radius: Some(px(6.0)),
        border_bottom_right_radius: Some(px(6.0)),
        ..Default::default()
    };
    let text_stack = StyleRules {
        flex_direction: Some(FlexDirection::Column),
        flex_grow: Some(grow(1.0)),
        ..Default::default()
    };
    let name_style = StyleRules {
        font_size: Some(px(14.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT)),
        ..Default::default()
    };
    let status_style = StyleRules {
        font_size: Some(px(12.0)),
        color: Some(col(TEXT_MUTED)),
        ..Default::default()
    };
    ui! {
        view(style = row_style) {
            view(style = avatar_style) {
                text(style = initial_style) { "Y".to_string() }
                view(style = dot_style) {}
            }
            view(style = text_stack) {
                text(style = name_style) { "you".to_string() }
                text(style = status_style) { "Online".to_string() }
            }
        }
    }
}

pub fn app() -> Element {
    let active_server: Signal<u32> = signal(SERVERS[0].id);
    let active_channel: Signal<u32> = signal(SERVERS[0].channels[0].id);
    let composer: Signal<String> = signal(String::new());
    // (channel_id, author, body) of every message the user has sent this
    // session, so each channel keeps its own additions. Read at rebuild.
    let sent: Rc<RefCell<Vec<(u32, &'static str, String)>>> = Rc::new(RefCell::new(Vec::new()));
    // Bumped on every send. `switch` dedups on its scrutinee VALUE (equal
    // re-fire keeps the mounted arm), so re-setting `active_channel` to its
    // current value does NOT rebuild the message list — this counter does.
    let revision: Signal<u32> = signal(0);

    // --- server rail -----------------------------------------------------
    let rail_style = StyleRules {
        width: Some(px(72.0)),
        background: Some(col(BG_RAIL)),
        flex_direction: Some(FlexDirection::Column),
        align_items: Some(AlignItems::Center),
        gap: Some(px(10.0)),
        padding_top: Some(px(12.0)),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };

    let server_rows: Vec<Element> = SERVERS.iter().map(|s| {
        let active = active_server.clone();
        let channel = active_channel.clone();
        // Reactive style: re-runs whenever `active_server` changes.
        let row_style = move || StyleRules {
            width: Some(px(48.0)),
            height: Some(px(48.0)),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::Center),
            background: Some(active_bg(active.get() == s.id, TEXT_CHANNEL, BG_SIDEBAR)),
            border_top_left_radius: Some(px(24.0)),
            border_top_right_radius: Some(px(24.0)),
            border_bottom_left_radius: Some(px(24.0)),
            border_bottom_right_radius: Some(px(24.0)),
            ..Default::default()
        };
        let label_style = move || StyleRules {
            font_size: Some(px(18.0)),
            font_weight: Some(FontWeight::Bold),
            color: Some(active_bg(active.get() == s.id, TEXT, TEXT_CHANNEL)),
            ..Default::default()
        };
        ui! {
            view(style = row_style) {
                text(style = label_style) { s.initial }
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    active.set(s.id);
                    if let Some(srv) = SERVERS.iter().find(|x| x.id == s.id) {
                        channel.set(srv.channels[0].id);
                    }
                }
                TouchResponse::CONSUMED
            })
        }
    }).collect();

    // --- channel sidebar -------------------------------------------------
    let sidebar_style = StyleRules {
        width: Some(px(244.0)),
        background: Some(col(BG_SIDEBAR)),
        border_right_width: Some(border_width(1.0)),
        border_right_color: Some(col(BORDER)),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    let sidebar_header_style = StyleRules {
        padding_top: Some(px(16.0)),
        padding_bottom: Some(px(12.0)),
        padding_left: Some(px(16.0)),
        padding_right: Some(px(16.0)),
        border_bottom_width: Some(border_width(1.0)),
        border_bottom_color: Some(col(BORDER)),
        ..Default::default()
    };
    let sidebar_title_style = StyleRules {
        font_size: Some(px(15.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT)),
        text_transform: Some(TextTransform::Uppercase),
        ..Default::default()
    };

    let sidebar = switch(
        move || (active_server.get(), active_channel.get()),
        move |(server_id, _channel_id): &(u32, u32)| {
            // `switch`'s render is `Fn` — clone the captured styles so the
            // closure borrows them rather than moving them out (whiteboard idiom).
            let sidebar_style = sidebar_style.clone();
            let sidebar_header_style = sidebar_header_style.clone();
            let sidebar_title_style = sidebar_title_style.clone();
            let channel = active_channel;
            let server = SERVERS.iter().find(|x| x.id == *server_id).unwrap();
            let rows: Vec<Element> = server.channels.iter().map(|c| {
                let active = channel;
                let row_style = move || StyleRules {
                    padding_top: Some(px(7.0)),
                    padding_bottom: Some(px(7.0)),
                    padding_left: Some(px(16.0)),
                    padding_right: Some(px(16.0)),
                    background: Some(active_bg(active.get() == c.id, "transparent", BG_ACTIVE)),
                    ..Default::default()
                };
                let label_style = move || StyleRules {
                    font_size: Some(px(15.0)),
                    font_weight: Some(if active.get() == c.id {
                        FontWeight::SemiBold
                    } else {
                        FontWeight::Normal
                    }),
                    color: Some(active_bg(active.get() == c.id, TEXT, TEXT_CHANNEL)),
                    ..Default::default()
                };
                let label = format!("#{}", c.name);
                ui! {
                    view(style = row_style) {
                        text(style = label_style) { label }
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            active.set(c.id);
                        }
                        TouchResponse::CONSUMED
                    })
                }
            }).collect();
            let title = server.name.to_string();
            // The user profile is the bottom-left of the sidebar (Discord
            // convention). The member list is a separate right-side panel —
            // see `members_switch` further down — so it's not built here.
            let user = user_panel();
            ui! {
                view(style = sidebar_style) {
                    view(style = sidebar_header_style) {
                        text(style = sidebar_title_style) { title }
                    }
                    view(style = StyleRules {
                        flex_grow: Some(grow(1.0)),
                        overflow: Some(Overflow::Hidden),
                        ..Default::default()
                    }) {
                        scroll_view(style = StyleRules {
                            flex_grow: Some(grow(1.0)),
                            ..Default::default()
                        }) { rows }
                    }
                    user
                }
            }
        },
    );

    // --- main view -------------------------------------------------------
    let header_style = StyleRules {
        padding_top: Some(px(10.0)),
        padding_bottom: Some(px(10.0)),
        padding_left: Some(px(16.0)),
        padding_right: Some(px(16.0)),
        border_bottom_width: Some(border_width(1.0)),
        border_bottom_color: Some(col(BORDER)),
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        gap: Some(px(8.0)),
        ..Default::default()
    };
    let hash_style = StyleRules { font_size: Some(px(20.0)), color: Some(col(TEXT_MUTED)), ..Default::default() };
    let title_style = StyleRules { font_size: Some(px(15.0)), font_weight: Some(FontWeight::SemiBold), color: Some(col(TEXT)), ..Default::default() };
    let topic_style = StyleRules { font_size: Some(px(13.0)), color: Some(col(TEXT_MUTED)), flex_grow: Some(grow(1.0)), ..Default::default() };

    let header = switch(
        move || (active_server.get(), active_channel.get()),
        move |(server_id, channel_id): &(u32, u32)| {
            let header_style = header_style.clone();
            let hash_style = hash_style.clone();
            let title_style = title_style.clone();
            let topic_style = topic_style.clone();
            let server = SERVERS.iter().find(|x| x.id == *server_id).unwrap();
            let channel = server.channels.iter().find(|c| c.id == *channel_id).unwrap();
            let title = channel.name.to_string();
            let topic = channel.topic.to_string();
            ui! {
                view(style = header_style) {
                    text(style = hash_style) { "#".to_string() }
                    text(style = title_style) { title }
                    text(style = topic_style) { topic }
                }
            }
        },
    );

    // Message list — the channel's seed messages plus any sent here this session.
    // `sent_read` is a dedicated clone so this closure and `on_send` don't both
    // `move`-capture the same non-`Copy` `Rc`. The key includes `revision`
    // because `switch` dedups on scrutinee VALUE — without it, a send (which
    // keeps the same channel) would not rebuild the list.
    let sent_read = sent.clone();
    let sent_messages = switch(
        move || (active_server.get(), active_channel.get(), revision.get()),
        move |(server_id, channel_id, _revision): &(u32, u32, u32)| {
            let server = SERVERS.iter().find(|x| x.id == *server_id).unwrap();
            let channel = server.channels.iter().find(|c| c.id == *channel_id).unwrap();
            let cid = *channel_id;
            let mut rows: Vec<Element> = Vec::with_capacity(channel.messages.len());
            for m in channel.messages.iter() {
                rows.push(message_row(m.author, m.color, m.body));
            }
            let mine: Vec<(u32, &'static str, String)> = sent_read.borrow().clone();
            for (_mcid, author, body) in mine.iter().filter(|(mcid, _, _)| *mcid == cid) {
                rows.push(message_row(author, BUTTON, body.as_str()));
            }
            ui! {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Column),
                    ..Default::default()
                }) { rows }
            }
        },
    );

    // --- composer --------------------------------------------------------
    let composer_style = StyleRules {
        padding_top: Some(px(12.0)),
        padding_bottom: Some(px(12.0)),
        padding_left: Some(px(16.0)),
        padding_right: Some(px(16.0)),
        border_top_width: Some(border_width(1.0)),
        border_top_color: Some(col(BORDER)),
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        gap: Some(px(8.0)),
        ..Default::default()
    };
    let input_style = StyleRules {
        flex_grow: Some(grow(1.0)),
        height: Some(px(40.0)),
        padding_left: Some(px(12.0)),
        padding_right: Some(px(12.0)),
        background: Some(col(COMPOSER)),
        color: Some(col(TEXT)),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        ..Default::default()
    };
    let send_style = StyleRules {
        height: Some(px(40.0)),
        padding_left: Some(px(18.0)),
        padding_right: Some(px(18.0)),
        background: Some(col(BUTTON)),
        color: Some(col(TEXT)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        ..Default::default()
    };

    let on_change = move |s: String| {
        composer.set(s);
    };
    // One send action shared by the Send button AND the Enter key on the
    // composer. Every captured signal/Rc is cloned behind an `Rc` so the
    // handler can be `Fn` (callable from both the `button on_click` and the
    // `text_input on_key_down` closures).
    let send_composer = composer.clone();
    let sent_write = sent.clone();
    let revision_bump = revision.clone();
    let do_send = move || {
        let body = send_composer.get();
        if body.trim().is_empty() {
            return;
        }
        let cid = active_channel.get();
        sent_write.borrow_mut().push((cid, "you", body));
        send_composer.set(String::new());
        // Bump the revision so the message-list `switch` rebuilds (its key
        // dedups on value, so re-setting the same channel alone would not).
        revision_bump.set(revision_bump.get() + 1);
    };
    // Button click → send.
    let on_send = Rc::new(do_send);
    let send_btn = on_send.clone();
    let on_click = move || (send_btn)();
    // Enter on the composer → send. Plain Enter only; Shift/Ctrl/Alt/Meta+
    // Enter fall through to the platform default. `PreventDefault` stops the
    // input's default Enter handling (no newline / no stray submit) now that
    // we've taken the action.
    let send_key = on_send.clone();
    let on_enter = move |ev: &KeyEvent| {
        if ev.key == "Enter" && !ev.shift && !ev.ctrl && !ev.alt && !ev.meta {
            (send_key)();
            KeyOutcome::PreventDefault
        } else {
            KeyOutcome::Default
        }
    };

    // --- member list (right side) ----------------------------------------
    // Rebuilds when the active server changes (each server has its own
    // roster). Rendered as the right-most top-level column.
    let members_switch = switch(
        move || active_server.get(),
        move |server_id: &u32| {
            let server = SERVERS.iter().find(|x| x.id == *server_id).unwrap();
            member_panel(server)
        },
    );

    let main_style = StyleRules {
        flex_grow: Some(grow(1.0)),
        background: Some(col(BG_MAIN)),
        flex_direction: Some(FlexDirection::Column),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };

    ui! {
        view(style = StyleRules {
            width: Some(pct(100.0)),
            height: Some(pct(100.0)),
            background: Some(col(BG_MAIN)),
            flex_direction: Some(FlexDirection::Row),
            overflow: Some(Overflow::Hidden),
            ..Default::default()
        }) {
            view(style = rail_style) { server_rows }
            sidebar
            view(style = main_style) {
                header
                scroll_view(style = StyleRules { flex_grow: Some(grow(1.0)), ..Default::default() }) { sent_messages }
                view(style = composer_style) {
                    text_input(value = composer, on_change = on_change, placeholder = "Message", style = input_style).on_key_down(on_enter)
                    button(label = "Send".to_string(), on_click = on_click, style = send_style)
                }
            }
            members_switch
        }
    }
}
