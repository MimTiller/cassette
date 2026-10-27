//! The app's colour themes: named colour roles, with three palettes (Dark,
//! Light, Purple).
//!
//! Every UI colour comes from a role here (`BG_PRIMARY`, `TEXT_MUTED`,
//! `ACCENT`, …) instead of a per-file hex constant. A role's constant is its
//! value in the Dark palette, and doubles as the role's identity: each role
//! is an idealyst theme token, `style_helpers::col` turns a role constant
//! into a reference to its token, and [`live`] turns it into the active
//! theme's colour for icons. So `col(theme::ACCENT)` and
//! `live(theme::ACCENT)` follow whichever theme is active, and switching
//! themes ([`set_theme_name`]) or the accent ([`set_accent`]) recolours the
//! app at once. A colour that is not a role (an avatar or role colour, a
//! swatch) stays a literal.
//!
//! Role constants must have distinct values (a unit test checks), which is
//! what lets a value name its role.

use std::cell::Cell;

use runtime_core::{active_theme, install_theme, set_theme, Color, ThemeTokens, TokenEntry, TokenValue, Tokenized};

// ── backgrounds ─────────────────────────────────────────────────────────────
/// The chat area and main panes.
pub const BG_PRIMARY: &str = "#313338";
/// Sidebars, cards, panels.
pub const BG_SECONDARY: &str = "#2b2d31";
/// The server rail, text inputs, the darkest frames.
pub const BG_TERTIARY: &str = "#1e1f22";
/// Floating menus and dropdowns.
pub const BG_FLOATING: &str = "#111214";
/// The message bar and other raised input surfaces.
pub const BG_INPUT: &str = "#383a40";
/// The user panel at the bottom of the channel sidebar; profile cards.
pub const BG_USER_PANEL: &str = "#232428";
/// A hovered row or button.
pub const BG_HOVER: &str = "#35373c";
/// A selected row (the open channel, the active settings section).
pub const BG_SELECTED: &str = "#404249";
/// A soft accent fill (a reaction you added).
pub const BG_ACCENT_SUBTLE: &str = "#363b63";

// ── text ────────────────────────────────────────────────────────────────────
/// Names, headings and most text.
pub const TEXT_STRONG: &str = "#f2f3f5";
/// Secondary body text.
pub const TEXT_NORMAL: &str = "#dbdee1";
/// Timestamps, labels, inactive items, icons.
pub const TEXT_MUTED: &str = "#949ba4";
/// Placeholders, offline status, the faintest text.
pub const TEXT_FAINT: &str = "#80848e";
/// Interactive icons at rest.
pub const INTERACTIVE: &str = "#b5bac1";
/// Links.
pub const TEXT_LINK: &str = "#00a8fc";
/// Text on an accent, danger or success fill.
pub const TEXT_ON_ACCENT: &str = "#ffffff";

// ── accent and status ───────────────────────────────────────────────────────
/// Brand accent: primary buttons, selection, mentions.
pub const ACCENT: &str = "#5865f2";
/// A hovered accent button.
pub const ACCENT_HOVER: &str = "#4752c4";
/// Neutral secondary buttons and borders.
pub const BUTTON_SECONDARY: &str = "#4e5058";
/// Divider lines.
pub const DIVIDER: &str = "#3f4147";
/// Errors, danger text and icons, unread badges.
pub const DANGER: &str = "#f23f43";
/// Danger buttons.
pub const DANGER_STRONG: &str = "#da373c";
/// Online, success, confirmations.
pub const SUCCESS: &str = "#23a559";
/// Success / confirm buttons.
pub const SUCCESS_STRONG: &str = "#248046";
/// Idle, warnings.
pub const WARNING: &str = "#f0b232";
/// Drop shadows.
pub const SHADOW: &str = "#000000";
/// The dimming layer behind dialogs.
pub const OVERLAY: &str = "rgba(0, 0, 0, 0.6)";

/// Every role: (role constant, token name). Palettes list values in this
/// order.
const ROLES: [(&str, &str); 27] = [
    (BG_PRIMARY, "color-bg-primary"),
    (BG_SECONDARY, "color-bg-secondary"),
    (BG_TERTIARY, "color-bg-tertiary"),
    (BG_FLOATING, "color-bg-floating"),
    (BG_INPUT, "color-bg-input"),
    (BG_USER_PANEL, "color-bg-user-panel"),
    (BG_HOVER, "color-bg-hover"),
    (BG_SELECTED, "color-bg-selected"),
    (BG_ACCENT_SUBTLE, "color-bg-accent-subtle"),
    (TEXT_STRONG, "color-text-strong"),
    (TEXT_NORMAL, "color-text-normal"),
    (TEXT_MUTED, "color-text-muted"),
    (TEXT_FAINT, "color-text-faint"),
    (INTERACTIVE, "color-interactive"),
    (TEXT_LINK, "color-text-link"),
    (TEXT_ON_ACCENT, "color-text-on-accent"),
    (ACCENT, "color-accent"),
    (ACCENT_HOVER, "color-accent-hover"),
    (BUTTON_SECONDARY, "color-button-secondary"),
    (DIVIDER, "color-divider"),
    (DANGER, "color-danger"),
    (DANGER_STRONG, "color-danger-strong"),
    (SUCCESS, "color-success"),
    (SUCCESS_STRONG, "color-success-strong"),
    (WARNING, "color-warning"),
    (SHADOW, "color-shadow"),
    (OVERLAY, "color-overlay"),
];
const ACCENT_INDEX: usize = 16;

