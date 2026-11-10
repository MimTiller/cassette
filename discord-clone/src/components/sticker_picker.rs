//! `sticker_picker` — the composer's sticker button and picker card.
//!
//! The card is the GIF picker's size and shape: a search field, then the
//! stickers of every server you are in, the open server's first, each under
//! its server's name. Search matches a sticker's name, description and
//! related emoji. Tapping one hands it to `on_pick`; the composer sends it at
//! once. Shared by the channel composer and the live DM composer.

use crate::backend::Sticker;
use crate::composer_icons::sticker_url;
use crate::style_helpers::{col, px, rounded};
use crate::theme;
use icons_lucide::SEARCH;
use runtime_core::{
    component, icon, memo, signal, ui, AlignItems, Element, FlexDirection, FlexWrap, FontWeight,
    JustifyContent, ObjectFit, Overflow, Signal, StyleRules, TouchPhase, TouchResponse, Tokenized,
};
use std::rc::Rc;

const CARD_W: f32 = 420.0;
const CARD_H: f32 = 440.0;
const PAD: f32 = 12.0;
const GAP: f32 = 8.0;
const TILE: f32 = 88.0;

/// One server's stickers, as the picker lists them.
#[derive(Clone, PartialEq)]
struct Group {
    server_id: i64,
    name: String,
    stickers: Vec<Sticker>,
}

/// The sticker button: opens/closes the picker (closing the others first),
/// beside the GIF and emoji buttons.
#[component]
pub(crate) fn StickerButton(
    on: Signal<bool>,
    #[prop(static)] close_others: Vec<Signal<bool>>,
) -> Element {
    let hovered = signal(false);
    ui! {
        view(style = move || StyleRules {
            width: Some(px(28.0)),
            height: Some(px(28.0)),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::Center),
            flex_shrink: Some(Tokenized::Literal(0.0)),
            background: Some(col(if hovered.get() { theme::BG_HOVER } else { "transparent" })),
            ..rounded(14.0)
        }, test_id = "sticker-button") {
            image(
                src = move || sticker_url(
                    theme::live(if hovered.get() { theme::INTERACTIVE } else { theme::TEXT_MUTED }).0
                ),
                alt = "Sticker",
                style = StyleRules { width: Some(px(24.0)), height: Some(px(24.0)), ..Default::default() },
            )
        }
        .on_hover(move |on_| hovered.set(on_))
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                for other in &close_others {
                    other.set(false);
                }
                on.set(!on.get());
            }
            TouchResponse::CONSUMED
        })
    }
}



/// Whether `s` matches the lower-cased search `q` (empty matches all).
fn matches(s: &Sticker, q: &str) -> bool {
    q.is_empty()
        || s.name.to_lowercase().contains(q)
        || s.description.to_lowercase().contains(q)
        || s.related_emoji.to_lowercase().contains(q)
}

/// A server's header line: its name in bold muted text, on a full row of its
/// own (the wide width forces the wrapping grid to break the line).
#[component]
fn StickerHeader(#[prop(static)] name: String) -> Element {
    ui! {
        view(style = StyleRules {
            width: Some(px(CARD_W - 2.0 * PAD)),
            align_items: Some(AlignItems::FlexStart),
            ..Default::default()
        }) {
            text(style = StyleRules {
                font_size: Some(px(12.0)),
                font_weight: Some(FontWeight::Bold),
                color: Some(col(theme::TEXT_MUTED)),
                ..Default::default()
            }) { name.to_uppercase() }
        }
    }
}

