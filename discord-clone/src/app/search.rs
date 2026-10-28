//! The header search box: Enter runs `search_messages` over the active
//! server (with `from:` / `in:` / `has:` … filters, see
//! `discord_server::search`) and lists the results in a dropdown; clicking a
//! result jumps to the message.

use crate::theme;
use discord_server::search::{SearchInput, SearchResults};
use super::*;

const FILTER_TIPS: &str =
    "Filters: from:user  mentions:user  in:channel  has:link / image / file  pinned:true  before: / after: / during:2026-10-28";

#[cfg(target_arch = "wasm32")]
fn tz_offset_minutes() -> i64 {
    -(js_sys::Date::new_0().get_timezone_offset() as i64)
}

#[cfg(not(target_arch = "wasm32"))]
fn tz_offset_minutes() -> i64 {
    0
}

/// The search box for the server in `server`. `on_jump(channel_id,
/// message_id)` opens a result.
pub(super) fn search_box(server: Signal<Option<i64>>, server_name: String, on_jump: Rc<dyn Fn(i64, i64)>) -> Element {
    let placeholder = if server_name.is_empty() { "Search".to_string() } else { format!("Search {server_name}") };
    let query = signal(String::new());
    let results: Signal<Option<SearchResults>> = signal(None);
    let busy = signal(false);
    let error = signal(String::new());

    // Run the query; `offset > 0` appends the next page.
    let run = move |offset: i64| {
        let Some(server_id) = server.get() else { return };
        let text = query.get();
        if text.trim().is_empty() {
            results.set(None);
            return;
        }
        busy.set(true);
        error.set(String::new());
        let input = SearchInput { server_id, query: text, tz_offset_minutes: tz_offset_minutes(), offset };
        runtime_core::driver::spawn_async(async move {
            match discord_server::search::search_messages(input).await {
                Ok(mut page) => {
                    if offset > 0 {
                        if let Some(prev) = results.get() {
                            let mut hits = prev.hits;
                            hits.extend(page.hits);
                            page.hits = hits;
                        }
                    }
                    results.set(Some(page));
                }
                Err(e) => error.set(crate::errors::friendly(&e)),
            }
            busy.set(false);
        });
    };
    let on_key = move |ev: &KeyEvent| match ev.key.as_str() {
        "Enter" => {
            run(0);
            KeyOutcome::PreventDefault
        }
        "Escape" => {
            results.set(None);
            KeyOutcome::PreventDefault
        }
        _ => KeyOutcome::Default,
    };

    let dropdown = switch(
        move || (results.get(), busy.get(), error.get()),
        move |(found, searching, err): &(Option<SearchResults>, bool, String)| {
            if found.is_none() && !*searching && err.is_empty() {
                return ui! { view(style = StyleRules { width: Some(px(0.0)), height: Some(px(0.0)), ..Default::default() }) {} };
            }
            let hits = found.as_ref().map(|r| r.hits.clone()).unwrap_or_default();
            let total = found.as_ref().map_or(0, |r| r.total);
            let heading = if *searching && hits.is_empty() {
                "Searching…".to_string()
            } else if !err.is_empty() {
                err.clone()
            } else {
                match total {
                    0 => "No results".to_string(),
                    1 => "1 result".to_string(),
                    n => format!("{n} results"),
                }
            };
            let muted = |size: f32| StyleRules { font_size: Some(px(size)), color: Some(col(theme::TEXT_MUTED)), ..Default::default() };
            let rows: Vec<Element> = hits
                .iter()
                .map(|hit| {
                    let jump = on_jump.clone();
                    let (cid, mid) = (hit.channel_id, hit.message_id);
                    let author = hit.author_display_name.clone().unwrap_or_else(|| hit.author_username.clone());
                    let mut body = crate::reply_reference_strip::excerpt_text(&hit.content);
                    if body.chars().count() > 300 {
                        body = body.chars().take(300).collect::<String>() + "…";
                    }
                    if body.is_empty() && hit.attachment_count > 0 {
                        body = "(attachment)".to_string();
                    }
                    let meta = format!("#{}  ·  {}", hit.channel_name, super::time_fmt::format_full_when(hit.created_at / 1000));
                    ui! {
                        view(style = StyleRules {
                            flex_direction: Some(FlexDirection::Column),
                            gap: Some(px(4.0)),
                            background: Some(col(theme::BG_PRIMARY)),
                            padding_top: Some(px(10.0)),
                            padding_bottom: Some(px(10.0)),
                            padding_left: Some(px(12.0)),
                            padding_right: Some(px(12.0)),
                            border_top_left_radius: Some(px(6.0)),
                            border_top_right_radius: Some(px(6.0)),
                            border_bottom_left_radius: Some(px(6.0)),
                            border_bottom_right_radius: Some(px(6.0)),
                            ..Default::default()
                        }) {
                            text(style = muted(12.0)) { meta }
                            text(style = StyleRules {
                                font_size: Some(px(14.0)),
                                font_weight: Some(FontWeight::SemiBold),
                                color: Some(col(theme::TEXT_STRONG)),
                                ..Default::default()
                            }) { author }
                            text(style = StyleRules { font_size: Some(px(14.0)), color: Some(col(theme::TEXT_NORMAL)), ..Default::default() }) { body }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                jump(cid, mid);
                                results.set(None);
                            }
                            TouchResponse::CONSUMED
                        })
                    }
                })
                .collect();
            let tips = (found.is_some() && hits.is_empty()).then(|| {
                ui! { text(style = muted(12.0)) { FILTER_TIPS.to_string() } }
            });
            let more = ((hits.len() as i64) < total).then(|| {
                let label = if *searching { "Loading…".to_string() } else { format!("Load more ({} left)", total - hits.len() as i64) };
                let next = hits.len() as i64;
                ui! {
                    view(style = StyleRules {
                        height: Some(px(32.0)),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        background: Some(col(theme::BUTTON_SECONDARY)),
                        border_top_left_radius: Some(px(4.0)),
                        border_top_right_radius: Some(px(4.0)),
                        border_bottom_left_radius: Some(px(4.0)),
                        border_bottom_right_radius: Some(px(4.0)),
                        ..Default::default()
                    }) {
                        text(style = StyleRules { font_size: Some(px(13.0)), color: Some(col(theme::TEXT_STRONG)), ..Default::default() }) { label }
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended && !busy.get() {
                            run(next);
                        }
                        TouchResponse::CONSUMED
                    })
                }
            });
            ui! {
                view(style = StyleRules {
                    position: Some(Position::Absolute),
                    top: Some(px(34.0)),
                    right: Some(px(0.0)),
                    width: Some(px(420.0)),
                    max_height: Some(px(520.0)),
                    flex_direction: Some(FlexDirection::Column),
                    background: Some(col(theme::BG_SECONDARY)),
                    border_top_left_radius: Some(px(8.0)),
                    border_top_right_radius: Some(px(8.0)),
                    border_bottom_left_radius: Some(px(8.0)),
                    border_bottom_right_radius: Some(px(8.0)),
                    shadow: Some(runtime_core::Shadow { x: 0.0, y: 8.0, blur: 24.0, color: col(theme::SHADOW) }),
                    // The shared popup level (`popups::POPUP_Z`).
                    z_index: Some(60),
                    overflow: Some(Overflow::Hidden),
                    ..Default::default()
                }) {
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::SpaceBetween),
                        height: Some(px(40.0)),
                        padding_left: Some(px(16.0)),
                        padding_right: Some(px(12.0)),
                        ..Default::default()
                    }) {
                        text(style = StyleRules {
                            font_size: Some(px(13.0)),
                            font_weight: Some(FontWeight::SemiBold),
                            color: Some(col(theme::TEXT_MUTED)),
                            ..Default::default()
                        }) { heading }
                        view(style = StyleRules { padding_left: Some(px(8.0)), padding_right: Some(px(4.0)), ..Default::default() }) {
                            text(style = StyleRules { font_size: Some(px(18.0)), color: Some(col(theme::TEXT_MUTED)), ..Default::default() }) { "×".to_string() }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                results.set(None);
                                error.set(String::new());
                            }
                            TouchResponse::CONSUMED
                        })
                    }
                    scroll_view(style = StyleRules { flex_shrink: Some(grow(1.0)), ..Default::default() }) {
                        view(style = StyleRules {
                            flex_direction: Some(FlexDirection::Column),
                            gap: Some(px(8.0)),
                            padding_left: Some(px(12.0)),
                            padding_right: Some(px(12.0)),
                            padding_bottom: Some(px(12.0)),
                            ..Default::default()
                        }) {
                            { rows }
                            if let Some(el) = tips {
                                el
                            }
                            if let Some(el) = more {
                                el
                            }
                        }
                    }
                }
            }
        },
    );

    ui! {
        view(style = StyleRules {
            position: Some(Position::Relative),
            width: Some(px(220.0)),
            ..Default::default()
        }) {
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                height: Some(px(28.0)),
                padding_left: Some(px(8.0)),
                padding_right: Some(px(8.0)),
                gap: Some(px(6.0)),
                background: Some(col(theme::BG_TERTIARY)),
                border_top_left_radius: Some(px(4.0)),
                border_top_right_radius: Some(px(4.0)),
                border_bottom_left_radius: Some(px(4.0)),
                border_bottom_right_radius: Some(px(4.0)),
                ..Default::default()
            }) {
                text_input(
                    value = query,
                    on_change = move |v: String| query.set(v),
                    placeholder = placeholder,
                    test_id = "message-search",
                    style = StyleRules {
                        flex_grow: Some(grow(1.0)),
                        font_size: Some(px(13.0)),
                        color: Some(col(theme::TEXT_STRONG)),
                        ..borderless_input()
                    },
                )
                .on_key_down(on_key)
                icon(icons_lucide::SEARCH).color(|| theme::live(theme::TEXT_MUTED)).size(14.0)
            }
            dropdown
        }
    }
}
