//! `discover_servers_modal` — the "Discover Servers" overlay for the server
//! rail.
//!
//! A full-screen overlay over a dimmed backdrop (the same build-once /
//! toggle idiom as `create_server_modal.rs`: when `open` is false the
//! wrapper collapses to zero size; when true it fills the window). It shows
//! a heading above a short static list of suggested mock servers (name +
//! description, hardcoded — no real join backend). Each row has a **Join**
//! button that calls the caller's `on_join` callback with the server name;
//! the caller decides what "join" does (in `app.rs` it mints the server via
//! `create_server`, appends it to the rail, and closes the overlay). A
//! close button (or tapping the backdrop) closes the overlay without
//! joining anything.
//!
//! Idioms mirror `create_server_modal.rs`: the `open` signal is owned by
//! the caller (`app.rs`); the button `.on_touch` chains are built as `let`
//! bindings so the method chain lives outside the `ui!` child position;
//! `Signal` is `Copy`, so every `move` closure captures a copy and stays
//! `Fn`.
use std::rc::Rc;

use runtime_core::{
    ui, AlignItems, Element, FlexDirection, FontWeight, JustifyContent, Overflow, Position,
    Signal, StyleRules, Tokenized, TouchPhase, TouchResponse,
};

use crate::style_helpers::{col, pct, px};
// Discovery-area components relocated here from the server rail (they render
// full-width cards / chip rows that don't fit a 72px icon rail): the "server
// stats" card (members / online / messages), the "server discovery" card
// (name + Join button), the "server event" card (the "Interested" CTA), and
// the category filter chips (Gaming / Music / Education — the list's filter
// row). All are self-contained (local state, no props required), so they
// render verbatim — see the note at each call site.
use crate::category_filter_chips::category_filter_chips;
use crate::server_discovery_card::server_discovery_card;
use crate::server_event_card::ServerEventCard;
use crate::server_stats_card::server_stats_card;
use crate::server_folder_group::server_folder_group;
use crate::server_template_card::server_template_card;
use crate::template_gallery_grid::TemplateGalleryGrid;
use crate::server_tag_chips::server_tag_chips;
use crate::featured_server_banner::FeaturedServerBanner;

// Discord dark palette (kept local so the component is self-contained).
const CARD: &str = "#2b2d31";
const ROW_BG: &str = "#1e1f22";
const BTN_BG: &str = "#5865f2";
const TEXT: &str = "#f2f3f5";
const TEXT_MUTED: &str = "#949ba4";

/// One suggested mock server: display name + short description. Hardcoded
/// placeholder data — the join button needs no backend.
struct SuggestedServer {
    name: &'static str,
    desc: &'static str,
}

/// The static suggestion list shown in the overlay.
const SUGGESTED: &[SuggestedServer] = &[
    SuggestedServer {
        name: "Rustaceans",
        desc: "Talk Rust, async, and embedded with 12k members",
    },
    SuggestedServer {
        name: "Game Dev Guild",
        desc: "Indie devs sharing engines, art, and playtests",
    },
    SuggestedServer {
        name: "Music Producers",
        desc: "Producers trading beats, plugins, and feedback",
    },
    SuggestedServer {
        name: "Study Together",
        desc: "Focus rooms and deadlines across every major",
    },
];

/// One suggested-server row: name + description on the left, a **Join**
/// button on the right. `on_join` receives the row's server name.
fn suggested_row(s: &SuggestedServer, on_join: Rc<dyn Fn(String)>) -> Element {
    let row_style = StyleRules {
        padding_top: Some(px(10.0)),
        padding_bottom: Some(px(10.0)),
        padding_left: Some(px(12.0)),
        padding_right: Some(px(12.0)),
        background: Some(col(ROW_BG)),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        gap: Some(px(10.0)),
        ..Default::default()
    };
    let name_style = StyleRules {
        font_size: Some(px(15.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT)),
        flex_grow: Some(Tokenized::Literal(1.0)),
        ..Default::default()
    };
    let desc_style = StyleRules {
        font_size: Some(px(12.0)),
        color: Some(col(TEXT_MUTED)),
        margin_top: Some(px(2.0)),
        ..Default::default()
    };
    let join_btn_style = StyleRules {
        height: Some(px(32.0)),
        padding_left: Some(px(14.0)),
        padding_right: Some(px(14.0)),
        background: Some(col(BTN_BG)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        border_top_left_radius: Some(px(6.0)),
        border_top_right_radius: Some(px(6.0)),
        border_bottom_left_radius: Some(px(6.0)),
        border_bottom_right_radius: Some(px(6.0)),
        ..Default::default()
    };
    let join_label_style = StyleRules {
        font_size: Some(px(13.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT)),
        ..Default::default()
    };
    let name_text = s.name.to_string();
    let desc_text = s.desc.to_string();
    let join = on_join.clone();
    let join_name = name_text.clone();
    let join_btn = ui! {
        view(style = join_btn_style.clone(), test_id = "discover-join") {
            text(style = join_label_style.clone()) { "Join".to_string() }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                join(join_name.clone());
            }
            TouchResponse::CONSUMED
        })
    };
    ui! {
        view(style = row_style) {
            view {
                text(style = name_style) { name_text }
                text(style = desc_style) { desc_text }
            }
            { join_btn }
        }
    }
}