/// Discord's dark palette (the role constants themselves).
const DARK: [&str; 27] = [
    BG_PRIMARY, BG_SECONDARY, BG_TERTIARY, BG_FLOATING, BG_INPUT, BG_USER_PANEL, BG_HOVER, BG_SELECTED,
    BG_ACCENT_SUBTLE, TEXT_STRONG, TEXT_NORMAL, TEXT_MUTED, TEXT_FAINT, INTERACTIVE, TEXT_LINK,
    TEXT_ON_ACCENT, ACCENT, ACCENT_HOVER, BUTTON_SECONDARY, DIVIDER, DANGER, DANGER_STRONG, SUCCESS,
    SUCCESS_STRONG, WARNING, SHADOW, OVERLAY,
];

/// "Ash": a soft light theme of mid greys rather than white, with dark grey
/// text, so it is easy on the eyes.
const LIGHT: [&str; 27] = [
    "#e9eaed", // bg primary
    "#dcdee2", // bg secondary
    "#cdd0d5", // bg tertiary
    "#f4f5f7", // bg floating (menus stand out a little lighter)
    "#d5d8dc", // bg input
    "#d1d4d9", // bg user panel
    "#d3d5da", // bg hover
    "#c6c9cf", // bg selected
    "#d3d6f2", // bg accent subtle
    "#1e2023", // text strong
    "#2f3236", // text normal
    "#5b5f67", // text muted
    "#767a83", // text faint
    "#4b4f56", // interactive
    "#0064d8", // text link
    "#ffffff", // text on accent
    "#5865f2", // accent
    "#4752c4", // accent hover
    "#a9adb5", // button secondary
    "#c2c5cb", // divider
    "#d23c3f", // danger
    "#bd2f32", // danger strong
    "#1d8a4b", // success
    "#187640", // success strong
    "#b8770b", // warning
    "#000000", // shadow
    "rgba(20, 22, 26, 0.35)", // overlay
];

/// "Dark Purple": dark backgrounds with a violet tint and a lavender accent.
const PURPLE: [&str; 27] = [
    "#2a2633", // bg primary
    "#221e2b", // bg secondary
    "#18151f", // bg tertiary
    "#120f18", // bg floating
    "#332e3e", // bg input
    "#1d1a25", // bg user panel
    "#322c3d", // bg hover
    "#3d3549", // bg selected
    "#3b2f5c", // bg accent subtle
    "#f1edf7", // text strong
    "#ddd6e8", // text normal
    "#a198b0", // text muted
    "#807790", // text faint
    "#c1b8cf", // interactive
    "#b89cff", // text link
    "#ffffff", // text on accent
    "#9b6bff", // accent
    "#8455eb", // accent hover
    "#4e465c", // button secondary
    "#3d3647", // divider
    "#f2556a", // danger
    "#d63c55", // danger strong
    "#3dbb7a", // success
    "#2b9a60", // success strong
    "#f0b232", // warning
    "#000000", // shadow
    "rgba(10, 5, 20, 0.65)", // overlay
];

/// The themes the user can pick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeName {
    Dark,
    Light,
    Purple,
}

impl ThemeName {
    pub const ALL: [ThemeName; 3] = [ThemeName::Dark, ThemeName::Light, ThemeName::Purple];

    /// The stored name ("dark" / "light" / "purple").
    pub fn key(self) -> &'static str {
        match self {
            ThemeName::Dark => "dark",
            ThemeName::Light => "light",
            ThemeName::Purple => "purple",
        }
    }

    /// The name shown in Settings.
    pub fn label(self) -> &'static str {
        match self {
            ThemeName::Dark => "Dark",
            ThemeName::Light => "Light",
            ThemeName::Purple => "Dark Purple",
        }
    }

    pub fn from_key(key: &str) -> Option<ThemeName> {
        ThemeName::ALL.into_iter().find(|t| t.key() == key)
    }

    fn palette(self) -> &'static [&'static str; 27] {
        match self {
            ThemeName::Dark => &DARK,
            ThemeName::Light => &LIGHT,
            ThemeName::Purple => &PURPLE,
        }
    }

    /// This theme's value for a role (by index), for previews.
    pub fn value(self, role: &str) -> &'static str {
        role_index(role).map_or(BG_PRIMARY, |i| self.palette()[i])
    }
}

