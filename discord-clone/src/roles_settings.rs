//! The "Roles" section of Server Settings: the server's named roles (colour,
//! name, whether anyone may @mention it) with delete and reorder, a row to
//! create one, and @everyone pinned at the bottom. Selecting a role (or
//! @everyone) opens its permissions below the list. Anyone with Manage Roles
//! edits roles below their own; everyone else sees the list. Members are
//! given roles from their profile card.

use crate::style_helpers::{borderless_input, col, px, rounded, with_radius};
use crate::theme;
use discord_server::permissions::{self, Permissions, Rank};
use discord_server::server_roles::{self, CustomRole};
use runtime_core::{
    signal, switch, ui, AlignItems, Element, FlexDirection, FlexWrap, FontWeight, JustifyContent,
    Signal, StyleRules, Tokenized, TouchPhase, TouchResponse,
};

/// Discord's role colour presets; "" is no colour.
const COLORS: [&str; 11] = [
    "", "#1abc9c", "#2ecc71", "#3498db", "#9b59b6", "#e91e63", "#f1c40f", "#e67e22", "#e74c3c",
    "#95a5a6", "#607d8b",
];

/// Which entry's permissions are open.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Selected {
    Role(i64),
    Everyone,
}

fn radius(r: f32) -> StyleRules {
    StyleRules {
        ..rounded(r)
    }
}

fn dot(color: &str, size: f32) -> Element {
    let fill = if color.is_empty() { col(theme::TEXT_MUTED) } else { theme::fixed(color) };
    ui! {
        view(style = StyleRules {
            width: Some(px(size)),
            height: Some(px(size)),
            flex_shrink: Some(Tokenized::Literal(0.0)),
            background: Some(fill),
            ..radius(size / 2.0)
        }) {}
    }
}

/// Reload the list, @everyone's permissions and the app's role colours
/// after a change.
fn reload(server_id: i64, roles: Signal<Vec<CustomRole>>, everyone: Signal<Permissions>) {
    runtime_core::driver::spawn_async(async move {
        if let Ok(snap) = server_roles::server_roles(server_roles::ServerRolesInput { server_id }).await {
            roles.set(snap.roles);
            everyone.set(snap.everyone_permissions);
        }
        crate::mentions::load_roles(server_id);
    });
}

/// Whether the signed-in user may edit a role at `position`: the owner may
/// edit any; others only roles below their own.
fn editable(position: i64) -> bool {
    let mine = crate::mentions::my_rank();
    mine == Rank::Owner || mine.outranks(Rank::Role(position))
}

/// A small square text button (the reorder arrows, delete).
fn icon_button(glyph: &'static str, enabled: bool, on_tap: impl Fn() + 'static) -> Element {
    ui! {
        view(style = StyleRules {
            width: Some(px(22.0)),
            height: Some(px(22.0)),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::Center),
            opacity: Some(Tokenized::Literal(if enabled { 1.0 } else { 0.3 })),
            ..Default::default()
        }) {
            text(style = StyleRules { font_size: Some(px(14.0)), color: Some(col(theme::TEXT_MUTED)), ..Default::default() }) { glyph.to_string() }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended && enabled {
                on_tap();
            }
            TouchResponse::CONSUMED
        })
    }
}

