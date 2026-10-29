//! The "Emoji" section of Server Settings (with Manage Expressions): upload
//! an image as a named emoji, rename or delete one, and see how many of the
//! server's 50 are used.

use discord_server::server_emoji::{self as se, ServerEmoji, MAX_PER_SERVER};
use file_picker::{FilePicker, MediaKind, PickOutcome, PickRequest};
use runtime_core::{
    signal, switch, ui, AlignItems, Element, FlexDirection, FontWeight, ObjectFit, Signal, StyleRules, Tokenized,
};

use crate::controls::{button_view, text_field, ButtonKind};
use crate::style_helpers::{borderless_input, col, px, with_radius};
use crate::theme;

fn reload(server_id: i64, list: Signal<Vec<ServerEmoji>>) {
    runtime_core::driver::spawn_async(async move {
        if let Ok(l) = se::server_emoji(se::ServerEmojiInput { server_id }).await {
            list.set(l);
        }
    });
}

fn input_style() -> StyleRules {
    StyleRules {
        height: Some(px(34.0)),
        flex_grow: Some(Tokenized::Literal(1.0)),
        min_width: Some(px(0.0)),
        padding_left: Some(px(10.0)),
        padding_right: Some(px(10.0)),
        background: Some(col(theme::BG_TERTIARY)),
        color: Some(col(theme::TEXT_NORMAL)),
        ..with_radius(borderless_input(), 8.0)
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
    reload(sid, list);

    // Pick an image and upload it under the typed name.
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
        runtime_core::driver::spawn_async(async move {
            let picked = FilePicker::new().pick(PickRequest::media(MediaKind::Images)).await;
            let file = match picked {
                Ok(PickOutcome::Picked(files)) => files.into_iter().next(),
                Ok(_) => None,
                Err(e) => {
                    error.set(format!("file picker: {e}"));
                    None
                }
            };
            if let Some(file) = file {
                match file.read_all().await {
                    Ok(bytes) => match crate::profile_settings::validate_and_encode_small_image(file.mime(), &bytes, "emoji") {
                        Ok(data_url) => {
                            let input = se::UploadEmojiInput { server_id: sid, name: n, data_url };
                            match se::upload_emoji(input).await {
                                Ok(_) => {
                                    name.set(String::new());
                                    reload(sid, list);
                                }
                                Err(e) => error.set(crate::errors::friendly(&e)),
                            }
                        }
                        Err(msg) => error.set(msg),
                    },
                    Err(e) => error.set(format!("could not read the file: {e}")),
                }
            }
            busy.set(false);
        });
    };

    let rows = switch(
        move || list.get(),
        move |items: &Vec<ServerEmoji>| {
            let rows: Vec<Element> = items
                .iter()
                .map(|e| {
                    let id = e.id;
                    let draft = signal(e.name.clone());
                    let original = e.name.clone();
                    let rename = move || {
                        let new = draft.get().trim().to_string();
                        if new == original {
                            return;
                        }
                        runtime_core::driver::spawn_async(async move {
                            match se::rename_emoji(se::RenameEmojiInput { emoji_id: id, name: new }).await {
                                Ok(_) => {
                                    error.set(String::new());
                                    reload(sid, list);
                                }
                                Err(e) => error.set(crate::errors::friendly(&e)),
                            }
                        });
                    };
                    let delete = move || {
                        runtime_core::driver::spawn_async(async move {
                            match se::delete_emoji(se::DeleteEmojiInput { emoji_id: id }).await {
                                Ok(()) => reload(sid, list),
                                Err(e) => error.set(crate::errors::friendly(&e)),
                            }
                        });
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
                })
                .collect();
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
                    { rows }
                }
            }
        },
    );

    ui! {
        view(style = StyleRules { flex_direction: Some(FlexDirection::Column), gap: Some(px(12.0)), max_width: Some(px(560.0)), ..Default::default() }) {
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
