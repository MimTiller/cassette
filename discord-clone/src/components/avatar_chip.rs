//! `avatar_chip` — the shared avatar chip every row surface draws: the user's
//! uploaded image (round-cropped) when the shared client avatar registry
//! (`crate::auth::avatar_for`) carries a mountable URL, else the colored
//! initial-letter circle.
//!
//! Why one shared helper instead of per-site `image(...)` literals: the web
//! backend refuses to mount an `image()` src it cannot resolve — anything that
//! is not `data:`/`http(s):` — and removes the `src` attribute, so the browser
//! paints a broken-image placeholder whose `alt` text leaks onto the row. The
//! gate here (plus the non-empty base64 body check) keeps such users on the
//! initial chip instead. A URL that passes this gate but fails to paint at
//! runtime (a 404 http image) is the only residual gap — the seeded data
//! paths carry valid data URLs, which paint or never mount.

use crate::theme;
use crate::style_helpers::{col, px, rounded};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FontWeight, JustifyContent, ObjectFit,
    PointerEvents, Position, StyleRules, Tokenized, TouchPhase, TouchResponse,
};

/// True when `url` may be mounted as an `image()` src: the
/// `image_src::classify` scheme gate, plus a non-empty base64 body (an empty
/// payload decodes to nothing to paint), plus a valid image signature (the
/// PNG/JPEG/WEBP/GIF magic bytes, and for PNG the `IHDR` chunk at offset 12 —
/// a corrupt-stream body decodes fine but paints as a broken image, which is
/// what leaked alt text onto friend/DM rows).
pub fn mountable_avatar_url(url: &str) -> bool {
    // A backend media path (post-migration posture) mounts once absolutized
    // by crate::auth::media_url at the render site.
    if url.starts_with("/media/") {
        return true
    }
    let scheme_ok = url.starts_with("data:")
        || url.starts_with("http://")
        || url.starts_with("https://");
    if !scheme_ok {
        return false
    }
    if !url.starts_with("http") {
        match url.find("base64,") {
            Some(i) => {
                let body = &url[i + "base64,".len()..];
                if body.is_empty() {
                    return false
                }
                match crate::profile_settings::b64_decode(body) {
                    Ok(bytes) => signature_ok(&bytes),
                    Err(_) => false,
                }
            }
            None => false,
        }
    } else {
        true
    }
}

/// Magic-number check for the avatar mimes the server accepts
/// (png/jpeg/webp/gif). For PNG the IHDR chunk must sit at offset 12 with the
/// canonical 13-byte length — a corrupt-stream body decodes fine from base64
/// but its chunk table is mangled (length reads 12, type bytes shifted), so it
/// never paints and must fall back to the chip instead of leaking alt text.
fn signature_ok(bytes: &[u8]) -> bool {
    let n = bytes.len();
    let png = n >= 16
        && bytes[0] == 0x89
        && bytes[1] == b'P'
        && bytes[2] == b'N'
        && bytes[3] == b'G'
        && bytes[8] == 0
        && bytes[9] == 0
        && bytes[10] == 0
        && bytes[11] == 13
        && bytes[12] == b'I'
        && bytes[13] == b'H'
        && bytes[14] == b'D'
        && bytes[15] == b'R';
    let jpeg = n >= 3 && bytes[0] == 0xff && bytes[1] == 0xd8 && bytes[2] == 0xff;
    let webp = n >= 12
        && bytes[0] == b'R'
        && bytes[1] == b'I'
        && bytes[2] == b'F'
        && bytes[3] == b'F'
        && bytes[8] == b'W'
        && bytes[9] == b'E'
        && bytes[10] == b'B'
        && bytes[11] == b'P';
    let gif = n >= 6
        && bytes[0] == b'G'
        && bytes[1] == b'I'
        && bytes[2] == b'F'
        && bytes[3] == b'8'
        && (bytes[4] == b'9' || bytes[4] == b'4')
        && bytes[5] == b'a';
    png || jpeg || webp || gif
}