pub fn roles_section(server_id: Option<i64>) -> Element {
    let Some(sid) = server_id else {
        return ui! { view {} };
    };
    let manage = crate::mentions::can(Permissions::MANAGE_ROLES);
    let snap = crate::mentions::roles();
    let roles: Signal<Vec<CustomRole>> = signal(snap.roles);
    let everyone: Signal<Permissions> = signal(snap.everyone_permissions);
    let name = signal(String::new());
    let color = signal(String::new());
    let error = signal(String::new());
    let selected: Signal<Option<Selected>> = signal(None);
    // The permissions being edited, and whose they are.
    let draft: Signal<Permissions> = signal(Permissions::NONE);
    reload(sid, roles, everyone);

    let small = |c: &'static str| StyleRules { font_size: Some(px(12.0)), color: Some(col(c)), ..Default::default() };
    let select = move |which: Selected, current: Permissions| {
        if selected.get() == Some(which) {
            selected.set(None);
        } else {
            draft.set(current);
            selected.set(Some(which));
        }
    };
    let list = switch(
        move || (roles.get(), everyone.get(), selected.get()),
        move |(list, everyone_perms, sel): &(Vec<CustomRole>, Permissions, Option<Selected>)| {
            let row_bg = |on: bool| if on { Some(col(theme::BG_SELECTED)) } else { None };
            let mut rows: Vec<Element> = list
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    let role = r.clone();
                    let toggle = role.clone();
                    let can_edit = manage && editable(r.position);
                    let name_color = if r.color.is_empty() { col(theme::TEXT_NORMAL) } else { theme::fixed(&r.color) };
                    let mention_label = if r.mentionable { "@ anyone" } else { "@ mods only" };
                    // Swapping with the neighbour above (`up`) or below: each
                    // takes the other's position.
                    let swap = |other: Option<&CustomRole>| {
                        let other = other.cloned();
                        let me = role.clone();
                        move || {
                            let Some(other) = other.clone() else { return };
                            let (a, b) = (me.clone(), other);
                            let (pa, pb) = if a.position == b.position { (b.position + 1, a.position) } else { (b.position, a.position) };
                            runtime_core::driver::spawn_async(async move {
                                let first = server_roles::move_role(server_roles::MoveRoleInput { role_id: a.id, position: pa }).await;
                                let second = match first {
                                    Ok(_) => server_roles::move_role(server_roles::MoveRoleInput { role_id: b.id, position: pb }).await.map(|_| ()),
                                    Err(e) => Err(e),
                                };
                                if let Err(e) = second {
                                    error.set(crate::errors::friendly(&e));
                                }
                                reload(sid, roles, everyone);
                            });
                        }
                    };
                    let above = if i > 0 { list.get(i - 1) } else { None };
                    let below = list.get(i + 1);
                    let up_ok = can_edit && above.is_some_and(|a| editable(a.position));
                    let down_ok = can_edit && below.is_some();
                    let up = swap(above);
                    let down = swap(below);
                    let perms = r.permissions;
                    let id = r.id;
                    ui! {
                        view(style = StyleRules {
                            flex_direction: Some(FlexDirection::Row),
                            align_items: Some(AlignItems::Center),
                            gap: Some(px(8.0)),
                            height: Some(px(34.0)),
                            padding_left: Some(px(6.0)),
                            padding_right: Some(px(4.0)),
                            background: row_bg(*sel == Some(Selected::Role(id))),
                            ..radius(4.0)
                        }) {
                            { dot(&r.color, 12.0) }
                            view(style = StyleRules {
                                flex_grow: Some(Tokenized::Literal(1.0)),
                                ..Default::default()
                            }) {
                                text(style = StyleRules {
                                    font_size: Some(px(14.0)),
                                    font_weight: Some(FontWeight::SemiBold),
                                    color: Some(name_color),
                                    ..Default::default()
                                }) { r.name.clone() }
                            }
                            .on_touch(move |ev| {
                                if ev.phase == TouchPhase::Ended {
                                    select(Selected::Role(id), perms);
                                }
                                TouchResponse::CONSUMED
                            })
                            view(style = StyleRules {
                                height: Some(px(22.0)),
                                padding_left: Some(px(8.0)),
                                padding_right: Some(px(8.0)),
                                align_items: Some(AlignItems::Center),
                                justify_content: Some(JustifyContent::Center),
                                background: Some(col(theme::BG_TERTIARY)),
                                ..radius(4.0)
                            }) {
                                text(style = small(theme::TEXT_MUTED)) { mention_label.to_string() }
                            }
                            .on_touch(move |ev| {
                                if ev.phase == TouchPhase::Ended && can_edit {
                                    let t = toggle.clone();
                                    runtime_core::driver::spawn_async(async move {
                                        let input = server_roles::UpdateRoleInput {
                                            role_id: t.id,
                                            name: t.name.clone(),
                                            color: t.color.clone(),
                                            mentionable: !t.mentionable,
                                            permissions: None,
                                        };
                                        match server_roles::update_role(input).await {
                                            Ok(_) => reload(sid, roles, everyone),
                                            Err(e) => error.set(crate::errors::friendly(&e)),
                                        }
                                    });
                                }
                                TouchResponse::CONSUMED
                            })
                            if manage {
                                { icon_button("▲", up_ok, up) }
                                { icon_button("▼", down_ok, down) }
                                {
                                    icon_button("×", can_edit, move || {
                                        runtime_core::driver::spawn_async(async move {
                                            match server_roles::delete_role(server_roles::DeleteRoleInput { role_id: id }).await {
                                                Ok(()) => {
                                                    if selected.get() == Some(Selected::Role(id)) {
                                                        selected.set(None);
                                                    }
                                                    reload(sid, roles, everyone);
                                                }
                                                Err(e) => error.set(crate::errors::friendly(&e)),
                                            }
                                        });
                                    })
                                }
                            }
                        }
                    }
                })
                .collect();
            // @everyone, pinned at the bottom: permissions only.
            let everyone_perms = *everyone_perms;
            rows.push(ui! {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    gap: Some(px(8.0)),
                    height: Some(px(34.0)),
                    padding_left: Some(px(6.0)),
                    background: row_bg(*sel == Some(Selected::Everyone)),
                    ..radius(4.0)
                }) {
                    { dot("", 12.0) }
                    text(style = StyleRules {
                        font_size: Some(px(14.0)),
                        font_weight: Some(FontWeight::SemiBold),
                        color: Some(col(theme::TEXT_NORMAL)),
                        flex_grow: Some(Tokenized::Literal(1.0)),
                        ..Default::default()
                    }) { "@everyone".to_string() }
                    text(style = small(theme::TEXT_MUTED)) { "every member".to_string() }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        select(Selected::Everyone, everyone_perms);
                    }
                    TouchResponse::CONSUMED
                })
            });
            ui! { view(style = StyleRules { flex_direction: Some(FlexDirection::Column), gap: Some(px(2.0)), ..Default::default() }) { rows } }
        },
    );

    // The selected entry's permissions: one switch per flag, then Save.
    let editor = switch(
        move || (selected.get(), draft.get(), roles.get()),
        move |(sel, perms, list): &(Option<Selected>, Permissions, Vec<CustomRole>)| {
            let Some(sel) = *sel else {
                return ui! { view {} };
            };
            let (title, role, can_edit) = match sel {
                Selected::Everyone => ("@everyone".to_string(), None, manage),
                Selected::Role(id) => match list.iter().find(|r| r.id == id) {
                    Some(r) => (r.name.clone(), Some(r.clone()), manage && editable(r.position)),
                    None => return ui! { view {} },
                },
            };
            let perms = *perms;
            let rows: Vec<Element> = Permissions::FLAGS
                .iter()
                // @everyone can't be made an administrator.
                .filter(|(flag, ..)| !(sel == Selected::Everyone && *flag == Permissions::ADMINISTRATOR))
                .map(|&(flag, label, about)| {
                    let on = perms.contains(flag);
                    let warn = if flag == Permissions::ADMINISTRATOR {
                        "This grants every permission and overrides channel settings. Give it only to people you trust."
                    } else {
                        ""
                    };
                    ui! {
                        view(style = StyleRules {
                            flex_direction: Some(FlexDirection::Column),
                            padding_top: Some(px(8.0)),
                            padding_bottom: Some(px(8.0)),
                            gap: Some(px(2.0)),
                            ..Default::default()
                        }) {
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
                                    text(style = StyleRules { font_size: Some(px(14.0)), color: Some(col(theme::TEXT_NORMAL)), ..Default::default() }) { label.to_string() }
                                    text(style = small(theme::TEXT_MUTED)) { about.to_string() }
                                }
                                {
                                    crate::controls::toggle(on, can_edit, move || {
                                        let now = draft.get();
                                        draft.set(if now.contains(flag) { now.minus(flag) } else { now.union(flag) });
                                    })
                                }
                            }
                            if !warn.is_empty() {
                                text(style = small(theme::WARNING)) { warn.to_string() }
                            }
                        }
                    }
                })
                .collect();
            let note = if can_edit {
                String::new()
            } else if !manage {
                "You need Manage Roles to change these.".to_string()
            } else {
                "Only roles below your highest role can be edited.".to_string()
            };
            let note_el = (!note.is_empty()).then(|| ui! { text(style = small(theme::TEXT_MUTED)) { note } });
            let save = crate::controls::button_view("Save Changes", crate::controls::ButtonKind::Primary, move || {
                let perms = draft.get();
                let role = role.clone();
                runtime_core::driver::spawn_async(async move {
                    let result = match role {
                        Some(r) => server_roles::update_role(server_roles::UpdateRoleInput {
                            role_id: r.id,
                            name: r.name,
                            color: r.color,
                            mentionable: r.mentionable,
                            permissions: Some(perms),
                        })
                        .await
                        .map(|_| ()),
                        None => permissions::set_everyone_permissions(permissions::SetEveryoneInput { server_id: sid, permissions: perms })
                            .await
                            .map(|_| ()),
                    };
                    match result {
                        Ok(()) => {
                            error.set(String::new());
                            reload(sid, roles, everyone);
                        }
                        Err(e) => error.set(crate::errors::friendly(&e)),
                    }
                });
            });
            let save_el = can_edit.then_some(save);
            ui! {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Column),
                    padding_top: Some(px(12.0)),
                    padding_bottom: Some(px(12.0)),
                    padding_left: Some(px(14.0)),
                    padding_right: Some(px(14.0)),
                    background: Some(col(theme::BG_SECONDARY)),
                    ..radius(8.0)
                }) {
                    // Save sits by the title so it stays in view above the
                    // long list.
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        gap: Some(px(12.0)),
                        ..Default::default()
                    }) {
                        text(style = StyleRules {
                            font_size: Some(px(16.0)),
                            font_weight: Some(FontWeight::SemiBold),
                            color: Some(col(theme::TEXT_STRONG)),
                            flex_grow: Some(Tokenized::Literal(1.0)),
                            ..Default::default()
                        }) { format!("Permissions — {title}") }
                        if let Some(b) = save_el {
                            b
                        }
                    }
                    if let Some(n) = note_el {
                        n
                    }
                    { rows }
                }
            }
        },
    );

    let create: Element = if manage {
        let swatches: Vec<Element> = COLORS
            .iter()
            .map(|c| {
                let c: &'static str = c;
                ui! {
                    view(style = move || {
                        let on = color.get() == c;
                        let ring = col(if on { theme::TEXT_STRONG } else { theme::BG_SECONDARY });
                        StyleRules {
                            width: Some(px(22.0)),
                            height: Some(px(22.0)),
                            align_items: Some(AlignItems::Center),
                            justify_content: Some(JustifyContent::Center),
                            border_top_width: Some(Tokenized::Literal(2.0)),
                            border_right_width: Some(Tokenized::Literal(2.0)),
                            border_bottom_width: Some(Tokenized::Literal(2.0)),
                            border_left_width: Some(Tokenized::Literal(2.0)),
                            border_top_color: Some(ring.clone()),
                            border_right_color: Some(ring.clone()),
                            border_bottom_color: Some(ring.clone()),
                            border_left_color: Some(ring),
                            ..radius(11.0)
                        }
                    }) {
                        { dot(c, 14.0) }
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            color.set(c.to_string());
                        }
                        TouchResponse::CONSUMED
                    })
                }
            })
            .collect();
        ui! {
            view(style = StyleRules { flex_direction: Some(FlexDirection::Column), gap: Some(px(8.0)), margin_bottom: Some(px(8.0)), ..Default::default() }) {
                view(style = StyleRules { flex_direction: Some(FlexDirection::Row), gap: Some(px(8.0)), ..Default::default() }) {
                    text_input(value = name, on_change = move |v: String| name.set(v), placeholder = "New role name", style = StyleRules {
                        height: Some(px(34.0)),
                        flex_grow: Some(Tokenized::Literal(1.0)),
                        flex_shrink: Some(Tokenized::Literal(1.0)),
                        min_width: Some(px(0.0)),
                        padding_left: Some(px(10.0)),
                        padding_right: Some(px(10.0)),
                        background: Some(col(theme::BG_TERTIARY)),
                        ..with_radius(borderless_input(), 8.0)
                    })
                    view(style = StyleRules {
                        height: Some(px(34.0)),
                        padding_left: Some(px(14.0)),
                        padding_right: Some(px(14.0)),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        background: Some(col(theme::ACCENT)),
                        ..radius(8.0)
                    }, test_id = "create-role") {
                        text(style = StyleRules { font_size: Some(px(14.0)), font_weight: Some(FontWeight::SemiBold), color: Some(col(theme::TEXT_ON_ACCENT)), ..Default::default() }) { "Create".to_string() }
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            let input = server_roles::CreateRoleInput {
                                server_id: sid,
                                name: name.get(),
                                color: color.get(),
                                mentionable: true,
                            };
                            runtime_core::driver::spawn_async(async move {
                                match server_roles::create_role(input).await {
                                    Ok(_) => {
                                        name.set(String::new());
                                        error.set(String::new());
                                        reload(sid, roles, everyone);
                                    }
                                    Err(e) => error.set(crate::errors::friendly(&e)),
                                }
                            });
                        }
                        TouchResponse::CONSUMED
                    })
                }
                view(style = StyleRules { flex_direction: Some(FlexDirection::Row), flex_wrap: Some(FlexWrap::Wrap), gap: Some(px(6.0)), ..Default::default() }) { swatches }
            }
        }
    } else {
        ui! { view {} }
    };

    ui! {
        view(style = StyleRules { flex_direction: Some(FlexDirection::Column), gap: Some(px(10.0)), max_width: Some(px(560.0)), ..Default::default() }) {
            text(style = small(theme::TEXT_MUTED)) {
                "Roles colour names, can be @mentioned and grant permissions. Higher roles outrank lower ones. Select a role to see its permissions; give a member a role from their profile card.".to_string()
            }
            { create }
            { list }
            text(style = StyleRules { font_size: Some(px(12.0)), color: Some(col(theme::DANGER)), ..Default::default() }) {
                move || error.get()
            }
            { editor }
        }
    }
}
