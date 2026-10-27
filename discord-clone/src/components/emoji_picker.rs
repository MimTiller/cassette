//! `emoji_picker` — the full Discord-style emoji picker card, shared by BOTH
//! entry points (the composer smiley and the per-message add-reaction button).
//!
//! One `#[component]` owns all picker state (search query, the highlighted
//! rail category, the hovered-emoji preview, and the locally-persisted
//! recently-used list) so it is genuinely one component used by both callers —
//! each caller positions it (the composer floats it above the input; the
//! reaction flow centers it in a dimmed backdrop) and supplies its own
//! `on_pick` callback: the composer appends the glyph to the draft, the
//! reaction flow routes it through the existing add/remove reaction path.
//!
//! Layout (per the reference): a dark rounded card with a search field up top,
//! a left rail of category tabs (Recent + Unicode's 9 groups), a scrollable
//! emoji grid on the right whose section headers pin (sticky) as you scroll,
//! and a footer preview of the currently-hovered emoji + its `:name:`. Tapping
//! a rail tab scrolls its category header to the top; scrolling syncs the
//! highlighted tab back (the real Discord behavior). Typing in the search
//! flattens the grid to the matching emoji.
//!
//! Idioms, each verified against this crate:
//! - `on_pick` is an `Option<Rc<dyn Fn(String)>>` prop — `#[component]`
//!   generates a `Default` for the props struct, and `Rc<dyn Fn(_)>` has no
//!   `Default`, so the callback is `Option`-wrapped (the `menu.rs`
//!   `on_dismiss` shape). Each cell clones the `Rc` into its `on_touch`.
//! - The grid body is a `switch` keyed on a `memo` of the visible sections
//!   (the `message_search_bar` `switch` idiom): the body rebuilds when the
//!   section list changes (a search edit flips the list between "full
//!   categories" and "flat matches"). Inside a build, the cells iterate the
//!   section's `&'static [Emoji]`.
//! - `recent` is a `Signal<Vec<&'static Emoji>>` seeded from `localStorage`
//!   (web) on first build; every pick prepends the glyph and re-persists it
//!   (the `notify` persistence idiom). `&'static Emoji` keeps the rows cheap
//!   to clone into the memo.
//! - Sticky section headers use `Position::Sticky` + a `top` inset — the same
//!   mechanism the web backend regression-tests for a scrolled list.
//! - Glyphs are `&'static str` (copyable), so closures capture them by value
//!   with no `String`-move issues.
use crate::emoji_data;
use crate::style_helpers::{borderless_input, col, px};
use icons_lucide::SEARCH;
use runtime_core::{
    component, icon, memo, signal, switch, ui, AlignItems, Color, Element, FlexDirection,
    FlexWrap, JustifyContent, Overflow, Position, Ref, ScrollViewHandle, Shadow, Signal,
    StyleRules, TouchPhase, TouchResponse, Tokenized,
};
use std::rc::Rc;

// Discord dark palette (kept local so the component is self-contained,
// mirroring `message_search_bar`).
const BG_CARD: &str = "#2b2d31"; // card surface (Discord sidebar gray)
const SURFACE: &str = "#313338"; // search field / hover
const RAIL_ACTIVE: &str = "#1e1f22"; // active rail tab
const BORDER: &str = "#1e1f22"; // 1px card border
const TEXT: &str = "#f2f3f5"; // primary text
const MUTED: &str = "#949ba4"; // muted text / inactive rail
const SHADOW: &str = "#000000"; // elevated card shadow

// Grid geometry (shared by the cell layout and the scroll-to-section math).
const COLS: usize = 8; // emoji per row
const CELL: f32 = 40.0; // one emoji cell (px)
const ROW: f32 = 46.0; // row pitch (cell + vertical gap)
const HEADER: f32 = 30.0; // sticky header height
const RECENT_CAP: usize = 16;

// Category rail: a display glyph per tab (Recent + Unicode's 9 groups, in the
// data's tab order). These are display-only text glyphs, not data — they keep
// the rail readable without an icon dependency.
const RAIL_ICONS: [&str; 10] = [
    "\u{1F31F}", // Recent
    "\u{1F60A}", // Smileys & People
    "\u{1F44B}", // People & Body (waving hand)
    "\u{1F436}", // Animals & Nature (dog)
    "\u{1F355}", // Food & Drink (pizza)
    "\u{26BD}", // Activities (soccer)
    "\u{1F680}", // Travel & Places (car)
    "\u{1F527}", // Objects (key)
    "\u{267E}", // Symbols (information)
    "\u{1F1E8}\u{1F1F3}", // Flags
];

