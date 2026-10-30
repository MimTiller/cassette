//! "Position your picture": after an avatar image is picked, drag it inside
//! the circle and zoom, then the browser crops what the circle shows to a
//! 512 px square and re-encodes it small enough for the upload cap. So a
//! full-size phone photo works, and the circle shows exactly the part the
//! person chose. Web only: elsewhere `profile_settings` uploads the picked
//! file as it is.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use runtime_core::{
    pan, signal, switch, ui, AlignItems, Cursor, Element, FlexDirection, FontWeight, JustifyContent, Overflow,
    PanEvent, PanRecognizer, PointerEvents, Position, Signal, StyleRules, TouchEvent, TouchPhase, TouchResponse,
};

use crate::controls::{button_view, ButtonKind};
use crate::style_helpers::{col, pct, px, rounded};
use crate::theme;

/// The circle's size on screen.
const FRAME: f32 = 280.0;
/// The most the picture can be zoomed past filling the circle.
const MAX_ZOOM: f32 = 4.0;
/// The zoom slider's track length.
const TRACK: f32 = 200.0;
/// The biggest picture accepted for cropping (it is shrunk before upload).
pub(crate) const MAX_SOURCE_BYTES: usize = 20 * 1024 * 1024;

/// The longest side of the on-screen copy of the picture.
const PREVIEW_MAX: f32 = 1200.0;

/// The picture being positioned: a downscaled copy to show (`image()` takes
/// http and data URLs, not `blob:`), the object URL the full-size original
/// was decoded from, and the original's size.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Picked {
    url: String,
    blob: String,
    w: f32,
    h: f32,
}

#[derive(Clone, Copy)]
struct State {
    open: Signal<Option<Picked>>,
    zoom: Signal<f32>,
    off: Signal<(f32, f32)>,
    error: Signal<String>,
}

thread_local! {
    static STATE: Cell<Option<State>> = const { Cell::new(None) };
    static ON_APPLY: RefCell<Option<Rc<dyn Fn(String)>>> = const { RefCell::new(None) };
    #[cfg(target_arch = "wasm32")]
    static IMG: RefCell<Option<web_sys::HtmlImageElement>> = const { RefCell::new(None) };
}

/// Set up the dialog's state (once, from `app_shell`).
pub(crate) fn install() {
    STATE.with(|s| {
        s.set(Some(State { open: signal(None), zoom: signal(1.0), off: signal((0.0, 0.0)), error: signal(String::new()) }))
    });
}

fn state() -> State {
    STATE.with(|s| s.get()).expect("avatar_crop::install runs in app_shell")
}

/// The open/closed signal, for Escape (see `app::popups`).
pub(crate) fn open_signal() -> Signal<Option<Picked>> {
    state().open
}

/// Open the dialog on picked image bytes. `on_apply` gets the cropped
/// picture as a `data:` URL. `Err` when the picture can't be shown here
/// (not web, or the browser can't decode it).
pub(crate) async fn open(bytes: Vec<u8>, mime: &str, on_apply: impl Fn(String) + 'static) -> Result<(), String> {
    let picked = load(bytes, mime).await?;
    let st = state();
    ON_APPLY.with(|a| *a.borrow_mut() = Some(Rc::new(on_apply)));
    st.zoom.set(1.0);
    st.off.set((0.0, 0.0));
    st.error.set(String::new());
    st.open.set(Some(picked));
    Ok(())
}

#[cfg(target_arch = "wasm32")]
async fn load(bytes: Vec<u8>, mime: &str) -> Result<Picked, String> {
    use wasm_bindgen::JsCast;
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(&bytes[..]));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type(mime);
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &opts)
        .map_err(|_| "could not read the picture".to_string())?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(|_| "could not read the picture".to_string())?;
    let img = web_sys::HtmlImageElement::new().map_err(|_| "could not read the picture".to_string())?;
    img.set_src(&url);
    if wasm_bindgen_futures::JsFuture::from(img.decode()).await.is_err() {
        let _ = web_sys::Url::revoke_object_url(&url);
        return Err("This browser can't open that picture. Try a PNG or JPEG.".to_string());
    }
    let (w, h) = (img.natural_width() as f32, img.natural_height() as f32);
    if w < 1.0 || h < 1.0 {
        let _ = web_sys::Url::revoke_object_url(&url);
        return Err("That picture is empty".to_string());
    }
    let fit = (PREVIEW_MAX / w.max(h)).min(1.0);
    let preview = match draw(&img, (0.0, 0.0, w, h), ((w * fit).round() as u32, (h * fit).round() as u32), 0.85) {
        Ok(p) => p,
        Err(e) => {
            let _ = web_sys::Url::revoke_object_url(&url);
            return Err(e);
        }
    };
    IMG.with(|i| *i.borrow_mut() = Some(img.unchecked_into()));
    Ok(Picked { url: preview, blob: url, w, h })
}

