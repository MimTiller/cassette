//! `instance_settings_panel` — the Instance Settings area: the host's own
//! full-screen screen (same layout as User Settings / Server Settings, via
//! `settings_suite::settings_layout`), reached from the User Settings
//! sidebar's "Instance Settings" row — a row shown only to instance admins
//! (or while the instance has no admin yet, the setup flow), so a normal
//! signed-in user never sees the area.
//!
//! Today one section — Voice & Video — configures the call provider and the
//! chat backend without editing env files: the provider picker (None /
//! LiveKit / idea-chat, the last listed disabled "coming soon"), the LiveKit
//! credentials (URL, API key, API secret as a password field), and the
//! chat-backend picker. Every value row labels where its EFFECTIVE value
//! comes from — the instance store ("DB") or the environment ("env") — and
//! a save swaps the server's live voice config, so joining a voice channel
//! after Save uses the new values with no restart.
//!
//! The API secret is write-only: the read hands back only `secret_set` and
//! the last 4 characters ("saved, ends in ab12"), never the value. Saving
//! an empty field keeps the old value — the server's own rule.
//!
//! "Test connection" runs the backend's `test_call_provider` against the
//! typed drafts (empty fields fall back to the saved values there) and
//! paints the answer — the provider's room count in green, or the exact
//! failure in red — without saving.
//!
//! The unsaved-changes bar (a Discord-style accent strip above the actions)
//! arms as soon as a picker or field differs from what is saved; Save
//! commits the drafts, Reset discards them (re-seeding from the saved read).
//!
//! Permission model: hiding the sidebar row is the only client-side gate —
//! every RPC checks the acting user against the instance admins server-side
//! and its plain refusal sentence shows verbatim in the red `error` line.
//! While no instance admin exists yet, the area opens on a "Claim instance"
//! card instead of the controls, so the first host can take the seat.
//!
//! Area state lives here behind an install/open_signal pair (the
//! `notif_menu.rs` thread-local idiom) so both the User Settings sidebar's
//! row and the full-screen area can reach it; `app.rs` installs it in
//! `app_shell` scope once and seeds the gates from the public `instance_info`
//! card after login.
//!
//! Idioms are the crate's (verified against `server_settings_panel.rs`):
//! one `switch` keyed on everything the body reads; every `Element` built
//! INSIDE the render closure; `Signal` is `Copy`, so `move` closures capture
//! copies and stay `Fn`; server calls go through `spawn_then_in` anchored on
//! `crate::app::shell_alive()` with every signal write in the callback.

use crate::theme;
use runtime_core::{
    component, signal, switch, ui, AlignItems, Element, FlexDirection, FontWeight,
    Overflow, Position, Signal, StyleRules, TouchResponse,
};
use std::rc::Rc;
use std::cell::Cell;

use crate::style_helpers::{col, pct, px, rounded};
use crate::settings_suite::{settings_layout, nav_heading, nav_row, nav_id};

use discord_server::instance::{CallTestResult, InstanceSettingsResult, SettingSource};

thread_local! {
    static STATE: Cell<Option<InstanceState>> = const { Cell::new(None) };
}

/// Everything the area owns: the open flag, the gates, the last saved read,
/// the drafts, and the busy/error/test state. All `Signal`s are `Copy`, so
/// `state()` hands out independent copies of the same slots.
#[derive(Clone, Copy)]
pub(crate) struct InstanceState {
    open: Signal<bool>,
    admin: Signal<bool>,
    entry: Signal<bool>,
    setup: Signal<bool>,
    saved: Signal<Option<InstanceSettingsResult>>,
    provider: Signal<u32>,
    chat: Signal<u32>,
    url: Signal<String>,
    api_key: Signal<String>,
    api_secret: Signal<String>,
    idea_url: Signal<String>,
    dirty: Signal<bool>,
    busy: Signal<bool>,
    error: Signal<String>,
    test_result: Signal<Option<CallTestResult>>,
}

