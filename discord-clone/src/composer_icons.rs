//! The composer's picker-button icons: Discord's GIF badge, peeled sticker,
//! and emoji smiley. The art lives as SVGs in `icons/` (outline style,
//! currentColor); here it is turned into a data URL painted in a given
//! colour, so a button can re-draw the icon when its hover chip brightens.
//! The colour always comes in as a theme role's live value — no hex here.

use crate::app_icon::svg_data_url;

/// The badge's fill is `currentColor`; the letters knock it out through the
/// mask, so they show the bar behind the button — Discord's GIF badge.
pub fn gif_url(color: String) -> String {
    painted(GIF_SVG, color)
}

/// A rounded square with a smiley face and its bottom-right corner folded
/// over, like a peeled sticker.
pub fn sticker_url(color: String) -> String {
    painted(STICKER_SVG, color)
}

/// A round smiley face outline: circle, two eyes, a smile.
pub fn emoji_url(color: String) -> String {
    painted(EMOJI_SVG, color)
}

const GIF_SVG: &'static str = include_str!("../icons/picker_gif.svg");
const STICKER_SVG: &'static str = include_str!("../icons/picker_sticker.svg");
const EMOJI_SVG: &'static str = include_str!("../icons/picker_emoji.svg");

/// `svg` with its `currentColor` painted `color`, as a `data:` URL for
/// `image()`.
fn painted(svg: &'static str, color: String) -> String {
    svg_data_url(&svg.replace("currentColor", color.as_str()))
}
