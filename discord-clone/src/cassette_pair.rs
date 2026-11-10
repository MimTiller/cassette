//! Pairing the colour themes with the cassette icons that share their name
//! (Sunset, Miami, Memphis and Bubblegum): picking one offers the other when
//! its partner is not already chosen, and a "No thanks" rides the `storage`
//! SDK so that pair never nags again. Themes with no cassette (Game Boy, …)
//! and the Classic icon never pair, since a match needs both `ThemeName::from_key`
//! and `AppIcon::from_key` to answer for the same key.
//!
//! Dismissals are per account: the storage SDK is the browser's, and the
//! Appearance picker is a user preference, so the list is keyed by the
//! signed-in username and starts empty for a new one.

use runtime_core::{
    ui, component, AlignItems, Element, FlexDirection, Signal, StyleRules,
};
use std::rc::Rc;
use crate::style_helpers::{col, px, rounded};

/// The storage-SDK key for an account's dismissed pairs.
const KEY_PREFIX: &str = "idealyst.discord.pairDismiss:";

/// The theme's cassette, when it has one (matched by key: `sunset` theme ↔
/// `sunset` cassette; `gameboy` has none).
pub fn cassette_for(name: crate::theme::ThemeName) -> Option<crate::app_icon::AppIcon> {
    crate::app_icon::AppIcon::from_key(name.key())
}

/// The icon's theme, when it has one (Classic has none).
pub fn theme_for(icon: crate::app_icon::AppIcon) -> Option<crate::theme::ThemeName> {
    crate::theme::ThemeName::from_key(icon.key())
}

/// The inline offer text for a matching cassette.
pub fn cassette_offer_text(label: &str) -> String {
    format!("Use the matching {label} cassette too?")
}

/// The inline offer text for a matching theme.
pub fn theme_offer_text(label: &str) -> String {
    format!("Use the matching {label} theme too?")
}

/// The pure half of the dismissed-list grammar: a `|`-separated list of pair
/// keys. Split out of `load_dismissed` so it can be unit-tested on any target
/// (mirrors `parse_last_view` in `crate::auth`).
pub fn parse_dismissed(raw: &str) -> Vec<String> {
    raw.split('|').filter(|s| !s.is_empty()).map(String::from).collect()
}

/// `existing` with `pair` added once, serialized back.
pub fn with_dismissal(existing: Vec<String>, pair: &str) -> String {
    let mut out = existing;
    if !out.iter().any(|p| p == pair) {
        out.push(pair.to_string());
    }
    out.join("|")
}

/// Has the signed-in account dismissed the pair named by `pair` (a theme/cassette key)?
pub fn dismissed(pair: &str) -> bool {
    load_dismissed(crate::auth::current_username().as_str()).iter().any(|p| p == pair)
}

/// Record that the signed-in account dismissed `pair`.
pub fn dismiss(pair: &str) {
    let user = crate::auth::current_username();
    let key = key_for(user.as_str());
    let stored = with_dismissal(load_dismissed(user.as_str()), pair);
    let _ = crate::auth::store().set_now(&key, &stored);
}

fn key_for(user: &str) -> String {
    format!("{KEY_PREFIX}{user}")
}

fn load_dismissed(user: &str) -> Vec<String> {
    let key = key_for(user);
    crate::auth::store()
        .get_now(&key)
        .ok()
        .flatten()
        .map(|raw| parse_dismissed(&raw))
        .unwrap_or_else(Vec::new)
}