/// The active theme: a palette, and the user's accent if they picked one.
#[derive(Clone)]
struct Theme {
    name: ThemeName,
    accent: Option<String>,
}

impl Theme {
    fn value(&self, i: usize) -> &str {
        match (&self.accent, i) {
            (Some(a), ACCENT_INDEX) => a.as_str(),
            _ => self.name.palette()[i],
        }
    }
}

impl ThemeTokens for Theme {
    fn tokens(&self) -> Vec<TokenEntry> {
        let mut out: Vec<TokenEntry> = ROLES
            .iter()
            .enumerate()
            .map(|(i, (_, name))| TokenEntry { name, value: TokenValue::Color(Color(self.value(i).into())) })
            .collect();
        // The framework's host-surface tokens: the page behind the app and
        // the platform scrollbar.
        for (name, i) in [("color-background", 2), ("color-border-strong", 2)] {
            out.push(TokenEntry { name, value: TokenValue::Color(Color(self.value(i).into())) });
        }
        out.push(TokenEntry { name: "color-surface-alt", value: TokenValue::Color(Color("transparent".into())) });
        out
    }
}

thread_local! {
    static CURRENT: Cell<ThemeName> = const { Cell::new(ThemeName::Dark) };
}

/// Install the saved theme and accent (call once before the first render).
pub fn install(name: Option<ThemeName>, accent: Option<String>) {
    let name = name.unwrap_or(ThemeName::Dark);
    CURRENT.with(|c| c.set(name));
    install_theme(Theme { name, accent });
}

/// The active theme's name.
pub fn current() -> ThemeName {
    CURRENT.with(|c| c.get())
}

/// Switch themes. The accent goes back to the theme's own.
pub fn set_theme_name(name: ThemeName) {
    CURRENT.with(|c| c.set(name));
    set_theme(Theme { name, accent: None });
}

/// Recolour the accent within the active theme.
pub fn set_accent(accent: &str) {
    set_theme(Theme { name: current(), accent: Some(accent.to_string()) });
}

fn role_index(value: &str) -> Option<usize> {
    ROLES.iter().position(|(v, _)| v.eq_ignore_ascii_case(value))
}

/// The style value for colour `value`: its role's token when it is a role
/// constant, otherwise the literal colour.
pub fn color(value: &str) -> Tokenized<Color> {
    match role_index(value) {
        Some(i) => Tokenized::token(ROLES[i].1, Color(ROLES[i].0.into())),
        None => Tokenized::Literal(Color(value.into())),
    }
}

/// A colour that never follows the theme (swatches, theme previews).
pub fn fixed(value: &str) -> Tokenized<Color> {
    Tokenized::Literal(Color(value.into()))
}

/// The active theme's colour for `value` (a role constant; anything else is
/// returned as is), for places that take a plain `Color` such as icons.
/// Reading it inside a reactive closure re-runs the closure on a theme
/// change.
pub fn live(value: &str) -> Color {
    let Some(i) = role_index(value) else { return Color(value.into()) };
    let theme = active_theme();
    match theme.downcast_ref::<Theme>() {
        Some(t) => Color(t.value(i).into()),
        None => Color(ROLES[i].0.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_values_are_distinct() {
        for (i, (a, _)) in ROLES.iter().enumerate() {
            for (b, _) in &ROLES[i + 1..] {
                assert!(!a.eq_ignore_ascii_case(b), "two roles share {a}");
            }
        }
    }

    #[test]
    fn dark_is_the_role_constants_and_accent_index_is_accent() {
        for (i, (v, _)) in ROLES.iter().enumerate() {
            assert_eq!(DARK[i], *v);
        }
        assert_eq!(ROLES[ACCENT_INDEX].0, ACCENT);
    }

    #[test]
    fn roles_resolve_to_tokens_and_others_stay_literal() {
        assert!(matches!(color(ACCENT), Tokenized::Token { name: "color-accent", .. }));
        assert!(matches!(color("#5865F2"), Tokenized::Token { name: "color-accent", .. }));
        assert!(matches!(color("#123456"), Tokenized::Literal(_)));
    }

    #[test]
    fn theme_names_round_trip() {
        for t in ThemeName::ALL {
            assert_eq!(ThemeName::from_key(t.key()), Some(t));
        }
        assert_eq!(ThemeName::Light.value(BG_PRIMARY), "#e9eaed");
    }
}
