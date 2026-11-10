//! The "Stickers" section of Server Settings (with Manage Expressions):
//! upload an image as a named sticker with a description and a related emoji
//! (what search matches), edit or delete one, and see how many of the
//! server's 30 are used.

use crate::backend::{Sticker, STICKERS_MAX_PER_SERVER};
use discord_server::stickers::{description_ok, name_ok, related_ok, MAX_BYTES};
use file_picker::{FilePicker, MediaKind, PickOutcome, PickRequest};
use runtime_core::{
    component, signal, ui, AlignItems, Element, FlexDirection, FontWeight, ObjectFit, Signal, StyleRules, Tokenized,
};

use crate::controls::{button_view, text_field, ButtonKind};
use crate::style_helpers::{col, px};
use crate::theme;

fn reload(server_id: i64, list: Signal<Vec<Sticker>>, alive: runtime_core::ScopeAlive) {
    runtime_core::spawn_then_in(&alive, crate::backend::list_stickers(server_id), move |result| {
        if let Ok(l) = result {
            list.set(l);
        }
    });
}

fn input_style(width: f32) -> StyleRules {
    StyleRules {
        width: Some(px(width)),
        min_width: Some(px(0.0)),
        ..crate::style_helpers::text_field_style()
    }
}

/// What is wrong with a sticker's fields, if anything.
fn check(name: &str, description: &str, related: &str) -> Option<&'static str> {
    if !name_ok(name) {
        return Some("Name it first: 2–30 characters, no colons or angle brackets");
    }
    if !related_ok(related) {
        return Some("Pick one related emoji (like 👋) or a :custom: emoji name");
    }
    if !description_ok(description) {
        return Some("Descriptions are up to 100 characters");
    }
    None
}

#[component]
fn StickerRow(
    #[prop(static)] id: i64,
    #[prop(static)] name: String,
    #[prop(static)] related_emoji: String,
    #[prop(static)] description: String,
    #[prop(static)] image: String,
    #[prop(static)] sid: i64,
    list: Signal<Vec<Sticker>>,
    error: Signal<String>,
) -> Element {
    // Props are moved out on every reactive rebuild; the signal handles are
    // cloned once so the save/delete handlers can hold them.
    let (list, error) = (list.clone(), error.clone());
    let name = signal(name);
    let related = signal(related_emoji);
    let description = signal(description);
    let al_rows = runtime_core::ScopeAlive::current();
    let al_save = al_rows.clone();
    let save = move || {
        let (n, r, d) = (name.get().trim().to_string(), related.get().trim().to_string(), description.get().trim().to_string());
        if let Some(msg) = check(&n, &d, &r) {
            error.set(msg.to_string());
            return;
        }
        let al = al_save.clone();
        runtime_core::spawn_then_in(&al.clone(), crate::backend::edit_sticker(id, n, d, r), move |result| match result {
            Ok(_) => {
                error.set(String::new());
                reload(sid, list, al);
            }
            Err(err) => error.set(err),
        });
    };
    let al_delete = al_rows.clone();
    let delete = move || {
        let al = al_delete.clone();
        runtime_core::spawn_then_in(&al.clone(), crate::backend::delete_sticker(id), move |result| match result {
            Ok(()) => reload(sid, list, al),
            Err(err) => error.set(err),
        });
    };
    let img = ui! {
        image(
            src = crate::auth::media_url(image),
            alt = name.get(),
            style = StyleRules {
                width: Some(px(64.0)),
                height: Some(px(64.0)),
                flex_shrink: Some(Tokenized::Literal(0.0)),
                object_fit: Some(ObjectFit::Contain),
                ..Default::default()
            },
        )
    };
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(10.0)),
            padding_top: Some(px(4.0)),
            padding_bottom: Some(px(4.0)),
            ..Default::default()
        }, test_id = "sticker-row") {
            { img }
            view(style = StyleRules { flex_direction: Some(FlexDirection::Column), gap: Some(px(6.0)), flex_grow: Some(Tokenized::Literal(1.0)), min_width: Some(px(0.0)), ..Default::default() }) {
                view(style = StyleRules { flex_direction: Some(FlexDirection::Row), gap: Some(px(6.0)), ..Default::default() }) {
                    { text_field(name, "name", input_style(180.0)) }
                    { text_field(related, "emoji", input_style(80.0)) }
                }
                { text_field(description, "description", input_style(266.0)) }
            }
            { button_view("Save", ButtonKind::Secondary, save) }
            { button_view("Delete", ButtonKind::Secondary, delete) }
        }
    }
}

