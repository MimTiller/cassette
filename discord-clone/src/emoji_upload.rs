//! The Emoji settings section's image picker turns the picked file into the
//! `data:` URL the server stores, mirroring how `sticker_settings::encode`
//! handles stickers: sniff the real format from the bytes (never trust the
//! declared MIME), refuse a non-image, and — since Discord draws emoji at
//! 128×128 — shrink a still image that's over the 256 KB cap or bigger than
//! 128×128 onto the browser's canvas (the way `avatar_crop` resizes avatars)
//! before uploading. An animated GIF/WebP/APNG over the cap is refused with a
//! clear message giving the limit rather than silently flattened.


/// The largest image bytes the server will accept for an emoji
/// (`server_emoji::MAX_BYTES`).
pub(crate) const MAX_BYTES: usize = 256 * 1024;

/// Discord draws emoji at 128×128, so a still is shrunk to fit that box
/// (keeping aspect) on the client before the upload.
#[cfg(target_arch = "wasm32")]
pub(crate) const MAX_DIM: f32 = 128.0;

/// Aspect-fit `(w, h)` of an image that's `w0` × `h0` into a `max` square,
/// never growing a small image.
#[cfg(any(target_arch = "wasm32", test))]
pub(crate) fn fit(w0: f32, h0: f32, max: f32) -> (u32, u32) {
    let s = (max / w0.max(h0)).min(1.0);
    ((w0 * s).round() as u32, (h0 * s).round() as u32)
}

/// Whether the bytes look like an animated GIF, WebP, or APNG. A still can be
/// resized; an animation over the cap is refused over the cap so the client
/// never flattens it.
pub(crate) fn animated(bytes: &[u8]) -> bool {
    if bytes.starts_with(b"GIF8") {
        return true;
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && bytes[8..12].starts_with(b"WEBP") {
        let needle = b"ANMF";
        return bytes.windows(needle.len()).any(|w| w == needle);
    }
    if bytes.starts_with(b"\x89PNG") {
        let needle = b"acTL";
        return bytes.windows(needle.len()).any(|w| w == needle);
    }
    false
}

/// Check a picked emoji image and turn it into the `data:` URL the upload
/// takes WITHOUT resizing — the small/still path shared with the robot hook.
/// The format is sniffed FROM THE BYTES (`sniff_mime`), so the claimed MIME we
/// send always agrees with what the server's media store detects from them; a
/// picker that declared the wrong type can't smuggle one through.
pub(crate) fn encode_as_is(_mime_declared: &str, bytes: &[u8]) -> Result<String, String> {
    let kind = sniff_mime(bytes).ok_or("Not a PNG/JPEG/GIF/WebP image — pick an image file")?;
    if bytes.len() > MAX_BYTES {
        return Err("emoji images can be at most 256 KB".to_string());
    }
    Ok(format!("data:{};base64,{}", kind, crate::profile_settings::b64_encode(bytes)))
}

/// The accepted image MIME detected from the bytes' magic numbers — PNG / JPEG
/// / GIF / WebP, else `None`. The format TRUTH used for the claimed MIME we
/// send the server (whose store also sniffs and rejects a claimed/detected
/// mismatch), not the declared picker MIME.
pub(crate) fn sniff_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG") {
        return Some("image/png");
    }
    if bytes.starts_with(b"\xFF\xD8\xFF") {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF8") {
        return Some("image/gif");
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && bytes[8..12].starts_with(b"WEBP") {
        return Some("image/webp");
    }
    None
}

// ── web-only: shrink a still onto the browser canvas before upload ─────────