// ── persistence (localStorage on web; ignored on native) ────────────────────

const KEY_RECENT: &str = "discord-clone/emoji-recent";

/// Load the persisted recently-used glyph list (newest first), or an empty
/// `Vec` on first run / absent storage.
fn load_recent() -> Vec<&'static emoji_data::Emoji> {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(window) = web_sys::window() else {
            return Vec::new();
        };
        let Ok(Some(storage)) = window.local_storage() else {
            return Vec::new();
        };
        let Ok(Some(raw)) = storage.get_item(KEY_RECENT) else {
            return Vec::new();
        };
        // Stored as `:`-separated glyphs; resolved back to the static emoji.
        raw.split(':')
            .filter(|g| !g.is_empty())
            .filter_map(|g| emoji_data::find(g))
            .collect()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = KEY_RECENT;
        Vec::new()
    }
}

/// Persist the recently-used list (newest first). Best-effort, like `notify`.
fn save_recent(glyphs: &[&str]) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window() {
            if let Ok(Some(storage)) = window.local_storage() {
                let _ = storage.set_item(KEY_RECENT, &glyphs.join(":"));
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (KEY_RECENT, glyphs);
    }
}

// ── pure section/scroll helpers (unit-testable) ─────────────────────────────

/// One grid section: a sticky-header label and its emoji. Owns the recent
/// slice as a `Vec` (a small, bounded allocation per rebuild) so the section
/// stays `PartialEq` (required by `switch`) without leaking memory.
#[derive(Clone, PartialEq)]
struct Section {
    label: &'static str,
    emojis: Vec<&'static emoji_data::Emoji>,
}

/// The full (non-search) grid: Recent (if any picks) + all 9 categories, in
/// the data's tab order.
fn full_sections(recent: &[&'static emoji_data::Emoji]) -> Vec<Section> {
    let mut out = Vec::with_capacity(1 + emoji_data::CATEGORIES.len());
    if !recent.is_empty() {
        out.push(Section { label: "Recently Used", emojis: recent.to_vec() });
    }
    for c in emoji_data::CATEGORIES {
        out.push(Section { label: c.label, emojis: c.emojis.iter().collect() });
    }
    out
}

/// The search grid: one section per category that has matching emoji.
fn search_sections(query: &str) -> Vec<Section> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for c in emoji_data::CATEGORIES {
        let hits: Vec<&'static emoji_data::Emoji> =
            c.emojis.iter().filter(|e| emoji_data::matches(&q, e)).collect();
        if !hits.is_empty() {
            out.push(Section { label: c.label, emojis: hits });
        }
    }
    out
}

