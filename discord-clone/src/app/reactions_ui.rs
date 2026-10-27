//! Message reactions: the reaction map, live add/remove requests, the reaction row and picker.
//! Split out of `app.rs`; `use super::*` keeps app.rs's imports and shared
//! colors/helpers in scope.

use super::*;

// --- message reactions (mock session-local + live persisted/synced) --------
// Per-message reaction state shared by BOTH the MOCK and the LIVE (gateway)
// message lists. Keyed by `MessageKey`: MOCK rows key by `(channel_id,
// rendered-list-index)` (seed messages first, then this session's sends — a
// stable identity across `revision` bumps, since the list order is
// deterministic), LIVE rows key by their real wire `message_id`. Each value is
// a list of `(emoji, count, reacted_by_me)` pills. Mutating the map does not
// fire reactivity, so the message-list `switch` folds `reaction_rev` into its
// key (the `revision` idiom) and re-reads the map on every rebuild. MOCK pills
// stay session-local (demo content); LIVE pills are persisted server-side:
// clicks call the `add_reaction` / `remove_reaction` RPCs (see `crate::reactions`),
// the gateway's history frames restore persisted pills on a cold load, and
// `WsServerMsg::ReactionAdd` / `ReactionRemove` frames update every connected
// client live.
pub(super) type Reaction = (String, u32, bool);

/// The identity key for per-message session-local state (the reaction `picker`,
/// the `reactions` pill map, and the `translate_key`). MOCK rows key by their
/// `(channel_id, rendered-list-index)` — the pre-existing stable-enough key —
/// while LIVE (gateway) rows key by their real wire `message_id`. The two are
/// disjoint enum variants, so a live message's id can never collide with a mock
/// `(cid, idx)`; the mock path's existing keys are preserved exactly (wrapped in
/// `Mock { .. }`) and the live path gets its own namespace beside them.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum MessageKey {
    Mock { cid: u32, idx: usize },
    Live { id: i64 },
}

pub(super) type ReactionMap = HashMap<MessageKey, Vec<Reaction>>;

/// Route a reaction click on a LIVE (gateway) row through the backend RPCs:
/// `add_reaction` / `remove_reaction` persist the `(message, user, emoji)` row
/// and broadcast a `ReactionAdd` / `ReactionRemove` frame to every connected
/// member (the caller's own echo included), so the pill map is updated in
/// `gateway_incoming_effect` — never optimistically here. A rejected call
/// leaves no UI change (no optimistic pill to roll back). Also closes the
/// picker overlay when one was open for this message. MOCK rows never call
/// this (they stay session-local demo state).
pub(super) fn request_live_reaction(
    key: MessageKey,
    emoji: String,
    remove: bool,
    picker: Signal<Option<MessageKey>>,
) {
    let id = match key {
        MessageKey::Live { id } => id,
        MessageKey::Mock { .. } => return,
    };
    picker.set(None);
    runtime_core::driver::spawn_async(async move {
        if remove {
            let _ = crate::reactions::remove_reaction(id, emoji).await;
        } else {
            let _ = crate::reactions::add_reaction(id, emoji).await;
        }
    });
}

/// Apply the server's authoritative post-state count for one emoji pill to the
/// shared reaction map (the LIVE path). The `ReactionAdd` / `ReactionRemove`
/// broadcast frames carry the total for the emoji, so the client SETS the pill
/// rather than tracking deltas — duplicate or out-of-order frames converge.
/// `reacted_override` is `Some(...)` only when the frame names this client's
/// own user (it sets the "you reacted" highlight); otherwise the existing
/// highlight is kept. Returns whether anything changed (the caller bumps
/// `reaction_rev` on `true` so the message list rebuilds).
pub(super) fn apply_reaction(
    map: Rc<RefCell<ReactionMap>>,
    key: MessageKey,
    emoji: String,
    count: u32,
    reacted_override: Option<bool>,
) -> bool {
    let mut m = map.borrow_mut();
    let inner = m.entry(key).or_default();
    let pos = inner.iter().position(|(e, _, _)| e == &emoji);
    let changed: bool;
    if count == 0 {
        if let Some(p) = pos {
            inner.remove(p);
            changed = true;
        } else {
            changed = false;
        }
    } else if let Some(p) = pos {
        let (e, c, reacted) = inner[p].clone();
        let reacted = reacted_override.unwrap_or(reacted);
        if c != count || reacted != inner[p].2 {
            inner[p] = (e, count, reacted);
            changed = true;
        } else {
            changed = false;
        }
    } else {
        inner.push((emoji, count, reacted_override.unwrap_or(false)));
        changed = true;
    }
    if m.get(&key).map_or(false, |v| v.is_empty()) {
        m.remove(&key);
    }
    changed
}

