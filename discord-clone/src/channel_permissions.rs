//! The channel permissions dialog: whether a channel is private, who can see
//! it, and what @everyone, a role or a member may do there (allow, neutral or
//! deny per channel-scoped permission). A channel's menu opens it for anyone
//! with Manage Channels there; the server enforces the same rules.

use std::cell::Cell;

use discord_server::permissions::{self, Overwrite, Permissions, Rank, Target};
use runtime_core::{
    signal, switch, ui, AlignItems, Element, FlexDirection, FlexWrap, FontWeight,
    JustifyContent, Overflow, Position, Signal, StyleRules, Tokenized, TouchPhase, TouchResponse,
};

use crate::style_helpers::{col, pct, px, rounded};
use crate::theme;

/// The dialog's state, shared by [`open_for`] and [`dialog`].
#[derive(Clone, Copy)]
struct State {
    /// The channel being edited and its name; `None` = closed.
    open: Signal<Option<(i64, String)>>,
    overwrites: Signal<Vec<Overwrite>>,
    /// Whose permissions the lower half edits.
    selected: Signal<Target>,
    /// The "Add role or member" picker is showing.
    adding: Signal<bool>,
    error: Signal<String>,
}

thread_local! {
    static STATE: Cell<Option<State>> = const { Cell::new(None) };
}

fn state() -> State {
    STATE.with(|s| s.get()).expect("channel_permissions::install runs in app_shell")
}

/// Set up the dialog's state (once, from `app_shell`).
pub(crate) fn install() {
    let s = State {
        open: signal(None),
        overwrites: signal(Vec::new()),
        selected: signal(Target::Everyone),
        adding: signal(false),
        error: signal(String::new()),
    };
    STATE.with(|c| c.set(Some(s)));
}

/// The open/closed signal, for Escape (see `app::popups`).
pub(crate) fn open_signal() -> Signal<Option<(i64, String)>> {
    state().open
}

/// Open the dialog for `channel_id` and load its overwrites.
pub(crate) fn open_for(channel_id: i64, name: String) {
    let s = state();
    s.overwrites.set(Vec::new());
    s.selected.set(Target::Everyone);
    s.adding.set(false);
    s.error.set(String::new());
    s.open.set(Some((channel_id, name)));
    runtime_core::driver::spawn_async(async move {
        let input = permissions::ChannelOverwritesInput { channel_id };
        match permissions::channel_overwrites(input).await {
            Ok(list) => s.overwrites.set(list),
            Err(e) => s.error.set(crate::errors::friendly(&e)),
        }
    });
}

/// `target`'s allow and deny on the open channel.
fn find(list: &[Overwrite], target: Target) -> (Permissions, Permissions) {
    list.iter().find(|o| o.target == target).map_or((Permissions::NONE, Permissions::NONE), |o| (o.allow, o.deny))
}

async fn put(channel_id: i64, target: Target, allow: Permissions, deny: Permissions) -> Result<Vec<Overwrite>, String> {
    let input = permissions::SetOverwriteInput { channel_id, target, allow, deny };
    permissions::set_channel_overwrite(input).await.map_err(|e| crate::errors::friendly(&e))
}

/// Save one target's overwrite and show the result.
fn save(channel_id: i64, target: Target, allow: Permissions, deny: Permissions) {
    let s = state();
    runtime_core::driver::spawn_async(async move {
        match put(channel_id, target, allow, deny).await {
            Ok(list) => {
                s.error.set(String::new());
                s.overwrites.set(list);
            }
            Err(e) => s.error.set(e),
        }
    });
}

/// Make the channel private or public. Going private first lets the acting
/// user in by name, unless they are the owner or an administrator, so they
/// don't lock themselves out.
fn set_private(channel_id: i64, private: bool) {
    let s = state();
    let list = s.overwrites.get();
    let (allow, deny) = find(&list, Target::Everyone);
    let me = crate::auth::CURRENT_USER_ID.load(std::sync::atomic::Ordering::SeqCst);
    let let_me_in = private
        && crate::mentions::my_rank() != Rank::Owner
        && !crate::mentions::can(Permissions::ADMINISTRATOR);
    let (my_allow, my_deny) = find(&list, Target::Member(me));
    runtime_core::driver::spawn_async(async move {
        let mut result = Ok(Vec::new());
        if let_me_in {
            result = put(channel_id, Target::Member(me), my_allow.union(Permissions::VIEW_CHANNEL), my_deny.minus(Permissions::VIEW_CHANNEL)).await;
        }
        if result.is_ok() {
            let deny = if private { deny.union(Permissions::VIEW_CHANNEL) } else { deny.minus(Permissions::VIEW_CHANNEL) };
            result = put(channel_id, Target::Everyone, allow.minus(Permissions::VIEW_CHANNEL), deny).await;
        }
        match result {
            Ok(list) => {
                s.error.set(String::new());
                s.overwrites.set(list);
            }
            Err(e) => s.error.set(e),
        }
    });
}

