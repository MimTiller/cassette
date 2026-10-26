//! Discord's click / right-click on a user: a profile card (avatar, name,
//! role, member-since date, mutual servers, Message / Add Friend) and a user
//! context menu (Profile, Mention, Message, friend and block actions,
//! moderation, Copy User ID). Both float at the press position over a
//! transparent full-window catcher that closes them.
//!
//! The open popup and the fetched profile live in `ShellState`
//! (`user_popup` / `user_profile`), owned by `app_shell`, so the async
//! profile fetch can always write its result.

use super::*;

use discord_server::profiles::{GetUserProfileInput, UserProfile};

/// Which popup is open, for whom, and where it was opened (window
/// coordinates of the press).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum UserPopup {
    Card { user_id: i64, x: f32, y: f32 },
    Menu { user_id: i64, x: f32, y: f32 },
}

/// The handles a user press needs, all `Copy`.
#[derive(Clone, Copy)]
pub(crate) struct UserPopupCtx {
    pub(crate) popup: Signal<Option<UserPopup>>,
    pub(crate) profile: Signal<Option<UserProfile>>,
    pub(crate) server: Signal<Option<i64>>,
}

impl UserPopupCtx {
    /// Open the card (or, with `menu`, the context menu) for `user_id` at
    /// window position (`x`, `y`), and fetch the profile it shows.
    pub(crate) fn open(self, user_id: i64, x: f32, y: f32, menu: bool) {
        self.popup.set(Some(if menu {
            UserPopup::Menu { user_id, x, y }
        } else {
            UserPopup::Card { user_id, x, y }
        }));
        self.refresh(user_id);
    }

    /// (Re)fetch `user_id`'s profile; a different user's stale profile is
    /// cleared first so the popup never shows the wrong person.
    pub(crate) fn refresh(self, user_id: i64) {
        if self.profile.get().map(|p| p.user_id) != Some(user_id) {
            self.profile.set(None);
        }
        let profile = self.profile;
        let server_id = self.server.get();
        runtime_core::driver::spawn_async(async move {
            let input = GetUserProfileInput { user_id, server_id };
            if let Ok(p) = discord_server::profiles::get_user_profile(input).await {
                profile.set(Some(p));
            }
        });
    }

    /// A press handler for a user's avatar or name: a primary click opens the
    /// card, a right-click (a secondary press, which only ever delivers
    /// `Began`) opens the context menu.
    pub(crate) fn on_press(self, user_id: i64) -> impl Fn(&TouchEvent) -> TouchResponse + Clone {
        move |ev: &TouchEvent| {
            let p = ev.window_position;
            let secondary = runtime_shared::touch::pointer_button().opens_context_menu();
            match ev.phase {
                TouchPhase::Began if secondary => self.open(user_id, p.x, p.y, true),
                TouchPhase::Ended if !secondary => self.open(user_id, p.x, p.y, false),
                _ => {}
            }
            TouchResponse::CONSUMED
        }
    }
}

/// "Oct 26, 2026" for an epoch-milliseconds instant, in local time.
pub(super) fn short_date(ms: i64) -> String {
    let (day, _, _) = local_day_clock(ms.div_euclid(1000));
    let (y, mo, d) = civil_ymd_from_day(day);
    let month = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
        [(mo - 1) as usize];
    format!("{month} {d}, {y}")
}

/// The card's banner color: one of a few muted tones, picked from the name
/// so a user always gets the same one.
fn banner_color(name: &str) -> &'static str {
    const TONES: [&str; 6] = ["#5865f2", "#3ba55c", "#faa61a", "#ed4245", "#9b59b6", "#1abc9c"];
    let sum: u32 = name.bytes().map(u32::from).sum();
    TONES[(sum % TONES.len() as u32) as usize]
}

/// Keep a `w` x `h` box opened at (`x`, `y`) inside the window.
fn place(x: f32, y: f32, w: f32, h: f32) -> (f32, f32) {
    let vp = viewport_size().get();
    let left = (x + 8.0).min(vp.width - w - 8.0).max(8.0);
    let top = (y - 16.0).min(vp.height - h - 8.0).max(8.0);
    (left, top)
}

pub(super) struct UserPopupsParts {
    pub(super) user_popup_el: Element,
    pub(super) chan_menu_catcher_el: Element,
}

