//! The active server's custom emoji, and how they sit in text. A message or
//! reaction carries `<:name:id>`, so a rename shows everywhere; typing
//! `:name:` (or picking one) turns into that token when the message is sent,
//! and the edit box shows tokens as `:name:` again.

use std::cell::RefCell;
use std::rc::Rc;

use discord_server::server_emoji::{self as se, ServerEmoji};
use runtime_core::{ui, AlignItems, Element, FlexDirection, FlexWrap, FontWeight, ObjectFit, StyleRules, TouchPhase, TouchResponse};

use crate::style_helpers::{col, px, rounded};
use crate::theme;

thread_local! {
    static LIST: RefCell<Vec<ServerEmoji>> = const { RefCell::new(Vec::new()) };
}

/// Fetch `server_id`'s emoji, then redraw everything that shows names.
pub(crate) fn load(server_id: i64) {
    runtime_core::driver::spawn_async(async move {
        if let Ok(list) = se::server_emoji(se::ServerEmojiInput { server_id }).await {
            LIST.with(|l| *l.borrow_mut() = list);
            crate::mentions::names_changed();
        }
    });
}

/// The active server's emoji, by name.
pub(crate) fn list() -> Vec<ServerEmoji> {
    LIST.with(|l| l.borrow().clone())
}

fn by_id(id: i64) -> Option<ServerEmoji> {
    LIST.with(|l| l.borrow().iter().find(|e| e.id == id).cloned())
}

fn by_name(name: &str) -> Option<i64> {
    LIST.with(|l| {
        let l = l.borrow();
        l.iter()
            .find(|e| e.name == name)
            .or_else(|| l.iter().find(|e| e.name.eq_ignore_ascii_case(name)))
            .map(|e| e.id)
    })
}

/// The token a message or reaction carries for `e`.
pub(crate) fn token(e: &ServerEmoji) -> String {
    format!("<:{}:{}>", e.name, e.id)
}

/// A piece of message text.
#[derive(Debug, PartialEq)]
pub(crate) enum Piece {
    Text(String),
    Emoji { id: i64, name: String },
}

/// The token at the start of `s`: `(name, id, length)`.
fn parse_token(s: &str) -> Option<(&str, i64, usize)> {
    let rest = s.strip_prefix("<:")?;
    let colon = rest.find(':')?;
    let name = &rest[..colon];
    let after = &rest[colon + 1..];
    let close = after.find('>')?;
    let id = after[..close].parse::<i64>().ok()?;
    se::name_ok(name).then_some((name, id, 2 + colon + 1 + close + 1))
}

/// Split text into plain runs and emoji tokens.
pub(crate) fn split(text: &str) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut plain = String::new();
    let mut i = 0;
    while i < text.len() {
        if let Some((name, id, len)) = parse_token(&text[i..]) {
            if !plain.is_empty() {
                out.push(Piece::Text(std::mem::take(&mut plain)));
            }
            out.push(Piece::Emoji { id, name: name.to_string() });
            i += len;
            continue;
        }
        let c = text[i..].chars().next().unwrap();
        plain.push(c);
        i += c.len_utf8();
    }
    if !plain.is_empty() {
        out.push(Piece::Text(plain));
    }
    out
}

/// Apply `f` to the parts of `text` outside code (``` blocks and `spans`),
/// keeping the code as it is.
fn outside_code(text: &str, f: impl Fn(&str) -> String) -> String {
    let mut out = String::new();
    for (i, block) in text.split("```").enumerate() {
        if i > 0 {
            out.push_str("```");
        }
        if i % 2 == 1 {
            out.push_str(block);
            continue;
        }
        for (j, span) in block.split('`').enumerate() {
            if j > 0 {
                out.push('`');
            }
            out.push_str(&if j % 2 == 1 { span.to_string() } else { f(span) });
        }
    }
    out
}

/// `:name:` → its token, outside code, for names `lookup` knows. Existing
/// tokens are left alone.
pub(crate) fn encode(text: &str, lookup: impl Fn(&str) -> Option<i64>) -> String {
    outside_code(text, |s| {
        let mut out = String::new();
        let mut i = 0;
        while i < s.len() {
            if let Some((_, _, len)) = parse_token(&s[i..]) {
                out.push_str(&s[i..i + len]);
                i += len;
                continue;
            }
            if s[i..].starts_with(':') {
                if let Some(end) = s[i + 1..].find(':') {
                    let name = &s[i + 1..i + 1 + end];
                    if se::name_ok(name) {
                        if let Some(id) = lookup(name) {
                            out.push_str(&format!("<:{name}:{id}>"));
                            i += end + 2;
                            continue;
                        }
                    }
                }
            }
            let c = s[i..].chars().next().unwrap();
            out.push(c);
            i += c.len_utf8();
        }
        out
    })
}

/// A draft about to be sent, with the active server's emoji encoded.
pub(crate) fn encode_draft(text: &str) -> String {
    encode(text, by_name)
}