/// The shared client registry's URL for `name` when mountable, else `None` =
/// draw the initial circle.
pub fn registered_avatar(name: &str) -> Option<String> {
    let url = crate::auth::avatar_for(name);
    if let Some(u) = url {
        if mountable_avatar_url(u.as_str()) {
            return Some(u)
        }
    }
    None
}

pub fn grow(v: f32) -> Tokenized<f32> {
    Tokenized::Literal(v)
}

/// Presence dots: the dot itself is this wide; the ring punched out of the
/// surface around it is that wide.
pub const PRESENCE_DOT_PX: f32 = 10.0;
pub const PRESENCE_RING_PX: f32 = 2.0;

/// Avatars smaller than this wear no decoration: the art stops reading.
const DECORATION_MIN_PX: f32 = 24.0;
/// How wide decoration art is, as a multiple of the avatar's size. Its viewBox
/// is 140 square, with the avatar's circle centred in it, 100 across.
const DECORATION_SCALE: f32 = 1.4;
/// Avatars at least this big (the profile card, the settings preview) play
/// their decoration once as they appear.
const DECORATION_GREET_PX: f32 = 64.0;

/// The art for a catalog id (`discord_server::profiles::AVATAR_DECORATIONS`),
/// or `None` for an id this build doesn't know. Each file's root element has
/// `class="still"`, which [`decoration_url`] swaps to start its motion.
pub(crate) fn decoration_svg(id: &str) -> Option<&'static str> {
    match id {
        "cat_ears" => Some(include_str!("../../icons/cat_ears.svg")),
        "cassette_reels" => Some(include_str!("../../icons/cassette_reels.svg")),
        "halo" => Some(include_str!("../../icons/halo.svg")),
        "flower_crown" => Some(include_str!("../../icons/flower_crown.svg")),
        "sparkles" => Some(include_str!("../../icons/sparkles.svg")),
        _ => None,
    }
}

/// `svg` as a mountable URL: moving on a loop while `hovered`, playing once
/// after a press, otherwise still. Each press gets its own URL, because a new
/// URL is what restarts the motion.
fn decoration_url(svg: &str, hovered: bool, presses: u32) -> String {
    let motion = if hovered {
        "moving"
    } else if presses > 0 {
        "once"
    } else {
        "still"
    };
    let root = format!("class=\"{motion}\" data-press=\"{presses}\"");
    crate::app_icon::svg_data_url(&svg.replacen("class=\"still\"", &root, 1))
}

/// The box every avatar on every surface sits in: `size` square and
/// `Position::Relative`, with the chip as its child and `name`'s decoration
/// over it. Surfaces that draw their own chip wrap it in this; [`avatar_chip`]
/// and [`avatar_chip_url`] do it for everyone else.
///
/// The art reads the mentions revision, so it follows the decoration registry
/// on any surface without the surface rebuilding. It takes no pointer events,
/// so a press on it lands on the avatar, and the frame only watches that press
/// on its way to the surface's own handler. A presence dot goes after this
/// frame in its parent, which keeps the dot on top.
#[component]
pub fn AvatarFrame(#[prop(static)] name: String, #[prop(static)] size: f32, children: Vec<Element>) -> Element {
    // A signal, not the String, so the closures below stay `Copy`.
    let name = signal(name);
    let hovered = signal(false);
    let presses = signal(u32::from(size >= DECORATION_GREET_PX));
    let overhang = size * (DECORATION_SCALE - 1.0) / 2.0;
    let art = move || {
        crate::mentions::revision();
        if size < DECORATION_MIN_PX {
            return None;
        }
        crate::auth::avatar_decoration_for(&name.get()).and_then(|id| decoration_svg(&id))
    };
    ui! {
        view(style = StyleRules {
            position: Some(Position::Relative),
            width: Some(px(size)),
            height: Some(px(size)),
            flex_shrink: Some(grow(0.0)),
            ..Default::default()
        }) {
            { children }
            if art().is_some() {
                image(
                    src = move || {
                        art()
                            .map(|svg| decoration_url(svg, hovered.get(), presses.get()))
                            .unwrap_or_default()
                    },
                    alt = "",
                    style = StyleRules {
                        position: Some(Position::Absolute),
                        left: Some(px(-overhang)),
                        top: Some(px(-overhang)),
                        width: Some(px(size * DECORATION_SCALE)),
                        height: Some(px(size * DECORATION_SCALE)),
                        pointer_events: Some(PointerEvents::None),
                        ..Default::default()
                    },
                    test_id = "avatar-decoration",
                )
            }
        }
        // Presses outlive the hover, so a press whose popup takes the pointer
        // away still plays. Coming back forgets them.
        .on_hover(move |on| {
            hovered.set(on);
            if on {
                presses.set(0);
            }
        })
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Began {
                presses.set(presses.get() + 1);
            }
            TouchResponse::IGNORED
        })
    }
}

