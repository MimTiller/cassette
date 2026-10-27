//! The app's colour theme: Discord's dark palette as named roles.
//!
//! Every UI colour comes from a role here (`BG_PRIMARY`, `TEXT_MUTED`,
//! `ACCENT`, …) instead of a per-file hex constant. Each role is also an
//! idealyst theme token: [`install`] registers the tokens at startup, and
//! `style_helpers::col` turns a role's value into a reference to its token
//! (`Tokenized::token`), so a style written as `col(theme::ACCENT)` follows
//! the theme when it changes at runtime ([`set_accent`] recolours the
//! accent). A colour that is not a role (an avatar or role colour, a
//! swatch) stays a literal.
//!
//! Roles have distinct values, which is what lets `col` map a value back to
//! its role. Icon colours (`Color::from(theme::X)`) are plain colours: they
//! take the role's value but do not follow a runtime change until rebuilt.

use runtime_core::{install_theme, set_theme, Color, ThemeTokens, TokenEntry, TokenValue, Tokenized};

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
/// Brand accent (Discord blurple): primary buttons, selection, mentions.
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

/// Every role: (default value, token name).
const ROLES: &[(&str, &str)] = &[
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

/// The installed theme: the default palette with an optional accent.
#[derive(Clone)]
struct Theme {
    accent: String,
}

impl ThemeTokens for Theme {
    fn tokens(&self) -> Vec<TokenEntry> {
        let mut out: Vec<TokenEntry> = ROLES
            .iter()
            .map(|(value, name)| {
                let value = if *name == "color-accent" { self.accent.as_str() } else { value };
                TokenEntry { name, value: TokenValue::Color(Color(value.into())) }
            })
            .collect();
        // The framework's host-surface tokens: the page behind the app and
        // the platform scrollbar.
        for (name, value) in [
            ("color-background", BG_TERTIARY),
            ("color-border-strong", BG_TERTIARY),
            ("color-surface-alt", "transparent"),
        ] {
            out.push(TokenEntry { name, value: TokenValue::Color(Color(value.into())) });
        }
        out
    }
}

/// Install the theme (call once before the first render). `accent` is the
/// user's saved accent, if any.
pub fn install(accent: Option<String>) {
    install_theme(Theme { accent: accent.unwrap_or_else(|| ACCENT.to_string()) });
}

/// Recolour the accent (everything styled with `col(theme::ACCENT)`).
pub fn set_accent(accent: &str) {
    set_theme(Theme { accent: accent.to_string() });
}

/// The style value for colour `value`: its role's token when it is a role's
/// value, otherwise the literal colour.
pub fn color(value: &str) -> Tokenized<Color> {
    match ROLES.iter().find(|(v, _)| v.eq_ignore_ascii_case(value)) {
        Some((v, name)) => Tokenized::token(name, Color((*v).into())),
        None => Tokenized::Literal(Color(value.into())),
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
    fn roles_resolve_to_tokens_and_others_stay_literal() {
        assert!(matches!(color(ACCENT), Tokenized::Token { name: "color-accent", .. }));
        assert!(matches!(color("#5865F2"), Tokenized::Token { name: "color-accent", .. }));
        assert!(matches!(color("#123456"), Tokenized::Literal(_)));
    }
}
