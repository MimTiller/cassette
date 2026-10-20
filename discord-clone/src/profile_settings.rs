//! `profile_settings` — the Profile screen: a display-name text input seeded
//! from the signed-in account (the real backend username from
//! `crate::auth::current_username()`, falling back to the mock `USER` identity
//! in guest mode), a circular avatar (uploaded image when set, else the
//! colored circle + first letter of the name), a status-message text input,
//! and the avatar controls: "Change avatar" (paste a `data:image/…;base64,`
//! URL — the same live URL+Save pattern the server-settings panel uses for
//! the server icon) and "Remove".
//!
//! The avatar URL rides the shared client avatar registry
//! (`crate::auth::remember_avatar` / `crate::auth::avatar_for`) so every
//! other surface (message rows, member lists, DMs) shows the uploaded image
//! without re-fetching.
//!
//! Idioms, each verified against this crate:
//! - styles are plain `StyleRules` via the `style_helpers` `px` / `col`
//!   constructors (one static pass, no live style engine);
//! - the placeholder-vs-image swap is a node-level reactive
//!   `if !avatar_url.get().is_empty() { … } else { … }` inside `ui!` — the
//!   `if` condition reads a signal (`.get()`), so it lowers to the `when`
//!   primitive (mounts the chosen branch, unmounts the other on toggle).
//!   `StyleRules` is not `Copy`, so EVERY style used inside the branch is
//!   written INLINE in `style = …` (created fresh per evaluation, never
//!   captured across the `when` boundary);
//! - the Save / Remove pills are `view {…}.on_touch(|ev| …)` chains with the
//!   `TouchPhase::Ended` guard — the app's standard click pattern;
//! - the upload RPC round trip is the `ServerSettingsPanel` triple: a `busy`
//!   signal + inline `error` signal keyed into the labels reactively, and
//!   `runtime_core::driver::spawn_async(async move { … })` for the stub call.

use crate::style_helpers::{borderless_input, col, px};
use runtime_core::{
    component,
    ui,
    AlignItems,
    Element,
    FlexDirection,
    FontWeight,
    JustifyContent,
    ObjectFit,
    StyleRules,
    Tokenized,
    TouchPhase,
    TouchResponse,
    signal,
};

// The same Discord dark palette as `app.rs`, kept local so the screen is
// self-contained.
const BG_MAIN: &str = "#313338";
const TEXT: &str = "#f2f3f5";
const COMPOSER: &str = "#383a40";
/// Blurple avatar tint (the signed-in user's chip color).
const AVATAR: &str = "#5865f2";
/// Button green (the Save / Remove pills, same token as `BTN_SAVE` elsewhere).
const BTN: &str = "#23a55a";
/// Discord red (the inline rejection line).
const ERROR: &str = "#f23f43";

fn grow(v: f32) -> Tokenized<f32> {
    Tokenized::Literal(v)
}

/// The signed-in account's current avatar URL, read straight off the shared
/// registry (`""` = none). ProfileSettings is rebuilt inside the `authed`
/// switch, so a plain read at build time captures the post-login value; the
/// "Change avatar" flow below writes the local `avatar_url` signal the chip
/// binds reactively, so within the open screen an upload re-renders.
fn seeded_avatar() -> String {
    let name = crate::auth::current_username();
    if name.is_empty() {
        return String::new();
    }
    crate::auth::avatar_for(name.as_str()).unwrap_or_default()
}