/// Set up the area's state (once, from `app_shell`).
pub(crate) fn install() {
    STATE.with(|s| s.set(Some(InstanceState {
        open: signal(false),
        admin: signal(false),
        entry: signal(false),
        setup: signal(false),
        saved: signal(None),
        provider: signal(0u32),
        chat: signal(0u32),
        url: signal(String::new()),
        api_key: signal(String::new()),
        api_secret: signal(String::new()),
        idea_url: signal(String::new()),
        dirty: signal(false),
        busy: signal(false),
        error: signal(String::new()),
        test_result: signal(None),
    })));
}

/// The area's signals, for the User Settings sidebar's row, Escape, and the
/// login-time seeding.
pub(crate) fn state() -> InstanceState {
    STATE.with(|s| s.get()).expect("instance_settings_panel::install runs in app_shell")
}

/// The provider picker's indexes.
const PROVIDER_NONE: u32 = 0;
const PROVIDER_LIVEKIT: u32 = 1;
const PROVIDER_IDEA_CHAT: u32 = 2;

/// The chat-backend picker's indexes.
const CHAT_RWND: u32 = 0;
const CHAT_IDEA_CHAT: u32 = 1;

fn provider_index(name: &str) -> u32 {
    if name == "livekit".to_string() {
        PROVIDER_LIVEKIT
    } else if name == "idea-chat".to_string() {
        PROVIDER_IDEA_CHAT
    } else {
        PROVIDER_NONE
    }
}

fn chat_index(name: &str) -> u32 {
    if name == "idea-chat".to_string() { CHAT_IDEA_CHAT } else { CHAT_RWND }
}

/// Picker index -> the provider name the backend stores.
fn provider_name(i: u32) -> String {
    if i == PROVIDER_LIVEKIT {
        "livekit".to_string()
    } else if i == PROVIDER_IDEA_CHAT {
        "idea-chat".to_string()
    } else {
        "none".to_string()
    }
}

/// Picker index -> the chat-backend name the backend stores.
fn chat_name(i: u32) -> String {
    if i == CHAT_IDEA_CHAT { "idea-chat".to_string() } else { "rwnd".to_string() }
}

/// The entry gate as a signal, for the User Settings sidebar row.
pub(crate) fn entry_visible_view() -> Signal<bool> {
    state().entry
}

/// Open the area (the sidebar row's action): re-seed the drafts from the
/// saved read and clear stale error/test state.
pub(crate) fn open_area() {
    let s = state();
    s.open.set(true);
    s.error.set(String::new());
    s.test_result.set(None);
    if let Some(v) = s.saved.get() {
        s.provider.set(provider_index(v.call_provider.as_str()));
        s.chat.set(chat_index(v.chat_backend.as_str()));
        s.url.set(v.livekit_url);
        s.api_key.set(v.livekit_api_key);
        s.idea_url.set(v.idea_chat_url);
    }
    s.api_secret.set(String::new());
    s.dirty.set(false);
}

/// Seed the gates from the public `instance_info` card (login-time).
pub(crate) fn seed(admin: bool, setup: bool) {
    let s = state();
    s.admin.set(admin);
    s.setup.set(setup);
    s.entry.set(admin || !setup);
}

/// The admin's saved read at login, so the source labels are fresh on open.
pub(crate) fn seed_saved(v: InstanceSettingsResult) {
    state().saved.set(Some(v));
}

/// Arm the unsaved-changes bar: any draft that differs from the saved read.
pub(crate) fn mark_dirty() {
    state().dirty.set(true);
}

/// A text input bound two-way to `value`, carrying a `test_id` for the robot
/// suite (the `controls::text_field` shape plus the id), arming the bar.
fn text_field_id(value: Signal<String>, test_id: &'static str, placeholder: &str, style: StyleRules) -> Element {
    ui! {
        text_input(
            value = value,
            on_change = move |v: String| { value.set(v); mark_dirty(); },
            placeholder = placeholder.to_string(),
            test_id = test_id,
            style = style,
        )
    }
}