/// The reaction strip under one message row: existing reaction pills on the
/// left, a spacer, and an "add reaction" button (the Lucide SMILE_PLUS glyph)
/// on the right — the add button only appears when the message already has at
/// least one reaction (Discord's behavior); a message with no reactions gets
/// an empty zero-height strip so rows sit closer together. Tapping the add
/// button opens the picker for this message; tapping a pill toggles the current user's reaction on it
/// (increment + highlight, or decrement + un-highlight, dropping the pill at 0).
/// Each clickable node is a `view(...)` builder with `.on_touch` chained on the
/// builder inside `ui!` (the `ac_el` / composer-emoji-cell idiom) — `Element`
/// itself has no `on_touch` method, so the handler must live on the builder,
/// not on the returned `Element`.
pub(super) fn reaction_area(
    key: MessageKey,
    reactions: Rc<RefCell<ReactionMap>>,
    reaction_rev: Signal<u32>,
    picker: Signal<Option<MessageKey>>,
) -> Element {
    // Current pills for this message (a clone — the borrow is released before
    // this call returns, so the list `switch` body can borrow again freely).
    let list: Vec<Reaction> = reactions.borrow().get(&key).cloned().unwrap_or_default();

    // One pill per current reaction, built as a `Vec<Element>` and splatted
    // below. Styled like the unwired `components/reaction_summary.rs` pill:
    // `reacted` selects a blurple border + tinted fill, otherwise a muted-gray
    // border on a neutral fill. Each pill's `.on_touch` gets fresh `Rc`/`Copy`
    // handles (`r`/`rev`) plus its own emoji clone (`em` for the label,
    // `em_close` for the closure), so it never moves the outer closure's values.
    // LIVE rows route the tap through `on_toggle` (the RPC-backed pill toggle —
    // the pill map then updates via the server's echo frames); MOCK rows keep
    // the direct session-local mutation.
    let pills: Vec<Element> = list.iter().map(|(emoji, count, reacted)| {
        let (bg, border) = if *reacted {
            (REACTED_BG, REACTED_BORDER)
        } else {
            (PLAIN_BG, PLAIN_BORDER)
        };
        let em = emoji.to_string();
        let em_close = em.clone();
        let cnt = *count;
        let r = reactions.clone();
        let rev = reaction_rev;
        // Whether this tap removes (user already reacted) or adds (user has not)
        // their own reaction. LIVE rows route the tap through the backend RPC
        // (see `request_live_reaction`); MOCK rows mutate the pill map directly.
        let will_remove = *reacted;
        let live_key = match key {
            MessageKey::Live { .. } => true,
            MessageKey::Mock { .. } => false,
        };
        let picker_l = picker;
        ui! {
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(5.0)),
                padding_left: Some(px(8.0)),
                padding_right: Some(px(8.0)),
                padding_top: Some(px(3.0)),
                padding_bottom: Some(px(3.0)),
                height: Some(px(22.0)),
                background: Some(col(bg)),
                border_top_left_radius: Some(px(11.0)),
                border_top_right_radius: Some(px(11.0)),
                border_bottom_left_radius: Some(px(11.0)),
                border_bottom_right_radius: Some(px(11.0)),
                // 1px border in the arm's color — `Tokenized<f32>` (not `px()`),
                // per the `reaction_summary` / `connection_quality_indicator` idiom.
                border_top_width: Some(Tokenized::Literal(1.0)),
                border_right_width: Some(Tokenized::Literal(1.0)),
                border_bottom_width: Some(Tokenized::Literal(1.0)),
                border_left_width: Some(Tokenized::Literal(1.0)),
                border_top_color: Some(col(border)),
                border_right_color: Some(col(border)),
                border_bottom_color: Some(col(border)),
                border_left_color: Some(col(border)),
                ..Default::default()
            }) {
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    ..Default::default()
                }) { em }
                text(style = StyleRules {
                    font_size: Some(px(12.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { cnt.to_string() }
            }
            .on_touch(move |ev| {
                // LIVE rows route the tap through the backend RPC: `request_live_reaction`
                // persists the row and broadcasts a count frame that updates the pill map
                // (for THIS client via its own echo AND every other connected member). No
                // optimistic mutation — the echo is the single source of truth, so a cold
                // reload and a live sync converge identically. MOCK rows keep the direct
                // session-local toggle: not-yet-reacted -> count+1 and highlight; already
                // reacted -> count-1 and un-highlight, dropping the pill at 0.
                if ev.phase == TouchPhase::Ended {
                    if live_key {
                        request_live_reaction(key, em_close.clone(), will_remove, picker_l);
                    } else {
                        let mut m = r.borrow_mut();
                        let inner = m.entry(key).or_default();
                        if let Some(pos) = inner.iter().position(|(e, _, _)| e == &em_close) {
                            let (e, c, was_reacted) = inner[pos].clone();
                            if was_reacted {
                                if c <= 1 {
                                    inner.remove(pos);
                                } else {
                                    inner[pos] = (e, c - 1, false);
                                }
                            } else {
                                inner[pos] = (e, c + 1, true);
                            }
                        }
                        if m.get(&key).map_or(false, |v| v.is_empty()) {
                            m.remove(&key);
                        }
                        rev.set(rev.get() + 1);
                    }
                }
                TouchResponse::CONSUMED
            })
        }
    }).collect();

    // The always-visible "add reaction" button: a small circular glyph target
    // (a `view` + icon, the composer icon-button shape) whose `.on_touch` opens
    // the picker for this message, keyed by the row's `MessageKey`.
    let add = picker;
    let add_key = key;
    let add_button = ui! {
        view(style = StyleRules {
            position: Some(Position::Relative),
            width: Some(px(26.0)),
            height: Some(px(26.0)),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::Center),
            background: Some(col(COMPOSER)),
            border_top_left_radius: Some(px(13.0)),
            border_top_right_radius: Some(px(13.0)),
            border_bottom_left_radius: Some(px(13.0)),
            border_bottom_right_radius: Some(px(13.0)),
            ..Default::default()
        }) {
            icon(SMILE_PLUS).color(|| Color::from(TEXT_MUTED)).size(15.0)
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                add.set(Some(add_key));
            }
            TouchResponse::CONSUMED
        })
    };

    // Discord shows the add-reaction button only next to existing pills: with
    // no reactions the strip is an empty zero-size view (takes no height), so
    // unreacted messages sit closer together.
    if pills.is_empty() {
        return ui! { view(style = StyleRules { ..Default::default() }) {} };
    }

    // The strip: pills left, spacer, add button right. `padding_left` of 66
    // aligns it with the message body (row padding 16 + avatar 40 + gap 10) so
    // the pills sit under the text, Discord-style.
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(4.0)),
            padding_top: Some(px(2.0)),
            padding_left: Some(px(66.0)),
            padding_right: Some(px(16.0)),
            ..Default::default()
        }) {
            pills
            view(style = StyleRules {
                flex_grow: Some(grow(1.0)),
                ..Default::default()
            }) {}
            add_button
        }
    }
}

