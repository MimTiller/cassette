//! `profile_effects` — the "Profile Effects" settings section: a preview card,
//! the avatar decoration picker and the profile effect picker. Every choice is
//! free and saves straight away. Also [`EffectOverlay`], which the profile card
//! draws its effect with.

use crate::app_icon::svg_data_url;
use crate::avatar_chip::{avatar_chip, decoration_svg};
use crate::style_helpers::{col, pct, px, rounded, selected_fill};
use crate::theme;
use runtime_core::{
    component, signal, ui, AlignItems, Cursor, Element, FlexDirection, FlexWrap, FontWeight,
    JustifyContent, ObjectFit, Overflow, PointerEvents, Position, Signal, StyleRules, TextAlign, TouchPhase,
    TouchResponse,
};
use std::rc::Rc;

/// What the decoration picker offers: test id, label, catalog id.
static DECORATION_CHOICES: [(&str, &str, Option<&str>); 6] = [
    ("profile-decoration-none", "None", None),
    ("profile-decoration-cat-ears", "Cat ears", Some("cat_ears")),
    ("profile-decoration-cassette-reels", "Cassette reels", Some("cassette_reels")),
    ("profile-decoration-halo", "Halo", Some("halo")),
    ("profile-decoration-flower-crown", "Flower crown", Some("flower_crown")),
    ("profile-decoration-sparkles", "Sparkles", Some("sparkles")),
];

/// What the effect picker offers: test id, label, catalog id.
static EFFECT_CHOICES: [(&str, &str, Option<&str>); 4] = [
    ("profile-effect-none", "None", None),
    ("profile-effect-vhs-static", "VHS static", Some("vhs_static")),
    ("profile-effect-falling-petals", "Falling petals", Some("falling_petals")),
    ("profile-effect-starfield", "Starfield", Some("starfield")),
];

/// The preview card's width.
const PREVIEW_W: f32 = 300.0;
/// A picker tile's thumbnail box, and the avatar circle inside a decoration's
/// (the art is 1.4 times the avatar it sits on).
const THUMB_PX: f32 = 56.0;
const THUMB_AVATAR_PX: f32 = 40.0;

/// The art for a catalog id (`discord_server::profiles::PROFILE_EFFECTS`), or
/// `None` for an id this build doesn't know.
fn effect_svg(id: &str) -> Option<&'static str> {
    match id {
        "vhs_static" => Some(include_str!("../icons/vhs_static.svg")),
        "falling_petals" => Some(include_str!("../icons/falling_petals.svg")),
        "starfield" => Some(include_str!("../icons/starfield.svg")),
        _ => None,
    }
}

/// `effect`'s art over the whole of a card: the last child of a positioned box
/// with hidden overflow. Each file slices itself to the box from the top, so
/// the image fills instead of keeping the file's own shape. It takes no pointer
/// events, so the card's controls still work under it.
#[component]
pub(crate) fn EffectOverlay(#[prop(static)] effect: Signal<Option<String>>, #[prop(static)] test_id: &'static str) -> Element {
    let art = move || effect.get().as_deref().and_then(effect_svg);
    ui! {
        view(style = StyleRules {
            position: Some(Position::Absolute),
            left: Some(px(0.0)),
            top: Some(px(0.0)),
            width: Some(pct(100.0)),
            height: Some(pct(100.0)),
            pointer_events: Some(PointerEvents::None),
            ..Default::default()
        }) {
            if art().is_some() {
                image(
                    src = move || art().map(svg_data_url).unwrap_or_default(),
                    alt = "",
                    style = StyleRules {
                        width: Some(pct(100.0)),
                        height: Some(pct(100.0)),
                        object_fit: Some(ObjectFit::Fill),
                        ..Default::default()
                    },
                    test_id = test_id,
                )
            }
        }
    }
}