/// A role's or member's name for the dialog.
fn target_name(target: Target) -> String {
    match target {
        Target::Everyone => "@everyone".to_string(),
        Target::Role(id) => crate::mentions::roles()
            .roles
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.name.clone())
            .unwrap_or_else(|| "(deleted role)".to_string()),
        Target::Member(id) => crate::mentions::username_of(id)
            .map(|u| crate::auth::shown_name(&u))
            .unwrap_or_else(|| format!("user {id}")),
    }
}

fn small(color: &'static str) -> StyleRules {
    StyleRules { font_size: Some(px(12.0)), color: Some(col(color)), ..Default::default() }
}

fn heading(label: &str) -> Element {
    let label = label.to_string();
    ui! {
        text(style = StyleRules {
            font_size: Some(px(12.0)),
            font_weight: Some(FontWeight::Bold),
            color: Some(col(theme::TEXT_MUTED)),
            margin_top: Some(px(8.0)),
            ..Default::default()
        }) { label }
    }
}

/// A small tappable pill; `on` fills it with the accent.
fn chip(label: String, on: bool, on_tap: impl Fn() + 'static) -> Element {
    ui! {
        view(style = StyleRules {
            height: Some(px(28.0)),
            padding_left: Some(px(10.0)),
            padding_right: Some(px(10.0)),
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(6.0)),
            background: Some(col(if on { theme::ACCENT } else { theme::BG_TERTIARY })),
            ..rounded(14.0)
        }) {
            text(style = StyleRules {
                font_size: Some(px(13.0)),
                color: Some(col(if on { theme::TEXT_ON_ACCENT } else { theme::TEXT_NORMAL })),
                ..Default::default()
            }) { label }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                on_tap();
            }
            TouchResponse::CONSUMED
        })
    }
}

/// One channel-scoped permission with its three-way control.
fn flag_row(channel_id: i64, target: Target, flag: Permissions, name: &str, about: &str, allow: Permissions, deny: Permissions) -> Element {
    let state = if allow.contains(flag) { 2 } else if deny.contains(flag) { 0 } else { 1 };
    let button = move |which: u8, glyph: &'static str, fill: &'static str| {
        let on = state == which;
        ui! {
            view(style = StyleRules {
                width: Some(px(30.0)),
                height: Some(px(26.0)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                background: Some(col(if on { fill } else { theme::BG_TERTIARY })),
                ..rounded(4.0)
            }) {
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    font_weight: Some(FontWeight::Bold),
                    color: Some(col(if on { theme::TEXT_ON_ACCENT } else { theme::TEXT_MUTED })),
                    ..Default::default()
                }) { glyph.to_string() }
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended && !on {
                    let (a, d) = match which {
                        0 => (allow.minus(flag), deny.union(flag)),
                        2 => (allow.union(flag), deny.minus(flag)),
                        _ => (allow.minus(flag), deny.minus(flag)),
                    };
                    save(channel_id, target, a, d);
                }
                TouchResponse::CONSUMED
            })
        }
    };
    let (name, about) = (name.to_string(), about.to_string());
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(12.0)),
            padding_top: Some(px(6.0)),
            padding_bottom: Some(px(6.0)),
            ..Default::default()
        }) {
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Column),
                flex_grow: Some(Tokenized::Literal(1.0)),
                flex_shrink: Some(Tokenized::Literal(1.0)),
                ..Default::default()
            }) {
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    color: Some(col(theme::TEXT_NORMAL)),
                    ..Default::default()
                }) { name }
                text(style = small(theme::TEXT_MUTED)) { about }
            }
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                gap: Some(px(2.0)),
                flex_shrink: Some(Tokenized::Literal(0.0)),
                ..Default::default()
            }) {
                { button(0, "✕", theme::DANGER) }
                { button(1, "/", theme::BUTTON_SECONDARY) }
                { button(2, "✓", theme::SUCCESS) }
            }
        }
    }
}

