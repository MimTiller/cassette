//! Short constructors so component files read as property lists,
//! not walls of `Some(Tokenized::Literal(...))`.

use crate::theme;
use std::rc::Rc;

use runtime_core::{Color, FontWeight, Length, StyleRules, StyleSheet, TextTransform, Tokenized};

/// A named static sheet. The identity feeds `premint_as`, so every
/// welcome style ships as build-time CSS under `--premint` instead of
/// dragging the live style engine in (an ANONYMOUS static sheet has no
/// premint class by construction — measured as 16 fall-throughs on the
/// website's premint report, all from this helper).
///
/// Contract: the id must be unique per rule CONTENT (parameterized
/// sheets bake the parameter into the id, e.g. `planet.2`), and every
/// sheet must be constructed at MOUNT time — both hold here because the
/// welcome scene builds its entire tree at load, which is also when the
/// premint dump's crawl captures it.
#[allow(dead_code)]
pub fn static_sheet(id: &str, rules: StyleRules) -> Rc<StyleSheet> {
    StyleSheet::r#static(rules).premint_as(&format!("welcome.v1.{id}"))
}

pub fn px(v: f32) -> Tokenized<Length> {
    Tokenized::Literal(Length::Px(v))
}

pub fn pct(v: f32) -> Tokenized<Length> {
    Tokenized::Literal(Length::Percent(v))
}

/// A colour for a style: a theme role's token when `s` is a role value
/// (see `crate::theme`), otherwise the literal colour.
pub fn col(s: &str) -> Tokenized<Color> {
    crate::theme::color(s)
}

/// Border-zeroing fragment for text inputs.
///
/// The browser UA stylesheet gives `<input type="text">` (and `<textarea>`) a
/// chunky `2px inset` gray border, but the native backends render the framework
/// `TextInput` primitive borderless — the box is just its own background. Left
/// alone, a web input therefore shows a UA border that no other backend has.
///
/// Use as a struct-update base in place of `..Default::default()` on an input's
/// `StyleRules`:
///
/// ```ignore
/// let input_style = StyleRules {
///     height: Some(px(40.0)),
///     background: Some(col(COMPOSER)),
///     border_top_left_radius: Some(px(8.0)),
///     // …
///     ..borderless_input()
/// };
/// ```
///
/// The struct-update copies the four `border_*_width: 0` fields from this base
/// (overriding `Default`'s `None`) while every field the caller sets explicitly
/// still wins. An author-origin `border-<side>-width: 0` beats the UA-origin
/// `border-width: 2px`, so the inset border disappears without touching the
/// input's own background / caret / box-shadow styling.
pub fn borderless_input() -> StyleRules {
    StyleRules {
        border_top_width: Some(Tokenized::Literal(0.0)),
        border_right_width: Some(Tokenized::Literal(0.0)),
        border_bottom_width: Some(Tokenized::Literal(0.0)),
        border_left_width: Some(Tokenized::Literal(0.0)),
        ..Default::default()
    }
}

/// The app's single canonical **section-label** style — the small, uppercase,
/// muted heading that labels a group of rows or controls inside a panel
/// ("Banned members", "Server name", "TEXT CHANNELS", "Direct Messages", the
/// member-panel status group headers, …).
///
/// Real Discord renders these uniformly: 11px, semibold, muted, uppercased,
/// with a slight (+0.5px) letter-spacing. This helper is the one place that
/// treatment lives so every panel built by a different author renders the same
/// scale — instead of the four ad-hoc `StyleRules` blocks this used to be
/// (12px no-uppercase in the moderation panel, 11px no-letter-spacing in the
/// category / DM / member headers, 10px −0.5 in the live-member header).
/// Callers that need extra spacing keep their own `margin_*`/`padding_*` on
/// top of this; the label's TYPE is always the same.
pub fn section_label() -> StyleRules {
    StyleRules {
        font_size: Some(px(11.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(theme::TEXT_MUTED)),
        text_transform: Some(TextTransform::Uppercase),
        letter_spacing: Some(Tokenized::Literal(0.5)),
        ..Default::default()
    }
}
