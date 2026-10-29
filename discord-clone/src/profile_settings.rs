//! `profile_settings` — the Profile screen: a circular avatar (uploaded image
//! when set, else the colored circle + first letter of the name), the display
//! name and "about me" (loaded from the server, stored with `set_profile`),
//! and the avatar controls: "Change avatar" (opens the `file-picker` SDK's
//! native file-open UI, reads the chosen image, validates its type and size
//! against the backend's cap, base64-encodes it into a `data:image/…;base64,`
//! URL and fires the `set_avatar` server fn) and "Remove".
//!
//! The avatar URL rides the shared client avatar registry
//! (`crate::auth::remember_avatar` / `crate::auth::avatar_for`) so every
//! other surface (message rows, member lists, DMs) shows the uploaded image
//! without re-fetching.
//!
//! Idioms, each verified against this crate:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the placeholder-vs-image swap is a node-level reactive
//!   `if !avatar_url.get().is_empty() { … } else { … }` inside `ui!` — the
//!   `if` condition reads a signal (`.get()`), so it lowers to the `when`
//!   primitive (mounts the chosen branch, unmounts the other on toggle).
//!   `StyleRules` is not `Copy`, so EVERY style used inside the branch is
//!   written INLINE in `style = …` (created fresh per evaluation, never
//!   captured across the `when` boundary);
//! - the Change-avatar / Remove pills are `view {…}.on_touch(|ev| …)` chains
//!   with the `TouchPhase::Ended` guard — the app's standard click pattern;
//! - the picker → RPC round trip runs in `runtime_core::driver::spawn_async`,
//!   with `av_busy` + `av_error` signals keying the labels reactively (the
//!   `ServerSettingsPanel` triple, minus the paste input).

use crate::theme;
use crate::style_helpers::{borderless_input, col, px};
use runtime_core::{
    component,
    ui,
    AlignItems,
    Element,
    FlexDirection,
    FontWeight,
    JustifyContent,
    ObjectFit,
    Overflow,
    StyleRules,
    Tokenized,
    TouchPhase,
    TouchResponse,
    signal,
};

// The file-picker SDK: "Change avatar" presents the platform's native
// file-open UI. Imperative API only — no scene handler — so it needs NO
// `register_scene_extensions` entry (unlike the markdown SDK).
use file_picker::{FilePicker, PickRequest, PickOutcome, MediaKind};

// The same Discord dark palette as `app.rs`, kept local so the screen is
// self-contained.

fn grow(v: f32) -> Tokenized<f32> {
    Tokenized::Literal(v)
}

/// The signed-in account's current avatar URL, read straight off the shared
/// registry (`""` = none). ProfileSettings is rebuilt inside the `authed`
/// switch, so a plain read at build time captures the post-login value; the
/// "Change avatar" flow below writes the local `avatar_url` signal the chip
/// binds reactively, so within the open screen an upload re-renders.
fn seeded_avatar() -> String {
    let name = crate::auth::current_username();
    if name.is_empty() {
        return String::new();
    }
    crate::auth::avatar_for(name.as_str()).unwrap_or_default()
}

// --- avatar upload plumbing (picker callback + robot hook share these) -----

/// The backend's avatar URL cap (`AVATAR_MAX_BYTES` in discord-server's auth
/// layer) — the whole `data:<mime>;base64,…` URL must fit it. Enforced
/// client-side so the rejection shows inline instead of a pointless RPC.
const AVATAR_URL_MAX_BYTES: usize = 256 * 1024;

