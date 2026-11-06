//! The app's colour themes: named colour roles, with seven palettes (Dark,
//! Light, Purple, Sunset, Miami, Memphis, Bubblegum).
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
/// A mention pill's background.
pub const MENTION_BG: &str = "rgba(88, 101, 242, 0.3)";
/// A mention pill's text.
pub const MENTION_TEXT: &str = "#c9cdfb";
/// The background of a message that mentions you.
pub const MENTION_ROW: &str = "rgba(240, 178, 50, 0.1)";

/// Every role: (role constant, token name). Palettes list values in this
/// order.
const ROLES: [(&str, &str); 30] = [
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
    (MENTION_BG, "color-mention-bg"),
    (MENTION_TEXT, "color-mention-text"),
    (MENTION_ROW, "color-mention-row"),
];
const ACCENT_INDEX: usize = 16;

/// Discord's dark palette (the role constants themselves).
const DARK: [&str; 30] = [
    BG_PRIMARY, BG_SECONDARY, BG_TERTIARY, BG_FLOATING, BG_INPUT, BG_USER_PANEL, BG_HOVER, BG_SELECTED,
    BG_ACCENT_SUBTLE, TEXT_STRONG, TEXT_NORMAL, TEXT_MUTED, TEXT_FAINT, INTERACTIVE, TEXT_LINK,
    TEXT_ON_ACCENT, ACCENT, ACCENT_HOVER, BUTTON_SECONDARY, DIVIDER, DANGER, DANGER_STRONG, SUCCESS,
    SUCCESS_STRONG, WARNING, SHADOW, OVERLAY, MENTION_BG, MENTION_TEXT, MENTION_ROW,
];

/// "Ash": a soft light theme of mid greys rather than white, with dark grey
/// text, so it is easy on the eyes.
const LIGHT: [&str; 30] = [
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
    "rgba(88, 101, 242, 0.15)", // mention bg
    "#3c47c0", // mention text
    "rgba(184, 119, 11, 0.12)", // mention row
];

/// "Dark Purple": dark backgrounds with a violet tint and a lavender accent.
const PURPLE: [&str; 30] = [
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
    "rgba(155, 107, 255, 0.3)", // mention bg
    "#ddd0ff", // mention text
    "rgba(240, 178, 50, 0.1)", // mention row
];

/// "Sunset": synthwave — deep indigo surfaces, magenta and orange accents,
/// a warm pink mention pill.
const SUNSET: [&str; 30] = [
    "#241a3e", // bg primary
    "#1c1433", // bg secondary
    "#120c24", // bg tertiary
    "#0c0819", // bg floating
    "#2c2149", // bg input
    "#171028", // bg user panel
    "#2e2452", // bg hover
    "#3a2e63", // bg selected
    "#3d2454", // bg accent subtle
    "#ffedf7", // text strong
    "#e8dce9", // text normal
    "#b0a3c0", // text muted
    "#8a7e9a", // text faint
    "#d8c6d8", // interactive
    "#ff9cf0", // text link
    "#ffffff", // text on accent
    "#c93a86", // accent
    "#a82e6e", // accent hover
    "#3d2c55", // button secondary
    "#332a4e", // divider
    "#ff6b7a", // danger
    "#d92e4f", // danger strong
    "#3fae6c", // success
    "#1e7547", // success strong
    "#ff8d3d", // warning
    "#000000", // shadow
    "rgba(8, 4, 16, 0.65)", // overlay
    "rgba(255, 110, 160, 0.25)", // mention bg
    "#ffb3d1", // mention text
    "rgba(255, 138, 77, 0.12)", // mention row
];

/// "Miami": Miami Vice — dark teal surfaces, hot pink accent and aqua
/// secondary, white text.
const MIAMI: [&str; 30] = [
    "#0e3138", // bg primary
    "#0a272e", // bg secondary
    "#071d22", // bg tertiary
    "#051519", // bg floating
    "#14414a", // bg input
    "#09252b", // bg user panel
    "#16474f", // bg hover
    "#1d5560", // bg selected
    "#1a3a44", // bg accent subtle
    "#ffffff", // text strong
    "#e8f2f0", // text normal
    "#8fb5b2", // text muted
    "#6e8b89", // text faint
    "#cfe9e6", // interactive
    "#3ee3db", // text link
    "#ffffff", // text on accent
    "#d8127e", // accent
    "#b00f66", // accent hover
    "#22505c", // button secondary
    "#1b4a52", // divider
    "#ff6b7a", // danger
    "#d12550", // danger strong
    "#35b56e", // success
    "#177f47", // success strong
    "#ffd27a", // warning
    "#000000", // shadow
    "rgba(4, 12, 16, 0.65)", // overlay
    "rgba(216, 18, 126, 0.3)", // mention bg
    "#f9b9da", // mention text
    "rgba(62, 227, 219, 0.12)", // mention row
];