/// The add-reaction picker: a full-window overlay (opaque backdrop + centered
/// emoji grid) shown while `picker` names a message. Tapping a glyph adds a
/// reaction for that message and closes the picker; tapping the backdrop closes
/// it without adding. Reuses the overlay idiom (a `Position::Absolute` child of
/// the root `Relative` ancestor, painted last like `profile_region` /
/// `notif_region`) and the composer's emoji grid.
pub(super) fn reaction_picker_overlay(
    picker: Signal<Option<MessageKey>>,
    reactions: Rc<RefCell<ReactionMap>>,
    reaction_rev: Signal<u32>,
) -> Element {
    switch(
        move || picker.get(),
        move |p: &Option<MessageKey>| {
            let Some(sel) = p else {
                return ui! {
                    view(style = StyleRules {
                        width: Some(px(0.0)),
                        height: Some(px(0.0)),
                        ..Default::default()
                    }) {}
                };
            };
            let key = *sel;
            // Add (or bump) the picked emoji's reaction for this message, then
            // close the picker. LIVE rows (gateway-keyed) route through the
            // backend RPC (`request_live_reaction`) which persists a row and
            // broadcasts the count frame back into `gateway_incoming_effect` —
            // the pill is NOT mutated optimistically, the echo frame is the
            // single source of truth. MOCK rows keep the session-local mutation.
            let r = reactions.clone();
            let rev = reaction_rev;
            let close = picker;
            let pick_cb: Rc<dyn Fn(String)> = Rc::new(move |glyph: String| {
                let live = matches!(key, MessageKey::Live { .. });
                if live {
                    request_live_reaction(key, glyph, false, close);
                } else {
                    let mut m = r.borrow_mut();
                    let inner = m.entry(key).or_default();
                    if let Some(pos) = inner.iter().position(|(e, _, _)| e == &glyph) {
                        let (e, c, _) = inner[pos].clone();
                        inner[pos] = (e, c + 1, true);
                    } else {
                        inner.push((glyph, 1, true));
                    }
                    rev.set(rev.get() + 1);
                    close.set(None);
                }
            });
            // Opaque full-window backdrop (dismiss-on-outside-tap) with the full
            // emoji picker card centered.
            let backdrop_close = picker;
            ui! {
                view(style = StyleRules {
                    position: Some(Position::Absolute),
                    left: Some(px(0.0)),
                    top: Some(px(0.0)),
                    width: Some(pct(100.0)),
                    height: Some(pct(100.0)),
                    background: Some(col(BG_RAIL)),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    ..Default::default()
                }) {
                    view(style = StyleRules {
                        background: Some(col(BG_ELEVATED)),
                        border_top_left_radius: Some(px(10.0)),
                        border_top_right_radius: Some(px(10.0)),
                        border_bottom_left_radius: Some(px(10.0)),
                        border_bottom_right_radius: Some(px(10.0)),
                        overflow: Some(Overflow::Hidden),
                        ..Default::default()
                    }) {
                        EmojiPicker(on_pick = Some(pick_cb))
                    }
                }
                .on_touch(move |ev| {
                    // Tap on the backdrop (not a glyph) closes the picker.
                    if ev.phase == TouchPhase::Ended {
                        backdrop_close.set(None);
                    }
                    TouchResponse::CONSUMED
                })
            }
        },
    )
}