/// The Discover Servers *panel* (the card itself, not the backdrop): the
/// "Discover Servers" heading + close button, the server-stats /
/// server-discovery / server-event cards, the category chips, and the
/// suggested-server Join list. `open` is the caller's signal; the close
/// button sets it back to `false`. `on_join` is forwarded to each Join row.
///
/// This is a free function (not inlined into the `when` branch) on purpose:
/// the reactive `if open.get() { … }` in `discover_servers_modal` lowers to a
/// `when` whose render is an `Fn` closure re-invoked on each activation, and
/// a branch rendering a non-`Copy` value must rebuild it per call. Building
/// the panel here — with all its `StyleRules` literals re-created inside — is
/// the shape the closure needs (see `jump_to_bottom_button.rs` for the same
/// inline-style / reactive-if idiom).
fn discover_panel(open: Signal<bool>, on_join: Rc<dyn Fn(String)>) -> Element {
    let close = open;

    // Shared panel styles (re-built per activation; `StyleRules` is not `Copy`).
    let panel_style = StyleRules {
        width: Some(px(420.0)),
        height: Some(px(840.0)),
        padding_top: Some(px(20.0)),
        padding_bottom: Some(px(20.0)),
        padding_left: Some(px(20.0)),
        padding_right: Some(px(20.0)),
        background: Some(col(CARD)),
        border_top_left_radius: Some(px(12.0)),
        border_top_right_radius: Some(px(12.0)),
        border_bottom_left_radius: Some(px(12.0)),
        border_bottom_right_radius: Some(px(12.0)),
        flex_direction: Some(FlexDirection::Column),
        align_items: Some(AlignItems::Center),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    let heading_style = StyleRules {
        font_size: Some(px(18.0)),
        font_weight: Some(FontWeight::Bold),
        color: Some(col(TEXT)),
        ..Default::default()
    };
    let close_btn_style = StyleRules {
        width: Some(px(28.0)),
        height: Some(px(28.0)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        font_size: Some(px(16.0)),
        color: Some(col(TEXT_MUTED)),
        border_top_left_radius: Some(px(6.0)),
        border_top_right_radius: Some(px(6.0)),
        border_bottom_left_radius: Some(px(6.0)),
        border_bottom_right_radius: Some(px(6.0)),
        ..Default::default()
    };
    let list_style = StyleRules {
        margin_top: Some(px(14.0)),
        width: Some(pct(100.0)),
        flex_direction: Some(FlexDirection::Column),
        gap: Some(px(8.0)),
        ..Default::default()
    };

    // Header: "Discover Servers" heading + close button.
    let close_btn = ui! {
        view(style = close_btn_style.clone(), test_id = "discover-close") {
            text { "✕".to_string() }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                close.set(false);
            }
            TouchResponse::CONSUMED
        })
    };

    // One row per suggested server; each Join button forwards the name.
    let list = {
        let join = on_join.clone();
        ui! {
            for s in SUGGESTED { suggested_row(s, join.clone()) }
        }
    };

    ui! {
        view(style = panel_style) {
            view(style = StyleRules {
                width: Some(pct(100.0)),
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                ..Default::default()
            }) {
                text(style = heading_style) { "Discover Servers".to_string() }
                view(style = StyleRules {
                    flex_grow: Some(Tokenized::Literal(1.0)),
                    ..Default::default()
                }) {}
                { close_btn }
            }
            // Server-stats card (members / online / messages), server-discovery
            // card (name + Join), and server-event card (the "Interested" CTA):
            // relocated here from the server rail during design review. All
            // three are full-width cards with no place in the 72px icon rail,
            // and this discover overlay is the app's discovery area. All are
            // self-contained (local hardcoded state, no props beyond the stats
            // defaults), so they render verbatim; each full-width wrapper
            // mirrors `list_style`'s width treatment.
            view(style = StyleRules {
                width: Some(pct(100.0)),
                margin_top: Some(px(14.0)),
                ..Default::default()
            }) {
                server_stats_card(512.into(), 48.into(), 1204.into())
            }
            view(style = StyleRules {
                width: Some(pct(100.0)),
                margin_top: Some(px(8.0)),
                ..Default::default()
            }) {
                server_discovery_card()
            }
            view(style = StyleRules {
                width: Some(pct(100.0)),
                margin_top: Some(px(8.0)),
                ..Default::default()
            }) {
                ServerEventCard()
            }
            // Category filter chips: the suggested list's filter row (Gaming /
            // Music / Education). Also relocated here from the server rail —
            // a chip row has no place in a 72px icon rail, and this is where
            // category filtering belongs.
            view(style = StyleRules {
                width: Some(pct(100.0)),
                margin_top: Some(px(10.0)),
                ..Default::default()
            }) {
                category_filter_chips()
            }
            // More discovery browsing content, relocated here from the server
            // rail during a later review (same reasoning as the four cards
            // above -- none of these fit a 72px icon rail): a folder of
            // servers, a templated server card, a template gallery grid, the
            // tag chips that filter a discovery result set, and a featured-
            // server promo banner. All mock/local state.
            view(style = StyleRules {
                width: Some(pct(100.0)),
                margin_top: Some(px(10.0)),
                ..Default::default()
            }) {
                server_folder_group()
            }
            view(style = StyleRules {
                width: Some(pct(100.0)),
                margin_top: Some(px(8.0)),
                ..Default::default()
            }) {
                server_template_card()
            }
            view(style = StyleRules {
                width: Some(pct(100.0)),
                margin_top: Some(px(8.0)),
                ..Default::default()
            }) {
                TemplateGalleryGrid()
            }
            view(style = StyleRules {
                width: Some(pct(100.0)),
                margin_top: Some(px(8.0)),
                ..Default::default()
            }) {
                server_tag_chips()
            }
            view(style = StyleRules {
                width: Some(pct(100.0)),
                margin_top: Some(px(8.0)),
                ..Default::default()
            }) {
                FeaturedServerBanner()
            }
            view(style = list_style) { { list } }
        }
        .on_touch(move |_ev| TouchResponse::CONSUMED)
    }
}

