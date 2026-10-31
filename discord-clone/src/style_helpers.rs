//! Short constructors so component files read as property lists,
//! not walls of `Some(Tokenized::Literal(...))`.

use crate::theme;

use runtime_core::{AlignItems, Color, FlexDirection, FontWeight, JustifyContent, Length, Shadow, StyleRules, TextTransform, Tokenized};

/// Rounded corners: all four radii `r` and nothing else, for use as a
/// struct-update base (`StyleRules { …, ..rounded(8.0) }`).
pub fn rounded(r: f32) -> StyleRules {
    with_radius(StyleRules::default(), r)
}

/// `rules` with all four corner radii set to `r`, for a literal whose base
/// is something else (`with_radius(StyleRules { …, ..borderless_input() }, 8.0)`).
pub fn with_radius(rules: StyleRules, r: f32) -> StyleRules {
    StyleRules {
        border_top_left_radius: Some(px(r)),
        border_top_right_radius: Some(px(r)),
        border_bottom_left_radius: Some(px(r)),
        border_bottom_right_radius: Some(px(r)),
        ..rules
    }
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

/// The one "this is the current one" row fill used by every nav list: the
/// server rail's channel list, the DM list, settings section tabs, dropdown
/// and picker current items. A selected row takes `BG_SELECTED`; a hovered
/// row is one step lighter (`BG_HOVER`, so hover never outshines selection);
/// anything else carries no fill. The rail's white edge pill stays Discord's
/// own signal and is drawn by the caller.
pub fn selected_fill(on: bool, hovered: bool) -> Tokenized<Color> {
    col(if on { theme::BG_SELECTED } else if hovered { theme::BG_HOVER } else { "transparent" })
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

/// The app's single canonical **modal-card** surface: one elevation
/// treatment for every full-screen modal (Create Channel, Invite, Join a
/// Server, Create a Server, rename / delete confirms, channel permissions,
/// the avatar crop dialog). A `BG_FLOATING` card lifted off the OVERLAY
/// scrim by a real drop shadow (SHADOW role, `0 8px 16px`) and radius 8, so
/// modals read as floating cards, never as punched holes in the page.
/// Callers struct-update off this to add their own `width`/`height`
/// overrides, `padding_*` etc. that live on the card itself; the scrim
/// (`col(theme::OVERLAY)` full-window wrapper) and the header/footer rows
/// are the callers' visible structure around this.
pub fn modal_card(width: f32) -> StyleRules {
    with_radius(
        StyleRules {
            position: Some(runtime_core::Position::Relative),
            width: Some(px(width)),
            background: Some(col(theme::BG_FLOATING)),
            shadow: Some(Shadow { x: 0.0, y: 8.0, blur: 16.0, color: col(theme::SHADOW) }),
            flex_direction: Some(FlexDirection::Column),
            ..Default::default()
        },
        8.0,
    )
}

/// The **footer strip** inside a modal card: the row of action buttons sits
/// on one step darker than the card (`BG_SECONDARY` — darker than
/// `BG_FLOATING` in every palette) so the buttons read as the modal's own
/// action band, not as loose pills on the card face. A flex row, right
/// aligned; a secondary button comes in left of the primary. Callers add
/// their `margin_*`/`padding_*` on top.
pub fn footer_strip() -> StyleRules {
    with_radius(
        StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::FlexEnd),
            gap: Some(px(8.0)),
            padding_top: Some(px(12.0)),
            padding_bottom: Some(px(16.0)),
            padding_left: Some(px(16.0)),
            padding_right: Some(px(16.0)),
            background: Some(col(theme::BG_SECONDARY)),
            ..Default::default()
        },
        8.0,
    )
}

/// The single **popover** (floating menu) surface: the emoji / GIF picker
/// cards, the profile card, context menus and the server dropdown all share
/// this — `BG_FLOATING` fill, radius 8, the same drop shadow as the modal
/// cards, and a 1px `DIVIDER` edge so a menu keeps its boundary even over a
/// near-identical fill. Callers struct-update off this for their own
/// `width`/`height`/`position`.
pub fn popover_panel() -> StyleRules {
    with_radius(
        StyleRules {
            flex_direction: Some(FlexDirection::Column),
            background: Some(col(theme::BG_FLOATING)),
            shadow: Some(Shadow { x: 0.0, y: 8.0, blur: 16.0, color: col(theme::SHADOW) }),
            border_top_width: Some(Tokenized::Literal(1.0)),
            border_right_width: Some(Tokenized::Literal(1.0)),
            border_bottom_width: Some(Tokenized::Literal(1.0)),
            border_left_width: Some(Tokenized::Literal(1.0)),
            border_top_color: Some(col(theme::DIVIDER)),
            border_right_color: Some(col(theme::DIVIDER)),
            border_bottom_color: Some(col(theme::DIVIDER)),
            border_left_color: Some(col(theme::DIVIDER)),
            ..Default::default()
        },
        8.0,
    )
}

/// The app's single canonical **text-field** style: every typed input
/// (login, sign-up, role name, emoji name, invite code, Join a Server,
/// DM search, settings fields) shares this treatment — a 1px DIVIDER edge
/// so the field reads as a field on any fill, `BG_INPUT` panel, a pinned
/// caret visible on the dark fill, and one height / horizontal padding /
/// corner radius. Callers override only what their row needs
/// (`flex_grow`, `align_self`, a `margin_*`) by struct-updating off this.
/// The framework has no author focus-state hook on a plain `StyleRules`,
/// so the edge stays DIVIDER at rest (see `style_helpers::borderless_input`
/// for the UA-border zeroing this replaces with a real border).
pub fn text_field_style() -> StyleRules {
    with_radius(
        StyleRules {
            height: Some(px(34.0)),
            padding_left: Some(px(10.0)),
            padding_right: Some(px(10.0)),
            background: Some(col(theme::BG_INPUT)),
            color: Some(col(theme::TEXT_STRONG)),
            caret_color: Some(col(theme::TEXT_STRONG)),
            border_top_width: Some(Tokenized::Literal(1.0)),
            border_right_width: Some(Tokenized::Literal(1.0)),
            border_bottom_width: Some(Tokenized::Literal(1.0)),
            border_left_width: Some(Tokenized::Literal(1.0)),
            border_top_color: Some(col(theme::DIVIDER)),
            border_right_color: Some(col(theme::DIVIDER)),
            border_bottom_color: Some(col(theme::DIVIDER)),
            border_left_color: Some(col(theme::DIVIDER)),
            ..Default::default()
        },
        8.0,
    )
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