/// The inline offer under the theme cards: shown while `offer` names a
/// cassette whose theme is active but not chosen; `switch` applies it and
/// `no_thanks` drops the pair for good. Both clear the offer so it never
/// shows again while dismissed.
#[component]
pub fn CassetteOfferView(
    offer: Signal<Option<crate::app_icon::AppIcon>>,
    switch: Option<Rc<dyn Fn()>>,
    no_thanks: Option<Rc<dyn Fn()>>,
) -> Element {
    let (switch_cb, thanks_cb) = (switch.clone(), no_thanks.clone());
    ui! {
        match offer.get() {
            Some(c) => {{
                let switch = switch_cb.clone();
                let thanks = thanks_cb.clone();
                ui! {
                    view(
                        style = StyleRules {
                            flex_direction: Some(FlexDirection::Column),
                            align_items: Some(AlignItems::Center),
                            gap: Some(px(8.0)),
                            padding_top: Some(px(10.0)),
                            padding_bottom: Some(px(10.0)),
                            padding_left: Some(px(12.0)),
                            padding_right: Some(px(12.0)),
                            background: Some(col(crate::theme::BG_SECONDARY)),
                            ..rounded(8.0)
                        },
                        test_id = "pair-offer-icon"
                    ) {
                        text(style = StyleRules {
                            font_size: Some(px(13.0)),
                            color: Some(col(crate::theme::TEXT_NORMAL)),
                            ..Default::default()
                        }, test_id = "pair-offer-icon-text") { cassette_offer_text(c.label()) }
                        button(label = "Switch icon".to_string(), on_click = move || {
                            if let Some(f) = &switch { f.clone()(); }
                        }, test_id = "pair-switch-icon", style = crate::styles::ButtonBox().kind(crate::styles::ButtonBoxKind::Primary))
                        button(label = "No thanks".to_string(), on_click = move || {
                            if let Some(f) = &thanks { f.clone()(); }
                        }, test_id = "pair-dismiss-icon", style = crate::styles::ButtonBox().kind(crate::styles::ButtonBoxKind::Secondary))
                    }
                }
            }},
            // No offer: nothing in the settings column's flow.
            None => {}
        }
    }
}

/// The inline offer under the app-icon cards, mirrored: shown while `offer`
/// names a theme matching the chosen cassette but not the active one.
#[component]
pub fn ThemeOfferView(
    offer: Signal<Option<crate::theme::ThemeName>>,
    switch: Option<Rc<dyn Fn()>>,
    no_thanks: Option<Rc<dyn Fn()>>,
) -> Element {
    let (switch_cb, thanks_cb) = (switch.clone(), no_thanks.clone());
    ui! {
        match offer.get() {
            Some(t) => {{
                let switch = switch_cb.clone();
                let thanks = thanks_cb.clone();
                ui! {
                    view(
                        style = StyleRules {
                            flex_direction: Some(FlexDirection::Column),
                            align_items: Some(AlignItems::Center),
                            gap: Some(px(8.0)),
                            padding_top: Some(px(10.0)),
                            padding_bottom: Some(px(10.0)),
                            padding_left: Some(px(12.0)),
                            padding_right: Some(px(12.0)),
                            background: Some(col(crate::theme::BG_SECONDARY)),
                            ..rounded(8.0)
                        },
                        test_id = "pair-offer-theme"
                    ) {
                        text(style = StyleRules {
                            font_size: Some(px(13.0)),
                            color: Some(col(crate::theme::TEXT_NORMAL)),
                            ..Default::default()
                        }, test_id = "pair-offer-theme-text") { theme_offer_text(t.label()) }
                        button(label = "Switch theme".to_string(), on_click = move || {
                            if let Some(f) = &switch { f.clone()(); }
                        }, test_id = "pair-switch-theme", style = crate::styles::ButtonBox().kind(crate::styles::ButtonBoxKind::Primary))
                        button(label = "No thanks".to_string(), on_click = move || {
                            if let Some(f) = &thanks { f.clone()(); }
                        }, test_id = "pair-dismiss-theme", style = crate::styles::ButtonBox().kind(crate::styles::ButtonBoxKind::Secondary))
                    }
                }
            }},
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{ThemeName};
    use crate::app_icon::{AppIcon};

    #[test]
    fn paired_names_offer_both_ways() {
        for t in ThemeName::ALL {
            match cassette_for(t) {
                Some(c) => assert_eq!(theme_for(c), Some(t), "a cassette must map back to its theme"),
                None => assert_eq!(AppIcon::from_key(t.key()), None),
            }
        }
    }

    #[test]
    fn unpaired_names_answer_none() {
        for key in ["gameboy", "win95", "vhs", "blockbuster"] {
            assert_eq!(AppIcon::from_key(key), None, "a theme without a cassette");
        }
        assert_eq!(theme_for(AppIcon::Classic), None);
    }

    #[test]
    fn dismissal_list_round_trips() {
        assert_eq!(parse_dismissed(""), Vec::<String>::new());
        assert_eq!(parse_dismissed("miami|memphis"), vec!("miami", "memphis"));
        let after = with_dismissal(parse_dismissed("miami"), "memphis");
        assert_eq!(after, "miami|memphis");
        assert_eq!(with_dismissal(parse_dismissed(&after), "miami"), after);
    }

    #[test]
    fn offer_text_names_the_pair() {
        assert_eq!(cassette_offer_text("Miami"), "Use the matching Miami cassette too?");
        assert_eq!(theme_offer_text("Memphis"), "Use the matching Memphis theme too?");
    }
}
