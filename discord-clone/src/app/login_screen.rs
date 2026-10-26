//! The sign-in / sign-up screen.
//! Split out of `app.rs`; `use super::*` keeps app.rs's imports and shared
//! colors/helpers in scope.

use super::*;

/// The login screen, shown while `authed` is false. A username + password
/// form with two modes toggled by `mode`: sign in (the default) and sign up.
/// The password field is `secure` (masks input like a real login); empty
/// fields show an inline error. On success — a real backend login or signup —
/// `authed` flips and this screen unmounts. "Continue as guest" skips the
/// fields entirely. State is local to this function — it unmounts the moment
/// `authed` flips, so no session survives a reload.
pub(super) fn login_screen(authed: Signal<bool>, busy: Signal<bool>) -> Element {
    let _ = &busy; // threaded for the submit button + handlers' guard (below).
    // "you" is the only identity in this mock build; authing in just flips
    // the shared signal the app tree is keyed on.
    // `mode` flips the form between sign-in (false, the default) and sign-up
    // (true); both reuse the same username + password fields. `error` holds
    // the inline message for a failed submit in either mode.
    let username: Signal<String> = signal(String::new());
    let password: Signal<String> = signal(String::new());
    let error: Signal<String> = signal(String::new());
    let mode: Signal<bool> = signal(false);
    // If a persisted session was just dropped as stale (the app shell cleared
    // the note before flipping us back here), surface it immediately instead
    // of a blank form. One-shot: it's `None` on a real logout or a first-time
    // visit, so those render cleanly.
    if let Some(note) = SESSION_NOTE
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .take()
    {
        error.set(note);
    }

    // Card + text styles (static — built once, cloned into the render).
    let card_style = StyleRules {
        width: Some(px(380.0)),
        padding_top: Some(px(40.0)),
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
        width: Some(px(64.0)),
        height: Some(px(64.0)),
        align_items: Some(AlignItems::Center),
        justify_content: Some(JustifyContent::Center),
        background: Some(col(BUTTON)),
        border_top_left_radius: Some(px(20.0)),
        border_top_right_radius: Some(px(20.0)),
        border_bottom_left_radius: Some(px(20.0)),
        border_bottom_right_radius: Some(px(20.0)),
        ..Default::default()
    };
    let logo_text_style = StyleRules {
        font_size: Some(px(30.0)),
        font_weight: Some(FontWeight::Bold),
        color: Some(col(TEXT)),
        ..Default::default()
    };
    let heading_style = StyleRules {
        margin_top: Some(px(20.0)),
        font_size: Some(px(22.0)),
        font_weight: Some(FontWeight::Bold),
        color: Some(col(TEXT)),
        ..Default::default()
    };
    let sub_style = StyleRules {
        margin_top: Some(px(4.0)),
        margin_bottom: Some(px(24.0)),
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

    // The signed-in account's numeric id (`CURRENT_USER_ID`) and the signed
    // session token (`CURRENT_TOKEN`) live in `crate::auth`. The
    // `server::configure` credential provider reads `current_token()` to
    // thread the `Authorization: Bearer` header; the id rides the realtime
    // gateway's connect URL. `0`/empty mean "not logged in".

    // The shared login action: validate, then auth in. `authed` flipping is
    // what unmounts this screen and mounts the app tree (the `switch` key in
    // `app()` re-evaluates on it). Every captured signal is `Copy`.
    let do_login = {
        let authed = authed.clone();
        let busy = busy.clone();
        move || {
            let em = username.get();
            let pw = password.get();
            if em.trim().is_empty() || pw.trim().is_empty() {
                error.set("Username and password are required.".to_string());
                return;
            }
            // Double-submit guard: a login is already in flight (covers the
            // Enter-key path, which bypasses the button's own press blocking).
            if busy.get() {
                return;
            }
            // Real login: spawn the backend `#[server]` login call and resolve
            // the session before flipping `authed`. On success `auth::login`
            // records the numeric id and the signed session token
            // process-wide; the client threads the token back as
            // `Authorization: Bearer *** on backend calls. On failure the
            // server's message is shown inline.
            error.set(String::new());
            // In-flight flag: disables the submit button (and flips its label)
            // until the login call lands.
            busy.set(true);
            runtime_core::driver::spawn_async(async move {
                match crate::auth::login(&em, &pw).await {
                    Ok(_) => {
                        authed.set(true);
                        busy.set(false);
                    }
                    Err(message) => {
                        error.set(message);
                        busy.set(false);
                    }
                }
            });
        }
    };
    // The shared sign-up action: same validation, but calls `auth::signup`,
    // which creates the account and logs it straight in (so the session
    // token is set too), then flips `authed`. A duplicate username surfaces
    // inline the same way a failed login does.
    let do_signup = {
        let authed = authed.clone();
        let busy = busy.clone();
        move || {
            let em = username.get();
            let pw = password.get();
            if em.trim().is_empty() || pw.trim().is_empty() {
                error.set("Username and password are required.".to_string());
                return;
            }
            // Double-submit guard (covers the Enter-key path).
            if busy.get() {
                return;
            }
            error.set(String::new());
            // In-flight flag: disables the submit button (and flips its label)
            // until the sign-up call lands.
            busy.set(true);
            runtime_core::driver::spawn_async(async move {
                match crate::auth::signup(&em, &pw).await {
                    Ok(_) => {
                        authed.set(true);
                        busy.set(false);
                    }
                    Err(message) => {
                        error.set(message);
                        busy.set(false);
                    }
                }
            });
        }
    };
    // Submit routes by the current `mode`: sign in or sign up. Both are `Copy`
    // closures (capturing only `Copy` signals), so they're callable from a
    // `Fn` button handler and the Enter-in-password handler alike.
    let on_submit_click = {
        let mode = mode.clone();
        move || {
            if mode.get() {
                do_signup();
            } else {
                do_login();
            }
        }
    };
    let on_pw_enter = move |ev: &KeyEvent| {
        if ev.key == "Enter" && !ev.shift && !ev.ctrl && !ev.alt && !ev.meta {
            on_submit_click();
            KeyOutcome::PreventDefault
        } else {
            KeyOutcome::Default
        }
    };
    // "Continue as guest" — a mock path: skips the fields and the backend
    // entirely and just flips `authed`. Real sign-in / sign-up go through the
    // fields and `crate::auth`; guest remains a local no-account shortcut.
    let on_guest = move || authed.set(true);

    // The form. The inline error is reactive: a nested `switch` keyed on the
    // `error` signal rebuilds the form whenever a failed submit sets it (the
    // same `switch(key, render)` pattern the sidebar/message list use —
    // `ui!` doesn't re-read signal state on its own, only on rebuild).
    // `form` is keyed on `(error, mode)` so it also rebuilds when the mode
    // toggles between sign-in and sign-up.
    let username_change = username.clone();
    let pw_change = password.clone();
    let on_username_change = move |s: String| username_change.set(s);
    let on_pw_change = move |s: String| pw_change.set(s);
    // Mode toggle: flip `mode` to its opposite (sign-in <-> sign-up), so a
    // single closure serves both reciprocal links. Captures only the `Copy`
    // `mode` + `error` signals, so it's itself `Copy` and can be cloned into
    // the (also `Copy`) switch render closure. Toggling clears any inline
    // error so a failure in one mode doesn't bleed into the other.
    let on_toggle_mode = {
        let mode = mode.clone();
        move || {
            mode.set(!mode.get());
            error.set(String::new());
        }
    };
    // The "Continue as guest" pill's parts. The pill `Element` is built *inside*
    // the render closure (Elements aren't `Copy` and the closure is `Fn`, so a
    // pre-built one would be moved on the first rebuild and gone the second).
    let guest_style = StyleRules {
        margin_top: Some(px(8.0)),
        padding_top: Some(px(2.0)),
        padding_bottom: Some(px(2.0)),
        font_size: Some(px(13.0)),
        font_weight: Some(FontWeight::SemiBold),
        ..Default::default()
    };
    let guest_text_style = StyleRules {
        color: Some(col(BUTTON)),
        font_size: Some(px(13.0)),
        font_weight: Some(FontWeight::SemiBold),
        ..Default::default()
    };
    let form = switch(
        // Keyed on `(error, mode, busy)` so the form rebuilds when a failed
        // submit sets `error`, when the mode toggles, AND when a submit flips
        // `busy` (which drives the submit button's `disabled` state and label
        // — the inert/dimmed button and in-flight wording must re-render as the
        // flag turns on and off).
        // Same `switch(key, render)` pattern the sidebar/message list use —
        // `ui!` doesn't re-read signal state on its own, only on rebuild.
        move || (error.get(), mode.get(), busy.get()),
        move |state: &(String, bool, bool)| {
            let err = state.0.clone();
            let is_signup = state.1;
            let in_flight = state.2;
            // The inline error, shown only while non-empty (a failed submit
            // sets it; a successful sign-in clears it and unmounts the screen).
            // Reused verbatim for both sign-in and sign-up failures.
            let err_style = StyleRules {
                margin_top: Some(px(12.0)),
                font_size: Some(px(13.0)),
                color: Some(col("#f23f43")),
                text_align: Some(TextAlign::Center),
                ..Default::default()
            };
            let err_el = if err.is_empty() {
                None
            } else {
                Some(ui! { text(style = err_style) { err } })
            };
            // The mode-scoped heading + sub. The `StyleRules` are cloned into
            // the render (whiteboard idiom); the copy changes with `mode`.
            let heading = if is_signup {
                "Create an account".to_string()
            } else {
                "Welcome back!".to_string()
            };
            let sub = if is_signup {
                "Ready to jump in? Create your account in seconds."
            } else {
                "We're so excited to see you again!"
            }
            .to_string();
            // The "Continue as guest" pill (mode-independent). Built *inside*
            // the render closure (Elements aren't `Copy` and the closure is
            // `Fn`, so a pre-built one would be moved on the first rebuild and
            // gone the second).
            let guest = on_guest.clone();
            let guest_style_i = guest_style.clone();
            let guest_text_style_i = guest_text_style.clone();
            let guest_btn = ui! {
                view(style = guest_style_i, test_id = "login-guest") {
                    text(style = guest_text_style_i) { "Continue as guest".to_string() }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        guest();
                    }
                    TouchResponse::CONSUMED
                })
            };
            // The reciprocal mode-toggle link: "No account? Sign up" in
            // sign-in mode, "Have an account? Sign in" in sign-up mode. Tapping
            // it flips `mode` (and clears any inline error), so the `(error,
            // mode)` key above rebuilds the form into the other mode.
            let toggle_label = if is_signup {
                "Have an account? Sign in".to_string()
            } else {
                "No account? Sign up".to_string()
            };
            let toggle_test_id = if is_signup {
                "login-signin-toggle"
            } else {
                "login-signup-toggle"
            };
            let toggle = on_toggle_mode.clone();
            let toggle_style = StyleRules {
                margin_top: Some(px(16.0)),
                font_size: Some(px(13.0)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                ..Default::default()
            };
            let toggle_text_style = StyleRules {
                color: Some(col(BUTTON)),
                font_size: Some(px(13.0)),
                font_weight: Some(FontWeight::SemiBold),
                ..Default::default()
            };
            let toggle_btn = ui! {
                view(style = toggle_style, test_id = toggle_test_id) {
                    text(style = toggle_text_style) { toggle_label }
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        toggle();
                    }
                    TouchResponse::CONSUMED
                })
            };
            // The submit button label flips with the mode; the handler
            // (`on_submit_click`) routes on `mode` internally. While a submit
            // is in flight (`busy`), the label shows the in-flight wording
            // ("Signing in…" / "Creating account…") and the button is disabled
            // (inert) — so the user gets clear feedback and can't double-submit.
            let submit_label = if in_flight {
                if is_signup {
                    "Creating account…".to_string()
                } else {
                    "Signing in…".to_string()
                }
            } else if is_signup {
                "Sign Up".to_string()
            } else {
                "Sign In".to_string()
            };
            ui! {
                // `ui!` moves each style into its render closure (the
                // whiteboard idiom: clone outer styles per use inside a
                // `switch` render).
                view(style = StyleRules {
                    align_self: Some(AlignSelf::Stretch),
                    flex_direction: Some(FlexDirection::Column),
                    ..Default::default()
                }) {
                    text(style = StyleRules {
                        // Centred (the heading/sub live in the form's full-width
                        // column now, so they need an explicit `text_align` to
                        // keep the card's centred look — `sub_style` already
                        // carries one, the heading didn't).
                        text_align: Some(TextAlign::Center),
                        ..heading_style.clone()
                    }) { heading }
                    text(style = sub_style.clone()) { sub }
                    text(style = label_style.clone()) { "Username".to_string() }
                    text_input(
                        value = username,
                        test_id = "login-email",
                        on_change = on_username_change,
                        placeholder = "username",
                        style = field_style.clone(),
                    )
                    // Enter in the username field submits too -- without this,
                    // only the password field (below) responded to Enter, so
                    // the natural "type username, hit Enter" flow silently did
                    // nothing until the user tabbed into the password field.
                    .on_key_down(on_pw_enter)
                    text(style = label_style.clone()) { "Password".to_string() }
                    text_input(
                        value = password,
                        test_id = "login-password",
                        on_change = on_pw_change,
                        placeholder = "••••••••",
                        secure = true,
                        style = field_style.clone(),
                    )
                    .on_key_down(on_pw_enter)
                    // The inline error, spliced in only when present.
                    if let Some(el) = err_el {
                        el
                    }
                    button(
                        label = submit_label,
                        test_id = "login-submit",
                        on_click = on_submit_click,
                        // Live `disabled` state (the proven in-repo idiom, cf.
                        // rules_accept_checkbox.rs): while a submit is in flight
                        // this reads `busy.get()` inside a closure, so the prop
                        // re-evaluates reactively as `busy` flips — muting the
                        // button (inert + visually dimmed) and blocking the
                        // press against double-taps. The Enter-key path is
                        // separately guarded by the `busy.get()` early-return at
                        // the top of `do_login`/`do_signup`.
                        disabled = move || busy.get(),
                        style = submit_style.clone(),
                    )
                    toggle_btn
                    guest_btn
                }
            }
        },
    );

    // Full-screen backdrop (Discord's login page: dark, centred card).
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
                // The mode-scoped heading + sub live inside the form now (so
                // they flip with the sign-in/sign-up mode); only the logo stays
                // static in the card here.
                form
            }
        }
    }
}

// --- the tree ----------------------------------------------------------------