/// One tile of the grid: the sticker image, tap handing it to `on_pick`.
/// `on_pick` is a static prop holding the shared `Rc`; each cell clones it
/// into its own `on_touch`, like the emoji picker's cells.
#[component]
fn StickerTile(
    #[prop(static)] id: i64,
    #[prop(static)] server_id: i64,
    #[prop(static)] name: String,
    #[prop(static)] description: String,
    #[prop(static)] related_emoji: String,
    #[prop(static)] image: String,
    on_pick: Option<Rc<dyn Fn(Sticker)>>,
) -> Element {
    let (src, alt) = (crate::auth::media_url(image.clone()), name.clone());
    ui! {
        view(style = StyleRules {
            width: Some(px(TILE)),
            height: Some(px(TILE)),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::Center),
            ..rounded(6.0)
        }, test_id = "sticker-tile") {
            image(
                src = src,
                alt = alt,
                style = StyleRules {
                    width: Some(px(TILE - 8.0)),
                    height: Some(px(TILE - 8.0)),
                    object_fit: Some(ObjectFit::Contain),
                    ..Default::default()
                },
            )
        }
        .on_touch({
            let p = on_pick;
            let (sid, server, nm, desc, related, img) = (id, server_id, name, description, related_emoji, image);
            move |ev| {
                if ev.phase == TouchPhase::Ended {
                    if let Some(p) = &p {
                        (p)(Sticker {
                            id: sid,
                            server_id: server,
                            name: nm.clone(),
                            description: desc.clone(),
                            related_emoji: related.clone(),
                            image: img.clone(),
                        });
                    }
                }
                TouchResponse::CONSUMED
            }
        })
    }
}

/// One grid item: a server's header or one of its sticker cells, keyed so a
/// search narrows or widens the list without rebuilding the cells that stay.
#[derive(Clone, PartialEq)]
enum GridItem {
    Header { server_id: i64, name: String },
    Cell { server_id: i64, sticker: Sticker },
}

impl GridItem {
    fn key(&self) -> String {
        match self {
            GridItem::Header { server_id, .. } => format!("h:{server_id}"),
            GridItem::Cell { sticker, .. } => format!("c:{}", sticker.id),
        }
    }
}

/// The groups in picker order: the open server first, then the rest as the
/// rail lists them; servers with no stickers are left out.
fn ordered(mut groups: Vec<Group>, current: Option<i64>) -> Vec<Group> {
    groups.retain(|g| !g.stickers.is_empty());
    if let Some(cur) = current {
        if let Some(i) = groups.iter().position(|g| g.server_id == cur) {
            let g = groups.remove(i);
            groups.insert(0, g);
        }
    }
    groups
}