#[cfg(not(target_arch = "wasm32"))]
async fn load(_bytes: Vec<u8>, _mime: &str) -> Result<Picked, String> {
    Err("positioning a picture needs the web app".to_string())
}

fn close() {
    let st = state();
    if let Some(p) = st.open.get() {
        #[cfg(target_arch = "wasm32")]
        let _ = web_sys::Url::revoke_object_url(&p.blob);
        #[cfg(not(target_arch = "wasm32"))]
        let _ = p;
    }
    #[cfg(target_arch = "wasm32")]
    IMG.with(|i| *i.borrow_mut() = None);
    ON_APPLY.with(|a| *a.borrow_mut() = None);
    st.open.set(None);
}

/// Screen pixels per picture pixel: at zoom 1 the shorter side fills the circle.
fn scale(p: &Picked, zoom: f32) -> f32 {
    FRAME / p.w.min(p.h) * zoom
}

/// `off` (the picture's centre, relative to the circle's) kept so the
/// picture always covers the whole circle.
fn clamp_off(p: &Picked, zoom: f32, (x, y): (f32, f32)) -> (f32, f32) {
    let s = scale(p, zoom);
    let mx = ((p.w * s - FRAME) / 2.0).max(0.0);
    let my = ((p.h * s - FRAME) / 2.0).max(0.0);
    (x.clamp(-mx, mx), y.clamp(-my, my))
}

/// Where the picture's top-left corner sits in the frame.
fn corner(p: &Picked, zoom: f32, (x, y): (f32, f32)) -> (f32, f32) {
    let s = scale(p, zoom);
    (FRAME / 2.0 - p.w * s / 2.0 + x, FRAME / 2.0 - p.h * s / 2.0 + y)
}

/// The square of the picture the circle shows, in picture pixels:
/// `(x, y, side)`.
fn crop_rect(p: &Picked, zoom: f32, off: (f32, f32)) -> (f32, f32, f32) {
    let s = scale(p, zoom);
    let (left, top) = corner(p, zoom, off);
    (-left / s, -top / s, FRAME / s)
}

/// Zoom to `z`, keeping the same point of the picture under the circle's centre.
fn set_zoom(st: State, p: &Picked, z: f32) {
    let z = z.clamp(1.0, MAX_ZOOM);
    let ratio = z / st.zoom.get();
    let (x, y) = st.off.get();
    st.off.set(clamp_off(p, z, (x * ratio, y * ratio)));
    st.zoom.set(z);
}

/// Draw `(sx, sy, sw, sh)` of the picture into a `w` x `h` canvas and encode
/// it: WebP where the browser can write it, else JPEG on white.
#[cfg(target_arch = "wasm32")]
fn draw(img: &web_sys::HtmlImageElement, (sx, sy, sw, sh): (f32, f32, f32, f32), (w, h): (u32, u32), quality: f64) -> Result<String, String> {
    use wasm_bindgen::{JsCast, JsValue};
    let fail = |_| "could not draw the picture".to_string();
    let doc = web_sys::window().and_then(|w| w.document()).ok_or("no document")?;
    let canvas: web_sys::HtmlCanvasElement = doc.create_element("canvas").map_err(fail)?.dyn_into().map_err(|_| fail(JsValue::NULL))?;
    canvas.set_width(w.max(1));
    canvas.set_height(h.max(1));
    let ctx: web_sys::CanvasRenderingContext2d =
        canvas.get_context("2d").map_err(fail)?.ok_or("no canvas")?.dyn_into().map_err(|_| fail(JsValue::NULL))?;
    let (dw, dh) = (w.max(1) as f64, h.max(1) as f64);
    ctx.set_image_smoothing_enabled(true);
    ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
        img, sx as f64, sy as f64, sw as f64, sh as f64, 0.0, 0.0, dw, dh,
    )
    .map_err(fail)?;
    let url = canvas.to_data_url_with_type_and_encoder_options("image/webp", &JsValue::from_f64(quality)).map_err(fail)?;
    if url.starts_with("data:image/webp") {
        return Ok(url);
    }
    ctx.set_global_composite_operation("destination-over").map_err(fail)?;
    ctx.set_fill_style_str("#ffffff");
    ctx.fill_rect(0.0, 0.0, dw, dh);
    canvas.to_data_url_with_type_and_encoder_options("image/jpeg", &JsValue::from_f64(quality)).map_err(fail)
}