pub(super) fn build(s: &ShellState) -> UserPopupsParts {
    let ShellState {
        active_live_server, composer, live_ban_confirm, live_dm_active, live_dm_messages,
        live_dm_mode, live_dm_revision, live_dms, live_kick_confirm, live_members, live_timeout_confirm,
        profile_mode, user_popup, user_profile, live_chan_menu, ..
    } = s.clone();
    let ctx = UserPopupCtx { popup: user_popup, profile: user_profile, server: active_live_server };
    // Open (or start) the DM with a user and show the DM screen.
    let open_dm: Rc<dyn Fn(i64)> = {
        let msgs = live_dm_messages.clone();
        Rc::new(move |user_id: i64| {
            crate::live_dm_view::open_dm_with(user_id, live_dms, live_dm_active, msgs.clone(), live_dm_revision);
            live_dm_mode.set(true);
        })
    };
    let user_popup_el = switch(
        move || (user_popup.get(), user_profile.get()),
        move |(popup, profile): &(Option<UserPopup>, Option<UserProfile>)| {
            let Some(popup) = *popup else {
                return ui! { view(style = StyleRules { width: Some(px(0.0)), height: Some(px(0.0)), ..Default::default() }) {} };
            };
            let profile = profile.clone().filter(|p| match popup {
                UserPopup::Card { user_id, .. } | UserPopup::Menu { user_id, .. } => p.user_id == user_id,
            });
            let body = match popup {
                UserPopup::Card { x, y, .. } => profile_card(profile, x, y, ctx, open_dm.clone(), profile_mode),
                UserPopup::Menu { user_id, x, y } => user_menu(
                    user_id, profile, x, y, ctx, open_dm.clone(), composer, live_members,
                    live_kick_confirm, live_ban_confirm, live_timeout_confirm,
                ),
            };
            ui! {
                view(style = StyleRules {
                    position: Some(Position::Absolute),
                    left: Some(px(0.0)),
                    top: Some(px(0.0)),
                    width: Some(pct(100.0)),
                    height: Some(pct(100.0)),
                    ..Default::default()
                }) {
                    { body }
                }
                // Any press outside the popup closes it (the popup itself
                // consumes its own presses).
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Began {
                        user_popup.set(None);
                    }
                    TouchResponse::CONSUMED
                })
            }
        },
    );
    // The channel menu (`action_menu`) floats without a backdrop; while it
    // is open this transparent full-window layer under it closes it on any
    // press elsewhere (a right-click on another channel just closes it).
    let chan_menu_catcher_el = switch(
        move || live_chan_menu.get().is_some(),
        move |open: &bool| {
            if !*open {
                return ui! { view(style = StyleRules { width: Some(px(0.0)), height: Some(px(0.0)), ..Default::default() }) {} };
            }
            ui! {
                view(style = StyleRules {
                    position: Some(Position::Absolute),
                    left: Some(px(0.0)),
                    top: Some(px(0.0)),
                    width: Some(pct(100.0)),
                    height: Some(pct(100.0)),
                    ..Default::default()
                }) {}
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Began {
                        live_chan_menu.set(None);
                    }
                    TouchResponse::CONSUMED
                })
            }
        },
    );
    UserPopupsParts { user_popup_el, chan_menu_catcher_el }
}

const CARD_W: f32 = 300.0;
const CARD_H: f32 = 360.0;
const CARD_BG: &str = "#232428";

