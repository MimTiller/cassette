//! Member lists: mock and live member panels, member rows, the profile panel, roles and moderation action menus, and the member filter.
//! Split out of `app.rs`; `use super::*` keeps app.rs's imports and shared
//! colors/helpers in scope.

use crate::theme;
use super::*;

/// The live member row's avatar chip size (px): the same 32px circle the
/// mock `member_row` and the user panel draw.
const MEMBER_CHIP_SIZE: f32 = 32.0;

/// Offline rows render at this opacity (Discord dims offline members).
const MEMBER_OFFLINE_OPACITY: f32 = 0.5;

/// The active live server's member roster: the distinct author labels that have
/// posted in any of its channels (from the already-buffered gateway messages).
/// No backend call — pure reuse of the data the client already holds. If no
/// channel is open yet (no frames buffered) and nothing has posted, fall back
/// to the signed-in user's own label so `@` still has a match.
pub(super) fn live_member_roster(
    live_channels: &[crate::servers::ChannelSummary],
    live_messages: &HashMap<i64, Vec<LiveMsg>>,
) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for c in live_channels {
        if let Some(msgs) = live_messages.get(&c.id) {
            for (_mid, _aid, author, _body, _ts, _edited, _reply_to) in msgs {
                if !names.contains(author) {
                    names.push(author.clone());
                }
            }
        }
    }
    if names.is_empty() {
        let my = crate::auth::CURRENT_USER_ID.load(std::sync::atomic::Ordering::SeqCst);
        if my != 0 {
            names.push(format!("user {}", my));
        }
    }
    names.sort();
    names
}

/// The role a member advances to when their cycle-button is tapped, in the
/// order member → moderator → admin → owner → member. `'owner'` is included in
/// the cycle (so all four role strings are reachable) even though the backend
/// rejects assigning it to anyone who isn't already the owner — the rejection
/// is surfaced as an inline error rather than pre-validated (the backend is
/// the single source of truth for who may hold which role).
pub(super) fn next_role(current: &str) -> &'static str {
    match current {
        "member" => "moderator",
        "moderator" => "admin",
        "admin" => "owner",
        // owner and anything unknown cycle back to member.
        _ => "member",
    }
}

/// Client-side permission pre-check mirroring the backend's role bars (the
/// backend stays the enforcer; this only decides which menu rows RENDER):
/// roles rank member < moderator < admin < owner; kick and channel-rename
/// need moderator+, ban / timeout / channel-delete need admin+.
pub(super) fn role_rank(role: String) -> i32 {
    match role.trim().to_lowercase().as_str() {
        "owner" => 3,
        "admin" => 2,
        "moderator" => 1,
        "member" => 0,
        _ => -1,
    }
}

/// The acting (signed-in) user's own role string in the active live roster —
/// the discriminator for rendering moderation controls (Kick/Ban/Timeout/
/// Rename/Delete) before the user even taps them. A user not on the roster
/// (or a guest) resolves to "" and ranks below every bar, so no controls
/// render. Data comes from the same `list_members` fetch the panel renders.
pub(super) fn acting_role(members: &Vec<crate::moderation::MemberInfo>, self_id: i64) -> String {
    members.iter()
        .find(|m| m.user_id == self_id)
        .map(|m| m.role.clone())
        .unwrap_or_default()
}

