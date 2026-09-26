//! `voice_participant_list` — a self-contained Discord voice-participant list.
//! Renders a hardcoded local list of EXACTLY three fake participants; each row
//! is a colored circle avatar holding the participant's first letter, the
//! participant's name, and a small mic icon. One of the three is hardcoded as
//! muted, so its mic icon renders in "muted" styling (danger red) while the
//! other two render the normal icon (secondary text color).
//!
//! Everything is hardcoded local state — no backend call, no props, no wiring
//! into any screen yet. Declared in `lib.rs` so it compiles as part of the
//! crate, but referenced by nothing (the same "declared so it compiles,
//! referenced by nothing" shape `profile_settings` and the other `components/`
//! files ship in).
//!
//! Idioms, each verified against this crate's files:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the rows come from a keyed `for (idx, &(name, color, muted)) in
//!   PARTICIPANTS.iter().enumerate(), key = idx` over the static
//!   `[(&str, &str, bool); 3]` slice — the exact `for` + `key = idx` idiom
//!   `member_role_list` uses for its static role pills. A real iterator
//!   (`.iter()`) is required, hence `.iter()`; the `&(...)` destructures the
//!   tuple fields by copy so `name`/`color`/`muted` are each `Copy` and safe
//!   to read inside the per-iteration style/content expressions;
//! - `StyleRules` is not `Copy`, so every row's styles are built INLINE inside
//!   the loop body (created fresh per iteration, never captured across the
//!   loop boundary);
//! - the mic icon is a small styled `text` glyph — the same "keep the glyph a
//!   styled text node" idiom `mute_channel_toggle` / `message_action_bar` use
//!   for their icon buttons — whose glyph and color are selected statically
//!   from the row's `muted` boolean (muted = red glyph, normal = secondary);
//! - the colored circle avatar is a square `view` with a solid participant
//!   color background and all four corners rounded to half its size (a full
//!   circle), holding one small centered white `text` with the name's first
//!   letter; the letter is a static `TextContent` expression
//!   (`name.chars().next()…`) so no reactive content is involved.

use crate::style_helpers::{col, px};
use runtime_core::{
    component, ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent, StyleRules,
};

// Discord dark palette (kept local so the component is self-contained).
const BG_MAIN: &str = "#313338"; // the list surface (Discord elevated surface)
const TEXT: &str = "#f2f3f5"; // participant names (near-white)
const TEXT_MUTED: &str = "#949ba4"; // the small section header
const AVATAR_TEXT: &str = "#ffffff"; // the first-letter on the colored circle
const ICON_ON: &str = "#b5bac1"; // normal mic glyph (Discord secondary text)
const ICON_MUTED: &str = "#f23f43"; // muted mic glyph (Discord danger red)

/// The hardcoded local participant list — EXACTLY three
/// `(name, avatar color, muted)` tuples. The avatar color follows Discord's
/// per-user accent convention; `muted` is a local boolean that decides whether
/// that row's mic icon renders in muted styling. One of the three (index 1) is
/// hardcoded as muted. No props, no backend.
const PARTICIPANTS: [(&str, &str, bool); 3] = [
    ("Ava Patel", "#5865f2", false), // blurple avatar, mic on
    ("Sam Okafor", "#23a559", true), // green avatar, mic MUTED
    ("Nia Reyes", "#f0b232", false), // gold avatar, mic on
];

/// The voice-participant list. A vertical stack of exactly three rows; each
/// row is `[colored circle avatar (first letter), name, mic icon]`. The mic
/// icon is statically styled by the row's hardcoded `muted` boolean. Purely
/// local — renders the hardcoded tuples above; no wiring into any screen.
#[component]
pub fn VoiceParticipantList() -> Element {
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Stretch),
            background: Some(col(BG_MAIN)),
            padding_top: Some(px(14.0)),
            padding_bottom: Some(px(14.0)),
            padding_left: Some(px(12.0)),
            padding_right: Some(px(12.0)),
            ..Default::default()
        }) {
            text(style = StyleRules {
                font_size: Some(px(12.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(TEXT_MUTED)),
                margin_left: Some(px(4.0)),
                margin_bottom: Some(px(10.0)),
                ..Default::default()
            }) { "VOICE — 3 CONNECTED".to_string() }

            // One row per participant, from a keyed `for` over the static
            // `(name, color, muted)` slice — the standard static-row idiom
            // (`member_role_list`). `.iter()` gives a real iterator; the
            // `&(...)` destructures the tuple fields by copy so each is a
            // `Copy` value readable inside this iteration's styles/content.
            for (idx, &(name, color, muted)) in PARTICIPANTS.iter().enumerate(), key = idx {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    gap: Some(px(10.0)),
                    margin_bottom: Some(px(8.0)),
                    ..Default::default()
                }) {
                    // Colored circle avatar: a square `view` filled with the
                    // participant's accent color, all four corners rounded to
                    // half the size (a full circle), centering the first
                    // letter in white.
                    view(style = StyleRules {
                        width: Some(px(32.0)),
                        height: Some(px(32.0)),
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        background: Some(col(color)),
                        border_top_left_radius: Some(px(16.0)),
                        border_top_right_radius: Some(px(16.0)),
                        border_bottom_left_radius: Some(px(16.0)),
                        border_bottom_right_radius: Some(px(16.0)),
                        ..Default::default()
                    }) {
                        // The name's first letter (a static String expression).
                        text(style = StyleRules {
                            font_size: Some(px(14.0)),
                            font_weight: Some(FontWeight::SemiBold),
                            color: Some(col(AVATAR_TEXT)),
                            ..Default::default()
                        }) { name.chars().next().unwrap().to_string() }
                    }

                    // The participant's name (primary text).
                    text(style = StyleRules {
                        font_size: Some(px(15.0)),
                        font_weight: Some(FontWeight::SemiBold),
                        color: Some(col(TEXT)),
                        ..Default::default()
                    }) { name.to_string() }

                    // The small mic icon: a styled text glyph (the
                    // `mute_channel_toggle` / `message_action_bar` idiom),
                    // statically chosen by this row's `muted` boolean —
                    // muted rows get the red glyph, the rest the normal one.
                    text(style = StyleRules {
                        font_size: Some(px(14.0)),
                        color: Some(col(if muted { ICON_MUTED } else { ICON_ON })),
                        ..Default::default()
                    }) { (if muted { "🔇" } else { "🎤" }).to_string() }
                }
            }
        }
    }
}