/// A circle chip from an explicit avatar URL (already gated mountable by the
/// caller): cover-cropped at `size`, radius always size/2.
pub fn avatar_chip_url(name: &str, url: &str, size: f32) -> Element {
    let radius = size / 2.0;
    ui! {
        AvatarFrame(name = name, size = size) {
            image(
                src = crate::auth::media_url(url.to_string()),
                alt = format!("{} avatar", name),
                style = StyleRules {
                    width: Some(px(size)),
                    height: Some(px(size)),
                    flex_shrink: Some(grow(0.0)),
                    object_fit: Some(ObjectFit::Cover),
                    ..rounded(radius)
                },
            )
        }
    }
}

/// The chip at a given box size (px). Every avatar is a CIRCLE — the radius
/// is always size/2 and decided here, so a call site cannot reintroduce a
/// rounded square; a surface that wants the chip bigger just passes a bigger
/// size. `tint` is the initial-circle background.
pub fn avatar_chip(name: &str, tint: &str, size: f32) -> Element {
    let radius = size / 2.0;
    let initial = name.chars().next().unwrap_or('?').to_uppercase().to_string();
    if let Some(url) = registered_avatar(name) {
        avatar_chip_url(name, url.as_str(), size)
    } else {
        ui! {
            AvatarFrame(name = name, size = size) {
                view(style = StyleRules {
                    width: Some(px(size)),
                    height: Some(px(size)),
                    flex_shrink: Some(grow(0.0)),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    background: Some(col(tint)),
                    ..rounded(radius)
                }) {
                    text(style = StyleRules {
                        font_size: Some(px(size * 0.44)),
                        font_weight: Some(FontWeight::Bold),
                        color: Some(col(theme::TEXT_STRONG)),
                        ..Default::default()
                    }) { initial }
                }
            }
        }
    }
}

/// One presence dot convention, everywhere the app shows one: a
/// `PRESENCE_DOT_PX` dot with a `PRESENCE_RING_PX` ring punched out in the
/// colour of the surface behind the avatar (Discord's cut-out look, so the
/// dot reads against the chip edge AND against a dimmed row). Returns an
/// absolutely-positioned element for the bottom-right corner — the avatar
/// box it rides must be `Position::Relative` and sized to the chip.
pub fn presence_dot(status_role: &str, ring_role: &str) -> Element {
    let total = PRESENCE_DOT_PX + 2.0 * PRESENCE_RING_PX;
    // Offline is Discord's hollow ring; the other statuses are filled discs.
    let hollow = status_role.eq_ignore_ascii_case(crate::theme::TEXT_FAINT);
    ui! {
        view(style = StyleRules {
            position: Some(Position::Absolute),
            right: Some(px(0.0)),
            bottom: Some(px(0.0)),
            width: Some(px(total)),
            height: Some(px(total)),
            background: Some(col(ring_role)),
            ..rounded(total / 2.0)
        }) {
            view(style = StyleRules {
                position: Some(Position::Absolute),
                right: Some(px(PRESENCE_RING_PX)),
                bottom: Some(px(PRESENCE_RING_PX)),
                width: Some(px(PRESENCE_DOT_PX)),
                height: Some(px(PRESENCE_DOT_PX)),
                background: Some(col(if hollow { "transparent" } else { status_role })),
                border_top_width: Some(Tokenized::Literal(if hollow { 2.0 } else { 0.0 })),
                border_right_width: Some(Tokenized::Literal(if hollow { 2.0 } else { 0.0 })),
                border_bottom_width: Some(Tokenized::Literal(if hollow { 2.0 } else { 0.0 })),
                border_left_width: Some(Tokenized::Literal(if hollow { 2.0 } else { 0.0 })),
                border_top_color: Some(col(if hollow { status_role } else { "transparent" })),
                border_right_color: Some(col(if hollow { status_role } else { "transparent" })),
                border_bottom_color: Some(col(if hollow { status_role } else { "transparent" })),
                border_left_color: Some(col(if hollow { status_role } else { "transparent" })),
                ..rounded(PRESENCE_DOT_PX / 2.0)
            }) {}
        }
    }
}