/// A small popup action menu anchored below a target element (the member-
/// row / channel-row context menu Discord opens from a row tap). Rendered by
/// a `switch` keyed on a SHARED `Signal<Option<i64>>` — the row's tap handler
/// sets it to `Some(key)`, so exactly one menu (the tapped row's) mounts,
/// anchored below that row. Backdrop is `BackdropMode::None`: the rest of the
/// screen stays interactive (a menu, not a full-screen modal). Rows are
/// (label, destructive-red?, handler); a row tap fires its handler then
/// closes the menu (the downstream confirm steps — kick/ban/timeout inline
/// two-steps, rename/delete modals — stay as they were).
pub(super) fn action_menu(
    open: Signal<Option<i64>>,
    key: i64,
    anchor: Ref<ViewHandle>,
    rows: Vec<(String, bool, Rc<dyn Fn()>)>,
) -> Element {
    switch(
        move || open.get(),
        move |sel: &Option<i64>| {
            if *sel != Some(key) {
                return ui! {
                    view(style = StyleRules {
                        width: Some(px(0.0)),
                        height: Some(px(0.0)),
                        ..Default::default()
                    }) {}
                };
            }
            // The render closure is `Fn` — clone the (non-`Copy`) row list
            // into a local so the `for` below can consume it each render.
            let rows = rows.clone();
            ui! {
                anchored_overlay(
                    target = AnchorTarget::from(anchor),
                    side = ElementSide::Below,
                    align = ElementAlign::Start,
                    offset = 6.0,
                    backdrop = BackdropMode::None,
                ) {
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Column),
                        width: Some(px(150.0)),
                        gap: Some(px(2.0)),
                        background: Some(col(theme::BG_FLOATING)),
                        border_top_left_radius: Some(px(8.0)),
                        border_top_right_radius: Some(px(8.0)),
                        border_bottom_left_radius: Some(px(8.0)),
                        border_bottom_right_radius: Some(px(8.0)),
                        border_top_width: Some(border_width(1.0)),
                        border_right_width: Some(border_width(1.0)),
                        border_bottom_width: Some(border_width(1.0)),
                        border_left_width: Some(border_width(1.0)),
                        border_top_color: Some(col(BORDER)),
                        border_right_color: Some(col(BORDER)),
                        border_bottom_color: Some(col(BORDER)),
                        border_left_color: Some(col(BORDER)),
                        overflow: Some(Overflow::Hidden),
                        ..Default::default()
                    }) {
                        for (label, red, on) in rows.into_iter() {
                            view(style = StyleRules {
                                padding_top: Some(px(8.0)),
                                padding_bottom: Some(px(8.0)),
                                padding_left: Some(px(12.0)),
                                padding_right: Some(px(12.0)),
                                flex_direction: Some(FlexDirection::Row),
                                align_items: Some(AlignItems::Center),
                                ..Default::default()
                            }) {
                                text(style = StyleRules {
                                    font_size: Some(px(14.0)),
                                    // Destructive actions read as RED TEXT only
                                    // (Discord's treatment) — never a filled pill.
                                    color: Some(col(if red { BADGE } else { TEXT })),
                                    ..Default::default()
                                }) { label.clone() }
                            }
                            .on_touch(move |ev| {
                                if ev.phase == TouchPhase::Ended {
                                    on();
                                    open.set(None);
                                }
                                TouchResponse::CONSUMED
                            })
                        }
                    }
                }
            }
        },
    )
}