/// Tokens back to `:name:` (current names where known), for the edit box,
/// Markdown bodies and short excerpts.
pub(crate) fn plain(text: &str) -> String {
    split(text)
        .into_iter()
        .map(|p| match p {
            Piece::Text(t) => t,
            Piece::Emoji { id, name } => format!(":{}:", by_id(id).map(|e| e.name).unwrap_or(name)),
        })
        .collect()
}

/// The text left once tokens are taken out, and how many there were.
pub(crate) fn without_tokens(text: &str) -> (String, usize) {
    let mut rest = String::new();
    let mut count = 0;
    for p in split(text) {
        match p {
            Piece::Text(t) => rest.push_str(&t),
            Piece::Emoji { .. } => count += 1,
        }
    }
    (rest, count)
}

/// The server emoji a reaction or token names, if this server has it.
pub(crate) fn token_emoji(s: &str) -> Option<ServerEmoji> {
    let (_, id, len) = parse_token(s)?;
    (len == s.len()).then(|| by_id(id)).flatten()
}

/// One emoji image at `size` px, or its `:name:` when this server doesn't
/// have it.
pub(crate) fn emoji_view(id: i64, name: &str, size: f32) -> Element {
    match by_id(id) {
        Some(e) => ui! {
            image(
                src = crate::auth::media_url(e.image.clone()),
                alt = format!(":{}:", e.name),
                style = StyleRules {
                    width: Some(px(size)),
                    height: Some(px(size)),
                    object_fit: Some(ObjectFit::Contain),
                    // The spaces around it collapse in the flow.
                    margin_left: Some(px(3.0)),
                    margin_right: Some(px(3.0)),
                    ..Default::default()
                },
            )
        },
        None => {
            let label = format!(":{name}:");
            ui! { text(style = crate::styles::BodyText()) { label } }
        }
    }
}

/// The picker's row of this server's emoji; a tap picks the token.
pub(crate) fn picker_row(on_pick: Rc<dyn Fn(String)>) -> Element {
    let list = list();
    if list.is_empty() {
        return ui! { view {} };
    }
    let cells: Vec<Element> = list
        .iter()
        .map(|e| {
            let tok = token(e);
            let pick = on_pick.clone();
            ui! {
                view(style = StyleRules {
                    width: Some(px(36.0)),
                    height: Some(px(36.0)),
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(runtime_core::JustifyContent::Center),
                    ..rounded(6.0)
                }) {
                    image(
                        src = crate::auth::media_url(e.image.clone()),
                        alt = format!(":{}:", e.name),
                        style = StyleRules {
                            width: Some(px(28.0)),
                            height: Some(px(28.0)),
                            object_fit: Some(ObjectFit::Contain),
                            ..Default::default()
                        },
                    )
                }
                .on_touch(move |ev| {
                    if ev.phase == TouchPhase::Ended {
                        pick(tok.clone());
                    }
                    TouchResponse::CONSUMED
                })
            }
        })
        .collect();
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            margin_left: Some(px(12.0)),
            margin_right: Some(px(12.0)),
            margin_bottom: Some(px(8.0)),
            max_height: Some(px(120.0)),
            overflow: Some(runtime_core::Overflow::Hidden),
            ..Default::default()
        }) {
            text(style = StyleRules {
                font_size: Some(px(11.0)),
                font_weight: Some(FontWeight::Bold),
                color: Some(col(theme::TEXT_MUTED)),
                ..Default::default()
            }) { "THIS SERVER".to_string() }
            view(style = StyleRules {
                flex_direction: Some(FlexDirection::Row),
                flex_wrap: Some(FlexWrap::Wrap),
                ..Default::default()
            }) { cells }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(name: &str) -> Option<i64> {
        match name {
            "party" => Some(7),
            "cat_2" => Some(12),
            _ => None,
        }
    }

    #[test]
    fn split_finds_tokens() {
        assert_eq!(
            split("hi <:party:7>!<:cat_2:12>"),
            vec![
                Piece::Text("hi ".into()),
                Piece::Emoji { id: 7, name: "party".into() },
                Piece::Text("!".into()),
                Piece::Emoji { id: 12, name: "cat_2".into() },
            ]
        );
        assert_eq!(split("<:x:7> <:bad name:1> <:ok:abc>"), vec![Piece::Text("<:x:7> <:bad name:1> <:ok:abc>".into())]);
    }

    #[test]
    fn encode_skips_code_and_unknown_names() {
        assert_eq!(encode("yay :party: :nope: :cat_2:", lookup), "yay <:party:7> :nope: <:cat_2:12>");
        assert_eq!(encode("`:party:` and ```\n:party:\n``` :party:", lookup), "`:party:` and ```\n:party:\n``` <:party:7>");
        assert_eq!(encode("<:party:7> again", lookup), "<:party:7> again", "tokens stay as they are");
        assert_eq!(encode("10:30:45 ok", lookup), "10:30:45 ok");
    }

    #[test]
    fn tokens_counted_apart_from_text() {
        assert_eq!(without_tokens("<:party:7> <:cat_2:12>"), (" ".into(), 2));
        assert_eq!(without_tokens("text <:party:7>").1, 1);
        assert_eq!(parse_token("<:party:7>"), Some(("party", 7, 10)));
    }
}