/// Number of vertical rows (of `COLS` cells) a section occupies, incl. its
/// sticky header (the +1).
fn section_rows(emojis: &[&'static emoji_data::Emoji]) -> usize {
    emojis.len().div_ceil(COLS).max(1) + 1
}

/// The vertical pitch (px) of a section: its header plus its cell rows.
fn section_pitch(emojis: &[&'static emoji_data::Emoji]) -> f32 {
    HEADER + section_rows(emojis) as f32 * ROW
}

/// Map a vertical scroll offset to the section index whose block contains it
/// (for the scroll-synced rail highlight).
fn section_for_scroll(y: f32, sections: &[Section]) -> usize {
    let mut acc = 0.0;
    for (i, s) in sections.iter().enumerate() {
        acc += section_pitch(&s.emojis);
        if y < acc {
            return i;
        }
    }
    sections.len().saturating_sub(1)
}

/// The scroll offset that brings `target`'s header to the top of the scroller.
fn scroll_offset_for(sections: &[Section], target: usize) -> f32 {
    sections.iter().take(target).map(|s| section_pitch(&s.emojis)).sum()
}

/// Map a rail tab index to the grid's section index (None if out of range).
/// Rail 0 = Recent (section 0 only when a recent section exists); rail i = the
/// i-th category (section i when recent exists, else i-1).
fn rail_to_section(rail: usize, sections: &[Section]) -> Option<usize> {
    let has_recent = sections.first().is_some_and(|s| s.label == "Recently Used");
    let target = match rail {
        0 => 0,
        i if has_recent => i,
        i => i - 1,
    };
    (target < sections.len()).then_some(target)
}

// ── leaf builders (pure; take handles as params so the closures are simple) ──

fn cell_style() -> StyleRules {
    StyleRules {
        width: Some(px(CELL)),
        height: Some(px(CELL)),
        flex_direction: Some(FlexDirection::Column),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        ..Default::default()
    }
}

/// One emoji cell: hover feeds the footer preview, tap fires the pick. The
/// glyph is a `&'static str` (copy), so the `Fn` closures capture it by value
/// without any `String` move.
fn emoji_cell(
    glyph: &'static str,
    _name: &'static str,
    hovered: Signal<Option<String>>,
    on_pick: Rc<dyn Fn(String)>,
) -> Element {
    let hover_glyph = glyph.to_string();
    let touch_glyph = glyph.to_string();
    ui! {
        view(style = cell_style()) {
            text(style = StyleRules {
                font_size: Some(px(22.0)),
                ..Default::default()
            }) { glyph.to_string() }
        }
        .on_hover({
            let h = hovered;
            let g = hover_glyph;
            move |on| h.set(if on { Some(g.clone()) } else { None })
        })
        .on_touch({
            let p = on_pick;
            let g = touch_glyph;
            move |ev| {
                if ev.phase == TouchPhase::Ended {
                    (p)(g.clone());
                }
                TouchResponse::CONSUMED
            }
        })
    }
}

/// A sticky section header (pinned to the top of the scroller as it scrolls).
fn section_header(label: &str) -> Element {
    ui! {
        view(style = StyleRules {
            position: Some(Position::Sticky),
            top: Some(px(0.0)),
            width: Some(px(10000.0)),
            height: Some(px(HEADER)),
            background: Some(col(BG_CARD)),
            padding_left: Some(px(8.0)),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::Center),
            ..Default::default()
        }) {
            text(style = StyleRules {
                font_size: Some(px(11.0)),
                color: Some(col(MUTED)),
                ..Default::default()
            }) { label.to_string() }
        }
    }
}

// ── the component ────────────────────────────────────────────────────────────

/// The shared emoji picker card. `on_pick` fires with the picked glyph's
/// string when a cell is tapped; the caller decides what to do with it (append
/// to the composer, or add/remove a reaction) and then closes itself.
#[component]
#[allow(non_snake_case)]
pub fn EmojiPicker(on_pick: Option<Rc<dyn Fn(String)>>) -> Element {
    // Local controlled state, owned by the component so it persists while the
    // picker is mounted (the `#[component]` macro keeps one instance alive).
    let query = signal(String::new());
    let hovered = signal(Option::<String>::None);
    let recent = signal(load_recent());
    // The rail tab currently at the top of the scroller (0..=9). Driven by the
    // grid's `on_scroll` and by tab taps, so the highlight tracks both.
    let current = signal(0usize);

    // The scroll handle, filled by the grid scroller's `bind`.
    let scroll_ref: Ref<ScrollViewHandle> = Ref::<ScrollViewHandle>::new();

    // Search field: type to filter; the `memo` below reads this so the grid
    // rebuilds on a keystroke.
    let q2 = query.clone();
    let on_change = move |s: String| q2.set(s);

    // Reactive derivation: which sections to show. `recent` is folded in as a
    // `Vec<&'static Emoji>` (the only owned data); the category sections are
    // `'static` borrows. The list changes when a search is typed/cleared or
    // when a pick adds to recent — so the `switch` below rebuilds the grid.
    let active_m = query.clone();
    let recent_m = recent.clone();
    let sections = memo(move || {
        let q = active_m.get();
        let r = recent_m.get();
        if q.trim().is_empty() {
            full_sections(&r)
        } else {
            search_sections(&q)
        }
    });

    // The pick path: record the glyph in recent (newest first, de-duped,
    // capped) + persist, then hand it to the caller's `on_pick`. `recent_pick`
    // and `pick_cb` are moved into one closure that the cells share by `Rc`.
    let recent_pick = recent.clone();
    let pick_cb = on_pick.clone().unwrap_or_else(|| Rc::new(|_g: String| {}));
    let on_pick_wrap = Rc::new(move |glyph: String| {
        let mut r: Vec<&'static emoji_data::Emoji> =
            recent_pick.peek().into_iter().filter(|e| e.glyph != glyph.as_str()).collect();
        if let Some(em) = emoji_data::find(&glyph) {
            r.insert(0, em);
        }
        let cap: Vec<&'static emoji_data::Emoji> = r.iter().take(RECENT_CAP).copied().collect();
        let glyphs: Vec<&str> = cap.iter().map(|e| e.glyph).collect();
        save_recent(&glyphs);
        recent_pick.set(cap);
        (pick_cb)(glyph);
    });

    // --- grid body: rebuild when the section list changes ---
    let grid_body = switch(
        move || sections.get(),
        move |secs: &Vec<Section>| {
            let mut rows: Vec<Element> = Vec::new();
            for s in secs.iter() {
                rows.push(section_header(s.label));
                for e in s.emojis.iter() {
                    rows.push(emoji_cell(e.glyph, e.name, hovered.clone(), on_pick_wrap.clone()));
                }
            }
            // A wrapping row of `COLS` cells per line (the grid).
            ui! {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Row),
                    flex_wrap: Some(FlexWrap::Wrap),
                    gap: Some(px(4.0)),
                    padding_top: Some(px(4.0)),
                    padding_right: Some(px(4.0)),
                    padding_bottom: Some(px(4.0)),
                    padding_left: Some(px(4.0)),
                    ..Default::default()
                }) { rows }
            }
        },
    );

    // --- left category rail: Recent + the 9 groups ---
    // Each tab: highlight when it is the current section; tap scrolls its
    // header to the top. The current section is derived from the scroll offset
    // (see `on_scroll`), so the highlight tracks scrolling AND tab taps.
    let rail = {
        let rail_items: Vec<Element> = (0..10)
            .map(|i| {
                let icon_glyph = RAIL_ICONS[i].to_string();
                let current_r = current.clone();
                let query_r = query.clone();
                let sections_r = sections.clone();
                let scroll_r = scroll_ref;
                ui! {
                    view(style = move || {
                        // Highlight: the tab is active when NOT searching and
                        // its index equals the section currently at the top.
                        let searching = !query_r.get().trim().is_empty();
                        let on = !searching && current_r.get() == i;
                        StyleRules {
                            flex_direction: Some(FlexDirection::Column),
                            align_items: Some(AlignItems::Center),
                            justify_content: Some(JustifyContent::Center),
                            width: Some(px(44.0)),
                            height: Some(px(44.0)),
                            border_top_left_radius: Some(px(8.0)),
                            border_top_right_radius: Some(px(8.0)),
                            border_bottom_left_radius: Some(px(8.0)),
                            border_bottom_right_radius: Some(px(8.0)),
                            background: Some(if on { col(RAIL_ACTIVE) } else { col(BG_CARD) }),
                            ..Default::default()
                        }
                    }) {
                        text(style = StyleRules {
                            font_size: Some(px(18.0)),
                            ..Default::default()
                        }) { move || icon_glyph.clone() }
                    }
                    .on_touch({
                        let current_t = current.clone();
                        let query_t = query.clone();
                        let sections_t = sections_r.clone();
                        let scroll_t = scroll_r;
                        move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                // Rail is a category driver; ignore it while
                                // searching (the grid is flattened then).
                                if !query_t.peek().trim().is_empty() {
                                    return TouchResponse::CONSUMED;
                                }
                                // Jump to this tab's section header + highlight it.
                                let secs = sections_t.peek();
                                if let Some(target) = rail_to_section(i, &secs) {
                                    current_t.set(i);
                                    let off = scroll_offset_for(&secs, target);
                                    scroll_t.get().map(|h| h.scroll_to(0.0, off));
                                }
                            }
                            TouchResponse::CONSUMED
                        }
                    })
                }
            })
            .collect();
        rail_items
    };

    // --- scroll-synced rail highlight: scroll Y -> current section -> tab ---
    let scroll_sync = {
        let sections_s = sections.clone();
        let current_s = current.clone();
        let query_s = query.clone();
        move |_x: f32, y: f32| {
            // Don't move the highlight while searching (the grid is flattened).
            if !query_s.peek().trim().is_empty() {
                return;
            }
            let secs = sections_s.peek();
            let has_recent = secs.first().is_some_and(|s| s.label == "Recently Used");
            let sec = section_for_scroll(y, &secs);
            // section index -> rail index (0 = Recent).
            let rail_idx = if has_recent { sec } else { sec + 1 };
            current_s.set(rail_idx);
        }
    };

    // --- hover footer preview: big glyph + `:name:` of the hovered emoji ---
    let footer = switch(
        move || hovered.get(),
        move |h: &Option<String>| {
            // Both states are the same fixed box (height, no shrink, the same
            // top border) so hovering never resizes the rail + grid above.
            let Some(g) = h else {
                return ui! {
                    view(style = StyleRules {
                        height: Some(px(52.0)),
                        flex_shrink: Some(Tokenized::Literal(0.0)),
                        border_top_width: Some(Tokenized::Literal(1.0)),
                        border_top_color: Some(col(BORDER)),
                        ..Default::default()
                    }) {}
                };
            };
            let em = emoji_data::find(g);
            let big = g.clone();
            let name = em.map(|e| format!(":{}:", e.name)).unwrap_or_else(|| g.clone());
            ui! {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    height: Some(px(52.0)),
                    flex_shrink: Some(Tokenized::Literal(0.0)),
                    padding_left: Some(px(16.0)),
                    padding_right: Some(px(16.0)),
                    gap: Some(px(10.0)),
                    border_top_width: Some(Tokenized::Literal(1.0)),
                    border_top_color: Some(col(BORDER)),
                    ..Default::default()
                }) {
                    text(style = StyleRules { font_size: Some(px(28.0)), ..Default::default() }) { big }
                    text(style = StyleRules { font_size: Some(px(14.0)), color: Some(col(MUTED)), ..Default::default() }) { name }
                }
            }
        },
    );

    // --- the card: search field / rail + scrollable grid / hover footer ---
    ui! {
        view(style = StyleRules {
            width: Some(px(420.0)),
            height: Some(px(440.0)),
            flex_direction: Some(FlexDirection::Column),
            background: Some(col(BG_CARD)),
            border_top_left_radius: Some(px(10.0)),
            border_top_right_radius: Some(px(10.0)),
            border_bottom_left_radius: Some(px(10.0)),
            border_bottom_right_radius: Some(px(10.0)),
            border_top_width: Some(Tokenized::Literal(1.0)),
            border_right_width: Some(Tokenized::Literal(1.0)),
            border_bottom_width: Some(Tokenized::Literal(1.0)),
            border_left_width: Some(Tokenized::Literal(1.0)),
            border_top_color: Some(col(BORDER)),
            border_right_color: Some(col(BORDER)),
            border_bottom_color: Some(col(BORDER)),
            border_left_color: Some(col(BORDER)),
            shadow: Some(Shadow { x: 0.0, y: 8.0, blur: 24.0, color: col(SHADOW) }),
            overflow: Some(Overflow::Hidden),
            ..Default::default()
        }) {
            // Search field (magnifier + input, the message_search_bar shape).
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                margin_top: Some(px(12.0)),
                margin_right: Some(px(12.0)),
                margin_bottom: Some(px(12.0)),
                margin_left: Some(px(12.0)),
                gap: Some(px(8.0)),
                height: Some(px(38.0)),
                background: Some(col(SURFACE)),
                padding_left: Some(px(10.0)),
                padding_right: Some(px(10.0)),
                border_top_left_radius: Some(px(6.0)),
                border_top_right_radius: Some(px(6.0)),
                border_bottom_left_radius: Some(px(6.0)),
                border_bottom_right_radius: Some(px(6.0)),
                ..borderless_input()
            }) {
                icon(SEARCH).color(|| Color::from(MUTED)).size(16.0)
                text_input(
                    value = query,
                    on_change = on_change,
                    placeholder = "Search",
                    style = StyleRules {
                        flex_grow: Some(Tokenized::Literal(1.0)),
                        color: Some(col(TEXT)),
                        background: Some(col(SURFACE)),
                        font_size: Some(px(14.0)),
                        ..borderless_input()
                    },
                )
            }

            // Rail + scrollable grid.
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                flex_grow: Some(Tokenized::Literal(1.0)),
                min_height: Some(px(0.0)),
                ..Default::default()
            }) {
                // Left category rail: taller than the space it gets, so it
                // scrolls on its own instead of overflowing the card.
                scroll_view(style = StyleRules {
                    width: Some(px(56.0)),
                    flex_shrink: Some(Tokenized::Literal(0.0)),
                    ..Default::default()
                }) {
                    view(style = StyleRules {
                        padding_top: Some(px(6.0)),
                        padding_right: Some(px(6.0)),
                        padding_bottom: Some(px(6.0)),
                        padding_left: Some(px(6.0)),
                        flex_direction: Some(FlexDirection::Column),
                        gap: Some(px(2.0)),
                        align_items: Some(AlignItems::Center),
                        ..Default::default()
                    }) { rail }
                }

                // Right: the scrollable emoji grid with sticky headers.
                scroll_view(style = StyleRules {
                    flex_grow: Some(Tokenized::Literal(1.0)),
                    ..Default::default()
                }) {
                    grid_body
                }
                .bind(scroll_ref)
                .on_scroll(scroll_sync)
            }

            // Hover preview footer.
            footer
        }
    }
}