/// The live-server member panel: the real membership of the active backend
/// server (fetched from the `#[server]` `list_members` endpoint, owner-first)
/// rendered as a right-side column — the live counterpart of the mock
/// `member_panel`. Each member row shows the
/// presence dot, the member's avatar chip (when the backend carries one) and
/// their username + role chip — the same content contract as the mock
/// `member_row` / the current user's row. Moderation actions (Kick / Ban /
/// Timeout) are NOT inline pills: tapping the row opens the `action_menu`
/// popup anchored below it (rendered only when the acting user's role clears
/// the backend's bar — moderator+ for Kick, admin+ for Ban/Timeout), and the
/// menu entry arms the row's existing inline two-step confirm.
pub(super) fn live_member_panel(
    server_id: i64,
    members: Signal<Vec<crate::moderation::MemberInfo>>,
    error: Signal<String>,
    kick_confirm: Signal<Option<i64>>,
    ban_confirm: Signal<Option<i64>>,
    timeout_confirm: Signal<Option<i64>>,
    timeout_duration: Signal<i64>,
    presence: Rc<RefCell<HashMap<i64, String>>>,
    popup: super::user_popups::UserPopupCtx,
) -> Element {
    // Snapshot of the current roster. The panel is rebuilt (by the caller's
    // `switch` keyed on this signal) whenever the roster changes — including
    // the re-fetch the mutation callbacks below trigger on success — so a
    // local snapshot is always current here.
    let roster = members.get();
    // Snapshot the live presence map the same way. The caller's `switch` key
    // also folds in `live_presence_rev`, so a `Presence` frame forces a
    // rebuild and this snapshot picks up the new status. A member absent from
    // the map (e.g. just after the roster loads, before any frame arrived) is
    // rendered as a grey "offline" dot below.
    let presence: HashMap<i64, String> = presence.borrow().clone();
    // The signed-in user's own numeric id — their own row is marked "(you)" and
    // the rest is left to the backend's permission checks (no client-side
    // pre-computation). `CURRENT_USER_ID` is `0` when signed out, which can't
    // match a real member id, so `(you)` simply never appears.
    let self_id: i64 = crate::auth::CURRENT_USER_ID
        .load(std::sync::atomic::Ordering::SeqCst);

    // Shared styles (built once per rebuild; `StyleRules` is not `Copy`, so the
    // per-row styles are written INLINE inside the loop, the `member_role_list`
    // idiom — nothing non-`Copy` is captured across the loop boundary).
    let name_style = StyleRules {
        font_size: Some(px(14.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT)),
        flex_grow: Some(grow(1.0)),
        flex_shrink: Some(grow(0.0)),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    let role_btn_style = StyleRules {
        padding_top: Some(px(2.0)),
        padding_bottom: Some(px(2.0)),
        padding_left: Some(px(8.0)),
        padding_right: Some(px(8.0)),
        background: Some(col(COMPOSER)),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        flex_shrink: Some(grow(0.0)),
        ..Default::default()
    };
    let role_label_style = StyleRules {
        font_size: Some(px(11.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT_CHANNEL)),
        ..Default::default()
    };
    // The Kick/Ban/Timeout actions live in the row-tap `action_menu` popup
    // (red TEXT labels only); this panel renders no moderation pills.
    let confirm_bg_style = StyleRules {
        background: Some(col(BG_ACTIVE)),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        padding_top: Some(px(4.0)),
        padding_bottom: Some(px(4.0)),
        padding_left: Some(px(8.0)),
        padding_right: Some(px(8.0)),
        gap: Some(px(8.0)),
        flex_direction: Some(FlexDirection::Row),
        align_items: Some(AlignItems::Center),
        ..Default::default()
    };
    let confirm_yes_style = StyleRules {
        background: Some(col(BADGE)),
        padding_top: Some(px(4.0)),
        padding_bottom: Some(px(4.0)),
        padding_left: Some(px(10.0)),
        padding_right: Some(px(10.0)),
        border_top_left_radius: Some(px(6.0)),
        border_top_right_radius: Some(px(6.0)),
        border_bottom_left_radius: Some(px(6.0)),
        border_bottom_right_radius: Some(px(6.0)),
        flex_shrink: Some(grow(0.0)),
        ..Default::default()
    };
    let confirm_no_style = StyleRules {
        background: Some(col(BG_HOVER)),
        padding_top: Some(px(4.0)),
        padding_bottom: Some(px(4.0)),
        padding_left: Some(px(10.0)),
        padding_right: Some(px(10.0)),
        border_top_left_radius: Some(px(6.0)),
        border_top_right_radius: Some(px(6.0)),
        border_bottom_left_radius: Some(px(6.0)),
        border_bottom_right_radius: Some(px(6.0)),
        flex_shrink: Some(grow(0.0)),
        ..Default::default()
    };
    let small_label_style = StyleRules {
        font_size: Some(px(12.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(theme::TEXT_ON_ACCENT)),
        ..Default::default()
    };
    let error_style = StyleRules {
        font_size: Some(px(11.0)),
        color: Some(col(BADGE)),
        ..Default::default()
    };

    // Build the rows as a local `Vec<Element>` (the mock `member_panel` shape).
    // Each row is a `move` closure: `server_id` (i64) and `member_id` (i64) are
    // `Copy`, and the three `Signal`s it captures are `Copy`, so every closure
    // owns a fresh copy and stays `Fn` across rebuilds.
    // Discord groups the roster into ONLINE / OFFLINE sections. Online = any
    // presence status other than offline (idle and DND count as online); a
    // member missing from the presence snapshot is offline. Rows go into two
    // buckets; each keeps the roster's existing order (push order).
    let mut online_rows: Vec<Element> = Vec::<Element>::new();
    let mut offline_rows: Vec<Element> = Vec::<Element>::new();
    for member in roster.iter() {
        let member_id = member.user_id;
        let is_self = member_id == self_id;
        let role = member.role.clone();
        let confirming = kick_confirm.clone();
        let kick = kick_confirm.clone();
        let banning = ban_confirm.clone();
        let ban = ban_confirm.clone();
        let timing = timeout_confirm.clone();
        let timeout = timeout_confirm.clone();
        let dur = timeout_duration.clone();
        let error = error.clone();
        // Fresh per-iteration clones of every style used below -- the whole
        // `ui! { ... }` row block compiles to one closure per iteration, and
        // a style declared once above this loop gets moved into that
        // closure on iteration 1, leaving nothing for iteration 2+ (the
        // same "StyleRules moved twice" class of bug documented elsewhere
        // in nodes.yaml's b60 history; the fix there was the same: clone
        // before the loop body captures it, not just at each use site).
        let name_style = name_style.clone();
        let role_btn_style = role_btn_style.clone();
        let role_label_style = role_label_style.clone();
        let confirm_bg_style = confirm_bg_style.clone();
        let confirm_yes_style = confirm_yes_style.clone();
        let confirm_no_style = confirm_no_style.clone();
        let small_label_style = small_label_style.clone();
        let error_style = error_style.clone();
        // The kick-confirm / ban-confirm / idle arms below share the confirm +
        // kick styles above. They render through ONE flat `match` (in the row
        // below), not an `if / else if / else` chain: a chain lowers to nested
        // `when` closures whose `else if`/`else` arms MOVE any non-`Copy` style
        // they use out of the parent `Fn` (the ui! macro's "B5" capture bug),
        // whereas a single `match` lowers to one `switch` dispatcher that every
        // arm BORROWS via `.clone()` — so no per-branch style copies are needed.
        // name_style is used in two mutually-exclusive branches below, but
        // the ui! macro's own parser doesn't support a nested `let` inside a
        // branch (tried it -- breaks its DSL parsing), so each branch gets
        // its own pre-made clone here instead, outside the macro entirely.
        let name_style_self_branch = name_style.clone();
        let name_style_other_branch = name_style;
        // Live presence dot for this row. `presence` is the snapshot taken at
        // the top of the fn (rebuilt on every `live_presence_rev` bump); the
        // status string is whatever this member last reported over the gateway.
        // Absent (no frame yet — e.g. right after the roster first loads) and
        // any value the backend doesn't send are both rendered as the offline
        // grey. The colors mirror the mock `Status::color()` convention so the
        // live panel reads like the mock one: green online, amber away (the
        // server's idle state), grey offline.
        // Grouping: a member is OFFLINE only when their status string is
        // "offline" or absent from the snapshot; every other status (online,
        // away/idle, dnd) counts as online, like Discord.
        let is_offline = match presence.get(&member_id).map(|s| s.as_str()) {
            Some("online") => false,
            Some("away") => false,
            Some("idle") => false,
            Some("dnd") => false,
            Some("do_not_disturb") => false,
            _ => true,
        };
        let presence_color = match presence.get(&member_id).map(|s| s.as_str()) {
            Some("online") => theme::SUCCESS,
            Some("away") => theme::WARNING,
            Some("offline") => theme::TEXT_FAINT,
            _ => theme::TEXT_FAINT,
        };
        // The row's avatar: the shared `avatar_chip` helper — uploaded image
        // when the client registry has one, else the letter chip — in a
        // `Position::Relative` wrapper carrying the presence dot absolutely at
        // the bottom-right corner with a panel-bg ring (same structure as the
        // mock `member_row` and the user panel's status dot).
        let chip = crate::avatar_chip::avatar_chip(
            member.username.as_str(),
            BUTTON,
            MEMBER_CHIP_SIZE,
            MEMBER_CHIP_SIZE / 2.0,
        );
        let avatar_wrap_style = StyleRules {
            position: Some(Position::Relative),
            width: Some(px(MEMBER_CHIP_SIZE)),
            height: Some(px(MEMBER_CHIP_SIZE)),
            flex_shrink: Some(grow(0.0)),
            ..Default::default()
        };
        // Presence ring matches the panel background so the dot reads as
        // "cut out" of the avatar edge (Discord's look).
        let ring_bg = BG_SIDEBAR;
        let dot_style = StyleRules {
            position: Some(Position::Absolute),
            right: Some(px(0.0)),
            bottom: Some(px(0.0)),
            width: Some(px(11.0)),
            height: Some(px(11.0)),
            background: Some(col(presence_color)),
            border_top_width: Some(border_width(2.0)),
            border_right_width: Some(border_width(2.0)),
            border_bottom_width: Some(border_width(2.0)),
            border_left_width: Some(border_width(2.0)),
            border_top_color: Some(col(ring_bg)),
            border_right_color: Some(col(ring_bg)),
            border_bottom_color: Some(col(ring_bg)),
            border_left_color: Some(col(ring_bg)),
            border_top_left_radius: Some(px(6.0)),
            border_top_right_radius: Some(px(6.0)),
            border_bottom_left_radius: Some(px(6.0)),
            border_bottom_right_radius: Some(px(6.0)),
            ..Default::default()
        };

        let row = ui! {
            view(style = StyleRules {
                padding_top: Some(px(6.0)),
                padding_bottom: Some(px(6.0)),
                padding_left: Some(px(12.0)),
                padding_right: Some(px(12.0)),
                flex_direction: Some(FlexDirection::Row),
                align_items: Some(AlignItems::Center),
                gap: Some(px(8.0)),
                // Discord dims offline rows (~50%) but keeps them clickable.
                opacity: Some(Tokenized::Literal(if is_offline { MEMBER_OFFLINE_OPACITY } else { 1.0 })),
                ..Default::default()
            }) {
                // The member's avatar chip: uploaded image when the client
                // registry has one, otherwise the letter chip, with the
                // presence dot overlaid at its bottom-right corner (ring in
                // the panel bg color), like the mock member row / user panel.
                view(style = avatar_wrap_style.clone()) {
                    { chip }
                    view(style = dot_style.clone()) {}
                }
                // Name (+ a "(you)" tag on the signed-in user's own row).
                // flex_shrink 0 + min content: the wrapper must NEVER collapse
                // to zero width (the bug this fixes: shrinking the name's
                // wrapper to fit alongside the row's inline action pills
                // rendered non-self rows with a role badge and NO username).
                view(style = StyleRules {
                    flex_shrink: Some(grow(0.0)),
                    overflow: Some(Overflow::Hidden),
                    ..Default::default()
                }) {
                    if is_self {
                        text(style = name_style_self_branch) {
                            format!("{} (you)", crate::auth::shown_name(&member.username))
                        }
                    } else {
                        text(style = name_style_other_branch) { crate::auth::shown_name(&member.username) }
                    }
                }

                // Role cycle-button: shows the current role; a tap advances it
                // (member→moderator→admin→owner→member) and calls
                // `set_member_role`, refreshing the roster on success. The
                // backend rejects invalid changes (e.g. assigning `owner`),
                // which surface via the inline error line below.
                if !is_self {
                    view(style = role_btn_style.clone()) {
                        text(style = role_label_style.clone()) {
                            format!("{role} ▸")
                        }
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            let sid = server_id;
                            let uid = member_id;
                            let new_role = next_role(&role);
                            let err = error;
                            let kick = kick;
                            let members = members;
                            runtime_core::driver::spawn_async(async move {
                                match crate::moderation::set_member_role(sid, uid, new_role).await {
                                    Ok(()) => {
                                        kick.set(None);
                                        if let Ok(fresh) = crate::moderation::list_members(sid).await {
                                            crate::mentions::members_loaded(sid, &fresh);
                                            members.set(fresh);
                                        }
                                    }
                                    Err(msg) => err.set(msg),
                                }
                            });
                        }
                        TouchResponse::CONSUMED
                    })
                }

                // Kick: a two-step inline confirm. First tap shows Confirm/Cancel;
                // a second tap ("Confirm") calls `remove_member` and refreshes;
                // "Cancel" (or tapping the row again) dismisses. The confirm
                // branch replaces the role+kick controls for this row only.
                // Kick / Ban: a two-step inline confirm each. The row's three
                // mutually-exclusive action states (kick confirm pending, ban
                // confirm pending, idle buttons) are one flat `match` over the
                // the confirm signals, not an `if / else if / else` chain — see
                // the "B5" capture note above. The timeout confirm is the
                // third mutually-exclusive pending state (kick / ban / timeout).
                match (confirming.get(), banning.get(), timing.get()) {
                    (Some(uid), _, _) if *uid == member_id => {
                    view(style = confirm_bg_style.clone()) {
                        text(style = StyleRules {
                            font_size: Some(px(12.0)),
                            color: Some(col(TEXT)),
                            flex_shrink: Some(grow(1.0)),
                            ..Default::default()
                        }) { "Kick?" }
                        view(style = confirm_yes_style.clone()) {
                            text(style = small_label_style.clone()) { "Yes" }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                let sid = server_id;
                                let uid = member_id;
                                let err = error;
                                let members = members;
                                runtime_core::driver::spawn_async(async move {
                                    match crate::moderation::kick_member(sid, uid).await {
                                        Ok(()) => {
                                            kick.set(None);
                                            if let Ok(fresh) = crate::moderation::list_members(sid).await {
                                                crate::mentions::members_loaded(sid, &fresh);
                                                members.set(fresh);
                                            }
                                        }
                                        Err(msg) => {
                                            kick.set(None);
                                            err.set(msg);
                                        }
                                    }
                                });
                            }
                            TouchResponse::CONSUMED
                        })
                        view(style = confirm_no_style.clone()) {
                            text(style = small_label_style.clone()) { "No" }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                kick.set(None);
                            }
                            TouchResponse::CONSUMED
                        })
                    }
                } (_, Some(uid), _) if *uid == member_id && !is_self => {
                    // Ban: same two-step shape as the kick confirm, but calls
                    // `ban_member` (remove AND bar rejoining) and clears the ban
                    // confirm on resolve. Takes priority over the kick confirm
                    // when both are somehow pending for a row (they're set from
                    // different buttons, so in practice never both at once).
                    view(style = confirm_bg_style.clone()) {
                        text(style = StyleRules {
                            font_size: Some(px(12.0)),
                            color: Some(col(TEXT)),
                            flex_shrink: Some(grow(1.0)),
                            ..Default::default()
                        }) { "Ban?" }
                        view(style = confirm_yes_style.clone()) {
                            text(style = small_label_style.clone()) { "Yes" }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                let sid = server_id;
                                let uid = member_id;
                                let err = error;
                                let members = members;
                                let ban = ban;
                                runtime_core::driver::spawn_async(async move {
                                    match crate::moderation::ban_member(sid, uid, None).await {
                                        Ok(()) => {
                                            ban.set(None);
                                            if let Ok(fresh) = crate::moderation::list_members(sid).await {
                                                crate::mentions::members_loaded(sid, &fresh);
                                                members.set(fresh);
                                            }
                                        }
                                        Err(msg) => {
                                            ban.set(None);
                                            err.set(msg);
                                        }
                                    }
                                });
                            }
                            TouchResponse::CONSUMED
                        })
                        view(style = confirm_no_style.clone()) {
                            text(style = small_label_style.clone()) { "No" }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                ban.set(None);
                            }
                            TouchResponse::CONSUMED
                        })
                    }
                } (_, _, Some(uid)) if *uid == member_id && !is_self => {
                    // Timeout: the mildest of the three severities — the member
                    // stays on the roster but is silenced from posting for the
                    // chosen duration. The row shows the duration picker (taps
                    // write the shared `timeout_duration` signal) plus Yes/No;
                    // Yes calls `timeout_member` with the duration read at
                    // click time, No (or a re-tap) dismisses.
                    // Stacks vertically (unlike the kick/ban confirm, whose
                    // one-word label + Yes/No fit in the 260px panel as a row):
                    // the inline duration picker is wide, so a flex Row pushes
                    // Yes/No past the panel's right edge (clipped, unreachable).
                    // A flex Column keeps every control inside the panel width.
                    view(style = StyleRules {
                        background: Some(col(BG_ACTIVE)),
                        border_top_left_radius: Some(px(8.0)),
                        border_top_right_radius: Some(px(8.0)),
                        border_bottom_left_radius: Some(px(8.0)),
                        border_bottom_right_radius: Some(px(8.0)),
                        padding_top: Some(px(4.0)),
                        padding_bottom: Some(px(4.0)),
                        padding_left: Some(px(8.0)),
                        padding_right: Some(px(8.0)),
                        gap: Some(px(8.0)),
                        flex_direction: Some(FlexDirection::Column),
                        align_items: Some(AlignItems::Center),
                        ..Default::default()
                    }) {
                        text(style = StyleRules {
                            font_size: Some(px(12.0)),
                            color: Some(col(TEXT)),
                            ..Default::default()
                        }) { "Time out?" }
                        TimeoutDurationPicker(duration = dur.clone())
                        view(style = confirm_yes_style.clone()) {
                            text(style = small_label_style.clone()) { "Yes" }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                let sid = server_id;
                                let uid = member_id;
                                let err = error;
                                let secs = dur.get();
                                let timeout = timeout;
                                runtime_core::driver::spawn_async(async move {
                                    match crate::moderation::timeout_member(sid, uid, secs).await {
                                        Ok(()) => {
                                            timeout.set(None);
                                        }
                                        Err(msg) => {
                                            timeout.set(None);
                                            err.set(msg);
                                        }
                                    }
                                });
                            }
                            TouchResponse::CONSUMED
                        })
                        view(style = confirm_no_style.clone()) {
                            text(style = small_label_style.clone()) { "No" }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                timeout.set(None);
                            }
                            TouchResponse::CONSUMED
                        })
                    }
                }
                // Idle rows carry NO inline action buttons: the moderation
                // actions (Kick / Ban / Timeout) live in the popup `action_menu`
                // anchored below the row, opened by a tap on the row itself
                // (below, in `.on_touch`). Only the confirm states render here.
                _ => {}
            }

            // Inline backend error for this row (the most recent one). Shown
            // only when a kick/role-change was rejected — the backend's own
            // message, no client-side permission logic.
            if !error.get().is_empty() {
                text(style = error_style.clone()) { move || error.get().clone() }
            }
        }
        // A click opens the member's profile card and a right-click the user
        // menu (which carries Kick / Timeout / Ban for permitted users), both
        // at the press. A click while one of this row's two-step confirms is
        // showing dismisses the confirm instead (the re-tap-cancel idiom); a
        // click on ANOTHER row while a confirm is pending clears that pending
        // confirm first and opens the clicked row's card in the same click.
        .on_touch(move |ev| {
            let secondary = runtime_shared::touch::pointer_button().opens_context_menu();
            let at = ev.window_position;
            if ev.phase == TouchPhase::Began && secondary {
                popup.open(member_id, at.x, at.y, true);
            } else if ev.phase == TouchPhase::Ended && !secondary {
                if confirming.get() == Some(member_id)
                    || banning.get() == Some(member_id)
                    || timing.get() == Some(member_id)
                {
                    confirming.set(None);
                    banning.set(None);
                    timing.set(None);
                } else {
                    // A pending two-step confirm on ANOTHER row is dismissed by
                    // this click too (one click closes A's confirm and opens
                    // B's card), then the card opens in the same click.
                    if confirming.get().is_some()
                        || banning.get().is_some()
                        || timing.get().is_some()
                    {
                        confirming.set(None);
                        banning.set(None);
                        timing.set(None);
                    }
                    popup.open(member_id, at.x, at.y, false);
                }
            }
            TouchResponse::CONSUMED
        })
        };
        if is_offline {
            offline_rows.push(row);
        } else {
            online_rows.push(row);
        }
    }

    // Section headers: same treatment as the mock panel's group labels
    // (uppercase, small, muted, semibold via section_label()) — one per
    // non-empty group, each carrying its count.
    let section_header_style = StyleRules {
        padding_top: Some(px(10.0)),
        padding_bottom: Some(px(2.0)),
        padding_left: Some(px(12.0)),
        ..section_label()
    };
    let online_header = ui! {
        text(style = section_header_style.clone()) {
            format!("ONLINE — {}", online_rows.len())
        }
    };
    let offline_header = ui! {
        text(style = section_header_style) {
            format!("OFFLINE — {}", offline_rows.len())
        }
    };
    let section_children: Vec<Element> = {
        let mut secs: Vec<Element> = Vec::<Element>::new();
        if !online_rows.is_empty() {
            secs.push(online_header);
            secs.extend(online_rows.into_iter());
        }
        if !offline_rows.is_empty() {
            secs.push(offline_header);
            secs.extend(offline_rows.into_iter());
        }
        secs
    };

    ui! {
        view(style = StyleRules {
            width: Some(px(260.0)),
            background: Some(col(BG_SIDEBAR)),
            border_left_width: Some(border_width(1.0)),
            border_left_color: Some(col(BORDER)),
            flex_direction: Some(FlexDirection::Column),
            overflow: Some(Overflow::Hidden),
            ..Default::default()
        }) {
            scroll_view(style = StyleRules {
                flex_grow: Some(grow(1.0)),
                ..Default::default()
            }) { section_children }
        }
    }
}

/// Case-insensitive prefix filter on the roster for the autocomplete dropdown.
pub(super) fn filter_members(q: &str, members: Vec<String>) -> Vec<String> {
    let q = q.to_lowercase();
    members
        .into_iter()
        .filter(|m| m.to_lowercase().starts_with(&q) || crate::auth::shown_name(m).to_lowercase().starts_with(&q))
        .collect()
}
