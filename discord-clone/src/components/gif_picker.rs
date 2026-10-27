//! `gif_picker` — the composer's GIF picker card (Discord's GIF button).
//!
//! A dark rounded card the size of the emoji picker: a search field up top,
//! a two-column masonry grid of GIFs (Giphy's trending GIFs until something
//! is typed), "Load more" at the end of the grid, and the "Powered by GIPHY"
//! attribution Giphy asks for. Tapping a GIF hands it to `on_pick`; the
//! composer sends it at once.
//!
//! Results come from `discord_server::gifs::gif_search`, which talks to Giphy
//! on the server (the API key never reaches the browser) and caches answers.
//! Typing is debounced (`DEBOUNCE_MS`) and every request carries a generation
//! number, so only the newest search's answer lands; a development key's
//! 100 calls an hour is not spent one keystroke at a time.
use crate::theme;
use crate::style_helpers::{borderless_input, col, px};
use discord_server::gifs::{Gif, GifPage, GifSearchInput};
use icons_lucide::SEARCH;
use runtime_core::{
    after_ms_detached, component, icon, signal, switch, ui, AlignItems, Color, Element,
    FlexDirection, JustifyContent, ObjectFit, Overflow, Shadow, Signal, StyleRules, TouchPhase,
    TouchResponse, Tokenized,
};
use std::rc::Rc;

// The emoji picker's palette, so the two cards match.

const CARD_W: f32 = 420.0;
const CARD_H: f32 = 440.0;
const PAD: f32 = 12.0;
const GAP: f32 = 8.0;
/// One grid column's width: two columns and a gap inside the padded card.
const COL_W: f32 = (CARD_W - 2.0 * PAD - GAP) / 2.0;
const DEBOUNCE_MS: i32 = 400;

/// Split `gifs` into two columns, each GIF going to the shorter one (the
/// masonry layout), with its tile height at `COL_W` wide.
fn masonry(gifs: &[Gif]) -> [Vec<(Gif, f32)>; 2] {
    let mut cols: [Vec<(Gif, f32)>; 2] = [Vec::new(), Vec::new()];
    let mut heights = [0.0f32; 2];
    for g in gifs {
        let h = (g.preview_height as f32 * COL_W / g.preview_width.max(1) as f32).clamp(60.0, 360.0);
        let c = if heights[0] <= heights[1] { 0 } else { 1 };
        heights[c] += h + GAP;
        cols[c].push((g.clone(), h));
    }
    cols
}

/// The picker's results so far, the offset of the next page, and a status
/// line ("" while results show).
#[derive(Clone, PartialEq, Default)]
struct Results {
    gifs: Vec<Gif>,
    next: Option<u32>,
    status: String,
}

/// Ask the server for `query` at `offset`; the answer replaces (offset 0) or
/// extends the results, unless a newer request (`gen`) was made meanwhile.
fn fetch(query: String, offset: u32, results: Signal<Results>, gen: Signal<u32>) {
    let mine = gen.get() + 1;
    gen.set(mine);
    if offset == 0 {
        results.set(Results { status: "Loading…".to_string(), ..Default::default() });
    }
    runtime_core::driver::spawn_async(async move {
        let answer = discord_server::gifs::gif_search(GifSearchInput { query, offset }).await;
        if gen.get() != mine {
            return;
        }
        let mut r = if offset == 0 { Results::default() } else { results.get() };
        match answer {
            Ok(GifPage { gifs, next_offset }) => {
                r.gifs.extend(gifs);
                r.next = next_offset;
                r.status = if r.gifs.is_empty() { "No GIFs found".to_string() } else { String::new() };
            }
            Err(e) => {
                r.next = None;
                let msg = e.to_string();
                r.status = msg.strip_prefix("server fn failed: ").unwrap_or(&msg).to_string();
            }
        }
        results.set(r);
    });
}