/// The same for the write-only secret field (`secure`: the browser masks it).
fn secret_field_id(value: Signal<String>, test_id: &'static str, placeholder: &str, style: StyleRules) -> Element {
    ui! {
        text_input(
            value = value,
            on_change = move |v: String| { value.set(v); mark_dirty(); },
            placeholder = placeholder.to_string(),
            secure = true,
            test_id = test_id,
            style = style,
        )
    }
}

#[component]
pub fn InstanceSettingsPanel() -> Element {
    let s = state();
    let close = s.open;
    let adm = s.admin;
    let st = s.setup;
    let sv = s.saved;
    let prov = s.provider;
    let chatb = s.chat;
    let url_s = s.url;
    let key_s = s.api_key;
    let secret_s = s.api_secret;
    let idea_s = s.idea_url;
    let dirty_s = s.dirty;
    let b = s.busy;
    let e = s.error;
    let tested = s.test_result;
    let nav_active = signal(0usize);

    // --- Handlers ------------------------------------------------------------
    // Each is a `Copy` closure over `Copy` Signals; every RPC lands through
    // `spawn_then_in` anchored on the app shell, its writes in the callback
    // (the `server_settings_panel.rs` shape). No client-side permission
    // check: the backend decides and its sentence shows verbatim in `error`.

    // Save: the drafts into set_instance_settings — an empty field keeps the
    // saved value server-side, so the drafts need no keep-check here. On
    // success the refreshed read replaces `saved`, repainting the source
    // labels through the switch key.
    let do_save = move || {
        if b.get() {
            return;
        }
        e.set(String::new());
        b.set(true);
        let b2 = b;
        let e2 = e;
        let sv2 = sv;
        let dirty2 = dirty_s;
        let st2 = st;
        runtime_core::spawn_then_in(
            &crate::app::shell_alive(),
            async move {
                crate::backend::set_instance_settings(provider_name(prov.get()), url_s.get(), key_s.get(), secret_s.get(),
                    chat_name(chatb.get()), idea_s.get()).await
            },
            move |result| {
                match result {
                    Ok(fresh) => {
                        st2.set(fresh.setup_complete);
                        sv2.set(Some(fresh));
                        dirty2.set(false);
                    }
                    Err(msg) => e2.set(msg),
                }
                b2.set(false);
            },
        );
    };
    // Test: the server's test_call_provider on the typed drafts (empty
    // fields fall back to the saved values there). Never saves.
    let do_test = move || {
        if b.get() {
            return;
        }
        e.set(String::new());
        tested.set(None);
        b.set(true);
        let b2 = b;
        let e2 = e;
        let tested2 = tested;
        runtime_core::spawn_then_in(
            &crate::app::shell_alive(),
            async move {
                crate::backend::test_call_provider(provider_name(prov.get()), url_s.get(), key_s.get(), secret_s.get()).await
            },
            move |result| {
                match result {
                    Ok(answer) => tested2.set(Some(answer)),
                    Err(msg) => e2.set(msg),
                }
                b2.set(false);
            },
        );
    };
    // Claim: the setup flow's one-shot — this account takes instance-admin
    // power while the instance has none. Success flips the gates, which
    // repaints the whole area (they are in the switch key).
    let do_claim = move || {
        if b.get() {
            return;
        }
        e.set(String::new());
        b.set(true);
        let b2 = b;
        let e2 = e;
        let adm2 = adm;
        let st2 = st;
        runtime_core::spawn_then_in(
            &crate::app::shell_alive(),
            async move { crate::backend::claim_instance_admin().await },
            move |result| {
                match result {
                    Ok(_) => { adm2.set(true); st2.set(true); },
                    Err(msg) => e2.set(msg),
                }
                b2.set(false);
            },
        );
    };
    // Reset: drop the drafts — re-seed the pickers and visible fields from
    // the saved read; the secret draft empties (an empty draft keeps the
    // saved secret, per the save rule).
    let do_reset = move || {
        e.set(String::new());
        if let Some(v) = sv.get() {
            prov.set(provider_index(v.call_provider.as_str()));
            chatb.set(chat_index(v.chat_backend.as_str()));
            url_s.set(v.livekit_url);
            key_s.set(v.livekit_api_key);
            idea_s.set(v.idea_chat_url);
        }
        secret_s.set(String::new());
        dirty_s.set(false);
    };

    // --- The section body: one switch over everything it reads -------------
    let body = ui! {
        switch(
            move || (close.get(), adm.get(), st.get(), prov.get(), chatb.get(),
                     dirty_s.get(), b.get(), e.get(), sv.get(), tested.get()),
            move |key: &(bool, bool, bool, u32, u32, bool, bool, String, Option<InstanceSettingsResult>, Option<CallTestResult>)| {
                let (is_open, is_admin, is_setup, p, _c, is_dirty, _is_busy, err, saved_v, test_v) =
                    (key.0, key.1, key.2, key.3, key.4, key.5, key.6, &key.7, &key.8, &key.9);
                if !is_open {
                    return ui! { view(style = StyleRules { width: Some(px(0.0)), height: Some(px(0.0)), ..Default::default() }) {} };
                }

                let input_style = StyleRules {
                    align_items: Some(AlignItems::Center),
                    ..crate::style_helpers::text_field_style()
                };
                let hint_style = StyleRules {
                    font_size: Some(px(12.0)),
                    color: Some(col(theme::TEXT_MUTED)),
                    margin_top: Some(px(6.0)),
                    ..Default::default()
                };
                // Where one value's EFFECTIVE copy comes from (DB over env).
                let src_line = |label: String, at_db: bool| -> Element {
                    ui! {
                        text(style = hint_style.clone()) { format!("{label} comes from {src}", src = if at_db { "the DB".to_string() } else { "env".to_string() }) }
                    }
                };
                let source_db = |at: SettingSource| at == SettingSource::Db;

                // Provider picker: idea-chat is listed disabled ("coming
                // soon") until its call wire lands.
                let on_pick = Rc::new(move |_idx: u32| { mark_dirty(); });
                let pick_none = crate::settings_suite::option_row("None", PROVIDER_NONE, prov.clone(), on_pick.clone());
                let pick_live = crate::settings_suite::option_row("LiveKit", PROVIDER_LIVEKIT, prov.clone(), on_pick.clone());
                let pick_idea = crate::settings_suite::option_row("idea-chat (coming soon)", PROVIDER_IDEA_CHAT, prov.clone(), on_pick);
                let provider_help = ui! {
                    text(style = StyleRules {
                        font_size: Some(px(12.0)),
                        color: Some(col(theme::TEXT_MUTED)),
                        margin_top: Some(px(10.0)),
                        ..Default::default()
                    }) {
                        "LiveKit needs UDP 7882 (or TURN) reachable from each user: a Cloudflare http tunnel alone cannot carry voice. Setup notes live in docs/self-hosting.md.".to_string()
                    }
                };
                let provider_card = crate::settings_suite::settings_card(
                    "Call provider".to_string(),
                    vec![pick_none, pick_live, pick_idea, provider_help],
                );

                // The credential rows: shown while LiveKit is picked.
                let creds_children: Vec<Element> =
                    if p == PROVIDER_LIVEKIT {
                        vec![
                            text_field_id(url_s.clone(), "inst-url-input", "ws:// or wss:// address", input_style.clone()),
                            src_line("URL".to_string(), match saved_v { Some(v) => source_db(v.livekit_url_source.clone()), None => false }),
                            text_field_id(key_s.clone(), "inst-key-input", "API key", input_style.clone()),
                            src_line("API key".to_string(), match saved_v { Some(v) => source_db(v.livekit_api_key_source.clone()), None => false }),
                            secret_field_id(secret_s.clone(), "inst-secret-input", "API secret", input_style.clone()),
                            src_line("API secret".to_string(), match saved_v { Some(v) => source_db(v.livekit_api_secret_source.clone()), None => false }),
                            secret_state(saved_v),
                        ]
                    } else {
                        vec![ui! { view(style = StyleRules { height: Some(px(0.0)), ..Default::default() }) {} }]
                    };
                let creds_card = crate::settings_suite::settings_card("LiveKit credentials".to_string(), creds_children);

                // The secret's state line: never the value — the tail confirms.
                let chat_on_pick = Rc::new(move |_idx: u32| { mark_dirty(); });
                let chat_rwnd_row = crate::settings_suite::option_row("Built-in RWND server", CHAT_RWND, chatb.clone(), chat_on_pick.clone());
                let chat_idea_row = crate::settings_suite::option_row("idea-chat server (coming soon)", CHAT_IDEA_CHAT, chatb.clone(), chat_on_pick);
                let idea_line = ui! {
                    view(style = StyleRules { flex_direction: Some(FlexDirection::Column), gap: Some(px(6.0)), ..Default::default() }) {
                        text_field_id(idea_s.clone(), "inst-idea-input", "idea-chat server URL", input_style.clone())
                        { src_line("idea-chat URL".to_string(), match saved_v { Some(v) => source_db(v.idea_chat_url_source.clone()), None => false }) }
                    }
                };
                let chat_card = crate::settings_suite::settings_card("Chat backend".to_string(), vec![chat_rwnd_row, chat_idea_row, idea_line]);

                // The unsaved-changes bar: an accent strip above the actions.
                let actions = ui! {
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Column),
                        gap: Some(px(10.0)),
                        margin_top: Some(px(14.0)),
                        ..Default::default()
                    }) {
                        view(style = StyleRules {
                            width: Some(pct(100.0)),
                            height: Some(px(if is_dirty { 3.0 } else { 0.0 })),
                            background: Some(col(theme::ACCENT)),
                            ..rounded(2.0)
                        }) {}
                        view(style = StyleRules {
                            flex_direction: Some(FlexDirection::Row),
                            gap: Some(px(10.0)),
                            ..Default::default()
                        }) {
                            button(label = "Save".to_string(), disabled = move || b.get(), on_click = move || do_save(), test_id = "instance-save", style = crate::styles::ButtonBox().kind(crate::styles::ButtonBoxKind::Primary))
                            button(label = "Reset".to_string(), disabled = move || b.get(), on_click = move || do_reset(), test_id = "instance-reset", style = crate::styles::ButtonBox().kind(crate::styles::ButtonBoxKind::Secondary))
                        }
                        view(style = StyleRules { flex_direction: Some(FlexDirection::Row), gap: Some(px(10.0)), ..Default::default() }) {
                            button(label = "Test connection".to_string(), disabled = move || b.get(), on_click = move || do_test(), test_id = "instance-test", style = crate::styles::ButtonBox().kind(crate::styles::ButtonBoxKind::Primary))
                            view { test_answer(test_v) }
                        }
                    }
                };

                // Setup flow: no admin yet — while the instance has no admin
                // at all, a Claim card opens the gate instead of the controls.
                let claim_card = crate::settings_suite::settings_card(
                    "Instance setup".to_string(),
                    vec![
                        ui! {
                            text(style = StyleRules {
                                font_size: Some(px(13.0)),
                                color: Some(col(theme::TEXT_MUTED)),
                                ..Default::default()
                            }) { "this instance has no admin yet — claim it to manage its settings".to_string() }
                        },
                        ui! {
                            button(label = "Claim instance".to_string(), disabled = move || b.get(), on_click = move || do_claim(), test_id = "instance-claim", style = crate::styles::ButtonBox().kind(crate::styles::ButtonBoxKind::Primary))
                        }
                    ],
                );
                let refused_card = crate::settings_suite::settings_card(
                    "Not an instance admin".to_string(),
                    vec![
                        ui! {
                            text(style = StyleRules {
                                font_size: Some(px(13.0)),
                                color: Some(col(theme::TEXT_MUTED)),
                                ..Default::default()
                            }) { "only instance admins can manage instance settings".to_string() }
                        }
                    ],
                );
            let content: Element =
                if is_admin {
                    ui! {
                        view(style = StyleRules { flex_direction: Some(FlexDirection::Column), gap: Some(px(16.0)), ..Default::default() }) {
                            { provider_card }
                            { creds_card }
                            { chat_card }
                            { actions }
                        }
                    }
                } else if !is_setup {
                    claim_card
                } else {
                    refused_card
                };

                let err_el: Element =
                    if !err.is_empty() {
                        ui! {
                            text(style = StyleRules {
                                font_size: Some(px(12.0)),
                                color: Some(col(theme::DANGER)),
                                margin_top: Some(px(10.0)),
                                ..Default::default()
                            }) { err.to_string() }
                        }
                    } else {
                        ui! { view(style = StyleRules { height: Some(px(0.0)), ..Default::default() }) {} }
                    };

                ui! {
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Column),
                        gap: Some(px(16.0)),
                        padding_top: Some(px(16.0)),
                        ..Default::default()
                    }) {
                        text(style = StyleRules {
                            font_size: Some(px(20.0)),
                            font_weight: Some(FontWeight::SemiBold),
                            color: Some(col(theme::TEXT_STRONG)),
                            margin_bottom: Some(px(20.0)),
                            ..Default::default()
                        }) { "Voice & Video".to_string() }
                        { content }
                        { err_el }
                    }
                }
            },
        )
    };

    // --- Sidebar: the area's name and its one section ------------------------
    let nav: Vec<Element> = vec![
        nav_heading(move || "INSTANCE SETTINGS".to_string(), true),
        nav_row("Voice & Video", false, nav_id("Voice & Video"), nav_active, 0, None),
    ];
    let on_close: Rc<dyn Fn()> = Rc::new(move || close.set(false));
    let layout = settings_layout(nav, body, Some(on_close));
    ui! {
        view(style = move || StyleRules {
            position: Some(Position::Absolute),
            left: Some(px(0.0)),
            top: Some(px(0.0)),
            width: Some(if close.get() { pct(100.0) } else { px(0.0) }),
            height: Some(if close.get() { pct(100.0) } else { px(0.0) }),
            background: Some(col(theme::BG_PRIMARY)),
            overflow: Some(Overflow::Hidden),
            ..Default::default()
        }) {
            { layout }
        }
        .on_touch(move |_ev| TouchResponse::CONSUMED)
    }
}