#[component]
pub fn profile_settings() -> Element {
    // Real signed-in identity: the backend username recorded at login in
    // `CURRENT_USERNAME` (see `crate::auth`), the same source the user panel
    // reads. Guest mode leaves it empty, so fall back to the mock `USER`
    // identity there. This component is rebuilt when the `authed` gate flips
    // on login, so a plain read here captures the real name.
    let real_name = crate::auth::current_username();
    let display_name = signal(if !real_name.is_empty() {
        real_name
    } else {
        crate::data::USER.name.to_string()
    });
    // Status message: no backend field yet, so seed from the real presence
    // status vocabulary (`Status::Online.name()` = "Online") rather than a
    // mock placeholder.
    let status_message = signal(crate::data::Status::Online.name().to_string());
    // The signed-in user's avatar URL (a `data:image/…;base64,` string,
    // seeded from the shared registry) — written by the "Change avatar"
    // input + Save flow below and read reactively by the avatar chip.
    let avatar_url = signal(seeded_avatar());

    // The avatar's first letter, recomputed whenever the name changes.
    let avatar_letter = {
        let name = display_name.clone();
        move || {
            name.get()
                .chars()
                .next()
                .map(|c| c.to_ascii_uppercase())
                .unwrap_or('?')
                .to_string()
        }
    };

    let input_style = StyleRules {
        flex_grow: Some(grow(1.0)),
        height: Some(px(40.0)),
        padding_left: Some(px(12.0)),
        padding_right: Some(px(12.0)),
        background: Some(col(COMPOSER)),
        color: Some(col(TEXT)),
        border_top_left_radius: Some(px(8.0)),
        border_top_right_radius: Some(px(8.0)),
        border_bottom_left_radius: Some(px(8.0)),
        border_bottom_right_radius: Some(px(8.0)),
        ..borderless_input()
    };

    // The avatar controls share their state with the handlers: the URL
    // signal doubles as the input value; `av_busy` + `av_error` are the
    // in-flight flag and the backend's inline rejection (the exact
    // `ServerSettingsPanel` triple: busy/error key the labels, the input
    // feeds the RPC).
    let av_busy = signal(false);
    let av_error = signal(String::new());

    // Save handler: read the URL at submit time, fire the `set_avatar` RPC
    // (self only — the acting user is the bearer token), and on success push
    // the value into the shared avatar registry keyed by the REAL username
    // (not the editable display name) so every other surface (message rows,
    // member lists, DMs) shows it too.
    let do_save = move || {
        if av_busy.get() {
            return;
        }
        let raw = avatar_url.get();
        let trimmed: String = raw.trim().to_string();
        if trimmed.is_empty() {
            av_error.set("Paste a data:image/…;base64, URL first, or tap Remove.".to_string());
            return;
        }
        let busy = av_busy;
        let err = av_error;
        runtime_core::driver::spawn_async(async move {
            match crate::auth::set_avatar(Some(trimmed.to_string())).await {
                Ok(_) => {
                    // Key the registry on the backend username, which is what
                    // every payload carries.
                    let name = crate::auth::current_username();
                    crate::auth::remember_avatar(name.as_str(), Some(trimmed));
                }
                Err(msg) => err.set(msg),
            }
            busy.set(false);
        });
    };
    // Remove handler: clear the stored avatar server-side and wipe the
    // registry entry + local signal so the initial circle comes back.
    let do_remove = move || {
        if av_busy.get() {
            return;
        }
        let busy = av_busy;
        let err = av_error;
        let av = avatar_url;
        busy.set(true);
        err.set(String::new());
        runtime_core::driver::spawn_async(async move {
            match crate::auth::set_avatar(None).await {
                Ok(_) => {
                    let name = crate::auth::current_username();
                    crate::auth::remember_avatar(name.as_str(), None);
                    av.set(String::new());
                }
                Err(msg) => err.set(msg),
            }
            busy.set(false);
        });
    };

    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            align_items: Some(AlignItems::Center),
            background: Some(col(BG_MAIN)),
            padding_top: Some(px(24.0)),
            padding_left: Some(px(20.0)),
            padding_right: Some(px(20.0)),
            gap: Some(px(16.0)),
            ..Default::default()
        }) {
            // The avatar chip: the uploaded image when set, else the colored
            // initial circle (the pre-existing look). `avatar_url.get()` is
            // read inside the `if`, so it lowers to the `when` primitive and
            // flips on Save/Remove. Styles are inline: nothing non-Copy is
            // captured across the `when` boundary.
            if !avatar_url.get().is_empty() {
                image(
                    src = move || avatar_url.get().clone(),
                    alt = "Your avatar",
                    style = StyleRules {
                        width: Some(px(72.0)),
                        height: Some(px(72.0)),
                        flex_shrink: Some(grow(0.0)),
                        object_fit: Some(ObjectFit::Cover),
                        border_top_left_radius: Some(px(36.0)),
                        border_top_right_radius: Some(px(36.0)),
                        border_bottom_left_radius: Some(px(36.0)),
                        border_bottom_right_radius: Some(px(36.0)),
                        ..Default::default()
                    },
                )
            } else {
                text(style = StyleRules {
                    width: Some(px(72.0)),
                    height: Some(px(72.0)),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    background: Some(col(AVATAR)),
                    color: Some(col(TEXT)),
                    font_size: Some(px(30.0)),
                    font_weight: Some(FontWeight::Bold),
                    border_top_left_radius: Some(px(36.0)),
                    border_top_right_radius: Some(px(36.0)),
                    border_bottom_left_radius: Some(px(36.0)),
                    border_bottom_right_radius: Some(px(36.0)),
                    ..Default::default()
                }) { avatar_letter }
            }
            text_input(
                value = display_name.clone(),
                placeholder = "Display name",
                style = input_style.clone(),
            )
            text_input(
                value = status_message,
                placeholder = "Status message",
                style = input_style.clone(),
            )
            // The "Change avatar" flow: paste the image URL, tap Save; tap
            // Remove to clear. Busy + rejection render inline in the labels
            // (reactive closures re-run when `av_busy` / `av_error` flip).
            text_input(
                value = avatar_url.clone(),
                placeholder = "Avatar image URL (data:image/…;base64,)",
                style = input_style,
            )
            view(style = StyleRules {
                height: Some(px(34.0)),
                background: Some(col(BTN)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                border_top_left_radius: Some(px(8.0)),
                border_top_right_radius: Some(px(8.0)),
                border_bottom_left_radius: Some(px(8.0)),
                border_bottom_right_radius: Some(px(8.0)),
                flex_shrink: Some(grow(0.0)),
                ..Default::default()
            }) {
                text(style = StyleRules {
                    font_size: Some(px(13.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { move || if av_busy.get() { "Saving…" } else { "Save avatar" } }
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    do_save();
                }
                TouchResponse::CONSUMED
            })
            view(style = StyleRules {
                height: Some(px(34.0)),
                background: Some(col(BTN)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                border_top_left_radius: Some(px(8.0)),
                border_top_right_radius: Some(px(8.0)),
                border_bottom_left_radius: Some(px(8.0)),
                border_bottom_right_radius: Some(px(8.0)),
                flex_shrink: Some(grow(0.0)),
                ..Default::default()
            }) {
                text(style = StyleRules {
                    font_size: Some(px(13.0)),
                    font_weight: Some(FontWeight::SemiBold),
                    color: Some(col(TEXT)),
                    ..Default::default()
                }) { move || if av_busy.get() { "Removing…" } else { "Remove" } }
            }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    do_remove();
                }
                TouchResponse::CONSUMED
            })
            if !av_error.get().is_empty() {
                text(style = StyleRules {
                    font_size: Some(px(12.0)),
                    color: Some(col(ERROR)),
                    ..Default::default()
                }) { move || av_error.get().clone() }
            }
        }
    }
}
