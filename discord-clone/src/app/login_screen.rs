//! The sign-in / sign-up screen.
//! `use super::*` keeps app.rs's imports and shared colors/helpers in scope.

use crate::theme;
use super::*;

/// Which form the login card shows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    LogIn,
    SignUp,
    /// Opened from a password reset link (`/?reset=<token>`).
    Reset,
}

/// The login screen, shown while `authed` is false. One card with two tabs,
/// "Log In" and "Sign Up". Sign-up has its own heading, a confirm-password
/// field, a username hint and a green submit button, so the two forms don't
/// look alike. A reset link shows a third form, without tabs, for choosing a
/// new password. On success `authed` flips and this screen unmounts; its
/// state is local and dies with it.
pub(super) fn login_screen(authed: Signal<bool>, busy: Signal<bool>) -> Element {
    let username: Signal<String> = signal(String::new());
    let password: Signal<String> = signal(String::new());
    let confirm: Signal<String> = signal(String::new());
    let error: Signal<String> = signal(String::new());
    // A green message above the form, e.g. after a password reset.
    let notice: Signal<String> = signal(String::new());
    // Whether the "Forgot your password?" link has been opened.
    let forgot: Signal<bool> = signal(false);
    let reset_token: Signal<String> = signal(crate::auth::reset_token_in_url().unwrap_or_default());
    let screen: Signal<Screen> =
        signal(if reset_token.get().is_empty() { Screen::LogIn } else { Screen::Reset });
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
        width: Some(px(crate::app::mobile::fit_width(400.0))),
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

    // Server calls finish in callbacks anchored to this screen, so a reply
    // that lands after it unmounted is dropped instead of writing freed
    // signals. (The handlers' own `busy` write rebuilds the form, so the
    // handler's node is the wrong anchor.)
    let alive = runtime_core::ScopeAlive::current();

    // Log in: validate, call the server, and flip `authed` on success. The
    // session id and token are recorded by `crate::auth`.
    let do_login = {
        let alive = alive.clone();
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
            notice.set(String::new());
            busy.set(true);
            runtime_core::spawn_then_in(&alive, async move { crate::auth::login(&name, &pw).await }, move |result| {
                match result {
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
        let alive = alive.clone();
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
            runtime_core::spawn_then_in(&alive, async move { crate::auth::signup(&name, &pw).await }, move |result| {
                match result {
                    Ok(_) => authed.set(true),
                    Err(message) => error.set(message),
                }
                busy.set(false);
            });
        }
    };
    // Reset: set the new password with the link's token, then show the log-in
    // form with the username filled in.
    let do_reset = {
        let alive = alive.clone();
        let busy = busy.clone();
        move || {
            let pw = password.get();
            if pw.trim().is_empty() {
                error.set("Pick a new password.".to_string());
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
            let token = reset_token.get();
            runtime_core::spawn_then_in(&alive, async move { crate::auth::reset_password(&token, &pw).await }, move |result| {
                match result {
                    Ok(name) => {
                        crate::auth::clear_reset_token_from_url();
                        reset_token.set(String::new());
                        username.set(name);
                        password.set(String::new());
                        confirm.set(String::new());
                        notice.set("Password changed. Log in with your new password.".to_string());
                        screen.set(Screen::LogIn);
                    }
                    Err(message) => error.set(message),
                }
                busy.set(false);
            });
        }
    };
    let on_submit: Rc<dyn Fn()> = Rc::new(move || match screen.get() {
        Screen::LogIn => do_login(),
        Screen::SignUp => do_signup(),
        Screen::Reset => do_reset(),
    });
    // Each input gets its own Enter-to-submit handler.
    let on_enter = {
        let on_submit = on_submit.clone();
        move || {
            let on_submit = on_submit.clone();
            move |ev: &KeyEvent| {
                if ev.key == "Enter" && !ev.shift && !ev.ctrl && !ev.alt && !ev.meta {
                    on_submit();
                    KeyOutcome::PreventDefault
                } else {
                    KeyOutcome::Default
                }
            }
        }
    };
    let on_username_change = move |s: String| username.set(s);
    let on_pw_change = move |s: String| password.set(s);
    let on_confirm_change = move |s: String| confirm.set(s);
    // Switching forms clears the passwords and any messages, so nothing typed
    // or reported in one form carries into the other. Leaving the reset form
    // drops the link's token.
    let set_screen = move |next: Screen| {
        if screen.get() != next {
            if screen.get() == Screen::Reset {
                crate::auth::clear_reset_token_from_url();
                reset_token.set(String::new());
            }
            screen.set(next);
            password.set(String::new());
            confirm.set(String::new());
            error.set(String::new());
            notice.set(String::new());
            forgot.set(false);
        }
    };

    // Rebuilt when any of these change; the inputs refill from their signals.
    let form = switch(
        move || (error.get(), screen.get(), busy.get(), notice.get(), forgot.get()),
        move |state: &(String, Screen, bool, String, bool)| {
            let (err, current, in_flight, note, forgot_open) = state.clone();
            let is_signup = current == Screen::SignUp;
            let is_reset = current == Screen::Reset;

            let tab = |label: &str, target: Screen, test_id: &'static str| {
                let active = target == current;
                let signup = target == Screen::SignUp;
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
                            set_screen(target);
                        }
                        TouchResponse::CONSUMED
                    })
                }
            };
            let tabs = (!is_reset).then(|| {
                ui! {
                    view(style = StyleRules {
                        align_self: Some(AlignSelf::Stretch),
                        margin_top: Some(px(20.0)),
                        flex_direction: Some(FlexDirection::Row),
                        ..Default::default()
                    }) {
                        tab("Log In", Screen::LogIn, "login-tab-signin")
                        tab("Sign Up", Screen::SignUp, "login-tab-signup")
                    }
                }
            });

            let (heading, sub) = match current {
                Screen::LogIn => ("Welcome back!", "We're so excited to see you again!"),
                Screen::SignUp => ("Create an account", "Pick a username and password to join."),
                Screen::Reset => ("Choose a new password", "This link works once."),
            };
            let notice_el = (!note.is_empty()).then(|| {
                ui! {
                    text(style = StyleRules {
                        margin_bottom: Some(px(4.0)),
                        font_size: Some(px(13.0)),
                        color: Some(col(theme::SUCCESS)),
                        text_align: Some(TextAlign::Center),
                        ..Default::default()
                    }) { note }
                }
            });
            let username_el = (!is_reset).then(|| {
                ui! {
                    view(style = StyleRules {
                        align_self: Some(AlignSelf::Stretch),
                        flex_direction: Some(FlexDirection::Column),
                        ..Default::default()
                    }) {
                        text(style = label_style.clone()) { "Username".to_string() }
                        text_input(
                            value = username,
                            test_id = "login-email",
                            on_change = on_username_change,
                            placeholder = "username",
                            style = field_style.clone(),
                        )
                        .on_key_down(on_enter())
                    }
                }
            });
            // Log in only: there is no email, so the link says who can help.
            let forgot_el = (current == Screen::LogIn).then(|| {
                if forgot_open {
                    ui! {
                        text(style = hint_style.clone()) {
                            "Ask whoever runs this site for a password reset link.".to_string()
                        }
                    }
                } else {
                    ui! {
                        view(style = StyleRules {
                            margin_top: Some(px(6.0)),
                            flex_direction: Some(FlexDirection::Row),
                            ..Default::default()
                        }, test_id = "login-forgot") {
                            text(style = StyleRules {
                                font_size: Some(px(12.0)),
                                color: Some(col(theme::TEXT_LINK)),
                                ..Default::default()
                            }) { "Forgot your password?".to_string() }
                        }
                        .on_touch(move |ev| {
                            if ev.phase == TouchPhase::Ended {
                                forgot.set(true);
                            }
                            TouchResponse::CONSUMED
                        })
                    }
                }
            });
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
            let confirm_el = (is_signup || is_reset).then(|| {
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
                        .on_key_down(on_enter())
                    }
                }
            });

            let submit_label = match (current, in_flight) {
                (Screen::LogIn, true) => "Logging in…",
                (Screen::LogIn, false) => "Log In",
                (Screen::SignUp, true) => "Creating account…",
                (Screen::SignUp, false) => "Create Account",
                (Screen::Reset, true) => "Saving…",
                (Screen::Reset, false) => "Set Password",
            }
            .to_string();
            let submit_bg = if is_signup { col(theme::SUCCESS) } else { col(BUTTON) };
            let password_label = if is_reset { "New password" } else { "Password" }.to_string();
            let submit_click = {
                let on_submit = on_submit.clone();
                move || on_submit()
            };

            let (switch_prompt, switch_action, switch_to, switch_test_id) = match current {
                Screen::LogIn => ("Need an account?", "Sign up", Screen::SignUp, "login-signup-toggle"),
                Screen::SignUp => ("Already have an account?", "Log in", Screen::LogIn, "login-signin-toggle"),
                Screen::Reset => ("Remembered it?", "Log in", Screen::LogIn, "login-signin-toggle"),
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
                        set_screen(switch_to);
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
                    if let Some(el) = tabs {
                        el
                    }
                    text(style = heading_style.clone()) { heading.to_string() }
                    text(style = sub_style.clone()) { sub.to_string() }
                    if let Some(el) = notice_el {
                        el
                    }
                    if let Some(el) = username_el {
                        el
                    }
                    if let Some(el) = username_hint {
                        el
                    }
                    text(style = label_style.clone()) { password_label }
                    text_input(
                        value = password,
                        test_id = "login-password",
                        on_change = on_pw_change,
                        placeholder = "••••••••",
                        secure = true,
                        style = field_style.clone(),
                    )
                    .on_key_down(on_enter())
                    if let Some(el) = forgot_el {
                        el
                    }
                    if let Some(el) = confirm_el {
                        el
                    }
                    if let Some(el) = err_el {
                        el
                    }
                    button(
                        label = submit_label,
                        test_id = "login-submit",
                        on_click = submit_click,
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
                    text(style = logo_text_style) { "C".to_string() }
                }
                form
            }
        }
    }
}
