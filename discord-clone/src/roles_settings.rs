//! The "Roles" section of Server Settings: the server's named roles (colour,
//! name, whether anyone may @mention it) with delete and reorder, a row to
//! create one, and @everyone pinned at the bottom. Selecting a role (or
//! @everyone) opens its permissions below the list. Anyone with Manage Roles
//! edits roles below their own; everyone else sees the list. Members are
//! given roles from their profile card.

use crate::style_helpers::{col, px, rounded};
use crate::theme;
use discord_server::permissions::Rank;
use crate::backend::CustomRole;
use crate::backend::Permissions;
use runtime_core::{
    signal, ui, AlignItems, Element, FlexDirection, FlexWrap, FontWeight, JustifyContent,
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
/// after a change. The reply lands in the caller's signals, so the fetch
/// runs in `spawn_then_in`'s future and every write in its callback,
/// anchored on the caller's scope: if that is torn down mid-flight the
/// callback is simply dropped.
fn reload(server_id: i64, roles: Signal<Vec<CustomRole>>, everyone: Signal<Permissions>, alive: runtime_core::ScopeAlive) {
    runtime_core::spawn_then_in(
        &alive,
        crate::backend::server_roles(server_id),
        move |result| {
            if let Ok(snap) = result {
                roles.set(snap.roles);
                everyone.set(snap.everyone_permissions);
            }
            crate::mentions::load_roles(server_id);
        },
    );
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

/// Robot anchors for the role list and its swatches: the colour dots by index
/// (the suite picks one and checks the created role's colour), and the
/// create-role name input.
pub fn swatch_id(color: &'static str) -> &'static str {
    match color {
        "" => "role-swatch-none",
        "#1abc9c" => "role-swatch-teal",
        "#2ecc71" => "role-swatch-green",
        "#3498db" => "role-swatch-blue",
        "#9b59b6" => "role-swatch-purple",
        "#e91e63" => "role-swatch-pink",
        "#f1c40f" => "role-swatch-yellow",
        "#e67e22" => "role-swatch-orange",
        "#e74c3c" => "role-swatch-red",
        "#95a5a6" => "role-swatch-grey",
        "#607d8b" => "role-swatch-slate",
        _ => "role-swatch-other",
    }
}

/// The section's state every list row reaches into.
#[derive(Clone)]
struct RolesCx {
    sid: i64,
    manage: bool,
    roles: Signal<Vec<CustomRole>>,
    everyone: Signal<Permissions>,
    selected: Signal<Option<Selected>>,
    draft: Signal<Permissions>,
    error: Signal<String>,
    /// Replies anchor on the section's scope: dropped if it is torn down.
    alive: runtime_core::ScopeAlive,
}

fn small(c: &'static str) -> StyleRules {
    StyleRules { font_size: Some(px(12.0)), color: Some(col(c)), ..Default::default() }
}

/// Open `which`'s permissions in the editor with `current` as the draft, or
/// close the editor when `which` is already open.
fn select(selected: Signal<Option<Selected>>, draft: Signal<Permissions>, which: Selected, current: Permissions) {
    if selected.get() == Some(which) {
        selected.set(None);
    } else {
        draft.set(current);
        selected.set(Some(which));
    }
}

/// One custom role: its colour and name (tap to open its permissions), who may
/// @mention it, and for managers the move and delete buttons. `i` is its place
/// in `list`; the arrows swap it with the neighbour above or below.
fn role_row(cx: &RolesCx, list: &[CustomRole], i: usize, sel: Option<Selected>) -> Element {
    let (sid, manage, roles, everyone, selected, draft, error) =
        (cx.sid, cx.manage, cx.roles, cx.everyone, cx.selected, cx.draft, cx.error);
    let r = &list[i];
    let role = r.clone();
    let toggle = role.clone();
    let can_edit = manage && editable(r.position);
    let name_color = if r.color.is_empty() { col(theme::TEXT_NORMAL) } else { theme::fixed(&r.color) };
    let mention_label = if r.mentionable { "@ anyone" } else { "@ mods only" };
    let al_row = cx.alive.clone();
    // Swapping with the neighbour above (`up`) or below: each takes the
    // other's position.
    let swap = |other: Option<&CustomRole>| {
        let other = other.cloned();
        let me = role.clone();
        let al_swap = al_row.clone();
        move || {
            let Some(other) = other.clone() else { return };
            let (a, b) = (me.clone(), other);
            let (pa, pb) = if a.position == b.position { (b.position + 1, a.position) } else { (b.position, a.position) };
            let al = al_swap.clone();
            runtime_core::spawn_then_in(
                &al.clone(),
                async move {
                    let first = crate::backend::move_role(a.id, pa).await;
                    match first {
                        Ok(_) => crate::backend::move_role(b.id, pb).await.map(|_| ()),
                        Err(e) => Err(e),
                    }
                },
                move |second| {
                    if let Err(e) = second {
                        error.set(e);
                    }
                    reload(sid, roles, everyone, al);
                },
            );
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
    // Each handler owns its own copy of the token (ScopeAlive is Clone, not
    // Copy).
    let al_toggle = al_row.clone();
    let al_del = al_row.clone();
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(8.0)),
            height: Some(px(34.0)),
            padding_left: Some(px(6.0)),
            padding_right: Some(px(4.0)),
            background: Some(crate::style_helpers::selected_fill(sel == Some(Selected::Role(id)), false)),
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
                    select(selected, draft, Selected::Role(id), perms);
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
                    let al = al_toggle.clone();
                    runtime_core::spawn_then_in(
                        &al.clone(),
                        crate::backend::update_role(t.id, t.name.clone(), t.color.clone(), !t.mentionable, None),
                        move |result| {
                            match result {
                                Ok(_) => reload(sid, roles, everyone, al),
                                Err(e) => error.set(e),
                            }
                        },
                    );
                }
                TouchResponse::CONSUMED
            })
            if manage {
                { icon_button("▲", up_ok, up) }
                { icon_button("▼", down_ok, down) }
                {
                    icon_button("×", can_edit, move || {
                        let al = al_del.clone();
                        runtime_core::spawn_then_in(
                            &al.clone(),
                            crate::backend::delete_role(id),
                            move |result| {
                                match result {
                                    Ok(()) => {
                                        if selected.get() == Some(Selected::Role(id)) {
                                            selected.set(None);
                                        }
                                        reload(sid, roles, everyone, al);
                                    }
                                    Err(e) => error.set(e),
                                }
                            },
                        );
                    })
                }
            }
        }
    }
}