/// "Memphis": 90s Memphis design — cream surfaces, bold primary accents
/// (red accent, yellow warning, blue links), near-black text and outlines.
const MEMPHIS: [&str; 30] = [
    "#f7f1de", // bg primary
    "#efe6cf", // bg secondary
    "#e6dbbc", // bg tertiary
    "#d8ccab", // bg floating
    "#fdf9ec", // bg input
    "#efe3c6", // bg user panel
    "#ecdfc0", // bg hover
    "#e2d4ac", // bg selected
    "#f3d9c9", // bg accent subtle
    "#1c1a17", // text strong
    "#2c2925", // text normal
    "#6d665c", // text muted
    "#8a8178", // text faint
    "#2e2a26", // interactive
    "#1565c8", // text link
    "#ffffff", // text on accent
    "#c62828", // accent
    "#9e1d1d", // accent hover
    "#e4d9c0", // button secondary
    "#5e5a52", // divider
    "#b3181e", // danger
    "#8c1115", // danger strong
    "#157f3f", // success
    "#0f5e2f", // success strong
    "#d9a400", // warning
    "#141210", // shadow
    "rgba(28, 26, 23, 0.35)", // overlay
    "rgba(240, 204, 92, 0.5)", // mention bg
    "#4d3908", // mention text
    "rgba(240, 204, 92, 0.25)", // mention row
];

/// "Bubblegum": a soft light theme in pastel pink and lavender with a deeper
/// pink accent.
const BUBBLEGUM: [&str; 30] = [
    "#fae9f2", // bg primary
    "#f5dfec", // bg secondary
    "#efd4e5", // bg tertiary
    "#e8daef", // bg floating
    "#ffffff", // bg input
    "#f7e4f2", // bg user panel
    "#f1dbeb", // bg hover
    "#ebd2ea", // bg selected
    "#e6d7f0", // bg accent subtle
    "#42213c", // text strong
    "#533049", // text normal
    "#7c5e75", // text muted
    "#9a8295", // text faint
    "#7c5a72", // interactive
    "#7a4fae", // text link
    "#ffffff", // text on accent
    "#c9356b", // accent
    "#a62a58", // accent hover
    "#e4d2e8", // button secondary
    "#c6a8c2", // divider
    "#ba2f42", // danger
    "#991f30", // danger strong
    "#20794f", // success
    "#175e3d", // success strong
    "#8c6458", // warning
    "#4b2545", // shadow
    "rgba(66, 33, 60, 0.35)", // overlay
    "rgba(236, 178, 220, 0.7)", // mention bg
    "#4d1d40", // mention text
    "rgba(236, 178, 220, 0.35)", // mention row
];

/// The themes the user can pick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeName {
    Dark,
    Light,
    Purple,
    Sunset,
    Miami,
    Memphis,
    Bubblegum,
}

impl ThemeName {
    pub const ALL: [ThemeName; 7] = [
        ThemeName::Dark,
        ThemeName::Light,
        ThemeName::Purple,
        ThemeName::Sunset,
        ThemeName::Miami,
        ThemeName::Memphis,
        ThemeName::Bubblegum,
    ];

    /// The stored name ("dark" / "light" / "purple" / "sunset" / "miami" / "memphis" / "bubblegum").
    pub fn key(self) -> &'static str {
        match self {
            ThemeName::Dark => "dark",
            ThemeName::Light => "light",
            ThemeName::Purple => "purple",
            ThemeName::Sunset => "sunset",
            ThemeName::Miami => "miami",
            ThemeName::Memphis => "memphis",
            ThemeName::Bubblegum => "bubblegum",
        }
    }

    /// The name shown in Settings.
    pub fn label(self) -> &'static str {
        match self {
            ThemeName::Dark => "Dark",
            ThemeName::Light => "Light",
            ThemeName::Purple => "Dark Purple",
            ThemeName::Sunset => "Sunset",
            ThemeName::Miami => "Miami",
            ThemeName::Memphis => "Memphis",
            ThemeName::Bubblegum => "Bubblegum",
        }
    }

    pub fn from_key(key: &str) -> Option<ThemeName> {
        ThemeName::ALL.into_iter().find(|t| t.key() == key)
    }

    fn palette(self) -> &'static [&'static str; 30] {
        match self {
            ThemeName::Dark => &DARK,
            ThemeName::Light => &LIGHT,
            ThemeName::Purple => &PURPLE,
            ThemeName::Sunset => &SUNSET,
            ThemeName::Miami => &MIAMI,
            ThemeName::Memphis => &MEMPHIS,
            ThemeName::Bubblegum => &BUBBLEGUM,
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