fn profile_card(
    profile: Option<UserProfile>,
    x: f32,
    y: f32,
    ctx: UserPopupCtx,
    open_dm: Rc<dyn Fn(i64)>,
    profile_mode: Signal<bool>,
) -> Element {
    let (left, top) = place(x, y, CARD_W, CARD_H);
    let frame = StyleRules {
        position: Some(Position::Absolute),
        left: Some(px(left)),
        top: Some(px(top)),
        width: Some(px(CARD_W)),
        flex_direction: Some(FlexDirection::Column),
        background: Some(col(CARD_BG)),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        overflow: Some(Overflow::Hidden),
        ..Default::default()
    };
    let Some(p) = profile else {
        return ui! {
            view(style = frame) {
                text(style = StyleRules { padding_top: Some(px(24.0)), padding_bottom: Some(px(24.0)), padding_left: Some(px(16.0)), color: Some(col(TEXT_MUTED)), ..Default::default() }) { "Loading…".to_string() }
            }
            .on_touch(|_| TouchResponse::CONSUMED)
        };
    };
    let label = StyleRules {
        font_size: Some(px(12.0)),
        font_weight: Some(FontWeight::Bold),
        color: Some(col(TEXT)),
        text_transform: Some(TextTransform::Uppercase),
        margin_top: Some(px(12.0)),
        margin_bottom: Some(px(4.0)),
        ..Default::default()
    };
    let value = StyleRules { font_size: Some(px(14.0)), color: Some(col(TEXT_CHANNEL)), ..Default::default() };
    let button = |bg: &str| StyleRules {
        flex_grow: Some(grow(1.0)),
        height: Some(px(34.0)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        background: Some(col(bg)),
        border_top_left_radius: Some(px(4.0)),
        border_top_right_radius: Some(px(4.0)),
        border_bottom_left_radius: Some(px(4.0)),
        border_bottom_right_radius: Some(px(4.0)),
        ..Default::default()
    };
    let button_text = StyleRules { font_size: Some(px(14.0)), font_weight: Some(FontWeight::SemiBold), color: Some(col(TEXT)), ..Default::default() };
    let since = match p.joined_server_at {
        Some(j) => format!("{}  ·  joined this server {}", short_date(p.created_at), short_date(j)),
        None => short_date(p.created_at),
    };
    let mutual = if p.mutual_servers.is_empty() {
        None
    } else {
        Some((
            format!("Mutual servers — {}", p.mutual_servers.len()),
            p.mutual_servers.iter().map(|s| s.1.clone()).collect::<Vec<_>>().join(", "),
        ))
    };
    let user_id = p.user_id;
    let name = p.username.clone();
    // The action row: your own card edits your profile; anyone else's
    // offers Message plus the friend action that fits.
    let actions: Element = if p.relationship == "self" {
        ui! {
            view(style = button(BUTTON)) { text(style = button_text.clone()) { "Edit Profile".to_string() } }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    ctx.popup.set(None);
                    profile_mode.set(true);
                }
                TouchResponse::CONSUMED
            })
        }
    } else {
        let friend: Option<(&str, bool)> = match p.relationship.as_str() {
            "none" => Some(("Add Friend", true)),
            "incoming" => Some(("Accept Request", true)),
            "outgoing" => Some(("Request Sent", false)),
            _ => None,
        };
        let relationship = p.relationship.clone();
        let friend_name = name.clone();
        ui! {
            view(style = StyleRules { flex_direction: Some(FlexDirection::Row), gap: Some(px(8.0)), ..Default::default() }) {
                view(style = button(BUTTON)) { text(style = button_text.clone()) { "Message".to_string() } }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        ctx.popup.set(None);
                        (open_dm)(user_id);
                    }
                    TouchResponse::CONSUMED
                })
                if let Some((label_text, enabled)) = friend {
                    view(style = button(if enabled { "#248046" } else { "#4e5058" })) {
                        text(style = button_text.clone()) { label_text.to_string() }
                    }
                    .on_touch({
                        let relationship = relationship.clone();
                        let friend_name = friend_name.clone();
                        move |ev| {
                            if ev.phase == TouchPhase::Ended && enabled {
                                friend_action(&relationship, user_id, friend_name.clone(), ctx);
                            }
                            TouchResponse::CONSUMED
                        }
                    })
                }
            }
        }
    };
    ui! {
        view(style = frame) {
            view(style = StyleRules { height: Some(px(60.0)), background: Some(col(banner_color(&name))), ..Default::default() }) {}
            view(style = StyleRules {
                margin_top: Some(px(-40.0)),
                margin_left: Some(px(16.0)),
                width: Some(px(88.0)),
                height: Some(px(88.0)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                background: Some(col(CARD_BG)),
                border_top_left_radius: Some(px(44.0)),
                border_top_right_radius: Some(px(44.0)),
                border_bottom_left_radius: Some(px(44.0)),
                border_bottom_right_radius: Some(px(44.0)),
                ..Default::default()
            }) {
                { crate::avatar_chip::avatar_chip(name.as_str(), BUTTON, 80.0, 40.0) }
            }
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Column),
                padding_top: Some(px(8.0)),
                padding_bottom: Some(px(16.0)),
                padding_left: Some(px(16.0)),
                padding_right: Some(px(16.0)),
                ..Default::default()
            }) {
                text(style = StyleRules { font_size: Some(px(20.0)), font_weight: Some(FontWeight::Bold), color: Some(col(TEXT)), ..Default::default() }) { name.clone() }
                if let Some(role) = &p.server_role {
                    text(style = StyleRules { font_size: Some(px(12.0)), color: Some(col(TEXT_MUTED)), margin_top: Some(px(2.0)), ..Default::default() }) { role.clone() }
                }
                text(style = label.clone()) { "Member since".to_string() }
                text(style = value.clone()) { since.clone() }
                if let Some((title, names)) = &mutual {
                    text(style = label.clone()) { title.clone() }
                    text(style = value.clone()) { names.clone() }
                }
                view(style = StyleRules { height: Some(px(16.0)), ..Default::default() }) {}
                { actions }
            }
        }
        .on_touch(|_| TouchResponse::CONSUMED)
    }
}

