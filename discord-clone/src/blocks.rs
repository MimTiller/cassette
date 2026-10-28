//! Client side of user blocking: the signed-in user's blocked list (fetched
//! once per session and after every change), block / unblock, and the
//! "blocked message" placeholder the channel list shows instead of a blocked
//! user's messages. The server side is `discord_server::blocks`.

use crate::theme;
use runtime_core::{
    ui, AlignItems, Element, FlexDirection, FontWeight, Signal, StyleRules, TouchPhase, TouchResponse,
};

use crate::style_helpers::{col, px};

pub use discord_server::blocks::BlockedUser;

/// Whether `user_id` is in the blocked list.
pub fn is_blocked(list: &[BlockedUser], user_id: i64) -> bool {
    list.iter().any(|b| b.user_id == user_id)
}

/// Fetch the blocked list into `blocked`, then bump `rev` so the message list
/// re-filters.
pub fn load(blocked: Signal<Vec<BlockedUser>>, rev: Signal<u32>) {
    runtime_core::spawn_then_in(&crate::app::shell_alive(), discord_server::blocks::list_blocked(), move |result| {
        if let Ok(list) = result {
            blocked.set(list);
            rev.set(rev.get() + 1);
        }
    });
}

/// Block (or, with `block` false, unblock) `user_id`, then reload the list.
/// `done` runs after the server answered, whatever it said.
pub fn set_blocked(
    user_id: i64,
    block: bool,
    blocked: Signal<Vec<BlockedUser>>,
    rev: Signal<u32>,
    done: impl FnOnce() + 'static,
) {
    let request = async move {
        let input = discord_server::blocks::BlockUserInput { user_id };
        let _ = if block {
            discord_server::blocks::block_user(input).await
        } else {
            discord_server::blocks::unblock_user(input).await
        };
        discord_server::blocks::list_blocked().await
    };
    runtime_core::spawn_then_in(&crate::app::shell_alive(), request, move |result| {
        if let Ok(list) = result {
            blocked.set(list);
            rev.set(rev.get() + 1);
        }
        done();
    });
}

/// The row that stands in for a run of consecutive messages from blocked
/// users: "N Blocked Message(s) — Show". Show reveals exactly those messages
/// (their ids join `revealed`) and bumps `rev` so the list rebuilds.
pub fn hidden_run_row(ids: Vec<i64>, revealed: Signal<Vec<i64>>, rev: Signal<u32>) -> Element {
    let label = if ids.len() == 1 {
        "1 Blocked Message".to_string()
    } else {
        format!("{} Blocked Messages", ids.len())
    };
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(6.0)),
            padding_top: Some(px(6.0)),
            padding_bottom: Some(px(6.0)),
            padding_left: Some(px(66.0)),
            ..Default::default()
        }) {
            text(style = StyleRules { font_size: Some(px(14.0)), color: Some(col(theme::TEXT_MUTED)), ..Default::default() }) { label }
            text(style = StyleRules { font_size: Some(px(14.0)), color: Some(col(theme::TEXT_MUTED)), ..Default::default() }) { "—".to_string() }
            view {
                text(style = StyleRules {
                    font_size: Some(px(14.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    color: Some(col(theme::TEXT_LINK)),
                    ..Default::default()
                }) { "Show".to_string() }
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    let mut shown = revealed.get();
                    shown.extend(ids.iter().copied());
                    revealed.set(shown);
                    rev.set(rev.get() + 1);
                }
                TouchResponse::CONSUMED
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn membership_is_by_user_id() {
        let list = vec![BlockedUser { user_id: 7, username: "zed".into(), avatar_url: None }];
        assert!(is_blocked(&list, 7));
        assert!(!is_blocked(&list, 8));
    }
}