/// The chosen square as a `data:` URL under the avatar cap, stepping size
/// and quality down until it fits.
#[cfg(target_arch = "wasm32")]
fn encode((sx, sy, side): (f32, f32, f32)) -> Result<String, String> {
    let img = IMG.with(|i| i.borrow().clone()).ok_or("the picture is gone; pick it again")?;
    for (size, quality) in [(512u32, 0.9), (512, 0.75), (384, 0.75), (256, 0.7)] {
        let url = draw(&img, (sx, sy, side, side), (size, size), quality)?;
        if url.len() <= crate::profile_settings::AVATAR_URL_MAX_BYTES {
            return Ok(url);
        }
    }
    Err("That picture is still too large after shrinking it".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
fn encode(_rect: (f32, f32, f32)) -> Result<String, String> {
    Err("positioning a picture needs the web app".to_string())
}

fn apply() {
    let st = state();
    let Some(p) = st.open.get() else { return };
    match encode(crop_rect(&p, st.zoom.get(), st.off.get())) {
        Ok(url) => {
            let cb = ON_APPLY.with(|a| a.borrow().clone());
            close();
            if let Some(cb) = cb {
                cb(url);
            }
        }
        Err(msg) => st.error.set(msg),
    }
}

fn round_button(label: &str, on_tap: impl Fn() + 'static) -> Element {
    let label = label.to_string();
    ui! {
        view(style = StyleRules {
            width: Some(px(28.0)),
            height: Some(px(28.0)),
            align_items: Some(AlignItems::Center),
            justify_content: Some(JustifyContent::Center),
            cursor: Some(Cursor::Pointer),
            ..rounded(14.0)
        }) {
            text(style = StyleRules { font_size: Some(px(18.0)), color: Some(col(theme::TEXT_MUTED)), ..Default::default() }) { label }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                on_tap();
            }
            TouchResponse::CONSUMED
        })
    }
}