const B64_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64-encode `data` (standard alphabet, padded). The app crate carries no
/// `base64` dependency — avatars are tiny and the encoder is fifteen lines.
fn b64_encode(data: &[u8]) -> String {
    let out_len = (data.len() + 2) / 3 * 4;
    let mut out: Vec<u8> = Vec::new();
    out.reserve(out_len);
    for chunk_start in 0..data.len().div_ceil(3) {
        let i = chunk_start * 3;
        let b0 = data[i];
        let b1 = if i + 1 < data.len() { data[i + 1] } else { 0 };
        let b2 = if i + 2 < data.len() { data[i + 2] } else { 0 };
        let v = (u32::from(b0) << 16) | (u32::from(b1) << 8) | u32::from(b2);
        out.push(B64_ALPHABET[((v >> 18) & 63) as usize]);
        out.push(B64_ALPHABET[((v >> 12) & 63) as usize]);
        out.push(B64_ALPHABET[((v >> 6) & 63) as usize]);
        out.push(B64_ALPHABET[(v & 63) as usize]);
    }
    match data.len() % 3 {
        1 => {
            let n = out.len();
            out[n - 2] = b'=';
            out[n - 1] = b'=';
        }
        2 => {
            let n = out.len();
            out[n - 1] = b'=';
        }
        _ => {}
    }
    String::from_utf8(out).unwrap()
}

/// Decode standard padded base64 (the robot hooks' argument; the picker path
/// never needs it).
pub fn b64_decode(s: &str) -> Result<Vec<u8>, String> {
    let mut out: Vec<u8> = Vec::new();
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    for i in 0..s.len() {
        let byte = s.as_bytes()[i];
        if byte == b'=' {
            break;
        }
        if byte.is_ascii_whitespace() {
            continue;
        }
        let sextet = B64_ALPHABET.iter().position(|x| *x == byte).ok_or("bad base64 character")?;
        acc = (acc << 6) | sextet as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xFF) as u8);
        }
    }
    Ok(out)
}

/// Canonicalize the picker's reported `mime` (possibly empty /
/// `application/octet-stream`) against the backend's accepted set, falling
/// back to magic-byte sniffing. `Some(mime)` = one of the four accepted
/// `data:image/…` kinds, `None` = rejected.
fn accepted_mime(mime: &str, bytes: &[u8]) -> Option<&'static str> {
    // The accepted set is exactly the backend's (`avatar_is_valid_spec`).
    const ACCEPTED: [&'static str; 4] = ["image/png", "image/jpeg", "image/webp", "image/gif"];
    let lowered = mime.to_ascii_lowercase();
    if lowered == "image/jpg" {
        return Some("image/jpeg");
    }
    if let Some(hit) = ACCEPTED.iter().find(|m| **m == lowered.as_str()) {
        return Some(hit);
    }
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

/// Validate picked bytes + reported MIME (type AND size against the backend's
/// cap) and encode into the data URL the backend stores. `Err` carries the
/// message shown inline on the screen.
fn validate_and_encode(mime: &str, bytes: &[u8]) -> Result<String, String> {
    validate_and_encode_small_image(mime, bytes, "avatar")
}

/// The avatar-cap validate + encode, with `what` naming the image in the
/// too-large message. Server icons share the avatar cap on the backend.
pub(crate) fn validate_and_encode_small_image(mime: &str, bytes: &[u8], what: &str) -> Result<String, String> {
    let kind = accepted_mime(mime, bytes).ok_or("Not a PNG/JPEG/WebP/GIF image — pick an image file")?;
    let url = format!("data:{};base64,{}", kind, b64_encode(bytes));
    if url.len() > AVATAR_URL_MAX_BYTES {
        return Err(format!("{what} too large (max 256 KB)"));
    }
    Ok(url)
}

// --- message-attachment upload plumbing (app.rs composer shares these) ----

/// Per-image decoded cap the backend enforces on channel-message attachments
/// (`MAX_ATTACHMENT_BYTES` in discord-server's realtime layer) — mirrored
/// client-side so an oversized pick rejects visibly, not in a dropped frame.
const ATTACHMENT_MAX_BYTES: usize = 2 * 1024 * 1024;

/// Max images one message may carry (`MAX_ATTACHMENTS_PER_MESSAGE`).
pub const ATTACHMENT_MAX_COUNT: usize = 4;

/// The same validate + encode as [`validate_and_encode`] but against the
/// message-attachment cap (avatars are capped far tighter). Reuses the
/// shared `b64_encode` and `accepted_mime` rather than duplicating them.
pub fn validate_and_encode_attachment(mime: &str, bytes: &[u8]) -> Result<String, String> {
    let kind = accepted_mime(mime, bytes).ok_or("Not a PNG/JPEG/WebP/GIF image — pick an image file")?;
    let url = format!("data:{};base64,{}", kind, b64_encode(bytes));
    if bytes.len() > ATTACHMENT_MAX_BYTES {
        return Err("image too large (max 2 MB)".to_string());
    }
    Ok(url)
}

/// Thumbnail cell for the composer's pending-attachment strip: the image
/// letterboxed into a `cell`-px square on a dark chip, so a strip of mixed
/// aspect ratios stays even. The full-resolution image is only carried in
/// the send frame, not rendered here.
pub fn attachment_thumbnail(src: runtime_core::Reactive<String>, filename: String, cell: f32) -> Element {
    ui! {
        view(style = StyleRules {
            width: Some(px(cell)),
            height: Some(px(cell)),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::Center),
            background: Some(col(theme::BG_TERTIARY)),
            border_top_left_radius: Some(px(6.0)),
            border_top_right_radius: Some(px(6.0)),
            border_bottom_left_radius: Some(px(6.0)),
            border_bottom_right_radius: Some(px(6.0)),
            overflow: Some(Overflow::Hidden),
            flex_shrink: Some(grow(0.0)),
            ..Default::default()
        }) {
            image(
                src = move || src.get().clone(),
                alt = format!("attachment: {filename}"),
                style = StyleRules {
                    width: Some(px(cell)),
                    height: Some(px(cell)),
                    object_fit: Some(ObjectFit::Contain),
                    ..Default::default()
                },
            )
        }
    }
}