/// Send / accept / cancel / remove, then refresh the profile so the card
/// and menu show the new relationship.
fn friend_action(relationship: &str, user_id: i64, username: String, ctx: UserPopupCtx) {
    let relationship = relationship.to_string();
    runtime_core::driver::spawn_async(async move {
        let _ = match relationship.as_str() {
            "none" => crate::friends::send_friend_request(username).await,
            "incoming" => crate::friends::accept_friend_request(user_id).await,
            "outgoing" => crate::friends::decline_or_cancel(user_id).await,
            _ => crate::friends::remove_friend(user_id).await,
        };
        ctx.refresh(user_id);
    });
}

fn block_action(block: bool, user_id: i64, ctx: UserPopupCtx) {
    runtime_core::driver::spawn_async(async move {
        let input = discord_server::blocks::BlockUserInput { user_id };
        let _ = if block {
            discord_server::blocks::block_user(input).await
        } else {
            discord_server::blocks::unblock_user(input).await
        };
        ctx.refresh(user_id);
    });
}

const MENU_W: f32 = 220.0;

#[allow(clippy::too_many_arguments)]
fn user_menu(
    user_id: i64,
    profile: Option<UserProfile>,
    x: f32,
    y: f32,
    ctx: UserPopupCtx,
    open_dm: Rc<dyn Fn(i64)>,
    composer: Signal<String>,
    members: Signal<Vec<crate::moderation::MemberInfo>>,
    kick: Signal<Option<i64>>,
    ban: Signal<Option<i64>>,
    timeout: Signal<Option<i64>>,
) -> Element {
    // (label, red, action); `None` is a divider.
    let mut items: Vec<Option<(String, bool, Rc<dyn Fn()>)>> = Vec::new();
    let close = move || ctx.popup.set(None);
    items.push(Some(("Profile".to_string(), false, Rc::new(move || {
        ctx.popup.set(Some(UserPopup::Card { user_id, x, y }));
    }))));
    let name = profile.as_ref().map(|p| p.username.clone());
    let me = profile.as_ref().is_some_and(|p| p.relationship == "self");
    if let Some(name) = name.clone() {
        items.push(Some(("Mention".to_string(), false, Rc::new(move || {
            let mut text = composer.get();
            if !text.is_empty() && !text.ends_with(' ') {
                text.push(' ');
            }
            text.push_str(&format!("@{name} "));
            composer.set(text);
            close();
        }))));
    }
    if let Some(p) = profile.as_ref().filter(|_| !me) {
        let dm = open_dm.clone();
        items.push(Some(("Message".to_string(), false, Rc::new(move || {
            close();
            (dm)(user_id);
        }))));
        items.push(None);
        let friend_label = match p.relationship.as_str() {
            "none" => "Add Friend",
            "incoming" => "Accept Friend Request",
            "outgoing" => "Cancel Friend Request",
            _ => "Remove Friend",
        };
        let (rel, uname) = (p.relationship.clone(), p.username.clone());
        items.push(Some((friend_label.to_string(), p.relationship == "friends", Rc::new(move || {
            friend_action(&rel, user_id, uname.clone(), ctx);
            close();
        }))));
        let blocked = p.blocked;
        items.push(Some((if blocked { "Unblock" } else { "Block" }.to_string(), !blocked, Rc::new(move || {
            block_action(!blocked, user_id, ctx);
            close();
        }))));
        // Moderation, for a moderator or above acting on someone ranked lower
        // (the backend enforces the real rules; this only picks what to show).
        let roster = members.get();
        let self_id = crate::auth::CURRENT_USER_ID.load(std::sync::atomic::Ordering::SeqCst);
        let mine = role_rank(acting_role(&roster, self_id));
        let theirs = roster.iter().find(|m| m.user_id == user_id).map(|m| role_rank(m.role.clone()));
        if let Some(theirs) = theirs.filter(|t| mine >= 1 && mine > *t) {
            items.push(None);
            items.push(Some((format!("Kick {}", p.username), true, Rc::new(move || { kick.set(Some(user_id)); close(); }))));
            if mine >= 2 && theirs < 2 {
                items.push(Some((format!("Timeout {}", p.username), false, Rc::new(move || { timeout.set(Some(user_id)); close(); }))));
                items.push(Some((format!("Ban {}", p.username), true, Rc::new(move || { ban.set(Some(user_id)); close(); }))));
            }
        }
    }
    items.push(None);
    items.push(Some(("Copy User ID".to_string(), false, Rc::new(move || {
        runtime_core::driver::spawn_async(async move {
            let _ = clipboard::set_text(&user_id.to_string()).await;
        });
        close();
    }))));

    let est_h = items.iter().map(|i| if i.is_some() { 34.0 } else { 9.0 }).sum::<f32>() + 12.0;
    let (left, top) = place(x, y, MENU_W, est_h);
    let rows: Vec<Element> = items.into_iter().map(|item| match item {
        None => ui! {
            view(style = StyleRules { height: Some(px(1.0)), margin_top: Some(px(4.0)), margin_bottom: Some(px(4.0)), margin_left: Some(px(8.0)), margin_right: Some(px(8.0)), background: Some(col(BORDER)), ..Default::default() }) {}
        },
        Some((label, red, action)) => {
            let hovered = signal(false);
            ui! {
                view(style = move || StyleRules {
                    height: Some(px(32.0)),
                    padding_left: Some(px(8.0)),
                    padding_right: Some(px(8.0)),
                    margin_left: Some(px(6.0)),
                    margin_right: Some(px(6.0)),
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    background: Some(col(if hovered.get() { if red { "#da373c" } else { "#4752c4" } } else { "transparent" })),
                    border_top_left_radius: Some(px(4.0)),
                    border_top_right_radius: Some(px(4.0)),
                    border_bottom_left_radius: Some(px(4.0)),
                    border_bottom_right_radius: Some(px(4.0)),
                    ..Default::default()
                }) {
                    text(style = move || StyleRules {
                        font_size: Some(px(14.0)),
                        font_weight: Some(FontWeight::SemiBold),
                        color: Some(col(if red && !hovered.get() { "#f23f43" } else { TEXT })),
                        ..Default::default()
                    }) { label.clone() }
                }
                .on_hover(move |on| hovered.set(on))
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        (action)();
                    }
                    TouchResponse::CONSUMED
                })
            }
        }
    }).collect();
    ui! {
        view(style = StyleRules {
            position: Some(Position::Absolute),
            left: Some(px(left)),
            top: Some(px(top)),
            width: Some(px(MENU_W)),
            flex_direction: Some(FlexDirection::Column),
            padding_top: Some(px(6.0)),
            padding_bottom: Some(px(6.0)),
            background: Some(col("#111214")),
            border_top_left_radius: Some(px(6.0)),
            border_top_right_radius: Some(px(6.0)),
            border_bottom_left_radius: Some(px(6.0)),
            border_bottom_right_radius: Some(px(6.0)),
            ..Default::default()
        }) { rows }
        .on_touch(|_| TouchResponse::CONSUMED)
    }
}

#[cfg(test)]
mod user_popup_tests {
    use super::banner_color;

    #[test]
    fn a_name_always_gets_the_same_banner() {
        assert_eq!(banner_color("blake"), banner_color("blake"));
        // Different names spread over the palette rather than all sharing one.
        let tones: std::collections::HashSet<_> =
            ["a", "b", "c", "d", "e", "f"].iter().map(|n| banner_color(n)).collect();
        assert!(tones.len() > 1);
    }
}