/// The circle with the picture in it: drag to move, wheel or pinch to zoom.
fn frame(st: State, p: Picked) -> Element {
    let (zoom, off) = (st.zoom, st.off);
    let shown = p.clone();
    let img_style = move || {
        let s = scale(&shown, zoom.get());
        let (left, top) = corner(&shown, zoom.get(), off.get());
        StyleRules {
            position: Some(Position::Absolute),
            left: Some(px(left)),
            top: Some(px(top)),
            width: Some(px(shown.w * s)),
            height: Some(px(shown.h * s)),
            pointer_events: Some(PointerEvents::None),
            ..Default::default()
        }
    };
    let img = ui! { image(src = p.url.clone(), alt = "Your picture", style = img_style) };
    // Dims everything outside the circle: a ring as thick as the frame's
    // corners are far from the circle, clipped by the frame.
    let band = 64.0;
    let outer = FRAME + 2.0 * band;
    let dim = ui! {
        view(style = StyleRules {
            position: Some(Position::Absolute),
            left: Some(px(-band)),
            top: Some(px(-band)),
            width: Some(px(outer)),
            height: Some(px(outer)),
            border_top_width: Some(runtime_core::Tokenized::Literal(band)),
            border_right_width: Some(runtime_core::Tokenized::Literal(band)),
            border_bottom_width: Some(runtime_core::Tokenized::Literal(band)),
            border_left_width: Some(runtime_core::Tokenized::Literal(band)),
            border_top_color: Some(col(theme::OVERLAY)),
            border_right_color: Some(col(theme::OVERLAY)),
            border_bottom_color: Some(col(theme::OVERLAY)),
            border_left_color: Some(col(theme::OVERLAY)),
            pointer_events: Some(PointerEvents::None),
            ..rounded(outer / 2.0)
        }) {}
    };
    let ring = ui! {
        view(style = StyleRules {
            position: Some(Position::Absolute),
            left: Some(px(0.0)),
            top: Some(px(0.0)),
            width: Some(px(FRAME)),
            height: Some(px(FRAME)),
            border_top_width: Some(runtime_core::Tokenized::Literal(2.0)),
            border_right_width: Some(runtime_core::Tokenized::Literal(2.0)),
            border_bottom_width: Some(runtime_core::Tokenized::Literal(2.0)),
            border_left_width: Some(runtime_core::Tokenized::Literal(2.0)),
            border_top_color: Some(col(theme::TEXT_STRONG)),
            border_right_color: Some(col(theme::TEXT_STRONG)),
            border_bottom_color: Some(col(theme::TEXT_STRONG)),
            border_left_color: Some(col(theme::TEXT_STRONG)),
            pointer_events: Some(PointerEvents::None),
            ..rounded(FRAME / 2.0)
        }) {}
    };
    let start = Cell::new((0.0f32, 0.0f32));
    let dragged = p.clone();
    let wheeled = p;
    ui! {
        view(style = StyleRules {
            position: Some(Position::Relative),
            width: Some(px(FRAME)),
            height: Some(px(FRAME)),
            overflow: Some(Overflow::Hidden),
            background: Some(col(theme::BG_TERTIARY)),
            cursor: Some(Cursor::Grab),
            ..rounded(8.0)
        }) {
            { img }
            { dim }
            { ring }
        }
        .on_touch({
            let h = pan(PanRecognizer::default(), move |ev: &PanEvent| match ev {
                PanEvent::Began { .. } => start.set(off.get()),
                PanEvent::Moved { delta, .. } => {
                    let (x, y) = start.get();
                    off.set(clamp_off(&dragged, zoom.get(), (x + delta.x, y + delta.y)));
                }
                PanEvent::Ended { .. } | PanEvent::Cancelled => {}
            });
            move |ev: &TouchEvent| h(ev)
        })
        .on_wheel(move |ev| {
            match ev.kind {
                runtime_core::WheelKind::Zoom => set_zoom(st, &wheeled, zoom.get() * ev.scale),
                runtime_core::WheelKind::Scroll => set_zoom(st, &wheeled, zoom.get() * (1.0 - ev.delta_y * 0.002)),
                _ => {}
            }
            TouchResponse::CONSUMED
        })
    }
}

/// The zoom slider with − and + either side.
fn zoom_row(st: State, p: Picked) -> Element {
    let zoom = st.zoom;
    let (slid, minus, plus) = (p.clone(), p.clone(), p);
    let knob_style = move || StyleRules {
        position: Some(Position::Absolute),
        left: Some(px((zoom.get() - 1.0) / (MAX_ZOOM - 1.0) * TRACK - 8.0)),
        top: Some(px(4.0)),
        width: Some(px(16.0)),
        height: Some(px(16.0)),
        background: Some(col(theme::TEXT_STRONG)),
        pointer_events: Some(PointerEvents::None),
        ..rounded(8.0)
    };
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            align_items: Some(AlignItems::Center),
            gap: Some(px(10.0)),
            ..Default::default()
        }) {
            { round_button("−", move || set_zoom(st, &minus, zoom.get() / 1.25)) }
            view(style = StyleRules {
                position: Some(Position::Relative),
                width: Some(px(TRACK)),
                height: Some(px(24.0)),
                justify_content: Some(JustifyContent::Center),
                cursor: Some(Cursor::Pointer),
                ..Default::default()
            }) {
                view(style = StyleRules {
                    height: Some(px(4.0)),
                    background: Some(col(theme::BG_TERTIARY)),
                    pointer_events: Some(PointerEvents::None),
                    ..rounded(2.0)
                }) {}
                view(style = knob_style) {}
            }
            .on_touch(move |ev| {
                if matches!(ev.phase, TouchPhase::Began | TouchPhase::Moved) {
                    let t = (ev.position.x / TRACK).clamp(0.0, 1.0);
                    set_zoom(st, &slid, 1.0 + t * (MAX_ZOOM - 1.0));
                }
                TouchResponse::CLAIMED
            })
            { round_button("+", move || set_zoom(st, &plus, zoom.get() * 1.25)) }
        }
    }
}

