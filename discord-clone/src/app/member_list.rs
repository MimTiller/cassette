//! `app_shell` section, split out of `app.rs`. Reads the shared state
//! through `ShellState`; returns the elements the final layout places.

use super::*;

pub(super) struct MemberListParts {
    pub(super) members_switch: Element,
}

pub(super) fn build(s: &ShellState) -> MemberListParts {
    let ShellState { active_live_server, blocked_users, live_ban_confirm, live_kick_confirm, live_member_error, live_member_menu, live_members, live_presence, live_presence_rev, live_revision, live_timeout_confirm, live_timeout_duration, user_popup, user_profile, .. } = s.clone();
    // --- member list (right side) ----------------------------------------
    // Rebuilds when the active server changes (each server has its own
    // roster) AND when the signed-in user's live status changes — the user
    // regroups to their new status bucket, and the member count follows.
    // Rendered as the right-most top-level column.
    // The right-side member column. Mirrors the middle view's mock/live split:
    // when a *live* server is active (`active_live_server` is `Some`, set the
    // moment a live channel is opened) render the real `live_member_panel`
    // backed by the gateway-fed `live_members` roster + `live_presence` map;
    // otherwise (a mock server is selected) render the static `member_panel`.
    // The key folds in every signal the live panel reads — roster, error,
    // kick-confirm, and the presence revision — so any of those changes (e.g.
    // a `Presence` frame arriving) rebuilds the panel and the dots update.
    // [Feature 16] The right-side member column. Mirrors the middle view's
    // live/empty split: when a *live* server is active (`active_live_server`
    // is `Some`, set the moment a live channel is opened) render the real
    // `live_member_panel` backed by the gateway-fed `live_members` roster +
    // `live_presence` map; otherwise every signed-in account with no live
    // server renders an empty roster (the mock `USER`/roster, which would
    // leak demo identities like "Nova", is gone). The key folds in every
    // signal the live panel reads — roster, error, kick-confirm, and the
    // presence revision — so any of those changes (e.g. a `Presence` frame
    // arriving) rebuilds the panel and the dots update.
    let live_members_panel = live_members.clone();
    let live_member_error_panel = live_member_error.clone();
    let live_kick_panel = live_kick_confirm.clone();
    let live_ban_panel = live_ban_confirm.clone();
    let live_timeout_panel = live_timeout_confirm.clone();
    let live_timeout_dur_panel = live_timeout_duration.clone();
    let live_presence_panel = live_presence.clone();
    // The mock message-search corpus was removed with the mock member panel;
    // the member column is now live-only.
    let members_switch = switch(
        move || {
            (
                active_live_server.get(),
                live_members.get(),
                live_member_error.get(),
                live_kick_confirm.get(),
                live_ban_confirm.get(),
                live_timeout_confirm.get(),
                live_timeout_duration.get(),
                live_presence_rev.get(),
                live_member_menu.get(),
            )
        },
        move |(
            live_active,
            _live_members,
            _live_err,
            _live_kick,
            _live_ban,
            _live_timeout,
            _live_timeout_dur,
            _presence_rev,
            _live_menu,
        ): &(
            Option<i64>,
            Vec<crate::moderation::MemberInfo>,
            String,
            Option<i64>,
            Option<i64>,
            Option<i64>,
            i64,
            u32,
            Option<i64>,
        )| {
            if let Some(ls) = *live_active {
                // Live server: real roster + live presence dots. The panel owns
                // its width internally (fixed 260px, same as the mock default
                // before a user drags the handle).
                return live_member_panel(
                    ls,
                    live_members_panel.clone(),
                    live_member_error_panel.clone(),
                    live_kick_panel.clone(),
                    live_ban_panel.clone(),
                    live_timeout_panel.clone(),
                    live_timeout_dur_panel.clone(),
                    // `Rc` isn't `Copy`, and this is an `Fn` (re-entrant) closure —
                    // clone the handle each render.
                    live_presence_panel.clone(),
                    super::user_popups::UserPopupCtx {
                        popup: user_popup,
                        profile: user_profile,
                        server: active_live_server,
                        blocked: blocked_users,
                        live_rev: live_revision,
                    },
                );
            }
            // Signed-in account, no live server yet -> an empty member panel
            // (never the mock `USER`/roster, which would leak demo identities
            // like "Nova" into the onboarding state).
            return ui! {
                view(style = StyleRules {
                    width: Some(px(240.0)),
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
                    }) {
                        text(style = StyleRules {
                            padding_top: Some(px(14.0)),
                            padding_left: Some(px(12.0)),
                            padding_right: Some(px(12.0)),
                            font_size: Some(px(13.0)),
                            color: Some(col(TEXT_MUTED)),
                            ..Default::default()
                        }) { "No members yet — join a server to see its roster.".to_string() }
                    }
                }
            };
        },
    );

    MemberListParts { members_switch }
}