/// One choice in a picker: a thumbnail of its art over its label, filled while
/// `picked` holds its `value`. A `round` thumbnail shows a decoration on an
/// avatar circle; the others show an effect on a little card.
#[component]
fn ChoiceTile(
    #[prop(static)] test_id: &'static str,
    #[prop(static)] label: &'static str,
    #[prop(static)] art: Option<&'static str>,
    #[prop(static)] round: bool,
    #[prop(static)] value: Option<&'static str>,
    #[prop(static)] picked: Signal<Option<String>>,
    #[prop(static)] on_pick: Option<Rc<dyn Fn(Option<String>)>>,
) -> Element {
    let backdrop = if round { THUMB_AVATAR_PX } else { THUMB_PX };
    ui! {
        view(style = move || StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Center),
            gap: Some(px(6.0)),
            width: Some(px(88.0)),
            padding_top: Some(px(8.0)),
            padding_bottom: Some(px(8.0)),
            cursor: Some(Cursor::Pointer),
            background: Some(selected_fill(picked.get().as_deref() == value, false)),
            ..rounded(6.0)
        }, test_id = test_id) {
            view(style = StyleRules {
                position: Some(Position::Relative),
                width: Some(px(THUMB_PX)),
                height: Some(px(THUMB_PX)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                overflow: Some(Overflow::Hidden),
                ..Default::default()
            }) {
                view(style = StyleRules {
                    width: Some(px(backdrop)),
                    height: Some(px(backdrop)),
                    background: Some(col(theme::BG_USER_PANEL)),
                    ..rounded(if round { backdrop / 2.0 } else { 6.0 })
                }) {}
                if let Some(svg) = art {
                    image(
                        src = svg_data_url(svg),
                        alt = "",
                        style = StyleRules {
                            position: Some(Position::Absolute),
                            left: Some(px(0.0)),
                            top: Some(px(0.0)),
                            width: Some(px(THUMB_PX)),
                            height: Some(px(THUMB_PX)),
                            object_fit: Some(ObjectFit::Fill),
                            ..Default::default()
                        },
                    )
                }
            }
            text(style = move || StyleRules {
                font_size: Some(px(12.0)),
                text_align: Some(TextAlign::Center),
                color: Some(col(if picked.get().as_deref() == value { theme::TEXT_STRONG } else { theme::TEXT_MUTED })),
                ..Default::default()
            }) { label.to_string() }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                if let Some(pick) = &on_pick {
                    pick(value.map(str::to_string));
                }
            }
            TouchResponse::CONSUMED
        })
    }
}