pub(crate) fn dialog() -> Element {
    let st = state();
    let open = st.open;
    let error = st.error;
    let body = switch(
        move || open.get(),
        move |picked: &Option<Picked>| {
            let Some(p) = picked.clone() else {
                return ui! { view {} };
            };
            let circle = frame(st, p.clone());
            let slider = zoom_row(st, p);
            ui! {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Column),
                    align_items: Some(AlignItems::Center),
                    gap: Some(px(14.0)),
                    padding_top: Some(px(20.0)),
                    padding_bottom: Some(px(16.0)),
                    padding_left: Some(px(20.0)),
                    padding_right: Some(px(20.0)),
                    ..Default::default()
                }) {
                    text(style = StyleRules {
                        font_size: Some(px(18.0)),
                        font_weight: Some(FontWeight::SemiBold),
                        color: Some(col(theme::TEXT_STRONG)),
                        ..Default::default()
                    }) { "Position your picture".to_string() }
                    text(style = StyleRules { font_size: Some(px(13.0)), color: Some(col(theme::TEXT_MUTED)), ..Default::default() }) {
                        "Drag to move it. Scroll or use the slider to zoom.".to_string()
                    }
                    { circle }
                    { slider }
                    text(style = StyleRules { font_size: Some(px(12.0)), color: Some(col(theme::DANGER)), ..Default::default() }) {
                        move || error.get()
                    }
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Row),
                        align_self: Some(runtime_core::AlignSelf::Stretch),
                        justify_content: Some(JustifyContent::FlexEnd),
                        gap: Some(px(8.0)),
                        ..Default::default()
                    }) {
                        { button_view("Cancel", ButtonKind::Secondary, close) }
                        { button_view("Apply", ButtonKind::Primary, apply) }
                    }
                }
            }
        },
    );
    let card = ui! {
        view(style = StyleRules {
            width: Some(px(crate::app::mobile::fit_width(360.0))),
            background: Some(col(theme::BG_PRIMARY)),
            overflow: Some(Overflow::Hidden),
            ..rounded(8.0)
        }) {
            { body }
        }
        .on_touch(|_| TouchResponse::CONSUMED)
    };
    ui! {
        view(style = move || {
            let shown = open.get().is_some();
            StyleRules {
                position: Some(Position::Absolute),
                left: Some(px(0.0)),
                top: Some(px(0.0)),
                width: Some(if shown { pct(100.0) } else { px(0.0) }),
                height: Some(if shown { pct(100.0) } else { px(0.0) }),
                background: Some(col(theme::OVERLAY)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                overflow: Some(Overflow::Hidden),
                ..Default::default()
            }
        }) {
            { card }
        }
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Ended {
                close();
            }
            TouchResponse::CONSUMED
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo() -> Picked {
        Picked { url: String::new(), blob: String::new(), w: 4000.0, h: 3000.0 }
    }

    #[test]
    fn centred_at_zoom_one_shows_the_middle_square() {
        let (x, y, side) = crop_rect(&photo(), 1.0, (0.0, 0.0));
        assert_eq!((x.round(), y.round(), side.round()), (500.0, 0.0, 3000.0));
    }

    #[test]
    fn dragging_stops_at_the_picture_edge() {
        // At zoom 1 the 3000 px side fills the circle, so no vertical slack.
        let (x, y) = clamp_off(&photo(), 1.0, (10_000.0, 50.0));
        assert_eq!(x, (4000.0 * FRAME / 3000.0 - FRAME) / 2.0);
        assert_eq!(y, 0.0);
        let (x, _, _) = crop_rect(&photo(), 1.0, (x, y));
        assert_eq!(x.round(), 0.0, "dragged right as far as it goes shows the left edge");
    }

    #[test]
    fn zooming_in_shows_a_smaller_square() {
        let (_, _, side) = crop_rect(&photo(), 2.0, (0.0, 0.0));
        assert_eq!(side.round(), 1500.0);
    }
}
