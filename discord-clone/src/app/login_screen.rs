//! The sign-in / sign-up screen.
//! `use super::*` keeps app.rs's imports and shared colors/helpers in scope.

use crate::theme;
use super::*;

/// The login screen, shown while `authed` is false. One card with two tabs,
/// "Log In" and "Sign Up", switched by `mode`. Sign-up has its own heading,
/// a confirm-password field, a username hint and a green submit button, so
/// the two forms don't look alike. On success `authed` flips and this screen
/// unmounts; its state is local and dies with it.
pub(super) fn login_screen(authed: Signal<bool>, busy: Signal<bool>) -> Element {
    let username: Signal<String> = signal(String::new());
    let password: Signal<String> = signal(String::new());
    let confirm: Signal<String> = signal(String::new());
    let error: Signal<String> = signal(String::new());
    // false = log in, true = sign up.
    let mode: Signal<bool> = signal(false);
    // A persisted session that was just dropped as stale leaves a one-shot
    // note; show it instead of a blank form.
    if let Some(note) = SESSION_NOTE
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .take()
    {
        error.set(note);
    }

    let card_style = StyleRules {
        width: Some(px(400.0)),
        padding_top: Some(px(32.0)),
        padding_bottom: Some(px(32.0)),
        padding_left: Some(px(32.0)),
        padding_right: Some(px(32.0)),
        background: Some(col(BG_SIDEBAR)),
        border_top_width: Some(border_width(1.0)),
        border_right_width: Some(border_width(1.0)),
        border_bottom_width: Some(border_width(1.0)),
        border_left_width: Some(border_width(1.0)),
        border_top_color: Some(col(BORDER)),
        border_right_color: Some(col(BORDER)),
        border_bottom_color: Some(col(BORDER)),
        border_left_color: Some(col(BORDER)),
        border_top_left_radius: Some(px(12.0)),
        border_top_right_radius: Some(px(12.0)),
        border_bottom_left_radius: Some(px(12.0)),
        border_bottom_right_radius: Some(px(12.0)),
        flex_direction: Some(FlexDirection::Column),
        align_items: Some(AlignItems::Center),
        ..Default::default()
    };
    let logo_style = StyleRules {
        width: Some(px(56.0)),
        height: Some(px(56.0)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        background: Some(col(BUTTON)),
        border_top_left_radius: Some(px(18.0)),
        border_top_right_radius: Some(px(18.0)),
        border_bottom_left_radius: Some(px(18.0)),
        border_bottom_right_radius: Some(px(18.0)),
        ..Default::default()
    };
    let logo_text_style = StyleRules {
        font_size: Some(px(26.0)),
        font_weight: Some(FontWeight::Bold),
        color: Some(col(TEXT)),
        ..Default::default()
    };
    let heading_style = StyleRules {
        margin_top: Some(px(24.0)),
        font_size: Some(px(22.0)),
        font_weight: Some(FontWeight::Bold),
        color: Some(col(TEXT)),
        text_align: Some(TextAlign::Center),
        ..Default::default()
    };
    let sub_style = StyleRules {
        margin_top: Some(px(4.0)),
        margin_bottom: Some(px(12.0)),
        font_size: Some(px(14.0)),
        color: Some(col(TEXT_MUTED)),
        text_align: Some(TextAlign::Center),
        ..Default::default()
    };
    let label_style = StyleRules {
        align_self: Some(AlignSelf::Stretch),
        margin_top: Some(px(12.0)),
        margin_bottom: Some(px(6.0)),
        font_size: Some(px(11.0)),
        font_weight: Some(FontWeight::SemiBold),
        color: Some(col(TEXT_MUTED)),
        text_transform: Some(TextTransform::Uppercase),
        ..Default::default()
    };
    let hint_style = StyleRules {
        margin_top: Some(px(6.0)),
        font_size: Some(px(12.0)),
        color: Some(col(theme::TEXT_FAINT)),
        ..Default::default()
    };
    let field_style = StyleRules {
        align_self: Some(AlignSelf::Stretch),
        height: Some(px(40.0)),
        padding_left: Some(px(12.0)),
        padding_right: Some(px(12.0)),
        background: Some(col(COMPOSER)),
        color: Some(col(TEXT)),
        font_size: Some(px(15.0)),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        ..borderless_input()
    };
    let submit_style = StyleRules {
        align_self: Some(AlignSelf::Stretch),
        margin_top: Some(px(20.0)),
        height: Some(px(40.0)),
        background: Some(col(BUTTON)),
        color: Some(col(TEXT)),
        font_size: Some(px(15.0)),
        font_weight: Some(FontWeight::SemiBold),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        ..Default::default()
    };

    // Log in: validate, call the server, and flip `authed` on success. The
    // session id and token are recorded by `crate::auth`.
    let do_login = {
        let authed = authed.clone();
        let busy = busy.clone();
        move || {
            let name = username.get();
            let pw = password.get();
            if name.trim().is_empty() || pw.trim().is_empty() {
                error.set("Username and password are required.".to_string());
                return;
            }
            // Enter can submit while a request is in flight; the button can't.
            if busy.get() {
                return;
            }
            error.set(String::new());
            busy.set(true);
            runtime_core::driver::spawn_async(async move {
                match crate::auth::login(&name, &pw).await {
                    Ok(_) => authed.set(true),
                    Err(message) => error.set(message),
                }
                busy.set(false);
            });
        }
    };
    // Sign up: the same checks plus a matching confirmation, then
    // `auth::signup`, which creates the account and logs it in.
    let do_signup = {
        let authed = authed.clone();
        let busy = busy.clone();
        move || {
            let name = username.get();
            let pw = password.get();
            if name.trim().is_empty() || pw.trim().is_empty() {
                error.set("Pick a username and a password.".to_string());
                return;
            }
            if pw != confirm.get() {
                error.set("The passwords don't match.".to_string());
                return;
            }
            if busy.get() {
                return;
            }
            error.set(String::new());
            busy.set(true);
            runtime_core::driver::spawn_async(async move {
                match crate::auth::signup(&name, &pw).await {
                    Ok(_) => authed.set(true),
                    Err(message) => error.set(message),
                }
                busy.set(false);
            });
        }
    };
    let on_submit = move || {
        if mode.get() {
            do_signup();
        } else {
            do_login();
        }
    };
    let on_enter = move |ev: &KeyEvent| {
        if ev.key == "Enter" && !ev.shift && !ev.ctrl && !ev.alt && !ev.meta {
            on_submit();
            KeyOutcome::PreventDefault
        } else {
            KeyOutcome::Default
        }
    };
    let on_username_change = move |s: String| username.set(s);
    let on_pw_change = move |s: String| password.set(s);
    let on_confirm_change = move |s: String| confirm.set(s);
    // Switching tabs clears the passwords and any error, so nothing typed or
    // reported in one form carries into the other.
    let set_mode = move |signup: bool| {
        if mode.get() != signup {
            mode.set(signup);
            password.set(String::new());
            confirm.set(String::new());
            error.set(String::new());
        }
    };

    // Rebuilt when the mode, the error or the in-flight flag changes; the
    // inputs refill from their signals.
    let form = switch(
        move || (error.get(), mode.get(), busy.get()),
        move |state: &(String, bool, bool)| {
            let (err, is_signup, in_flight) = state.clone();

            let tab = |label: &str, signup: bool, test_id: &'static str| {
                let active = signup == is_signup;
                let tab_style = StyleRules {
                    flex_grow: Some(grow(1.0)),
                    padding_top: Some(px(10.0)),
                    padding_bottom: Some(px(10.0)),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    border_bottom_width: Some(border_width(2.0)),
                    border_bottom_color: Some(if active {
                        col(if signup { theme::SUCCESS } else { BUTTON })
                    } else {
                        col(theme::DIVIDER)
                    }),
                    ..Default::default()
                };
                let text_style = StyleRules {
                    font_size: Some(px(15.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    color: Some(col(if active { TEXT } else { TEXT_MUTED })),
                    ..Default::default()
                };
                let label = label.to_string();
                ui! {
                    view(style = tab_style, test_id = test_id) {
                        text(style = text_style) { label }
                    }
                    .on_touch(move |ev| {
                        if ev.phase == TouchPhase::Ended {
                            set_mode(signup);
                        }
                        TouchResponse::CONSUMED
                    })
                }
            };
            let tabs = ui! {
                view(style = StyleRules {
                    align_self: Some(AlignSelf::Stretch),
                    margin_top: Some(px(20.0)),
                    flex_direction: Some(FlexDirection::Row),
                    ..Default::default()
                }) {
                    tab("Log In", false, "login-tab-signin")
                    tab("Sign Up", true, "login-tab-signup")
                }
            };

            let (heading, sub) = if is_signup {
                ("Create an account", "Pick a username and password to join.")
            } else {
                ("Welcome back!", "We're so excited to see you again!")
            };
            let err_el = (!err.is_empty()).then(|| {
                ui! {
                    text(style = StyleRules {
                        margin_top: Some(px(12.0)),
                        font_size: Some(px(13.0)),
                        color: Some(col(theme::DANGER)),
                        text_align: Some(TextAlign::Center),
                        ..Default::default()
                    }) { err }
                }
            });
            let username_hint = is_signup.then(|| {
                ui! {
                    text(style = hint_style.clone()) {
                        "This is how others see you. You can change it later.".to_string()
                    }
                }
            });
            let confirm_el = is_signup.then(|| {
                ui! {
                    view(style = StyleRules {
                        align_self: Some(AlignSelf::Stretch),
                        flex_direction: Some(FlexDirection::Column),
                        ..Default::default()
                    }) {
                        text(style = label_style.clone()) { "Confirm password".to_string() }
                        text_input(
                            value = confirm,
                            test_id = "login-confirm-password",
                            on_change = on_confirm_change,
                            placeholder = "type it again",
                            secure = true,
                            style = field_style.clone(),
                        )
                        .on_key_down(on_enter)
                    }
                }
            });

            let submit_label = match (in_flight, is_signup) {
                (true, true) => "Creating account…",
                (true, false) => "Logging in…",
                (false, true) => "Create Account",
                (false, false) => "Log In",
            }
            .to_string();
            let submit_bg = if is_signup { col(theme::SUCCESS) } else { col(BUTTON) };

            let (switch_prompt, switch_action, switch_test_id) = if is_signup {
                ("Already have an account?", "Log in", "login-signin-toggle")
            } else {
                ("Need an account?", "Sign up", "login-signup-toggle")
            };
            let switch_link = ui! {
                view(style = StyleRules {
                    margin_top: Some(px(16.0)),
                    gap: Some(px(4.0)),
                    flex_direction: Some(FlexDirection::Row),
                    justify_content: Some(JustifyContent::Center),
                    ..Default::default()
                }, test_id = switch_test_id) {
                    text(style = StyleRules {
                        font_size: Some(px(13.0)),
                        color: Some(col(TEXT_MUTED)),
                        ..Default::default()
                    }) { switch_prompt.to_string() }
                    text(style = StyleRules {
                        font_size: Some(px(13.0)),
                        font_weight: Some(FontWeight::SemiBold),
                        color: Some(col(theme::TEXT_LINK)),
                        ..Default::default()
                    }) { switch_action.to_string() }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        set_mode(!is_signup);
                    }
                    TouchResponse::CONSUMED
                })
            };

            ui! {
                view(style = StyleRules {
                    align_self: Some(AlignSelf::Stretch),
                    flex_direction: Some(FlexDirection::Column),
                    ..Default::default()
                }) {
                    tabs
                    text(style = heading_style.clone()) { heading.to_string() }
                    text(style = sub_style.clone()) { sub.to_string() }
                    text(style = label_style.clone()) { "Username".to_string() }
                    text_input(
                        value = username,
                        test_id = "login-email",
                        on_change = on_username_change,
                        placeholder = "username",
                        style = field_style.clone(),
                    )
                    .on_key_down(on_enter)
                    if let Some(el) = username_hint {
                        el
                    }
                    text(style = label_style.clone()) { "Password".to_string() }
                    text_input(
                        value = password,
                        test_id = "login-password",
                        on_change = on_pw_change,
                        placeholder = "••••••••",
                        secure = true,
                        style = field_style.clone(),
                    )
                    .on_key_down(on_enter)
                    if let Some(el) = confirm_el {
                        el
                    }
                    if let Some(el) = err_el {
                        el
                    }
                    button(
                        label = submit_label,
                        test_id = "login-submit",
                        on_click = on_submit,
                        // Read live so the button greys out while `busy`.
                        disabled = move || busy.get(),
                        style = StyleRules {
                            background: Some(submit_bg),
                            ..submit_style.clone()
                        },
                    )
                    switch_link
                }
            }
        },
    );

    ui! {
        view(style = StyleRules {
            width: Some(pct(100.0)),
            height: Some(pct(100.0)),
            background: Some(col(BG_RAIL)),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::Center),
            overflow: Some(Overflow::Hidden),
            ..Default::default()
        }) {
            view(style = card_style) {
                view(style = logo_style) {
                    text(style = logo_text_style) { "D".to_string() }
                }
                form
            }
        }
    }
}
