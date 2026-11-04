//! The "Emoji" section of Server Settings (with Manage Expressions): upload
//! an image as a named emoji, rename or delete one, and see how many of the
//! server's 50 are used.

use discord_server::server_emoji::{self as se, ServerEmoji, MAX_PER_SERVER};
use file_picker::{FilePicker, MediaKind, PickOutcome, PickRequest};
use runtime_core::{
    signal, switch, ui, AlignItems, Element, FlexDirection, FontWeight, ObjectFit, Signal, StyleRules, Tokenized,
};

use crate::controls::{button_view, text_field, ButtonKind};
use crate::style_helpers::{col, px};
use crate::theme;

fn reload(server_id: i64, list: Signal<Vec<ServerEmoji>>, alive: runtime_core::ScopeAlive) {
    runtime_core::spawn_then_in(&alive, se::server_emoji(se::ServerEmojiInput { server_id }), move |result| {
        if let Ok(l) = result {
            list.set(l);
        }
    });
}

fn input_style() -> StyleRules {
    StyleRules {
        flex_grow: Some(Tokenized::Literal(1.0)),
        min_width: Some(px(0.0)),
        ..crate::style_helpers::text_field_style()
    }
}

/// One emoji row: the image, a rename draft field, and Rename/Delete buttons.
fn emoji_row(
    e: ServerEmoji,
    sid: i64,
    list: Signal<Vec<ServerEmoji>>,
    error: Signal<String>,
) -> Element {
    let id = e.id;
    let draft = signal(e.name.clone());
    let original = e.name.clone();
    let al_rows = runtime_core::ScopeAlive::current();
    let al_rename = al_rows.clone();
    let rename = move || {
        let new = draft.get().trim().to_string();
        if new == original {
            return;
        }
        let al = al_rename.clone();
        runtime_core::spawn_then_in(
            &al.clone(),
            se::rename_emoji(se::RenameEmojiInput { emoji_id: id, name: new }),
            move |result| match result {
                Ok(_) => {
                    error.set(String::new());
                    reload(sid, list, al);
                }
                Err(err) => error.set(crate::errors::friendly(&err)),
            },
        );
    };
    let al_delete = al_rows.clone();
    let delete = move || {
        let al = al_delete.clone();
        runtime_core::spawn_then_in(
            &al.clone(),
            se::delete_emoji(se::DeleteEmojiInput { emoji_id: id }),
            move |result| match result {
                Ok(()) => reload(sid, list, al),
                Err(err) => error.set(crate::errors::friendly(&err)),
            },
        );
    };
    let img = ui! {
        image(
            src = crate::auth::media_url(e.image.clone()),
            alt = format!(":{}:", e.name),
            style = StyleRules {
                width: Some(px(32.0)),
                height: Some(px(32.0)),
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
        }) {
            { img }
            { text_field(draft, "name", input_style()) }
            { button_view("Rename", ButtonKind::Secondary, rename) }
            { button_view("Delete", ButtonKind::Secondary, delete) }
        }
    }
}

pub fn emoji_section(server_id: Option<i64>) -> Element {
    let Some(sid) = server_id else {
        return ui! { view {} };
    };
    let list: Signal<Vec<ServerEmoji>> = signal(crate::custom_emoji::list());
    let name = signal(String::new());
    let error = signal(String::new());
    let busy = signal(false);
    // Replies land in this section's signals, so they are anchored to it:
    // if the section is torn down mid-flight the callback is simply dropped.
    // Each `move` handler owns its own copy of the token (ScopeAlive is
    // Clone, not Copy).
    let alive = runtime_core::ScopeAlive::current();
    reload(sid, list, alive.clone());

    // Pick an image and upload it under the typed name.
    let al_upload = alive.clone();
    let upload = move || {
        if busy.get() {
            return;
        }
        let n = name.get().trim().to_string();
        if !se::name_ok(&n) {
            error.set("Name it first: 2–32 letters, digits or underscores".to_string());
            return;
        }
        busy.set(true);
        error.set(String::new());
        let al = al_upload.clone();
        // The pick, read, check and upload all happen in the future; the
        // callback only writes the section's signals. `None` = nothing picked.
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
                let data_url = match crate::profile_settings::validate_and_encode_small_image(file.mime(), &bytes, "emoji") {
                    Ok(data_url) => data_url,
                    Err(msg) => return Some(Err(msg)),
                };
                let input = se::UploadEmojiInput { server_id: sid, name: n, data_url };
                Some(se::upload_emoji(input).await.map(|_| ()).map_err(|e| crate::errors::friendly(&e)))
            },
            move |outcome| {
                match outcome {
                    Some(Ok(())) => {
                        name.set(String::new());
                        reload(sid, list, al);
                    }
                    Some(Err(msg)) => error.set(msg),
                    None => {}
                }
                busy.set(false);
            },
        );
    };

    let rows = switch(
        move || list.get(),
        move |items: &Vec<ServerEmoji>| {
            let count = format!("{} of {MAX_PER_SERVER} used", items.len());
            ui! {
                view(style = StyleRules { flex_direction: Some(FlexDirection::Column), ..Default::default() }) {
                    text(style = StyleRules {
                        font_size: Some(px(12.0)),
                        font_weight: Some(FontWeight::Bold),
                        color: Some(col(theme::TEXT_MUTED)),
                        margin_bottom: Some(px(4.0)),
                        ..Default::default()
                    }) { count }
                    for e in items.iter(), key = e.id {
                        { emoji_row(e.clone(), sid, list, error) }
                    }
                }
            }
        },
    );

    ui! {
        view(style = StyleRules { flex_direction: Some(FlexDirection::Column), gap: Some(px(16.0)), max_width: Some(px(660.0)), ..Default::default() }) {
            text(style = StyleRules { font_size: Some(px(12.0)), color: Some(col(theme::TEXT_MUTED)), ..Default::default() }) {
                "Anyone in the server can use these in messages (:name:) and reactions. PNG, GIF or WebP, up to 256 KB.".to_string()
            }
            view(style = StyleRules { flex_direction: Some(FlexDirection::Row), gap: Some(px(8.0)), align_items: Some(AlignItems::Center), ..Default::default() }) {
                { text_field(name, "Emoji name", input_style()) }
                { button_view("Upload Image", ButtonKind::Primary, upload) }
            }
            text(style = StyleRules { font_size: Some(px(12.0)), color: Some(col(theme::DANGER)), ..Default::default() }) {
                move || error.get()
            }
            { rows }
        }
    }
}