/// The Discover Servers overlay. `open` is owned by the caller (the rail's
/// GLOBE circle toggles it); `on_join` is called with a suggested server's
/// name when its Join button is tapped. The caller is responsible for any
/// join side effect and closing the overlay.
///
/// The backdrop wrapper keeps the build-once / toggle idiom (`width` /
/// `height` collapse to `0px` while `open` is false, fill the window while
/// true). The panel itself is additionally **gated behind `open`** with a
/// reactive `if open.get() { … }` (no `else`): when the overlay is closed the
/// panel unmounts entirely instead of sitting in the DOM at a negative
/// x-offset, clipped to nothing by the zero-size wrapper, paying layout cost
/// for no visible pixels. The `when` renders under a `display:contents`
/// anchor, so the panel stays a direct flex item of the (centering) wrapper
/// when open, and the auto-added `empty_absolute_view` else-branch occupies
/// no flex slot when closed.
pub fn discover_servers_modal(open: Signal<bool>, on_join: Rc<dyn Fn(String)>) -> Element {
    let close = open;
    // Full-screen backdrop (same idiom as create_server_modal.rs).
    let open_chk = open;
    let wrap_style = move || StyleRules {
        position: Some(Position::Absolute),
        left: Some(px(0.0)),
        top: Some(px(0.0)),
        width: Some(if open_chk.get() { pct(100.0) } else { px(0.0) }),
        height: Some(if open_chk.get() { pct(100.0) } else { px(0.0) }),
        background: Some(col("rgba(0, 0, 0, 0.6)")),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    ui! {
        view(style = wrap_style) {
            // Mount the panel only while the overlay is open. `Signal` is
            // `Copy` (re-read per activation) and `on_join` is cloned so the
            // `when` render closure stays `Fn`.
            if open.get() { discover_panel(open, on_join.clone()) }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                close.set(false);
            }
            TouchResponse::CONSUMED
        })
    }
}
