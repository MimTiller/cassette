//! The "Roles" section of Server Settings: the server's named roles (colour,
//! name, whether anyone may @mention it) with delete, and a row to create
//! one. Admins and the owner edit; everyone else sees the list. Members are
//! given roles from their profile card.

use crate::style_helpers::{borderless_input, col, px};
use crate::theme;
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

fn radius(r: f32) -> StyleRules {
    StyleRules {
        border_top_left_radius: Some(px(r)),
        border_top_right_radius: Some(px(r)),
        border_bottom_left_radius: Some(px(r)),
        border_bottom_right_radius: Some(px(r)),
        ..Default::default()
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

/// Reload the list (and the app's role colours) after a change.
fn reload(server_id: i64, roles: Signal<Vec<CustomRole>>) {
    runtime_core::driver::spawn_async(async move {
        if let Ok(snap) = server_roles::server_roles(server_roles::ServerRolesInput { server_id }).await {
            roles.set(snap.roles);
        }
        crate::mentions::load_roles(server_id);
    });
}

pub fn roles_section(server_id: Option<i64>) -> Element {
    let Some(sid) = server_id else {
        return ui! { view {} };
    };
    let manage = crate::mentions::can_manage_roles();
    let roles: Signal<Vec<CustomRole>> = signal(crate::mentions::roles().roles);
    let name = signal(String::new());
    let color = signal(String::new());
    let error = signal(String::new());
    reload(sid, roles);

    let small = |c: &'static str| StyleRules { font_size: Some(px(12.0)), color: Some(col(c)), ..Default::default() };
    let list = switch(
        move || roles.get(),
        move |list: &Vec<CustomRole>| {
            if list.is_empty() {
                return ui! { text(style = small(theme::TEXT_MUTED)) { "No roles yet.".to_string() } };
            }
            let rows: Vec<Element> = list
                .iter()
                .map(|r| {
                    let role = r.clone();
                    let toggle = role.clone();
                    let name_color = if r.color.is_empty() { col(theme::TEXT_NORMAL) } else { theme::fixed(&r.color) };
                    let mention_label = if r.mentionable { "@ anyone" } else { "@ mods only" };
                    ui! {
                        view(style = StyleRules {
                            flex_direction: Some(FlexDirection::Row),
                            align_items: Some(AlignItems::Center),
                            gap: Some(px(8.0)),
                            height: Some(px(32.0)),
                            ..Default::default()
                        }) {
                            { dot(&r.color, 12.0) }
                            text(style = StyleRules {
                                font_size: Some(px(14.0)),
                                font_weight: Some(FontWeight::SemiBold),
                                color: Some(name_color),
                                flex_grow: Some(Tokenized::Literal(1.0)),
                                ..Default::default()
                            }) { r.name.clone() }
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
                                if ev.phase == TouchPhase::Ended && manage {
                                    let t = toggle.clone();
                                    runtime_core::driver::spawn_async(async move {
                                        let input = server_roles::UpdateRoleInput {
                                            role_id: t.id,
                                            name: t.name.clone(),
                                            color: t.color.clone(),
                                            mentionable: !t.mentionable,
                                        };
                                        match server_roles::update_role(input).await {
                                            Ok(_) => reload(sid, roles),
                                            Err(e) => error.set(crate::errors::friendly(&e)),
                                        }
                                    });
                                }
                                TouchResponse::CONSUMED
                            })
                            if manage {
                                view(style = StyleRules {
                                    width: Some(px(22.0)),
                                    height: Some(px(22.0)),
                                    align_items: Some(AlignItems::Center),
                                    justify_content: Some(JustifyContent::Center),
                                    ..Default::default()
                                }) {
                                    text(style = StyleRules { font_size: Some(px(16.0)), color: Some(col(theme::TEXT_MUTED)), ..Default::default() }) { "×".to_string() }
                                }
                                .on_touch({
                                    let role = role.clone();
                                    move |ev| {
                                        if ev.phase == TouchPhase::Ended {
                                            let id = role.id;
                                            runtime_core::driver::spawn_async(async move {
                                                match server_roles::delete_role(server_roles::DeleteRoleInput { role_id: id }).await {
                                                    Ok(()) => reload(sid, roles),
                                                    Err(e) => error.set(crate::errors::friendly(&e)),
                                                }
                                            });
                                        }
                                        TouchResponse::CONSUMED
                                    }
                                })
                            }
                        }
                    }
                })
                .collect();
            ui! { view(style = StyleRules { flex_direction: Some(FlexDirection::Column), ..Default::default() }) { rows } }
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
                        border_top_left_radius: Some(px(8.0)),
                        border_top_right_radius: Some(px(8.0)),
                        border_bottom_left_radius: Some(px(8.0)),
                        border_bottom_right_radius: Some(px(8.0)),
                        ..borderless_input()
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
                                        reload(sid, roles);
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
                "Roles colour names and can be @mentioned. Give a member a role from their profile card.".to_string()
            }
            { create }
            { list }
            text(style = StyleRules { font_size: Some(px(12.0)), color: Some(col(theme::DANGER)), ..Default::default() }) {
                move || error.get()
            }
        }
    }
}