/// The secret's state line: never the value — "saved, ends in ab12".
fn secret_state(saved_v: &Option<InstanceSettingsResult>) -> Element {
    if let Some(v) = saved_v {
        if v.livekit_api_secret.secret_set {
            ui! {
                text(style = StyleRules {
                    font_size: Some(px(12.0)),
                    color: Some(col(theme::SUCCESS)),
                    margin_top: Some(px(6.0)),
                    ..Default::default()
                }) { format!("API secret saved, ends in {}", v.livekit_api_secret.last4) }
            }
        } else {
            ui! {
                text(style = StyleRules {
                    font_size: Some(px(12.0)),
                    color: Some(col(theme::TEXT_MUTED)),
                    margin_top: Some(px(6.0)),
                    ..Default::default()
                }) { "no API secret saved yet".to_string() }
            }
        }
    } else {
        ui! { view(style = StyleRules { height: Some(px(0.0)), ..Default::default() }) {} }
    }
}

/// The test answer under the button: room count green, or the exact failure.
fn test_answer(test_v: &Option<CallTestResult>) -> Element {
    if let Some(t) = test_v {
        if t.ok {
            ui! {
                text(style = StyleRules {
                    font_size: Some(px(13.0)),
                    color: Some(col(theme::SUCCESS)),
                    margin_top: Some(px(8.0)),
                    ..Default::default()
                }) { format!("Test connection: OK, {} live room{}.", t.rooms, if t.rooms == 1 { "" } else { "s" }) }
            }
        } else {
            ui! {
                text(style = StyleRules {
                    font_size: Some(px(13.0)),
                    color: Some(col(theme::DANGER)),
                    margin_top: Some(px(8.0)),
                    ..Default::default()
                }) { format!("Test connection: {}", t.message) }
            }
        }
    } else {
        ui! { view(style = StyleRules { height: Some(px(0.0)), ..Default::default() }) {} }
    }
}