/// @everyone, pinned under the roles: permissions only.
fn everyone_row(cx: &RolesCx, sel: Option<Selected>, everyone_perms: Permissions) -> Element {
    let (selected, draft) = (cx.selected, cx.draft);
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(8.0)),
            height: Some(px(34.0)),
            padding_left: Some(px(6.0)),
            background: Some(crate::style_helpers::selected_fill(sel == Some(Selected::Everyone), false)),
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
                select(selected, draft, Selected::Everyone, everyone_perms);
            }
            TouchResponse::CONSUMED
        })
    }
}

/// One permission in the editor: its name and description, and a switch that
/// flips it in the draft.
fn permission_row(flag: Permissions, label: &'static str, about: &'static str, on: bool, can_edit: bool, draft: Signal<Permissions>) -> Element {
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
}

/// One colour preset for a new role, ringed while it is the picked one.
fn swatch(c: &'static str, color: Signal<String>) -> Element {
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
        }, test_id = swatch_id(c)) {
            { dot(c, 14.0) }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                color.set(c.to_string());
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
    // Replies to these signals belong to this section's scope: tasks anchor
    // on it, and if it is torn down mid-flight the callback is simply dropped.
    // Handlers built per row take their own copy (ScopeAlive is Clone, not
    // Copy).
    let alive = runtime_core::ScopeAlive::current();
    reload(sid, roles, everyone, alive.clone());

    let cx = RolesCx { sid, manage, roles, everyone, selected, draft, error, alive: alive.clone() };
    let render_list =
        move |(list, everyone_perms, sel): &(Vec<CustomRole>, Permissions, Option<Selected>)| -> Element {
            let (everyone_perms, sel) = (*everyone_perms, *sel);
            let cx = cx.clone();
            ui! {
                view(style = StyleRules { flex_direction: Some(FlexDirection::Column), gap: Some(px(2.0)), ..Default::default() }) {
                    for (i, r) in list.iter().enumerate(), key = r.id {
                        { role_row(&cx, list, i, sel) }
                    }
                    { everyone_row(&cx, sel, everyone_perms) }
                }
            }
        };
    let list = ui! {
        match (roles.get(), everyone.get(), selected.get()) {
            key => {{ (render_list)(&key) }},
        }
    };

    let al_editor = alive.clone();
    // The selected entry's permissions: one switch per flag, then Save.
    let render_editor =
        move |(sel, perms, list): &(Option<Selected>, Permissions, Vec<CustomRole>)| -> Element {
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
            // @everyone can't be made an administrator.
            let flags: Vec<(Permissions, &'static str, &'static str)> = Permissions::FLAGS
                .iter()
                .copied()
                .filter(|(flag, ..)| !(sel == Selected::Everyone && *flag == Permissions::ADMINISTRATOR))
                .collect();
            let note = if can_edit {
                String::new()
            } else if !manage {
                "You need Manage Roles to change these.".to_string()
            } else {
                "Only roles below your highest role can be edited.".to_string()
            };
            let note_el = (!note.is_empty()).then(|| ui! { text(style = small(theme::TEXT_MUTED)) { note } });
            let al_save = al_editor.clone();
            let save = crate::controls::button_view("Save Changes", crate::controls::ButtonKind::Primary, move || {
                let perms = draft.get();
                let role = role.clone();
                let al = al_save.clone();
                runtime_core::spawn_then_in(
                    &al.clone(),
                    async move {
                        match role {
                            Some(r) => crate::backend::update_role(r.id, r.name, r.color, r.mentionable, Some(perms))
                            .await
                            .map(|_| ()),
                            None => crate::backend::set_everyone_permissions(sid, perms)
                                .await
                                .map(|_| ()),
                        }
                    },
                    move |result| {
                        match result {
                            Ok(()) => {
                                error.set(String::new());
                                reload(sid, roles, everyone, al);
                            }
                            Err(e) => error.set(e),
                        }
                    },
                );
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
                    for f in flags.iter(), key = f.0.0 {
                        { permission_row(f.0, f.1, f.2, perms.contains(f.0), can_edit, draft) }
                    }
                }
            }
        };
    let editor = ui! {
        match (selected.get(), draft.get(), roles.get()) {
            key => {{ (render_editor)(&key) }},
        }
    };

    let create: Element = if manage {
        let al_create = alive.clone();
        ui! {
            view(style = StyleRules { flex_direction: Some(FlexDirection::Column), gap: Some(px(8.0)), margin_bottom: Some(px(8.0)), ..Default::default() }) {
                view(style = StyleRules { flex_direction: Some(FlexDirection::Row), gap: Some(px(8.0)), ..Default::default() }) {
                    text_input(value = name, on_change = move |v: String| name.set(v), placeholder = "New role name", style = StyleRules {
                       flex_grow: Some(Tokenized::Literal(1.0)),
                       flex_shrink: Some(Tokenized::Literal(1.0)),
                       min_width: Some(px(0.0)),
                       ..crate::style_helpers::text_field_style()
                    }, test_id = "role-create-name-input")
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
                            let al = al_create.clone();
                            runtime_core::spawn_then_in(
                                &al.clone(),
                                crate::backend::create_role(sid, name.get(), color.get(), true),
                                move |result| {
                                    match result {
                                        Ok(_) => {
                                            name.set(String::new());
                                            error.set(String::new());
                                            reload(sid, roles, everyone, al);
                                        }
                                        Err(e) => error.set(e),
                                    }
                                },
                            );
                        }
                        TouchResponse::CONSUMED
                    })
                }
                view(style = StyleRules { flex_direction: Some(FlexDirection::Row), flex_wrap: Some(FlexWrap::Wrap), gap: Some(px(6.0)), ..Default::default() }) {
                    for c in COLORS.iter(), key = *c {
                        { swatch(c, color) }
                    }
                }
            }
        }
    } else {
        ui! { view {} }
    };

    ui! {
        view(style = StyleRules { flex_direction: Some(FlexDirection::Column), gap: Some(px(16.0)), max_width: Some(px(660.0)), ..Default::default() }) {
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
