//! Group DMs: the friend picker (new group, or add people to one), the
//! members panel (owner badge, remove, leave), the conversation title, and
//! the refresh after a `DmChanged` frame. `live_dm_view` hosts them.

use std::cell::RefCell;
use std::rc::Rc;

use icons_lucide::{CROWN, LOG_OUT, X};
use runtime_core::{
    icon, signal, switch, ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent, Overflow, Position,
    Signal, StyleRules, TouchPhase, TouchResponse,
};

use crate::dms::{self, Dm};
use crate::live_dm_view::LiveDmMsg;
use crate::style_helpers::{borderless_input, col, pct, px};
use crate::theme;

/// What the picker is for: a new group, or adding people to group `dm_id`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Picker {
    NewGroup,
    AddTo(i64),
}

fn grow(v: f32) -> runtime_core::Tokenized<f32> {
    runtime_core::Tokenized::Literal(v)
}

/// The signed-in user's id.
fn me() -> i64 {
    crate::auth::CURRENT_USER_ID.load(std::sync::atomic::Ordering::SeqCst)
}

/// A DM's title: a group's name, else the other members' names; a 1:1 DM's
/// other person.
pub(crate) fn dm_title(dm: &Dm) -> String {
    if let Some(name) = dm.name.as_ref().filter(|n| !n.is_empty()) {
        return name.clone();
    }
    let me = me();
    let others: Vec<String> = dm
        .participants
        .iter()
        .filter(|p| p.user_id != me)
        .map(|p| crate::auth::shown_name(&p.username))
        .collect();
    if others.is_empty() {
        if dm.is_group { "Just you".to_string() } else { format!("user #{}", dm.channel_id) }
    } else {
        others.join(", ")
    }
}

/// Load a DM's history into the open-conversation buffer (app-shell state,
/// so the reply is anchored to the shell).
pub(crate) fn load_history(dm_id: i64, msgs: Rc<RefCell<Vec<LiveDmMsg>>>, rev: Signal<u32>) {
    runtime_core::spawn_then_in(&crate::app::shell_alive(), dms::list_dm_messages(dm_id, None), move |result| {
        if let Ok(history) = result {
            let mut buf = msgs.borrow_mut();
            buf.clear();
            buf.extend(history.into_iter().map(|m| (m.author_username, m.content, m.created_at, m.edited_at)));
            drop(buf);
            rev.set(rev.get() + 1);
        }
    });
}

/// Open `dm` (adding it to the list if it's new).
fn open(dm: Dm, live_dms: Signal<Vec<Dm>>, active: Signal<Option<i64>>, msgs: Rc<RefCell<Vec<LiveDmMsg>>>, rev: Signal<u32>) {
    let id = dm.channel_id;
    let mut list = live_dms.get();
    match list.iter_mut().find(|d| d.channel_id == id) {
        Some(existing) => *existing = dm,
        None => list.insert(0, dm),
    }
    live_dms.set(list);
    active.set(Some(id));
    crate::auth::save_last_view_dm(id);
    load_history(id, msgs, rev);
}

/// Refetch the DM list after a `DmChanged` frame; if the open DM is gone
/// (the user left or was removed), close it.
pub(crate) fn refresh(live_dms: Signal<Vec<Dm>>, active: Signal<Option<i64>>, msgs: Rc<RefCell<Vec<LiveDmMsg>>>, rev: Signal<u32>) {
    runtime_core::spawn_then_in(&crate::app::shell_alive(), dms::list_dms(), move |result| {
        if let Ok(list) = result {
            if let Some(open) = active.get() {
                if !list.iter().any(|d| d.channel_id == open) {
                    active.set(None);
                    msgs.borrow_mut().clear();
                    rev.set(rev.get() + 1);
                }
            }
            live_dms.set(list);
        }
    });
}