#[component]
pub fn ProfileEffects() -> Element {
    let username = crate::auth::current_username();
    let decoration = signal(crate::auth::avatar_decoration_for(&username));
    let effect = signal(None::<String>);
    let error = signal(String::new());
    {
        let me = crate::auth::CURRENT_USER_ID.load(std::sync::atomic::Ordering::SeqCst);
        runtime_core::spawn_then_in(&runtime_core::ScopeAlive::current(), crate::backend::get_user_profile(me, None), move |result| {
            if let Ok(p) = result {
                decoration.set(p.avatar_decoration);
                effect.set(p.profile_effect);
            }
        });
    }
    let pick_decoration: Rc<dyn Fn(Option<String>)> = Rc::new(move |choice| {
        error.set(String::new());
        runtime_core::spawn_then_in(&crate::app::shell_alive(), crate::backend::set_avatar_decoration(choice), move |result| {
            match result {
                Ok(saved) => {
                    crate::auth::remember_avatar_decoration(&crate::auth::current_username(), saved.clone());
                    crate::mentions::names_changed();
                    decoration.set(saved);
                }
                Err(e) => error.set(e),
            }
        });
    });
    let pick_effect: Rc<dyn Fn(Option<String>)> = Rc::new(move |choice| {
        error.set(String::new());
        runtime_core::spawn_then_in(&crate::app::shell_alive(), crate::backend::set_profile_effect(choice), move |result| {
            match result {
                Ok(saved) => effect.set(saved),
                Err(e) => error.set(e),
            }
        });
    });
    let tile_row = StyleRules {
        flex_direction: Some(FlexDirection::Row),
        flex_wrap: Some(FlexWrap::Wrap),
        gap: Some(px(8.0)),
        ..Default::default()
    };
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            gap: Some(px(16.0)),
            ..Default::default()
        }) {
            view(style = StyleRules {
                position: Some(Position::Relative),
                width: Some(px(PREVIEW_W)),
                background: Some(col(theme::BG_USER_PANEL)),
                overflow: Some(Overflow::Hidden),
                ..rounded(8.0)
            }) {
                view(style = StyleRules { height: Some(px(60.0)), background: Some(col(theme::ACCENT)), ..Default::default() }) {}
                view(style = StyleRules {
                    margin_top: Some(px(-40.0)),
                    margin_left: Some(px(16.0)),
                    width: Some(px(88.0)),
                    height: Some(px(88.0)),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    background: Some(col(theme::BG_USER_PANEL)),
                    ..rounded(44.0)
                }) {
                    { avatar_chip(username.as_str(), theme::ACCENT, 80.0) }
                }
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Column),
                    padding_top: Some(px(8.0)),
                    padding_bottom: Some(px(16.0)),
                    padding_left: Some(px(16.0)),
                    padding_right: Some(px(16.0)),
                    ..Default::default()
                }) {
                    text(style = StyleRules {
                        font_size: Some(px(20.0)),
                        font_weight: Some(FontWeight::Bold),
                        color: Some(col(theme::TEXT_STRONG)),
                        ..Default::default()
                    }) { crate::auth::shown_name(username.as_str()) }
                    text(style = StyleRules {
                        font_size: Some(px(14.0)),
                        color: Some(col(theme::TEXT_MUTED)),
                        ..Default::default()
                    }) { username.clone() }
                }
                EffectOverlay(effect = effect, test_id = "profile-effect-preview")
            }
            { crate::settings_suite::settings_card("Avatar decoration".to_string(), vec![
                ui! {
                    view(style = tile_row.clone()) {
                        for choice in DECORATION_CHOICES.iter(), key = choice.0 {
                            ChoiceTile(
                                test_id = choice.0,
                                label = choice.1,
                                art = choice.2.and_then(decoration_svg),
                                round = true,
                                value = choice.2,
                                picked = decoration,
                                on_pick = Some(pick_decoration.clone()),
                            )
                        }
                    }
                }
            ]) }
            { crate::settings_suite::settings_card("Profile effect".to_string(), vec![
                ui! {
                    view(style = tile_row.clone()) {
                        for choice in EFFECT_CHOICES.iter(), key = choice.0 {
                            ChoiceTile(
                                test_id = choice.0,
                                label = choice.1,
                                art = choice.2.and_then(effect_svg),
                                round = false,
                                value = choice.2,
                                picked = effect,
                                on_pick = Some(pick_effect.clone()),
                            )
                        }
                    }
                }
            ]) }
            if !error.get().is_empty() {
                text(style = StyleRules {
                    font_size: Some(px(12.0)),
                    color: Some(col(theme::DANGER)),
                    ..Default::default()
                }) { move || error.get().clone() }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{effect_svg, DECORATION_CHOICES, EFFECT_CHOICES};
    use discord_server::profiles::{AVATAR_DECORATIONS, PROFILE_EFFECTS};

    #[test]
    fn the_pickers_offer_the_whole_catalog_and_nothing_else() {
        let offered = |choices: &[(&str, &str, Option<&'static str>)]| -> Vec<&'static str> {
            choices.iter().filter_map(|c| c.2).collect()
        };
        assert_eq!(offered(&DECORATION_CHOICES), AVATAR_DECORATIONS);
        assert_eq!(offered(&EFFECT_CHOICES), PROFILE_EFFECTS);
    }

    #[test]
    fn every_catalog_effect_has_art() {
        for id in PROFILE_EFFECTS {
            assert!(effect_svg(id).is_some(), "{id} has no art");
        }
        assert!(effect_svg("cat_ears").is_none());
    }
}