// Decode the picked bytes to an image, and if it's over the cap or bigger
// than the 128×128 draw box, redraw it into that box on a canvas and export,
// stepping quality down until the URL fits under the byte cap.
#[cfg(target_arch = "wasm32")]
pub(crate) async fn encode_still_resized(bytes: Vec<u8>, _mime: &str) -> Result<String, String> {
    use wasm_bindgen::{JsCast, JsValue};
    use web_sys::{Blob, BlobPropertyBag, HtmlCanvasElement, HtmlImageElement, CanvasRenderingContext2d, Url};
    use js_sys::{Array, Uint8Array};

    let kind = sniff_mime(&bytes[..])
        .ok_or("Not a PNG/JPEG/GIF/WebP image — pick an image file")?;

    let parts = Array::of1(&Uint8Array::from(&bytes[..]));
    let opts = BlobPropertyBag::new();
    opts.set_type(kind);
    let blob = Blob::new_with_u8_array_sequence_and_options(&parts, &opts)
        .map_err(|_| "could not read the picture".to_string())?;
    let url = Url::create_object_url_with_blob(&blob)
        .map_err(|_| "could not read the picture".to_string())?;
    let img = HtmlImageElement::new().map_err(|_| "could not read the picture".to_string())?;
    img.set_src(&url);
    if wasm_bindgen_futures::JsFuture::from(img.decode()).await.is_err() {
        let _ = Url::revoke_object_url(&url);
        return Err("That picture won't open in this browser. Try a PNG, JPEG, GIF or WebP.".to_string());
    }
    let (w, h) = (img.natural_width() as f32, img.natural_height() as f32);
    if w < 1.0 || h < 1.0 {
        let _ = Url::revoke_object_url(&url);
        return Err("That picture is empty".to_string());
    }

    // Small enough already: upload the bytes as they are.
    if bytes.len() <= MAX_BYTES && w <= MAX_DIM && h <= MAX_DIM {
        let _ = Url::revoke_object_url(&url);
        return Ok(format!("data:{};base64,{}", kind, crate::profile_settings::b64_encode(&bytes[..])));
    }

    // Redraw into a ≤128×128 box on a canvas, then export at the first quality
    // small enough to fit the cap.
    let (tw, th) = fit(w, h, MAX_DIM);
    let (cw, ch) = (tw.max(1), th.max(1));
    let doc = web_sys::window().and_then(|win| win.document()).ok_or("no document")?;
    let canvas: HtmlCanvasElement = doc.create_element("canvas")
        .map_err(|_| "could not draw the picture".to_string())?
        .unchecked_into();
    canvas.set_width(cw);
    canvas.set_height(ch);
    let ctx: CanvasRenderingContext2d = canvas.get_context("2d").map_err(|_| "no canvas".to_string())?
        .ok_or("no canvas").map_err(|_| "no canvas".to_string())?
        .dyn_into().map_err(|_| "no canvas".to_string())?;
    ctx.set_image_smoothing_enabled(true);
    ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
        &img.unchecked_into(),
        0.0, 0.0, w as f64, h as f64,
        0.0, 0.0,
        (tw as f64).max(1.0), (th as f64).max(1.0),
    ).map_err(|_| "could not draw the picture".to_string())?;

    let out_kind = if kind == "image/jpeg" { "image/jpeg" } else { "image/webp" };
    for quality in [0.9, 0.75, 0.6] {
        let data_url = canvas
            .to_data_url_with_type_and_encoder_options(
                &out_kind.to_string(),
                &JsValue::from_f64(quality),
            )
            .map_err(|_| "could not draw the picture".to_string())?;
        if data_url.starts_with(format!("data:{}", out_kind).as_str()) && data_url.len() <= MAX_BYTES {
            let _ = Url::revoke_object_url(&url);
            return Ok(data_url.to_string());
        }
    }
    let _ = Url::revoke_object_url(&url);
    Err("That picture is still too large after shrinking it to 128×128".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn encode_still_resized(_bytes: Vec<u8>, _mime: &str) -> Result<String, String> {
    Err("a web browser is needed to resize the picture".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_keeps_aspect_and_never_grows() {
        // A 4000×3000 photo shrinks to 128×96 (long side fills the box).
        assert_eq!(fit(4000.0, 3000.0, 128.0), (128, 96));
        // A tall 100×500 shrinks to 26×128.
        assert_eq!(fit(100.0, 500.0, 128.0), (26, 128));
        // A 100×80 already fits, so it stays at its own size (never grown).
        assert_eq!(fit(100.0, 80.0, 128.0), (100, 80));
        // A square 300×300 fills the box.
        assert_eq!(fit(300.0, 300.0, 128.0), (128, 128));
    }

    #[test]
    fn animated_gif_webp_apng_are_flagged() {
        assert!(animated(b"GIF89a\x00\x00\x00\x00"));
        let webp_still = b"RIFF\x10\x00\x00\x00WEBPVP8 \x08\x00\x00\x00\x00\x00\x00\x00";
        assert!(!animated(webp_still));
        let webp_anim = b"RIFF\x10\x00\x00\x00WEBPVP8X\x08\x00\x00\x00ANIM\x00\x00\x00ANMF";
        assert!(animated(webp_anim));
        let png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR";
        assert!(!animated(png));
        let apng = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR\x00\x00\x00acTL";
        assert!(animated(apng));
    }

    #[test]
    fn a_jpeg_and_a_non_image_behave_like_the_server_does() {
        // accepted_mime sniffs the real format from the bytes.
        let jpeg = b"\xff\xd8\xff\xe0\x00\x10JFIF";
        assert!(encode_as_is("image/png", jpeg).unwrap().starts_with("data:image/jpeg;base64,"),
            "a JPEG is stored as image/jpeg even when the picker declares PNG");
        assert!(encode_as_is("text/plain", b"hello").is_err(), "not an image");
    }
}