fn button(label: &str, background: &str, on_press: impl Fn() + 'static) -> Element {
    let label = label.to_string();
    let bg = background.to_string();
    ui! {
        view(style = StyleRules {
            height: Some(px(36.0)),
            padding_left: Some(px(16.0)),
            padding_right: Some(px(16.0)),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::Center),
            background: Some(col(&bg)),
            border_top_left_radius: Some(px(4.0)),
            border_top_right_radius: Some(px(4.0)),
            border_bottom_left_radius: Some(px(4.0)),
            border_bottom_right_radius: Some(px(4.0)),
            ..Default::default()
        }) {
            text(style = StyleRules {
                font_size: Some(px(14.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(theme::TEXT_ON_ACCENT)),
                ..Default::default()
            }) { label }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                on_press();
            }
            TouchResponse::CONSUMED
        })
    }
}

/// The friend picker dialog. Shown while `picker` is `Some`. Picking one
/// friend for a new group opens a plain DM with them instead.
pub(crate) fn picker_dialog(
    picker: Signal<Option<Picker>>,
    live_dms: Signal<Vec<Dm>>,
    active: Signal<Option<i64>>,
    msgs: Rc<RefCell<Vec<LiveDmMsg>>>,
    rev: Signal<u32>,
) -> Element {
    // Replies land in this dialog's signals, so they are anchored to it.
    let alive = runtime_core::ScopeAlive::current();
    let friends: Signal<Vec<(i64, String)>> = signal(Vec::new());
    let selected: Signal<Vec<i64>> = signal(Vec::new());
    let name = signal(String::new());
    let error = signal(String::new());
    let busy = signal(false);
    switch(
        move || (picker.get(), friends.get(), selected.get(), error.get(), busy.get()),
        move |(mode, friend_list, chosen, err, working): &(Option<Picker>, Vec<(i64, String)>, Vec<i64>, String, bool)| {
            let Some(mode) = *mode else {
                return ui! { view(style = StyleRules { width: Some(px(0.0)), height: Some(px(0.0)), ..Default::default() }) {} };
            };
            let existing: Vec<i64> = match mode {
                Picker::AddTo(id) => live_dms
                    .get()
                    .iter()
                    .find(|d| d.channel_id == id)
                    .map(|d| d.participants.iter().map(|p| p.user_id).collect())
                    .unwrap_or_default(),
                Picker::NewGroup => vec![me()],
            };
            // Fetch accepted friends the first time the dialog opens.
            if friend_list.is_empty() {
                runtime_core::spawn_then_in(&alive, crate::friends::list_friends(), move |result| {
                    if let Ok(list) = result {
                        let mut rows: Vec<(i64, String)> = list.accepted.into_iter().map(|u| (u.user_id, u.username)).collect();
                        rows.sort_by_key(|(_, n)| crate::auth::shown_name(n).to_lowercase());
                        friends.set(rows);
                    }
                });
            }
            let room = discord_server::dms::MAX_GROUP_MEMBERS.saturating_sub(existing.len().max(1) + chosen.len());
            let candidates: Vec<(i64, String)> = friend_list.iter().filter(|(id, _)| !existing.contains(id)).cloned().collect();
            let rows: Vec<Element> = candidates
                .into_iter()
                .map(|(id, username)| {
                    let on = chosen.contains(&id);
                    let shown = crate::auth::shown_name(&username);
                    let check_bg = if on { theme::ACCENT } else { "transparent" };
                    let mark = if on { "✓" } else { "" }.to_string();
                    ui! {
                        view(style = StyleRules {
                            flex_direction: Some(FlexDirection::Row),
                            align_items: Some(AlignItems::Center),
                            gap: Some(px(10.0)),
                            height: Some(px(40.0)),
                            padding_left: Some(px(8.0)),
                            padding_right: Some(px(8.0)),
                            border_top_left_radius: Some(px(4.0)),
                            border_top_right_radius: Some(px(4.0)),
                            border_bottom_left_radius: Some(px(4.0)),
                            border_bottom_right_radius: Some(px(4.0)),
                            background: Some(col(if on { theme::BG_SELECTED } else { "transparent" })),
                            ..Default::default()
                        }) {
                            { crate::avatar_chip::avatar_chip(username.as_str(), theme::ACCENT, 28.0, 13.0) }
                            text(style = StyleRules {
                                flex_grow: Some(grow(1.0)),
                                font_size: Some(px(15.0)),
                                color: Some(col(theme::TEXT_STRONG)),
                                ..Default::default()
                            }) { shown }
                            view(style = StyleRules {
                                width: Some(px(20.0)),
                                height: Some(px(20.0)),
                                align_items: Some(AlignItems::Center),
                                justify_content: Some(JustifyContent::Center),
                                background: Some(col(check_bg)),
                                border_top_width: Some(runtime_core::Tokenized::Literal(2.0)),
                                border_right_width: Some(runtime_core::Tokenized::Literal(2.0)),
                                border_bottom_width: Some(runtime_core::Tokenized::Literal(2.0)),
                                border_left_width: Some(runtime_core::Tokenized::Literal(2.0)),
                                border_top_color: Some(col(theme::INTERACTIVE)),
                                border_right_color: Some(col(theme::INTERACTIVE)),
                                border_bottom_color: Some(col(theme::INTERACTIVE)),
                                border_left_color: Some(col(theme::INTERACTIVE)),
                                border_top_left_radius: Some(px(4.0)),
                                border_top_right_radius: Some(px(4.0)),
                                border_bottom_left_radius: Some(px(4.0)),
                                border_bottom_right_radius: Some(px(4.0)),
                                ..Default::default()
                            }) {
                                text(style = StyleRules { font_size: Some(px(13.0)), color: Some(col(theme::TEXT_ON_ACCENT)), ..Default::default() }) { mark }
                            }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                let mut now = selected.get();
                                if let Some(i) = now.iter().position(|x| *x == id) {
                                    now.remove(i);
                                } else if room > 0 {
                                    now.push(id);
                                }
                                selected.set(now);
                            }
                            TouchResponse::CONSUMED
                        })
                    }
                })
                .collect();
            let (title, action) = match mode {
                Picker::NewGroup => ("Select Friends", if chosen.len() == 1 { "Open DM" } else { "Create Group DM" }),
                Picker::AddTo(_) => ("Add Friends to Group", "Add"),
            };
            let hint = if rows.is_empty() {
                "No friends left to add.".to_string()
            } else {
                format!("You can add {room} more {}.", if room == 1 { "friend" } else { "friends" })
            };
            let close = move || {
                picker.set(None);
                selected.set(Vec::new());
                name.set(String::new());
                error.set(String::new());
            };
            let msgs_go = msgs.clone();
            let confirm_alive = alive.clone();
            let confirm = move || {
                let picked = selected.get();
                if picked.is_empty() || busy.get() {
                    return;
                }
                busy.set(true);
                error.set(String::new());
                let group_name = name.get();
                let msgs = msgs_go.clone();
                let request = async move {
                    let result: Result<Dm, String> = match mode {
                        Picker::NewGroup if picked.len() == 1 => dms::start_dm(picked[0]).await,
                        Picker::NewGroup => discord_server::dms::create_group_dm(discord_server::dms::CreateGroupDmInput {
                            user_ids: picked.clone(),
                            name: group_name,
                        })
                        .await
                        .map_err(|e| crate::errors::friendly(&e)),
                        Picker::AddTo(dm_id) => {
                            let mut last = Err("nothing to add".to_string());
                            for user_id in picked.iter().copied() {
                                last = discord_server::dms::add_group_dm_member(discord_server::dms::GroupDmMemberInput { dm_id, user_id })
                                    .await
                                    .map_err(|e| crate::errors::friendly(&e));
                                if last.is_err() {
                                    break;
                                }
                            }
                            last
                        }
                    };
                    result
                };
                runtime_core::spawn_then_in(&confirm_alive, request, move |result| {
                    match result {
                        Ok(dm) => {
                            picker.set(None);
                            selected.set(Vec::new());
                            name.set(String::new());
                            open(dm, live_dms, active, msgs, rev);
                        }
                        Err(e) => error.set(e),
                    }
                    busy.set(false);
                });
            };
            let name_el = (mode == Picker::NewGroup && chosen.len() >= 2).then(|| {
                ui! {
                    text_input(
                        value = name,
                        on_change = move |v: String| name.set(v),
                        placeholder = "Group name (optional)",
                        style = StyleRules {
                            height: Some(px(36.0)),
                            padding_left: Some(px(10.0)),
                            padding_right: Some(px(10.0)),
                            background: Some(col(theme::BG_TERTIARY)),
                            color: Some(col(theme::TEXT_STRONG)),
                            font_size: Some(px(14.0)),
                            border_top_left_radius: Some(px(4.0)),
                            border_top_right_radius: Some(px(4.0)),
                            border_bottom_left_radius: Some(px(4.0)),
                            border_bottom_right_radius: Some(px(4.0)),
                            ..borderless_input()
                        },
                    )
                }
            });
            let err_el = (!err.is_empty()).then(|| {
                let err = err.clone();
                ui! { text(style = StyleRules { font_size: Some(px(13.0)), color: Some(col(theme::DANGER)), ..Default::default() }) { err } }
            });
            let action_label = if *working { "Working…".to_string() } else { action.to_string() };
            let confirm_bg = if chosen.is_empty() { theme::BUTTON_SECONDARY } else { theme::ACCENT };
            ui! {
                view(style = StyleRules {
                    position: Some(Position::Absolute),
                    top: Some(px(0.0)),
                    left: Some(px(0.0)),
                    width: Some(pct(100.0)),
                    height: Some(pct(100.0)),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    background: Some(col(theme::OVERLAY)),
                    z_index: Some(70),
                    ..Default::default()
                }) {
                    view(style = StyleRules {
                        width: Some(px(440.0)),
                        max_width: Some(pct(92.0)),
                        max_height: Some(pct(85.0)),
                        flex_direction: Some(FlexDirection::Column),
                        gap: Some(px(10.0)),
                        padding_top: Some(px(16.0)),
                        padding_bottom: Some(px(16.0)),
                        padding_left: Some(px(16.0)),
                        padding_right: Some(px(16.0)),
                        background: Some(col(theme::BG_PRIMARY)),
                        border_top_left_radius: Some(px(8.0)),
                        border_top_right_radius: Some(px(8.0)),
                        border_bottom_left_radius: Some(px(8.0)),
                        border_bottom_right_radius: Some(px(8.0)),
                        overflow: Some(Overflow::Hidden),
                        ..Default::default()
                    }) {
                        text(style = StyleRules { font_size: Some(px(20.0)), font_weight: Some(FontWeight::Bold), color: Some(col(theme::TEXT_STRONG)), ..Default::default() }) { title.to_string() }
                        text(style = StyleRules { font_size: Some(px(13.0)), color: Some(col(theme::TEXT_MUTED)), ..Default::default() }) { hint }
                        scroll_view(style = StyleRules { flex_shrink: Some(grow(1.0)), max_height: Some(px(320.0)), ..Default::default() }) {
                            view(style = StyleRules { flex_direction: Some(FlexDirection::Column), gap: Some(px(2.0)), ..Default::default() }) {
                                { rows }
                            }
                        }
                        if let Some(el) = name_el {
                            el
                        }
                        if let Some(el) = err_el {
                            el
                        }
                        view(style = StyleRules {
                            flex_direction: Some(FlexDirection::Row),
                            justify_content: Some(JustifyContent::FlexEnd),
                            gap: Some(px(8.0)),
                            ..Default::default()
                        }) {
                            { button("Cancel", theme::BUTTON_SECONDARY, close) }
                            { button(&action_label, confirm_bg, confirm) }
                        }
                    }
                }
            }
        },
    )
}

