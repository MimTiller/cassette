//! `message_search_bar` — a working "search messages" bar for the Discord
//! clone. A `text_input` bound to a local `query` signal sits above a list of
//! matches drawn from a real message corpus the caller passes in as a
//! `ReadSignal<Vec<(String, String)>>` (each entry is `(author, text)`).
//!
//! Typing filters that corpus (case-insensitive substring match over the
//! text, or over the author name). The RESULTS CARD only mounts while a
//! search is active (query non-blank): an empty query leaves just the search
//! field, matching Discord's blank input before a search runs. The card has a
//! "×" close control that clears the query (collapsing the card away), and its
//! results live in their own bounded `scroll_view`, so a long result set
//! scrolls inside the card instead of growing past the panel/rail it sits in.
//! A non-empty query matching nothing shows a single muted "no matches" row.
//! The corpus is a *signal* prop (not a plain value) so the bar tracks
//! reactive changes to it — the caller derives it with `runtime_core::memo`
//! over the active channel + its sent messages, so switching channels or
//! sending a new message updates the search results without a remount.
//!
//! Idioms, each verified against this crate:
//! - a `ReadSignal<T>` prop is a *signal handle* the `#[component]` macro
//!   leaves unwrapped (`is_signal_handle_type` covers `Signal`/`ReadSignal`/
//!   `WriteSignal`), so the body holds the `ReadSignal` directly and reads it
//!   with `.get()` — the same shape `category_header` uses for its `Signal`
//!   prop. `ReadSignal` is `Copy`, so the local `query` + `corpus` are cheap
//!   to share between the `memo` closure and the input wiring.
//! - the filtered rows come from a `runtime_core::memo` whose closure reads
//!   both `query` and `corpus`; the memo re-runs (and the list rebuilds) only
//!   when either changes, reusing the previous result otherwise. A reactive
//!   derivation over two signals is exactly what `memo` is for — no manual
//!   `switch` key plumbing.
//! - the results are a `switch` keyed on the memo's filtered output (the
//!   pinned-messages panel's versioned-list idiom), each row a plain
//!   `view`/`text` built inside the macro; no `Vec<Element>` push loop
//!   outside the macro. A plain `for` over `matches.get()` is NOT
//!   acceptable here: `ui!` routes `for`-over-`Vec` to `StaticForEach`
//!   (rows frozen at mount), so narrowing queries would keep showing the
//!   mount-time snapshot.

use crate::theme;
use crate::style_helpers::{borderless_input, col, px};
use icons_lucide::SEARCH;
use runtime_core::{
    component, icon, memo, signal, switch, ui, AlignItems, Element, FlexDirection,
    JustifyContent, Overflow, Position, ReadSignal, Shadow, StyleRules, Tokenized, TouchPhase, TouchResponse,
};

// Discord dark palette (kept local so the component is self-contained).

/// A message-search bar that live-filters a caller-provided message corpus.
/// `corpus` is a `ReadSignal<Vec<(String, String)>>` of `(author, text)` rows
/// for the currently open conversation; typing narrows it to case-insensitive
/// matches, clearing shows everything, and a dead-end query shows a muted
/// "no matches" line.
#[component]
#[allow(non_snake_case)]
/// The channel header's search field ("Search <server>"). While a query is
/// typed, the hits drop down in a card under the field (author + text, a hit
/// count and a "×" that clears the query).
pub fn MessageSearchBar(corpus: ReadSignal<Vec<(String, String)>>, server_name: String) -> Element {
    let name = server_name.get();
    let placeholder = if name.is_empty() { "Search".to_string() } else { format!("Search {name}") };
    let query = signal(String::new());
    let on_change = move |s: String| query.set(s);

    // Case-insensitive substring hits on the text or the author.
    let matches = memo(move || -> Vec<(String, String)> {
        let needle = query.get().to_lowercase();
        if needle.trim().is_empty() {
            return Vec::new();
        }
        // A GIF message is matched and shown as "GIF", not its media URL.
        corpus
            .get()
            .into_iter()
            .map(|(author, text)| (author, crate::reply_reference_strip::excerpt_text(&text)))
            .filter(|(author, text)| text.to_lowercase().contains(&needle) || author.to_lowercase().contains(&needle))
            .collect()
    });

    let results = switch(
        move || (!query.get().trim().is_empty(), matches.get()),
        move |(active, rows): &(bool, Vec<(String, String)>)| {
            if !active {
                return ui! { view(style = StyleRules { width: Some(px(0.0)), height: Some(px(0.0)), ..Default::default() }) {} };
            }
            let count = match rows.len() {
                0 => "No results".to_string(),
                1 => "1 result".to_string(),
                n => format!("{n} results"),
            };
            let rows = rows.clone();
            ui! {
                view(style = StyleRules {
                    position: Some(Position::Absolute),
                    top: Some(px(34.0)),
                    right: Some(px(0.0)),
                    width: Some(px(420.0)),
                    max_height: Some(px(480.0)),
                    flex_direction: Some(FlexDirection::Column),
                    background: Some(col(theme::BG_SECONDARY)),
                    border_top_left_radius: Some(px(8.0)),
                    border_top_right_radius: Some(px(8.0)),
                    border_bottom_left_radius: Some(px(8.0)),
                    border_bottom_right_radius: Some(px(8.0)),
                    shadow: Some(Shadow { x: 0.0, y: 8.0, blur: 24.0, color: col(theme::SHADOW) }),
                    // The shared popup level (`app::popups::POPUP_Z`).
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
                            font_weight: Some(runtime_core::FontWeight::SemiBold),
                            color: Some(col(theme::TEXT_MUTED)),
                            ..Default::default()
                        }) { count }
                        view(style = StyleRules { padding_left: Some(px(8.0)), padding_right: Some(px(4.0)), ..Default::default() }) {
                            text(style = StyleRules { font_size: Some(px(18.0)), color: Some(col(theme::TEXT_MUTED)), ..Default::default() }) { "×".to_string() }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                query.set(String::new());
                            }
                            TouchResponse::CONSUMED
                        })
                    }
                    scroll_view(style = StyleRules { flex_shrink: Some(Tokenized::Literal(1.0)), ..Default::default() }) {
                        view(style = StyleRules {
                            flex_direction: Some(FlexDirection::Column),
                            gap: Some(px(8.0)),
                            padding_left: Some(px(12.0)),
                            padding_right: Some(px(12.0)),
                            padding_bottom: Some(px(12.0)),
                            ..Default::default()
                        }) {
                            for (author, text) in rows.into_iter() {
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
                                    text(style = StyleRules {
                                        font_size: Some(px(13.0)),
                                        font_weight: Some(runtime_core::FontWeight::SemiBold),
                                        color: Some(col(theme::TEXT_STRONG)),
                                        ..Default::default()
                                    }) { author }
                                    text(style = StyleRules { font_size: Some(px(14.0)), color: Some(col(theme::TEXT_STRONG)), ..Default::default() }) { text }
                                }
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
                    value = query.clone(),
                    on_change = on_change,
                    placeholder = placeholder,
                    style = StyleRules {
                        flex_grow: Some(Tokenized::Literal(1.0)),
                        min_width: Some(px(0.0)),
                        height: Some(px(28.0)),
                        background: Some(col(theme::BG_TERTIARY)),
                        color: Some(col(theme::TEXT_STRONG)),
                        font_size: Some(px(14.0)),
                        ..borderless_input()
                    },
                )
                icon(SEARCH).color(|| theme::live(theme::TEXT_MUTED)).size(15.0)
            }
            results
        }
    }
}