/// Map a wire presence status (the gateway `Presence` frame strings and the
/// `Status::wire` names) to its dot's colour role. Unknown / absent /
/// invisible statuses are offline: `TEXT_FAINT`.
pub fn presence_status_role(status: &str) -> &str {
    match status {
        "online" => theme::SUCCESS,
        "away" => theme::WARNING,
        "idle" => theme::WARNING,
        "dnd" => theme::DANGER,
        "do_not_disturb" => theme::DANGER,
        _ => theme::TEXT_FAINT,
    }
}

#[cfg(test)]
mod avatar_gate_tests {
    use crate::avatar_chip::{decoration_svg, decoration_url, mountable_avatar_url};

    #[test]
    fn every_catalog_decoration_has_art_that_can_move() {
        for id in discord_server::profiles::AVATAR_DECORATIONS {
            let svg = decoration_svg(id).unwrap_or_else(|| panic!("{id} has no art"));
            assert!(svg.contains("class=\"still\""), "{id} has no still root class to swap");
        }
        assert!(decoration_svg("dog_ears").is_none());
    }

    #[test]
    fn decoration_art_moves_on_hover_and_plays_once_per_press() {
        let svg = "<svg class=\"still\"/>";
        assert!(decoration_url(svg, false, 0).contains("class='still'"));
        assert!(decoration_url(svg, true, 0).contains("class='moving'"));
        assert!(decoration_url(svg, false, 1).contains("class='once'"));
        assert!(decoration_url(svg, true, 1).contains("class='moving'"), "hover outranks a press");
        assert_ne!(decoration_url(svg, false, 1), decoration_url(svg, false, 2), "a new press restarts");
        assert_ne!(decoration_url(svg, true, 1), decoration_url(svg, true, 2), "a press restarts the hover loop");
    }

    #[test]
    fn regression_avatar_corrupt_png_chunk_never_mounts() {
        // A seeded account's avatar: base64 decodes but the IHDR chunk length
        // reads 12, not 13 — the PNG never paints, so mounting it showed a
        // broken-image placeholder with leaking alt text on friend/DM rows.
        let corrupt = "data:image/png;base64,iVBORw0KGgoAAAAMSUhEUgAAAAQAAAAECAAAAEbXIXgAAAASSURBVHicY/jPwPAfGTOQLgAAPEAf4eCBd7QAAAAASUVORK5CYII=";
        // A well-formed 1x1 PNG (what the seed scripts upload).
        let good = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQg";
        assert!(!mountable_avatar_url(corrupt), "corrupt chunk stream falls back to the initial chip");
        assert!(mountable_avatar_url(good), "a valid PNG mounts");
        // Scheme gate still holds, and an empty body never mounts.
        assert!(!mountable_avatar_url("not a url"), "no scheme = no mount");
        assert!(!mountable_avatar_url("data:image/png;base64,"), "empty body = no mount");
    }
}