#[component]
pub fn StickerPicker(on_pick: Option<Rc<dyn Fn(Sticker)>>) -> Element {
    let query = signal(String::new());
    let groups: Signal<Option<Vec<Group>>> = signal(None);
    let current = crate::mentions::server_id();

    // Every server's stickers, fetched once per opening.
    let alive = runtime_core::ScopeAlive::current();
    runtime_core::spawn_then_in(
        &alive,
        async move {
            let servers = crate::backend::list_servers().await.unwrap_or_default();
            let mut out = Vec::new();
            for s in servers {
                let stickers = crate::backend::list_stickers(s.id).await.unwrap_or_default();
                out.push(Group { server_id: s.id, name: s.name, stickers });
            }
            out
        },
        move |out| groups.set(Some(ordered(out, current))),
    );

    let shown = memo(move || -> Option<Vec<Group>> {
        let q = query.get().trim().to_lowercase();
        groups.get().map(|gs| {
            gs.into_iter()
                .map(|g| Group { stickers: g.stickers.into_iter().filter(|s| matches(s, &q)).collect(), ..g })
                .filter(|g| !g.stickers.is_empty())
                .collect()
        })
    });

    // The status line while loading or empty ("" when results show), and the
    // groups to grid otherwise, so the keyed loops re-reconcile rows when a
    // search narrows or widens the list.
    let status = memo(move || -> String {
        match shown.get() {
            None => "Loading stickers…".to_string(),
            Some(gs) if gs.is_empty() && query.get().trim().is_empty() =>
                "No stickers yet. Server admins can add them in Server Settings → Stickers.".to_string(),
            Some(gs) if gs.is_empty() => "No stickers match that search.".to_string(),
            Some(_) => String::new(),
        }
    });
    // The grid's status line and keyed items, so the keyed loop re-reconciles
    // rows when a search narrows or widens the list (the emoji picker's flat
    // header/cell idiom).
    let grid_items = memo(move || -> Vec<GridItem> {
        shown.get()
            .unwrap_or_default()
            .into_iter()
            .flat_map(|g| {
                let (sid, name, stickers) = (g.server_id, g.name, g.stickers);
                std::iter::once(GridItem::Header { server_id: sid, name })
                    .chain(stickers.into_iter().map(move |s| GridItem::Cell { server_id: sid, sticker: s }))
            })
            .collect()
    });
    let pick_wrap = on_pick.clone();
    let grid = ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            gap: Some(px(GAP)),
            padding_left: Some(px(PAD)),
            padding_right: Some(px(PAD)),
            padding_bottom: Some(px(PAD)),
            ..Default::default()
        }) {
            if !status.get().is_empty() {
                view(style = StyleRules {
                    height: Some(px(200.0)),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    padding_left: Some(px(24.0)),
                    padding_right: Some(px(24.0)),
                    ..Default::default()
                }) {
                    text(style = StyleRules { font_size: Some(px(14.0)), color: Some(col(theme::TEXT_MUTED)), ..Default::default() }) { move || status.get() }
                }
            }
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                flex_wrap: Some(FlexWrap::Wrap),
                gap: Some(px(4.0)),
                ..Default::default()
            }) {
                for item in grid_items, key = item.key() {
                    {
                        match item {
                            GridItem::Header { name, .. } => crate::sticker_picker::StickerHeader(name.clone()),
                            GridItem::Cell { server_id, sticker, .. } => crate::sticker_picker::StickerTile(
                                sticker.id,
                                server_id,
                                sticker.name.clone(),
                                sticker.description.clone(),
                                sticker.related_emoji.clone(),
                                sticker.image.clone(),
                                pick_wrap.clone(),
                            ),
                        }
                    }
                }
            }
        }
    };
    ui! {
        view(style = StyleRules {
            width: Some(px(CARD_W)),
            height: Some(px(CARD_H)),
            overflow: Some(Overflow::Hidden),
            ..crate::style_helpers::popover_panel()
        }, test_id = "sticker-picker") {
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
                ..Default::default()
            }) {
                icon(SEARCH).color(|| theme::live(theme::TEXT_MUTED)).size(16.0)
                text_input(
                    value = query,
                    on_change = move |s: String| query.set(s),
                    placeholder = "Search stickers",
                    test_id = "sticker-search-input",
                    style = StyleRules {
                        flex_grow: Some(Tokenized::Literal(1.0)),
                        color: Some(col(theme::TEXT_STRONG)),
                        font_size: Some(px(14.0)),
                        ..crate::style_helpers::text_field_style()
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sticker(id: i64, server_id: i64, name: &str, description: &str, related: &str) -> Sticker {
        Sticker {
            id,
            server_id,
            name: name.to_string(),
            description: description.to_string(),
            related_emoji: related.to_string(),
            image: format!("/media/{id}.png"),
        }
    }

    #[test]
    fn search_matches_name_description_and_related_emoji() {
        let s = sticker(1, 1, "Wave", "a friendly hello", "👋");
        assert!(matches(&s, ""));
        assert!(matches(&s, "wav"));
        assert!(matches(&s, "hello"));
        assert!(matches(&s, "👋"));
        assert!(!matches(&s, "cat"));
    }

    #[test]
    fn the_open_server_comes_first_and_empty_servers_drop_out() {
        let g = |id: i64, n: usize| Group {
            server_id: id,
            name: format!("S{id}"),
            stickers: (0..n as i64).map(|i| sticker(id * 10 + i, id, "x", "", "🙂")).collect(),
        };
        let out = ordered(vec![g(1, 1), g(2, 0), g(3, 2)], Some(3));
        assert_eq!(out.iter().map(|g| g.server_id).collect::<Vec<_>>(), vec![3, 1]);
    }
}
