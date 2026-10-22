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

use crate::style_helpers::{borderless_input, col, px};
use icons_lucide::SEARCH;
use runtime_core::{
    component, icon, memo, signal, switch, ui, AlignItems, Color, Element, FlexDirection,
    JustifyContent, ReadSignal, StyleRules, Tokenized, TouchPhase, TouchResponse,
};

// Discord dark palette (kept local so the component is self-contained).
const BG_MAIN: &str = "#313338"; // screen background
const COMPOSER: &str = "#383a40"; // input surface
const TEXT: &str = "#f2f3f5"; // primary text (near-white)
const MUTED: &str = "#949ba4"; // secondary / muted text
const RESULT_BG: &str = "#2b2d31"; // result-row surface (Discord sidebar gray)
const ACCENT: &str = "#5865f2"; // author-name accent (Discord blurple)

/// A message-search bar that live-filters a caller-provided message corpus.
/// `corpus` is a `ReadSignal<Vec<(String, String)>>` of `(author, text)` rows
/// for the currently open conversation; typing narrows it to case-insensitive
/// matches, clearing shows everything, and a dead-end query shows a muted
/// "no matches" line.
#[component]
#[allow(non_snake_case)]
pub fn MessageSearchBar(corpus: ReadSignal<Vec<(String, String)>>) -> Element {
    // Local controlled state: the search query (empty until the user types).
    let query = signal(String::new());
    // The corpus is `Copy` (it is a `ReadSignal` handle), so the `memo`
    // closure below captures its own copy and shares it with the input wiring.
    let on_change = move |s: String| query.set(s);

    // The input sits OUTSIDE the reactive results below, so its style is a
    // plain local (created once). Flat background + corner radius, the same
    // look as the composer input in `dm_view.rs`. The `text_input` primitive
    // takes no children, so the leading magnifier is a row sibling to its
    // left. The input `flex_grow`s to fill the row after the fixed icon + gap.
    let input_style = StyleRules {
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        height: Some(px(40.0)),
        flex_grow: Some(Tokenized::Literal(1.0)),
        padding_left: Some(px(10.0)),
        padding_right: Some(px(12.0)),
        background: Some(col(COMPOSER)),
        color: Some(col(TEXT)),
        font_size: Some(px(15.0)),
        border_top_left_radius: Some(px(6.0)),
        border_top_right_radius: Some(px(6.0)),
        border_bottom_left_radius: Some(px(6.0)),
        border_bottom_right_radius: Some(px(6.0)),
        ..borderless_input()
    };

    // Reactive derivation: `(author, text)` rows of the corpus that the query
    // matches. Empty query -> the whole corpus; non-empty -> case-insensitive
    // substring hits on either the text or the author. Reading `query` and
    // `corpus` inside the memo closure is what subscribes it to both, so the
    // list rebuilds on a keystroke AND on a corpus change (new channel / new
    // send), reusing the prior result when neither moved.
    let matches = memo(move || -> Vec<(String, String)> {
        let q = query.get();
        let corpus = corpus.get();
        let needle = q.to_lowercase();
        if needle.trim().is_empty() {
            corpus
        } else {
            corpus
                .into_iter()
                .filter(|(author, text)| {
                    text.to_lowercase().contains(&needle)
                        || author.to_lowercase().contains(&needle)
                })
                .collect()
        }
    });

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Stretch),
            background: Some(col(BG_MAIN)),
            padding_top: Some(px(8.0)),
            padding_bottom: Some(px(8.0)),
            padding_left: Some(px(12.0)),
            padding_right: Some(px(12.0)),
            gap: Some(px(10.0)),
            ..Default::default()
        }) {
            text(style = StyleRules {
                font_size: Some(px(15.0)),
                color: Some(col(TEXT)),
                ..Default::default()
            }) { "Search messages".to_string() }

            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(8.0)),
                ..Default::default()
            }) {
                // Leading magnifier: fixed size + muted color via the icon
                // builder, a row sibling left of the input (the `text_input`
                // primitive takes no children).
                icon(SEARCH).color(|| Color::from(MUTED)).size(16.0)

                text_input(
                    value = query.clone(),
                    on_change = on_change,
                    placeholder = "Search",
                    style = input_style,
                )
            }

            // The results card is shown ONLY while a search is active
            // (query typed and non-blank) — an empty query leaves just the
            // search field itself, like Discord's blank input before a
            // search runs. The card carries a "×" close control that clears
            // the query, and its results live in their own `scroll_view` at a
            // bounded height, so a long result set scrolls INSIDE the card
            // instead of growing past the panel/rail it sits in.
            switch(
                move || (!query.get().trim().is_empty(), matches.get()),
                move |(active, rows): &(bool, Vec<(String, String)>)| {
                    if !active {
                        // No query: no card at all (zero-size hole).
                        ui! {
                            view(style = StyleRules {
                                width: Some(px(0.0)),
                                height: Some(px(0.0)),
                                ..Default::default()
                            }) {}
                        }
                    } else if rows.is_empty() {
                        // A non-empty query with zero hits: a single muted row.
                        ui! {
                            text(style = StyleRules {
                                font_size: Some(px(13.0)),
                                color: Some(col(MUTED)),
                                ..Default::default()
                            }) { "No matches".to_string() }
                        }
                    } else {
                        let rows = rows.clone();
                        // The card's "×" close control: tapping clears the query,
                        // collapsing the card away (built outside the `ui!` so its
                        // touch handler chains unambiguously to the button node).
                        let close_btn = ui! {
                            view(style = StyleRules {
                                flex_direction: Some(FlexDirection::Row),
                                justify_content: Some(JustifyContent::FlexEnd),
                                height: Some(px(24.0)),
                                ..Default::default()
                            }) {
                                text(style = StyleRules {
                                    font_size: Some(px(15.0)),
                                    color: Some(col(MUTED)),
                                    ..Default::default()
                                }) { "×".to_string() }
                            }
                            .on_touch(move |ev| {
                                if ev.phase == TouchPhase::Ended {
                                    query.set(String::new());
                                }
                                TouchResponse::CONSUMED
                            })
                        };
                        ui! {
                            // Wrapper view for the row list: an element in ONE
                            // slot must be one node, and the rows' own styles
                            // stay untouched inside it.
                            view(style = StyleRules {
                                flex_direction: Some(FlexDirection::Column),
                                gap: Some(px(0.0)),
                                ..Default::default()
                            }) {
                                { close_btn }
                                // Bounded scroll region: the hits scroll
                                // inside the card, never past its own box.
                                scroll_view(style = StyleRules {
                                    height: Some(px(220.0)),
                                    flex_grow: Some(Tokenized::Literal(1.0)),
                                    ..Default::default()
                                }) {
                                for (author, text) in rows.into_iter() {
                                    view(style = StyleRules {
                                        flex_direction: Some(FlexDirection::Column),
                                        gap: Some(px(4.0)),
                                        background: Some(col(RESULT_BG)),
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
                                            color: Some(col(ACCENT)),
                                            ..Default::default()
                                        }) { author }
                                        text(style = StyleRules {
                                            font_size: Some(px(14.0)),
                                            color: Some(col(TEXT)),
                                            ..Default::default()
                                        }) { text }
                                    }
                                }
                                }
                            }
                        }
                    }
                },
            )
        }
    }
}