/// The members panel of group `dm`: everyone in it (the owner marked), a
/// remove button on each for the owner, and Leave Group.
pub(crate) fn members_panel(dm: &Dm) -> Element {
    let me = me();
    let dm_id = dm.channel_id;
    let owner = dm.owner_id;
    let error = signal(String::new());
    let rows: Vec<Element> = dm
        .participants
        .iter()
        .map(|p| {
            let uid = p.user_id;
            let username = p.username.clone();
            let shown = crate::auth::shown_name(&username);
            let crown = (owner == Some(uid)).then(|| ui! { icon(CROWN).color(|| theme::live(theme::WARNING)).size(14.0) });
            let remove = (owner == Some(me) && uid != me).then(|| {
                ui! {
                    view(style = StyleRules { padding_left: Some(px(4.0)), padding_right: Some(px(4.0)), ..Default::default() }) {
                        icon(X).color(|| theme::live(theme::TEXT_MUTED)).size(14.0)
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            let input = discord_server::dms::GroupDmMemberInput { dm_id, user_id: uid };
                            runtime_core::spawn_then(discord_server::dms::remove_group_dm_member(input), move |result| {
                                if let Err(e) = result {
                                    error.set(crate::errors::friendly(&e));
                                }
                            });
                        }
                        TouchResponse::CONSUMED
                    })
                }
            });
            ui! {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    gap: Some(px(8.0)),
                    height: Some(px(40.0)),
                    padding_left: Some(px(8.0)),
                    padding_right: Some(px(8.0)),
                    ..Default::default()
                }) {
                    { crate::avatar_chip::avatar_chip(username.as_str(), theme::ACCENT, 28.0, 13.0) }
                    text(style = StyleRules {
                        flex_grow: Some(grow(1.0)),
                        min_width: Some(px(0.0)),
                        font_size: Some(px(14.0)),
                        color: Some(col(theme::TEXT_NORMAL)),
                        ..Default::default()
                    }) { shown }
                    if let Some(el) = crown {
                        el
                    }
                    if let Some(el) = remove {
                        el
                    }
                }
            }
        })
        .collect();
    let count = format!("MEMBERS — {}", dm.participants.len());
    ui! {
        view(style = StyleRules {
            width: Some(px(240.0)),
            flex_shrink: Some(grow(0.0)),
            flex_direction: Some(FlexDirection::Column),
            background: Some(col(theme::BG_SECONDARY)),
            padding_top: Some(px(12.0)),
            padding_left: Some(px(8.0)),
            padding_right: Some(px(8.0)),
            gap: Some(px(2.0)),
            ..Default::default()
        }) {
            text(style = StyleRules {
                padding_left: Some(px(8.0)),
                padding_bottom: Some(px(6.0)),
                font_size: Some(px(12.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(theme::TEXT_MUTED)),
                ..Default::default()
            }) { count }
            { rows }
            if !error.get().is_empty() {
                text(style = StyleRules { font_size: Some(px(12.0)), color: Some(col(theme::DANGER)), padding_left: Some(px(8.0)), ..Default::default() }) {
                    move || error.get()
                }
            }
            view(style = StyleRules { flex_grow: Some(grow(1.0)), ..Default::default() }) {}
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(8.0)),
                height: Some(px(40.0)),
                padding_left: Some(px(8.0)),
                margin_bottom: Some(px(8.0)),
                ..Default::default()
            }) {
                icon(LOG_OUT).color(|| theme::live(theme::DANGER)).size(16.0)
                text(style = StyleRules { font_size: Some(px(14.0)), color: Some(col(theme::DANGER)), ..Default::default() }) { "Leave Group".to_string() }
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    let input = discord_server::dms::GroupDmMemberInput { dm_id, user_id: me };
                    runtime_core::spawn_then(discord_server::dms::remove_group_dm_member(input), move |result| {
                        if let Err(e) = result {
                            error.set(crate::errors::friendly(&e));
                        }
                    });
                }
                TouchResponse::CONSUMED
            })
        }
    }
}