#[component]
pub fn StickerSection(#[prop(static)] server_id: Option<i64>) -> Element {
    let Some(sid) = server_id else {
        return ui! { view {} };
    };
    let list: Signal<Vec<Sticker>> = signal(Vec::new());
    let name = signal(String::new());
    let related = signal(String::new());
    let description = signal(String::new());
    let error = signal(String::new());
    let busy = signal(false);
    // Replies land in this section's signals, so they are anchored to it.
    let alive = runtime_core::ScopeAlive::current();
    reload(sid, list, alive.clone());

    // Pick an image and upload it with the typed name, emoji and description.
    let al_upload = alive.clone();
    let upload = move || {
        if busy.get() {
            return;
        }
        let (n, r, d) = (name.get().trim().to_string(), related.get().trim().to_string(), description.get().trim().to_string());
        if let Some(msg) = check(&n, &d, &r) {
            error.set(msg.to_string());
            return;
        }
        busy.set(true);
        error.set(String::new());
        let al = al_upload.clone();
        runtime_core::spawn_then_in(
            &al.clone(),
            async move {
                let file = match FilePicker::new().pick(PickRequest::media(MediaKind::Images)).await {
                    Ok(PickOutcome::Picked(files)) => files.into_iter().next(),
                    Ok(_) => None,
                    Err(e) => return Some(Err(format!("file picker: {e}"))),
                };
                let file = file?;
                let bytes = match file.read_all().await {
                    Ok(bytes) => bytes,
                    Err(e) => return Some(Err(format!("could not read the file: {e}"))),
                };
                let data_url = match encode(file.mime(), &bytes) {
                    Ok(data_url) => data_url,
                    Err(msg) => return Some(Err(msg)),
                };
                Some(crate::backend::upload_sticker(sid, n, d, r, data_url).await.map(|_| ()))
            },
            move |outcome| {
                match outcome {
                    Some(Ok(())) => {
                        name.set(String::new());
                        related.set(String::new());
                        description.set(String::new());
                        reload(sid, list, al);
                    }
                    Some(Err(msg)) => error.set(msg),
                    None => {}
                }
                busy.set(false);
            },
        );
    };

    let rows = ui! {
        match list.get() {
            items => {{
                let count = format!("{} of {STICKERS_MAX_PER_SERVER} used", items.len());
                ui! {
                    view(style = StyleRules { flex_direction: Some(FlexDirection::Column), ..Default::default() }) {
                        text(style = StyleRules {
                            font_size: Some(px(12.0)),
                            font_weight: Some(FontWeight::Bold),
                            color: Some(col(theme::TEXT_MUTED)),
                            margin_bottom: Some(px(4.0)),
                            ..Default::default()
                        }) { count }
                        for s in items.iter(), key = s.id {
                            StickerRow(
                                id = s.id,
                                name = s.name.clone(),
                                related_emoji = s.related_emoji.clone(),
                                description = s.description.clone(),
                                image = s.image.clone(),
                                sid = sid,
                                list = list.clone(),
                                error = error.clone(),
                            )
                        }
                    }
                }
            }},
        }
    };

    ui! {
        view(style = StyleRules { flex_direction: Some(FlexDirection::Column), gap: Some(px(16.0)), max_width: Some(px(660.0)), ..Default::default() }) {
            text(style = StyleRules { font_size: Some(px(12.0)), color: Some(col(theme::TEXT_MUTED)), ..Default::default() }) {
                "Anyone in the server can send these from the sticker picker, here and in DMs. PNG, APNG, GIF or WebP, up to 512 KB; shown at 160×160.".to_string()
            }
            view(style = StyleRules { flex_direction: Some(FlexDirection::Row), gap: Some(px(8.0)), align_items: Some(AlignItems::Center), ..Default::default() }) {
                { text_field(name, "Sticker name", input_style(180.0)) }
                { text_field(related, "Related emoji", input_style(110.0)) }
            }
            view(style = StyleRules { flex_direction: Some(FlexDirection::Row), gap: Some(px(8.0)), align_items: Some(AlignItems::Center), ..Default::default() }) {
                { text_field(description, "Description (optional)", input_style(298.0)) }
                { button_view("Upload Image", ButtonKind::Primary, upload) }
            }
            text(style = StyleRules { font_size: Some(px(12.0)), color: Some(col(theme::DANGER)), ..Default::default() }) {
                move || error.get()
            }
            { rows }
        }
    }
}

/// Check a picked sticker image and turn it into the `data:` URL the upload
/// takes. Stickers are PNG (APNG included), GIF or WebP up to 512 KB.
pub(crate) fn encode(mime: &str, bytes: &[u8]) -> Result<String, String> {
    let kind = crate::profile_settings::accepted_mime(mime, bytes)
        .filter(|k| *k != "image/jpeg")
        .ok_or("Stickers must be PNG, APNG, GIF or WebP")?;
    if bytes.len() > MAX_BYTES {
        return Err("Sticker too large (max 512 KB)".to_string());
    }
    Ok(format!("data:{};base64,{}", kind, crate::profile_settings::b64_encode(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_are_checked_like_the_server_does() {
        assert_eq!(check("wave", "", "👋"), None);
        assert!(check("w", "", "👋").is_some(), "too short");
        assert!(check("wave", "", "").is_some(), "needs an emoji");
        assert!(check("wave", &"x".repeat(101), "👋").is_some(), "description too long");
        assert_eq!(check("wave", "", ":party_parrot:"), None);
    }

    #[test]
    fn jpeg_and_oversize_images_are_refused() {
        let png = b"\x89PNG\r\n\x1a\n0000";
        assert!(encode("image/png", png).unwrap().starts_with("data:image/png;base64,"));
        assert!(encode("image/jpeg", b"\xff\xd8\xff\xe0").is_err());
        let mut big = png.to_vec();
        big.resize(MAX_BYTES + 1, 0);
        assert!(encode("image/png", &big).is_err());
    }
}