thread_local! {
    static ANCHOR: std::cell::Cell<Option<(f32, f32)>> = const { std::cell::Cell::new(None) };
}

/// Remember where (window coordinates) the button that is opening the
/// reaction picker was pressed, so the picker can open beside it.
pub fn set_anchor(x: f32, y: f32) {
    ANCHOR.with(|a| a.set(Some((x, y))));
}

/// The last press `set_anchor` recorded, if any.
pub fn anchor() -> Option<(f32, f32)> {
    ANCHOR.with(|a| a.get())
}

/// Where a `w` x `h` picker card goes for a press at `at` in a `vw` x `vh`
/// window: to the left of the press (to the right when there is no room),
/// its top a little above the press, kept 8px inside the window.
pub fn place_card(at: (f32, f32), w: f32, h: f32, vw: f32, vh: f32) -> (f32, f32) {
    let (x, y) = at;
    let left = if x - w - 12.0 >= 8.0 { x - w - 12.0 } else { (x + 12.0).min(vw - w - 8.0) };
    let top = (y - 40.0).min(vh - h - 8.0).max(8.0);
    (left.max(8.0), top)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_opens_beside_the_press_and_stays_inside() {
        // Room on the left: the card ends 12px left of the press.
        assert_eq!(place_card((1000.0, 300.0), 420.0, 440.0, 1600.0, 900.0), (568.0, 260.0));
        // Near the bottom: pulled up to fit.
        assert_eq!(place_card((1000.0, 880.0), 420.0, 440.0, 1600.0, 900.0), (568.0, 452.0));
        // Near the left edge: opens to the right instead.
        assert_eq!(place_card((100.0, 20.0), 420.0, 440.0, 1600.0, 900.0), (112.0, 8.0));
    }

    #[test]
    fn full_sections_order() {
        let s = full_sections(&[]);
        // No recent: exactly the 9 categories, in tab order.
        assert_eq!(s.len(), emoji_data::CATEGORIES.len());
        for (i, c) in emoji_data::CATEGORIES.iter().enumerate() {
            assert_eq!(s[i].label, c.label);
            assert_eq!(s[i].emojis, c.emojis.iter().collect::<Vec<_>>());
        }
    }

    #[test]
    fn full_sections_with_recent() {
        let r: Vec<&'static emoji_data::Emoji> =
            emoji_data::CATEGORIES[0].emojis.iter().take(3).collect();
        let s = full_sections(&r);
        assert_eq!(s.len(), 1 + emoji_data::CATEGORIES.len());
        assert_eq!(s[0].label, "Recently Used");
        assert_eq!(s[0].emojis.len(), 3);
    }

    #[test]
    fn search_sections_filters() {
        let s = search_sections("thumbs");
        // Non-empty for a known keyword; only sections with hits appear.
        assert!(!s.is_empty());
        for sec in &s {
            assert!(!sec.emojis.is_empty());
        }
        // A nonsense query yields no sections.
        assert!(search_sections("zzz_no_such_emoji_zzz").is_empty());
    }

    #[test]
    fn section_for_scroll_maps_to_block() {
        let s = full_sections(&[]);
        // y=0 -> section 0.
        assert_eq!(section_for_scroll(0.0, &s), 0);
        // A large y -> the last section.
        assert_eq!(section_for_scroll(1e9, &s), s.len() - 1);
        // The offset of the last section maps back to itself.
        let off = scroll_offset_for(&s, s.len() - 1);
        assert_eq!(section_for_scroll(off + 1.0, &s), s.len() - 1);
    }

    #[test]
    fn scroll_offset_for_accumulates() {
        let s = full_sections(&[]);
        // Monotonic and strictly increasing across sections.
        let o0 = scroll_offset_for(&s, 0);
        let o1 = scroll_offset_for(&s, 1);
        let o2 = scroll_offset_for(&s, 2);
        assert!(o0 < o1 && o1 < o2);
    }
}
