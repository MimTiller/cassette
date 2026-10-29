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
    ui, AlignItems, Element, FontWeight, JustifyContent, ObjectFit, Position,
    StyleRules, Tokenized,
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

/// The chip at a given box size (px) and corner radius (px). `tint` is the
/// initial-circle background. Callers pass the same sizes they drew before,
/// so only the broken-image class is fixed — the layout is untouched.
pub fn avatar_chip(name: &str, tint: &str, size: f32, radius: f32) -> Element {
    let initial = name.chars().next().unwrap_or('?').to_uppercase().to_string();
    if let Some(url) = registered_avatar(name) {
        ui! {
            view(style = StyleRules {
                position: Some(Position::Relative),
                width: Some(px(size)),
                height: Some(px(size)),
                flex_shrink: Some(grow(0.0)),
                ..Default::default()
            }) {
                image(
                    src = crate::auth::media_url(url.clone()),
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
    } else {
        ui! {
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

#[cfg(test)]
mod avatar_gate_tests {
    use crate::avatar_chip::mountable_avatar_url;

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