#[component]
pub fn GifPicker(on_pick: Option<Rc<dyn Fn(Gif)>>) -> Element {
    let query = signal(String::new());
    let results = signal(Results::default());
    let gen = signal(0u32);

    // Trending on open; a search DEBOUNCE_MS after the last keystroke.
    fetch(String::new(), 0, results, gen);
    let on_change = move |s: String| {
        query.set(s.clone());
        let typed = gen.get() + 1;
        gen.set(typed);
        after_ms_detached(DEBOUNCE_MS, move || {
            if gen.get() == typed {
                fetch(s.trim().to_string(), 0, results, gen);
            }
        });
    };

    let grid = switch(
        move || results.get(),
        move |r: &Results| {
            if !r.status.is_empty() {
                let status = r.status.clone();
                return ui! {
                    view(style = StyleRules {
                        height: Some(px(200.0)),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        padding_left: Some(px(24.0)),
                        padding_right: Some(px(24.0)),
                        ..Default::default()
                    }) {
                        text(style = StyleRules { font_size: Some(px(14.0)), color: Some(col(theme::TEXT_MUTED)), ..Default::default() }) { status }
                    }
                };
            }
            let columns: Vec<Element> = masonry(&r.gifs)
                .into_iter()
                .map(|col_gifs| {
                    let tiles: Vec<Element> = col_gifs
                        .into_iter()
                        .map(|(gif, h)| {
                            let pick = on_pick.clone();
                            let (src, alt) = (gif.preview_url.clone(), gif.title.clone());
                            ui! {
                                view(style = StyleRules {
                                    width: Some(px(COL_W)),
                                    height: Some(px(h)),
                                    background: Some(col(theme::BG_PRIMARY)),
                                    border_top_left_radius: Some(px(6.0)),
                                    border_top_right_radius: Some(px(6.0)),
                                    border_bottom_left_radius: Some(px(6.0)),
                                    border_bottom_right_radius: Some(px(6.0)),
                                    overflow: Some(Overflow::Hidden),
                                    ..Default::default()
                                }) {
                                    image(
                                        src = src,
                                        alt = alt,
                                        style = StyleRules {
                                            width: Some(px(COL_W)),
                                            height: Some(px(h)),
                                            object_fit: Some(ObjectFit::Cover),
                                            ..Default::default()
                                        },
                                    )
                                }
                                .on_touch(move |ev| {
                                    if ev.phase == TouchPhase::Ended {
                                        if let Some(p) = &pick {
                                            (p)(gif.clone());
                                        }
                                    }
                                    TouchResponse::CONSUMED
                                })
                            }
                        })
                        .collect();
                    ui! {
                        view(style = StyleRules {
                            width: Some(px(COL_W)),
                            flex_direction: Some(FlexDirection::Column),
                            gap: Some(px(GAP)),
                            ..Default::default()
                        }) { tiles }
                    }
                })
                .collect();
            let more: Element = match r.next {
                Some(next) => ui! {
                    view(style = StyleRules {
                        height: Some(px(36.0)),
                        margin_top: Some(px(GAP)),
                        align_items: Some(AlignItems::Center),
                        justify_content: Some(JustifyContent::Center),
                        background: Some(col(theme::BG_PRIMARY)),
                        border_top_left_radius: Some(px(6.0)),
                        border_top_right_radius: Some(px(6.0)),
                        border_bottom_left_radius: Some(px(6.0)),
                        border_bottom_right_radius: Some(px(6.0)),
                        ..Default::default()
                    }) {
                        text(style = StyleRules { font_size: Some(px(14.0)), color: Some(col(theme::TEXT_STRONG)), ..Default::default() }) { "Load more".to_string() }
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            fetch(query.get().trim().to_string(), next, results, gen);
                        }
                        TouchResponse::CONSUMED
                    })
                },
                None => ui! { view {} },
            };
            ui! {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Column),
                    padding_left: Some(px(PAD)),
                    padding_right: Some(px(PAD)),
                    padding_bottom: Some(px(PAD)),
                    ..Default::default()
                }) {
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Row),
                        align_items: Some(AlignItems::FlexStart),
                        gap: Some(px(GAP)),
                        ..Default::default()
                    }) { columns }
                    { more }
                }
            }
        },
    );

    ui! {
        view(style = StyleRules {
            width: Some(px(CARD_W)),
            height: Some(px(CARD_H)),
            flex_direction: Some(FlexDirection::Column),
            background: Some(col(theme::BG_SECONDARY)),
            border_top_left_radius: Some(px(10.0)),
            border_top_right_radius: Some(px(10.0)),
            border_bottom_left_radius: Some(px(10.0)),
            border_bottom_right_radius: Some(px(10.0)),
            border_top_width: Some(Tokenized::Literal(1.0)),
            border_right_width: Some(Tokenized::Literal(1.0)),
            border_bottom_width: Some(Tokenized::Literal(1.0)),
            border_left_width: Some(Tokenized::Literal(1.0)),
            border_top_color: Some(col(theme::BG_TERTIARY)),
            border_right_color: Some(col(theme::BG_TERTIARY)),
            border_bottom_color: Some(col(theme::BG_TERTIARY)),
            border_left_color: Some(col(theme::BG_TERTIARY)),
            shadow: Some(Shadow { x: 0.0, y: 8.0, blur: 24.0, color: col(theme::SHADOW) }),
            overflow: Some(Overflow::Hidden),
            ..Default::default()
        }, test_id = "gif-picker") {
            // Search field (the emoji picker's shape).
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                margin_top: Some(px(PAD)),
                margin_right: Some(px(PAD)),
                margin_bottom: Some(px(PAD)),
                margin_left: Some(px(PAD)),
                gap: Some(px(8.0)),
                height: Some(px(38.0)),
                flex_shrink: Some(Tokenized::Literal(0.0)),
                background: Some(col(theme::BG_PRIMARY)),
                padding_left: Some(px(10.0)),
                padding_right: Some(px(10.0)),
                border_top_left_radius: Some(px(6.0)),
                border_top_right_radius: Some(px(6.0)),
                border_bottom_left_radius: Some(px(6.0)),
                border_bottom_right_radius: Some(px(6.0)),
                ..borderless_input()
            }) {
                icon(SEARCH).color(|| Color::from(theme::TEXT_MUTED)).size(16.0)
                text_input(
                    value = query,
                    on_change = on_change,
                    placeholder = "Search GIPHY",
                    style = StyleRules {
                        flex_grow: Some(Tokenized::Literal(1.0)),
                        color: Some(col(theme::TEXT_STRONG)),
                        background: Some(col(theme::BG_PRIMARY)),
                        font_size: Some(px(14.0)),
                        ..borderless_input()
                    },
                )
            }

            scroll_view(style = StyleRules {
                flex_grow: Some(Tokenized::Literal(1.0)),
                min_height: Some(px(0.0)),
                ..Default::default()
            }) {
                grid
            }

            // Giphy's required attribution.
            view(style = StyleRules {
                height: Some(px(28.0)),
                flex_shrink: Some(Tokenized::Literal(0.0)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                border_top_width: Some(Tokenized::Literal(1.0)),
                border_top_color: Some(col(theme::BG_TERTIARY)),
                ..Default::default()
            }) {
                text(style = StyleRules { font_size: Some(px(11.0)), color: Some(col(theme::TEXT_MUTED)), ..Default::default() }) {
                    "Powered by GIPHY".to_string()
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gif(w: u32, h: u32) -> Gif {
        Gif {
            id: format!("{w}x{h}"),
            title: String::new(),
            preview_url: String::new(),
            preview_width: w,
            preview_height: h,
            url: String::new(),
            width: w,
            height: h,
        }
    }

    #[test]
    fn masonry_fills_the_shorter_column() {
        // A tall GIF goes left; the next two both fit under it on the right.
        let cols = masonry(&[gif(100, 300), gif(200, 100), gif(200, 100)]);
        assert_eq!(cols[0].len(), 1);
        assert_eq!(cols[1].len(), 2);
        assert!((cols[1][0].1 - COL_W / 2.0).abs() < 0.5);
    }
}