// Robot test hook (`avatar.upload`): the picker callback's validate → encode
// → `set_avatar` path, callable through the robot bridge so an E2E test can
// verify upload behaviour without driving the native file-open dialog (which
// no in-page tool can tap). Compiled only into robot builds; the imports it
// needs live inside the same gate.
runtime_vocabulary::robot_only! {
    use runtime_vocabulary::robot_fn::{RobotArg, RobotCall, RobotFunction, RobotReply, register as robot_register};
    use runtime_shared::__serde_json as robot_json;
    use std::rc::Rc;
}

#[component]
pub fn ProfileSettings() -> Element {
    // The display name and "about me", loaded from the server; Save stores
    // them with `set_profile`. An empty display name shows the username.
    let username = crate::auth::current_username();
    let display_name = signal(String::new());
    let about = signal(String::new());
    let profile_note = signal(String::new());
    let profile_busy = signal(false);
    // Replies land in this page's signals, so they are anchored to it.
    let alive = runtime_core::ScopeAlive::current();
    {
        let me = crate::auth::CURRENT_USER_ID.load(std::sync::atomic::Ordering::SeqCst);
        let input = discord_server::profiles::GetUserProfileInput { user_id: me, server_id: None };
        runtime_core::spawn_then_in(&alive, discord_server::profiles::get_user_profile(input), move |result| {
            if let Ok(p) = result {
                display_name.set(p.display_name.unwrap_or_default());
                about.set(p.about);
            }
        });
    }
    let do_save = move || {
        if profile_busy.get() {
            return;
        }
        profile_busy.set(true);
        profile_note.set(String::new());
        let input = discord_server::profiles::SetProfileInput { display_name: display_name.get(), about: about.get() };
        runtime_core::spawn_then_in(&alive, discord_server::profiles::set_profile(input), move |result| {
            match result {
                Ok(saved) => {
                    crate::auth::remember_display_name(&crate::auth::current_username(), saved.display_name);
                    crate::mentions::names_changed();
                    profile_note.set("Saved.".to_string());
                }
                Err(e) => profile_note.set(crate::errors::friendly(&e)),
            }
            profile_busy.set(false);
        });
    };
    // The signed-in user's avatar URL (a `data:image/…;base64,` string,
    // seeded from the shared registry) — written by the "Change avatar"
    // picker flow below and read reactively by the avatar chip.
    let avatar_url = signal(seeded_avatar());

    // The avatar's first letter, recomputed whenever the name changes.
    let avatar_letter = {
        let name = display_name.clone();
        // A signal, not the String, so this closure stays `Copy`.
        let username = signal(username.clone());
        move || {
            let shown = name.get();
            if shown.is_empty() { username.get() } else { shown }
                .chars()
                .next()
                .map(|c| c.to_ascii_uppercase())
                .unwrap_or('?')
                .to_string()
        }
    };

    let input_style = StyleRules {
        flex_grow: Some(grow(1.0)),
        height: Some(px(40.0)),
        padding_left: Some(px(12.0)),
        padding_right: Some(px(12.0)),
        background: Some(col(theme::BG_INPUT)),
        color: Some(col(theme::TEXT_STRONG)),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        ..borderless_input()
    };

    // The avatar controls share their state with the handlers: `av_busy` +
    // `av_error` are the in-flight flag and the rejection line (the
    // `ServerSettingsPanel` triple minus the paste input); `av_picking`
    // separates "choose a file" from "saving" in the pill label.
    let av_busy = signal(false);
    let av_error = signal(String::new());
    let av_picking = signal(false);

    // Register the robot test hook so an MCP E2E can call `avatar.upload
    // {mime, data_b64}` — the same validate → encode → `set_avatar` path the
    // picker callback below runs — without the native file dialog.
    // Rebuilt per screen open: `robot_fn::register` replaces by name, and the
    // hook is a no-op outside robot builds.
    runtime_vocabulary::robot_only! {
        register_avatar_upload_hook(avatar_url.clone(), av_busy.clone(), av_error.clone());
    };

    // Change-avatar handler: open the native picker (single image), read the
    // file in, validate type + size, base64 it into the data URL, fire the
    // `set_avatar` RPC, and on success push into the shared avatar registry
    // keyed by the REAL username (not the editable display name) so every
    // other surface (message rows, member lists, DMs) shows it too.
    let do_change = move || {
        if av_busy.get() {
            return;
        }
        let busy = av_busy;
        let err = av_error;
        let av = avatar_url;
        let picking = av_picking;
        busy.set(true);
        err.set(String::new());
        runtime_core::driver::spawn_async(async move {
            picking.set(true);
            let outcome = match FilePicker::new()
                .pick(PickRequest::media(MediaKind::Images))
                .await
            {
                Ok(outcome) => outcome,
                Err(e) => {
                    err.set(format!("file picker: {e}"));
                    picking.set(false);
                    busy.set(false);
                    return;
                }
            };
            picking.set(false);
            match outcome {
                PickOutcome::Picked(files) => {
                    if let Some(file) = files.get(0) {
                        match file.read_all().await {
                            Ok(bytes) => {
                                match validate_and_encode(file.mime(), &bytes) {
                                    Ok(url) => {
                                        let url_for_save = url.clone();
                                        match crate::auth::set_avatar(Some(url)).await {
                                            Ok(_) => {
                                                // Key the registry on the backend username, which is
                                                // what every payload carries.
                                                let name = crate::auth::current_username();
                                                crate::auth::remember_avatar(name.as_str(), Some(url_for_save.clone()));
                                                // The persisted session's avatar segment must follow the
                                                // upload so a page refresh restores the new image.
                                                crate::auth::refresh_saved_avatar(Some(url_for_save.clone()));
                                                av.set(url_for_save);
                                            }
                                            Err(msg) => err.set(msg),
                                        }
                                    }
                                    Err(msg) => err.set(msg),
                                }
                            }
                            Err(e) => err.set(format!("could not read the file: {e}")),
                        }
                    } else {
                        err.set("No file was chosen".to_string());
                    }
                }
                // Cancelled (or an unexpected empty outcome): back to idle,
                // not an error — dismissing a picker is not a failure.
                _ => {}
            }
            busy.set(false);
        });
    };
    // Remove handler: clear the stored avatar server-side and wipe the
    // registry entry + local signal so the initial circle comes back.
    let do_remove = move || {
        if av_busy.get() {
            return;
        }
        let busy = av_busy;
        let err = av_error;
        let av = avatar_url;
        busy.set(true);
        err.set(String::new());
        runtime_core::driver::spawn_async(async move {
            match crate::auth::set_avatar(None).await {
                Ok(_) => {
                    let name = crate::auth::current_username();
                    crate::auth::remember_avatar(name.as_str(), None);
                    crate::auth::refresh_saved_avatar(None);
                    av.set(String::new());
                }
                Err(msg) => err.set(msg),
            }
            busy.set(false);
        });
    };

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Center),
            background: Some(col(theme::BG_PRIMARY)),
            padding_top: Some(px(24.0)),
            padding_left: Some(px(20.0)),
            padding_right: Some(px(20.0)),
            gap: Some(px(16.0)),
            ..Default::default()
        }) {
            // The avatar chip: the uploaded image when set, else the colored
            // initial circle (the pre-existing look). `avatar_url.get()` is
            // read inside the `if`, so it lowers to the `when` primitive and
            // flips on Change-avatar/Remove. Styles are inline: nothing
            // non-Copy is captured across the `when` boundary.
            if !avatar_url.get().is_empty() {
                image(
                    src = move || crate::auth::media_url(avatar_url.get().clone()),
                    alt = "Your avatar",
                    style = StyleRules {
                        width: Some(px(72.0)),
                        height: Some(px(72.0)),
                        flex_shrink: Some(grow(0.0)),
                        object_fit: Some(ObjectFit::Cover),
                        border_top_left_radius: Some(px(36.0)),
                        border_top_right_radius: Some(px(36.0)),
                        border_bottom_left_radius: Some(px(36.0)),
                        border_bottom_right_radius: Some(px(36.0)),
                        ..Default::default()
                    },
                )
            } else {
                text(style = StyleRules {
                    width: Some(px(72.0)),
                    height: Some(px(72.0)),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    background: Some(col(theme::ACCENT)),
                    color: Some(col(theme::TEXT_STRONG)),
                    font_size: Some(px(30.0)),
                    font_weight: Some(FontWeight::Bold),
                    border_top_left_radius: Some(px(36.0)),
                    border_top_right_radius: Some(px(36.0)),
                    border_bottom_left_radius: Some(px(36.0)),
                    border_bottom_right_radius: Some(px(36.0)),
                    ..Default::default()
                }) { avatar_letter }
            }
            text(style = StyleRules {
                align_self: Some(runtime_core::AlignSelf::Stretch),
                font_size: Some(px(11.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(theme::TEXT_MUTED)),
                ..Default::default()
            }) { "DISPLAY NAME — SHOWN INSTEAD OF YOUR USERNAME".to_string() }
            { crate::controls::text_field(display_name, &username, input_style.clone()) }
            text(style = StyleRules {
                align_self: Some(runtime_core::AlignSelf::Stretch),
                font_size: Some(px(11.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(theme::TEXT_MUTED)),
                ..Default::default()
            }) { "ABOUT ME".to_string() }
            { crate::controls::text_field(about, "A line about yourself", input_style.clone()) }
            view(style = StyleRules {
                height: Some(px(34.0)),
                padding_left: Some(px(16.0)),
                padding_right: Some(px(16.0)),
                background: Some(col(theme::ACCENT)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                border_top_left_radius: Some(px(8.0)),
                border_top_right_radius: Some(px(8.0)),
                border_bottom_left_radius: Some(px(8.0)),
                border_bottom_right_radius: Some(px(8.0)),
                flex_shrink: Some(grow(0.0)),
                ..Default::default()
            }, test_id = "profile-save") {
                text(style = StyleRules {
                    font_size: Some(px(13.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    color: Some(col(theme::TEXT_ON_ACCENT)),
                    ..Default::default()
                }) { move || if profile_busy.get() { "Saving…".to_string() } else { "Save Profile".to_string() } }
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    do_save();
                }
                TouchResponse::CONSUMED
            })
            if !profile_note.get().is_empty() {
                text(style = StyleRules {
                    font_size: Some(px(12.0)),
                    color: Some(col(theme::TEXT_MUTED)),
                    ..Default::default()
                }) { move || profile_note.get().clone() }
            }
            // The "Change avatar" pill: tap it, the platform's native file-open
            // dialog appears, and the chosen image is uploaded on the fly.
            // The label tells the user which phase the upload is in
            // (reactive closures re-run when `av_busy` / `av_picking` flip).
            view(style = StyleRules {
                height: Some(px(34.0)),
                background: Some(col(theme::SUCCESS)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                border_top_left_radius: Some(px(8.0)),
                border_top_right_radius: Some(px(8.0)),
                border_bottom_left_radius: Some(px(8.0)),
                border_bottom_right_radius: Some(px(8.0)),
                flex_shrink: Some(grow(0.0)),
                ..Default::default()
            }, test_id = "profile-change-avatar") {
                text(style = StyleRules {
                    font_size: Some(px(13.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    color: Some(col(theme::TEXT_STRONG)),
                    ..Default::default()
                }) {
                    move ||
                        if av_picking.get() {
                            "Choose a file…".to_string()
                        } else if av_busy.get() {
                            "Saving…".to_string()
                        } else {
                            "Change avatar".to_string()
                        }
                }
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    do_change();
                }
                TouchResponse::CONSUMED
            })
            view(style = StyleRules {
                height: Some(px(34.0)),
                background: Some(col(theme::SUCCESS)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                border_top_left_radius: Some(px(8.0)),
                border_top_right_radius: Some(px(8.0)),
                border_bottom_left_radius: Some(px(8.0)),
                border_bottom_right_radius: Some(px(8.0)),
                flex_shrink: Some(grow(0.0)),
                ..Default::default()
            }, test_id = "profile-remove-avatar") {
                text(style = StyleRules {
                    font_size: Some(px(13.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    color: Some(col(theme::TEXT_STRONG)),
                    ..Default::default()
                }) { move || if av_busy.get() { "Removing…".to_string() } else { "Remove".to_string() } }
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    do_remove();
                }
                TouchResponse::CONSUMED
            })
            if !av_error.get().is_empty() {
                text(style = StyleRules {
                    font_size: Some(px(12.0)),
                    color: Some(col(theme::DANGER)),
                    ..Default::default()
                }) { move || av_error.get().clone() }
            }
        }
    }
}

runtime_vocabulary::robot_only! {
/// Register `avatar.upload {mime, data_b64}` with the robot: the picker
/// callback's validate → encode → `set_avatar` pipeline minus the native
/// file-open dialog (which an in-page test cannot tap). `data_b64` is the
/// raw image bytes base64-encoded.
pub(crate) fn register_avatar_upload_hook(
    avatar_url: runtime_core::Signal<String>,
    busy: runtime_core::Signal<bool>,
    err: runtime_core::Signal<String>,
) {
    let hook: RobotCall = Rc::new(
        move |args: &robot_json::Value| -> Result<RobotReply, String> {
            let mime = args.get("mime").and_then(|v| v.as_str()).ok_or("avatar.upload: 'mime' must be a string")?;
            let data_b64 = args.get("data_b64").and_then(|v| v.as_str()).ok_or("avatar.upload: 'data_b64' must be a string")?;
            let bytes = b64_decode(data_b64)?;
            let url = validate_and_encode(mime, &bytes)?;
            if busy.get() {
                return Err("an avatar upload is already in flight".to_string());
            }
            busy.set(true);
            err.set(String::new());
            runtime_core::driver::spawn_async(async move {
                let url_for_save = url.clone();
                match crate::auth::set_avatar(Some(url)).await {
                    Ok(_) => {
                        let name = crate::auth::current_username();
                        crate::auth::remember_avatar(name.as_str(), Some(url_for_save.clone()));
                        crate::auth::refresh_saved_avatar(Some(url_for_save.clone()));
                        avatar_url.set(url_for_save);
                    }
                    Err(msg) => err.set(msg),
                }
                busy.set(false);
            });
            let reply = RobotReply::Ready(robot_json::Value::Null);
            Ok(reply)
        },
    );
    robot_register(
        RobotFunction {
            name: "avatar.upload",
            doc: "Upload an avatar for the signed-in account from base64 image bytes ({mime, data_b64}) — the picker callback's encode+save path, for tests that cannot drive the native file dialog.",
            args: &[
                RobotArg { name: "mime", ty: "String" },
                RobotArg { name: "data_b64", ty: "String" },
            ],
            returns: "()",
            is_async: false,
            call: Some(hook),
        },
    );
}
}

#[cfg(test)]
mod tests {
    // Encode/validate half of the upload path — the functions the picker
    // callback (and the robot hook) run. The picker dialog itself is OS UI,
    // exercised per the file-picker SDK tests.
    use super::{b64_encode, b64_decode, validate_and_encode, accepted_mime, AVATAR_URL_MAX_BYTES};

    #[test]
    fn b64_encode_matches_reference_vectors() {
        assert!(b64_encode("".as_bytes()) == "");
        assert!(b64_encode("f".as_bytes()) == "Zg==");
        assert!(b64_encode("fo".as_bytes()) == "Zm8=");
        assert!(b64_encode("foo".as_bytes()) == "Zm9v");
        assert!(b64_encode("foob".as_bytes()) == "Zm9vYg==");
        assert!(b64_encode("fooba".as_bytes()) == "Zm9vYmE=");
    }

    #[test]
    fn b64_roundtrips_through_decode() {
        for len in [0, 1, 2, 3, 4, 5, 255, 256] {
            let data: Vec<u8> = (0..len).map(|i| (((i * 31 + 7) % 256) as u32) as u8).collect();
            let enc = b64_encode(data.as_slice());
            assert!(b64_decode(&enc) == Ok(data));
        }
    }

    #[test]
    fn accepted_mime_covers_backend_set_with_sniffing() {
        assert!(accepted_mime("image/png", "".as_bytes()) == Some("image/png"));
        assert!(accepted_mime("IMAGE/JPG", "".as_bytes()) == Some("image/jpeg"));
        assert!(accepted_mime("application/octet-stream", &[0x89, 0x50, 0x4E, 0x47]) == Some("image/png"));
        assert!(accepted_mime("text/plain", "hello".as_bytes()) == None);
    }

    #[test]
    fn regression_size_cap_enforced_before_rpc() {
        // Just under the cap: accepted; over: rejected with the backend message.
        let room = AVATAR_URL_MAX_BYTES - "data:image/png;base64,".len();
        let fits = "A".repeat(room / 4 * 3);
        assert!(validate_and_encode("image/png", fits.as_bytes()).is_ok());
        let big = fits + "AAAA";
        let res = validate_and_encode("image/png", big.as_bytes());
        assert!(res.unwrap_err() == "avatar too large (max 256 KB)");
    }
}