/// The dialog, built once by `app_shell`; collapsed while closed.
pub(crate) fn dialog() -> Element {
    let s = state();
    let body = switch(
        move || (s.open.get(), s.overwrites.get(), s.selected.get(), s.adding.get(), s.error.get()),
        move |(open, list, selected, adding, error): &(Option<(i64, String)>, Vec<Overwrite>, Target, bool, String)| {
            let Some((cid, name)) = open.clone() else {
                return ui! { view {} };
            };
            let (selected, adding) = (*selected, *adding);
            let (_, everyone_deny) = find(list, Target::Everyone);
            let private = everyone_deny.contains(Permissions::VIEW_CHANNEL);

            // Private switch.
            let private_row = ui! {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    gap: Some(px(12.0)),
                    ..Default::default()
                }) {
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Column),
                        flex_grow: Some(Tokenized::Literal(1.0)),
                        flex_shrink: Some(Tokenized::Literal(1.0)),
                        ..Default::default()
                    }) {
                        text(style = StyleRules {
                            font_size: Some(px(15.0)),
                            font_weight: Some(FontWeight::SemiBold),
                            color: Some(col(theme::TEXT_STRONG)),
                            ..Default::default()
                        }) { "Private channel".to_string() }
                        text(style = small(theme::TEXT_MUTED)) {
                            "Only the roles and members you choose can see it.".to_string()
                        }
                    }
                    { crate::controls::toggle(private, true, move || set_private(cid, !private)) }
                }
            };

            // Who can access: roles and members let in by name.
            let access: Vec<Target> = list
                .iter()
                .filter(|o| o.target != Target::Everyone && o.allow.contains(Permissions::VIEW_CHANNEL))
                .map(|o| o.target)
                .collect();
            let access_rows: Vec<Element> = access
                .iter()
                .map(|&t| {
                    let (a, d) = find(list, t);
                    let label = target_name(t);
                    ui! {
                        view(style = StyleRules {
                            flex_direction: Some(FlexDirection::Row),
                            align_items: Some(AlignItems::Center),
                            height: Some(px(32.0)),
                            ..Default::default()
                        }) {
                            text(style = StyleRules {
                                font_size: Some(px(14.0)),
                                color: Some(col(theme::TEXT_NORMAL)),
                                flex_grow: Some(Tokenized::Literal(1.0)),
                                ..Default::default()
                            }) { label }
                            view(style = StyleRules {
                                width: Some(px(24.0)),
                                height: Some(px(24.0)),
                                align_items: Some(AlignItems::Center),
                                justify_content: Some(JustifyContent::Center),
                                ..Default::default()
                            }) {
                                text(style = StyleRules { font_size: Some(px(16.0)), color: Some(col(theme::TEXT_MUTED)), ..Default::default() }) { "×".to_string() }
                            }
                            .on_touch(move |ev| {
                                if ev.phase == TouchPhase::Ended {
                                    save(cid, t, a.minus(Permissions::VIEW_CHANNEL), d);
                                }
                                TouchResponse::CONSUMED
                            })
                        }
                    }
                })
                .collect();
            let access_el = if private {
                let picker: Vec<Element> = if adding {
                    let roles = crate::mentions::roles().roles.into_iter().map(|r| Target::Role(r.id));
                    let members = crate::mentions::member_ids().into_iter().map(Target::Member);
                    roles
                        .chain(members)
                        .filter(|t| !access.contains(t))
                        .map(|t| {
                            let (a, d) = find(list, t);
                            chip(format!("+ {}", target_name(t)), false, move || {
                                state().adding.set(false);
                                save(cid, t, a.union(Permissions::VIEW_CHANNEL), d.minus(Permissions::VIEW_CHANNEL));
                            })
                        })
                        .collect()
                } else {
                    vec![chip("Add role or member".to_string(), false, move || state().adding.set(true))]
                };
                ui! {
                    view(style = StyleRules { flex_direction: Some(FlexDirection::Column), ..Default::default() }) {
                        { heading("WHO CAN ACCESS THIS CHANNEL") }
                        { access_rows }
                        view(style = StyleRules {
                            flex_direction: Some(FlexDirection::Row),
                            flex_wrap: Some(FlexWrap::Wrap),
                            gap: Some(px(6.0)),
                            margin_top: Some(px(4.0)),
                            ..Default::default()
                        }) { picker }
                    }
                }
            } else {
                ui! { view {} }
            };

            // Whose permissions to edit: @everyone, then everyone with an
            // overwrite, then every other role.
            let mut targets = vec![Target::Everyone];
            for o in list {
                if !targets.contains(&o.target) {
                    targets.push(o.target);
                }
            }
            for r in crate::mentions::roles().roles {
                if !targets.contains(&Target::Role(r.id)) {
                    targets.push(Target::Role(r.id));
                }
            }
            let target_chips: Vec<Element> = targets
                .iter()
                .map(|&t| chip(target_name(t), t == selected, move || state().selected.set(t)))
                .collect();
            let (allow, deny) = find(list, selected);
            let flag_rows: Vec<Element> = Permissions::FLAGS
                .iter()
                .filter(|(flag, ..)| Permissions::CHANNEL_SCOPED.contains(*flag))
                .map(|(flag, name, about)| flag_row(cid, selected, *flag, name, about, allow, deny))
                .collect();

            let close = s.open;
            let title = format!("#{name}");
            let error = error.clone();
            ui! {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Column),
                    gap: Some(px(10.0)),
                    padding_top: Some(px(20.0)),
                    padding_bottom: Some(px(20.0)),
                    padding_left: Some(px(20.0)),
                    padding_right: Some(px(20.0)),
                    ..Default::default()
                }) {
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        ..Default::default()
                    }) {
                        view(style = StyleRules {
                            flex_direction: Some(FlexDirection::Column),
                            flex_grow: Some(Tokenized::Literal(1.0)),
                            ..Default::default()
                        }) {
                            text(style = StyleRules {
                                font_size: Some(px(20.0)),
                                font_weight: Some(FontWeight::SemiBold),
                                color: Some(col(theme::TEXT_STRONG)),
                                ..Default::default()
                            }) { "Channel Permissions".to_string() }
                            text(style = small(theme::TEXT_MUTED)) { title }
                        }
                        view {
                            text(style = StyleRules { font_size: Some(px(20.0)), color: Some(col(theme::TEXT_MUTED)), ..Default::default() }) { "✕".to_string() }
                        }
                        .on_touch(move |ev| {
                                if ev.phase == TouchPhase::Ended {
                                    close.set(None);
                                }
                                TouchResponse::CONSUMED
                            })
                    }
                    { private_row }
                    { access_el }
                    { heading("PERMISSIONS") }
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Row),
                        flex_wrap: Some(FlexWrap::Wrap),
                        gap: Some(px(6.0)),
                        ..Default::default()
                    }) { target_chips }
                    view(style = StyleRules { flex_direction: Some(FlexDirection::Column), ..Default::default() }) { flag_rows }
                    text(style = small(theme::DANGER)) { error }
                }
            }
        },
    );

    let open = s.open;
    let card = ui! {
        view(style = StyleRules {
            width: Some(px(crate::app::mobile::fit_width(520.0))),
            max_height: Some(pct(85.0)),
            background: Some(col(theme::BG_PRIMARY)),
            overflow: Some(Overflow::Hidden),
            ..rounded(8.0)
        }) {
            scroll_view(style = StyleRules {
                flex_grow: Some(Tokenized::Literal(1.0)),
                ..Default::default()
            }) {
                { body }
            }
        }
        .on_touch(|_| TouchResponse::CONSUMED)
    };
    ui! {
        view(style = move || {
            let shown = open.get().is_some();
            StyleRules {
                position: Some(Position::Absolute),
                left: Some(px(0.0)),
                top: Some(px(0.0)),
                width: Some(if shown { pct(100.0) } else { px(0.0) }),
                height: Some(if shown { pct(100.0) } else { px(0.0) }),
                background: Some(col(theme::OVERLAY)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                overflow: Some(Overflow::Hidden),
                ..Default::default()
            }
        }) {
            { card }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                open.set(None);
            }
            TouchResponse::CONSUMED
        })
    }
}
