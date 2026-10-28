//! Message rendering: bodies (markdown, links, previews), rows, the overflow menu, edit row, attachments, grouping, and the live message buffer's insert.
//! Split out of `app.rs`; `use super::*` keeps app.rs's imports and shared
//! colors/helpers in scope.

use crate::theme;
use super::*;

/// Discord's message-grouping window: consecutive messages from the same
/// author closer together than this (seconds) collapse into one group —
/// avatar + name + timestamp on the first only, tightly-spaced body-only
/// rows after. Discord's own window is ~7 minutes (420s).
pub(super) const GROUP_WINDOW_SECS: i64 = 420;

/// "Does `cur` continue `prev`'s message group?" — same author and the gap
/// between the two instants inside `GROUP_WINDOW_SECS`. Used by the mock
/// channel, live channel, and both DM render paths (the list is always
/// oldest-first, so "previous" is the row rendered just above).
pub(crate) fn same_group(prev_author: &str, prev_ts: i64, cur_author: &str, cur_ts: i64) -> bool {
    prev_author == cur_author && cur_ts >= prev_ts && cur_ts - prev_ts < GROUP_WINDOW_SECS
}
/// The largest box a single attachment is shown in. Smaller images keep
/// their own size (never upscaled); larger ones shrink to fit, keeping their
/// shape.
const ATTACHMENT_MAX_W: f32 = 520.0;
const ATTACHMENT_MAX_H: f32 = 360.0;
/// With two or more images in one message, each fits a smaller square, so a
/// pair sits side by side.
const ATTACHMENT_GRID_MAX: f32 = 256.0;

/// The on-screen size of an image of `size` pixels inside a `max_w` x `max_h`
/// box: scaled down to fit, never up. An unknown size (a legacy attachment or
/// an older server) gets the whole box, and the image is letterboxed in it.
pub(super) fn attachment_box(size: Option<(u32, u32)>, max_w: f32, max_h: f32) -> (f32, f32) {
    match size {
        Some((w, h)) if w > 0 && h > 0 => {
            let (w, h) = (w as f32, h as f32);
            let scale = (max_w / w).min(max_h / h).min(1.0);
            ((w * scale).round().max(1.0), (h * scale).round().max(1.0))
        }
        _ => (max_w, max_h),
    }
}

/// The images attached to one live message, rendered as a row UNDER the
/// body and lined up with its text (the same 66px inset as the reaction
/// strip). Each image gets a definite box from the pixel size the server
/// reports (`attachment_box`), with `object_fit` Contain as a guard. The
/// size comes from the server because a reactive style fed from the image's
/// `on_load` natural size never took effect in the live app (the node
/// rendered 0x0).
pub(super) fn attachment_row(atts: &[discord_server::server_settings::MessageAttachment]) -> Element {
    let (max_w, max_h) = if atts.len() > 1 {
        (ATTACHMENT_GRID_MAX, ATTACHMENT_GRID_MAX)
    } else {
        (ATTACHMENT_MAX_W, ATTACHMENT_MAX_H)
    };
    let cells: Vec<Element> = atts.iter().map(|a| {
        let data = crate::auth::media_url(a.url.clone());
        let name = a.filename.clone();
        let (w, h) = attachment_box(a.width.zip(a.height), max_w, max_h);
        ui! {
            image(
                src = data,
                alt = name,
                style = StyleRules {
                    width: Some(px(w)),
                    height: Some(px(h)),
                    flex_shrink: Some(grow(0.0)),
                    object_fit: Some(ObjectFit::Contain),
                    border_top_left_radius: Some(px(8.0)),
                    border_top_right_radius: Some(px(8.0)),
                    border_bottom_left_radius: Some(px(8.0)),
                    border_bottom_right_radius: Some(px(8.0)),
                    ..Default::default()
                },
            )
        }
    }).collect();
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Row),
            flex_wrap: Some(FlexWrap::Wrap),
            align_items: Some(AlignItems::FlexStart),
            gap: Some(px(8.0)),
            margin_top: Some(px(4.0)),
            margin_bottom: Some(px(6.0)),
            padding_left: Some(px(66.0)),
            padding_right: Some(px(16.0)),
            ..Default::default()
        }) { cells }
    }
}

#[cfg(test)]
mod attachment_size_tests {
    use super::attachment_box;

    #[test]
    fn small_images_keep_their_size() {
        assert_eq!(attachment_box(Some((320, 180)), 520.0, 360.0), (320.0, 180.0));
        assert_eq!(attachment_box(Some((16, 16)), 520.0, 360.0), (16.0, 16.0));
    }

    #[test]
    fn large_images_shrink_to_fit_keeping_their_shape() {
        // Wide: limited by width.
        assert_eq!(attachment_box(Some((2600, 1300)), 520.0, 360.0), (520.0, 260.0));
        // Tall: limited by height.
        assert_eq!(attachment_box(Some((1000, 2000)), 520.0, 360.0), (180.0, 360.0));
        // A long thin strip never collapses to zero.
        assert_eq!(attachment_box(Some((8000, 4)), 520.0, 360.0), (520.0, 1.0));
    }

    #[test]
    fn unknown_size_fills_the_box() {
        assert_eq!(attachment_box(None, 520.0, 360.0), (520.0, 360.0));
        assert_eq!(attachment_box(Some((0, 10)), 256.0, 256.0), (256.0, 256.0));
    }
}

/// One buffered live (gateway-fed) message: the server's wire `message_id`,
/// then `author_id` + `author_label`, `content`, `created_at_secs`, and
/// `edited_at` (the `MessageEdited` timestamp in seconds, `None` until edited).
/// The leading fields are the mock display tuple; `message_id` + `edited_at` are
/// what the per-row edit/delete affordance and the "(edited)" marker need on
/// top of them (the mock path carries no wire id, so it renders `None`).
pub(super) type LiveMsg = (i64, i64, String, String, i64, Option<i64>, Option<i64>);

/// Insert a `LiveMsg` into a channel's buffer in (created_at_secs, message_id)
/// ascending order, skipping a duplicate `message_id` if one is already present
/// (dedup on absorption). Returns `true` if the message was added.
///
/// This keeps the buffer sorted oldest-first no matter which way a frame arrives:
/// a live `new: true` frame (the newest message) sorts to the end, and a
/// scrollback page — the server's `before_id` cursor returns strictly-older
/// messages — stacks in at the front, all in one call. It is the single write
/// path for `live_messages`, so the buffer's ordering invariant is maintained in
/// one place rather than assumed by every call site.
pub(super) fn insert_live_msg(
    buf: &mut Vec<LiveMsg>,
    msg: (i64, i64, String, String, i64, Option<i64>, Option<i64>),
) -> bool {
    let (id, ..) = msg;
    // Dedup: a `message_id` already in the buffer is a re-delivery (the server
    // may echo a History page that overlaps the live tail), so skip it.
    if buf.iter().any(|(e, ..)| *e == id) {
        return false;
    }
    // Binary-search the position by (created_at_secs, message_id), the same key
    // the server's `ORDER BY created_at ASC, id ASC` uses for a page. Each buffer
    // element's (created_at_secs, message_id) is compared as an owned tuple so the
    // closure's two sides have the same type.
    let pos = buf
        .partition_point(|e| (e.4, e.0) < (msg.4, msg.0))
        .min(buf.len());
    buf.insert(pos, msg);
    true
}

/// The "start of conversation" anchor for a live channel — a centered, muted
/// line (e.g. "Jump to the start of the #general conversation") shown ABOVE the
/// earliest loaded message. It is the empty-state's sibling for channels that
/// have content: where a zero-message channel shows a centered "No messages
/// yet -- say hi!", a one-message channel (or any freshly opened one) shows this
/// anchor as the first row so the pane reads as "you're at the top of a real
/// conversation" rather than a single orphaned message floating in a void.
///
/// It is a plain `fn -> Element` (not `#[component]`) like `live_edit_row` — it
/// takes only a channel name and a static style, so there is no props struct to
/// generate. `channel_name` is looked up by the caller from `live_channels`.
#[allow(non_snake_case)]
pub(super) fn channel_anchor(channel_name: &str) -> Element {
    ui! {
        view(style = StyleRules {
            align_items: Some(AlignItems::Center),
            padding_top: Some(px(16.0)),
            padding_bottom: Some(px(8.0)),
            ..Default::default()
        }) {
            view(style = StyleRules {
                width: Some(px(16.0)),
                height: Some(px(16.0)),
                align_items: Some(AlignItems::Center),
                justify_content: Some(JustifyContent::Center),
                background: Some(col(BG_ELEVATED)),
                border_top_left_radius: Some(px(8.0)),
                border_top_right_radius: Some(px(8.0)),
                border_bottom_left_radius: Some(px(8.0)),
                border_bottom_right_radius: Some(px(8.0)),
                ..Default::default()
            }) {
                text(style = StyleRules {
                    font_size: Some(px(12.0)),
                    color: Some(col(TEXT_MUTED)),
                    ..Default::default()
                }) { "#".to_string() }
            }
            text(style = StyleRules {
                margin_left: Some(px(8.0)),
                font_size: Some(px(13.0)),
                font_weight: Some(FontWeight::SemiBold),
                color: Some(col(TEXT_MUTED)),
                ..Default::default()
            }) { format!("You're looking at the beginning of the #{} conversation.", channel_name) }
        }
    }
}

/// Discord's date divider: a 1px line in the border color across the message
/// column with the calendar day ("October 20, 2026") centered ON the line —
/// muted ~12px semibold label sitting above the rule, 16px/8px vertical
/// margins, and the same 16px side padding as the message rows so the rule
/// spans exactly the message area. Pushed before the first message of each
/// local calendar day (see the live loop in `messages.rs`).
pub(super) fn date_divider(label: String) -> Element {
    ui! {
        view(style = crate::styles::DateDivider()) {
            text(style = crate::styles::DateDividerLabel()) { label }
            view(style = crate::styles::DateDividerRule())
        }
    }
}


/// The inline content of a message body, split on Discord spoiler markers.
/// Discord marks spoiler text with `||…||`; everything between a pair of
/// `||` is rendered through `SpoilerTag` (the blurred, click-to-reveal tag)
/// while the rest renders as plain text. The split runs are laid out in a
/// `flex_direction: Row` + `flex_wrap: Wrap` container so an inline spoiler
/// sits on the same line as the surrounding words and wraps like the rest of
/// the body. A body with no `||…||` marker renders exactly as one plain text
/// run (the previous behavior), so this is a content-dependent upgrade, not an
/// unconditional per-row widget.

/// Whether `c` draws as (part of) an emoji: the pictographic blocks, plus
/// the joiners, variation selectors, skin tones, keycap and tag characters
/// that glue a multi-codepoint emoji together.
fn is_emoji_char(c: char) -> bool {
    matches!(c as u32,
        0x1F000..=0x1FAFF   // pictographs, emoticons, transport, flags, symbols
        | 0x2600..=0x27BF   // misc symbols + dingbats (☀ ✨ ❤ …)
        | 0x2B05..=0x2B07 | 0x2B1B | 0x2B1C | 0x2B50 | 0x2B55
        | 0x200D | 0xFE0F | 0x20E3 // ZWJ, VS16, keycap
        | 0xE0020..=0xE007F)        // tag sequences (subdivision flags)
}

/// Split plain text into alternating (is_emoji, run) pieces.
pub(super) fn emoji_runs(text: &str) -> Vec<(bool, String)> {
    let mut out: Vec<(bool, String)> = Vec::new();
    for c in text.chars() {
        let e = is_emoji_char(c);
        match out.last_mut() {
            Some((kind, run)) if *kind == e => run.push(c),
            _ => out.push((e, c.to_string())),
        }
    }
    out
}

/// A body made only of emoji (and spaces): it gets the bigger size.
pub(super) fn is_emoji_only(text: &str) -> bool {
    let mut any = false;
    for c in text.chars() {
        if is_emoji_char(c) {
            any = true;
        } else if !c.is_whitespace() {
            return false;
        }
    }
    any
}

/// A mention inside a message: a tinted pill (a coloured role's pill takes
/// its colour). A member's pill opens their card.
fn mention_pill(seg: discord_server::server_roles::MentionSeg) -> Element {
    use discord_server::server_roles::MentionSeg;
    let (label, fg, bg, user) = match seg {
        MentionSeg::User { id, name } => {
            (format!("@{}", crate::auth::shown_name(&name)), col(theme::MENTION_TEXT), col(theme::MENTION_BG), Some(id))
        }
        MentionSeg::Role { name, color, .. } if !color.is_empty() => {
            (format!("@{name}"), theme::fixed(&color), theme::fixed(&crate::mentions::tint(&color, 0.12)), None)
        }
        MentionSeg::Role { name, .. } => (format!("@{name}"), col(theme::MENTION_TEXT), col(theme::MENTION_BG), None),
        MentionSeg::Everyone(w) => (format!("@{w}"), col(theme::MENTION_TEXT), col(theme::MENTION_BG), None),
        MentionSeg::Text(t) => (t, col(TEXT), col("transparent"), None),
    };
    let style = crate::styles::MentionPill().background(bg);
    let text_style = crate::styles::MentionPillText().color(fg);
    match user {
        Some(id) => ui! {
            view(style = style) { text(style = text_style) { label } }
            .on_touch(move |ev| {
                if ev.phase == TouchPhase::Ended {
                    let p = ev.window_position;
                    crate::mentions::open_user(id, p.x, p.y);
                }
                TouchResponse::CONSUMED
            })
        },
        None => ui! { view(style = style) { text(style = text_style) { label } } },
    }
}

/// A message that mentions the signed-in user: a warm tint and a bar on the
/// left, like Discord.
pub(super) fn mention_highlight(row: Element) -> Element {
    ui! {
        view(style = crate::styles::MentionHighlight()) { row }
    }
}

pub(super) fn message_body(body: &str) -> Element {
    // A body that is just a Giphy media link (a GIF sent from the picker, or
    // a pasted one) shows as the animated image, sized from the link's
    // `#WxH` fragment like an attachment.
    if let Some((url, size)) = discord_server::gifs::gif_body(body) {
        let (w, h) = attachment_box(size, ATTACHMENT_MAX_W, ATTACHMENT_MAX_H);
        return ui! {
            image(
                src = url,
                alt = "GIF".to_string(),
                style = StyleRules {
                    width: Some(px(w)),
                    height: Some(px(h)),
                    flex_shrink: Some(grow(0.0)),
                    object_fit: Some(ObjectFit::Contain),
                    margin_top: Some(px(2.0)),
                    border_top_left_radius: Some(px(8.0)),
                    border_top_right_radius: Some(px(8.0)),
                    border_bottom_left_radius: Some(px(8.0)),
                    border_bottom_right_radius: Some(px(8.0)),
                    ..Default::default()
                },
            )
        };
    }
    // A body that carries real Markdown syntax (emphasis, code, links, quotes,
    // lists…) renders the WHOLE thing through the `markdown` SDK — one scene
    // payload whose caps-generic handler builds the semantic DOM (styled runs,
    // `<pre>` blocks, `<blockquote>`). Bodies with no Markdown sigils — every
    // plain chat line, and every mock seed row — skip the SDK entirely and
    // keep the exact node shape they had before, so plain messages keep their
    // current metrics and grouped consecutive rows stay tight. A body
    // carrying `||…||` spoilers keeps the spoiler path (SDK v1 has no spoiler
    // construct); the two features never stack on one row.
    if !body.contains("||") && looks_like_markdown(&crate::mentions::without_mentions(body)) {
        return Element::from(markdown::markdown(linkify_bare_urls(body), md_chat_theme()));
    }
    // Split the body into `(is_spoiler, text)` runs. A `||…||` pair becomes a
    // spoiler run; the surrounding slices become plain runs. We walk the body
    // by finding each marker pair in turn so the runs stay in document order.
    let mut runs: Vec<(bool, String)> = Vec::new();
    let chars: Vec<char> = body.chars().collect();
    let len = chars.len();
    let mut i = 0usize;
    let mut plain = String::new();
    while i < len {
        if i + 1 < len && chars[i] == '|' && chars[i + 1] == '|' {
            // Opening marker: flush the accumulated plain run, then look for
            // the closing `||` after it.
            if !plain.is_empty() {
                runs.push((false, std::mem::take(&mut plain)));
            }
            let start = i + 2;
            let mut close = None;
            let mut j = start;
            while j + 1 < len {
                if chars[j] == '|' && chars[j + 1] == '|' {
                    close = Some(j);
                    break;
                }
                j += 1;
            }
            match close {
                Some(c) => {
                    // Spoiler body is the slice between the two markers.
                    let inner: String = chars[start..c].iter().collect();
                    runs.push((true, inner));
                    i = c + 2;
                }
                None => {
                    // Unmatched marker: treat the `||` as literal text.
                    plain.push('|');
                    plain.push('|');
                    i += 2;
                }
            }
        } else {
            plain.push(chars[i]);
            i += 1;
        }
    }
    if !plain.is_empty() {
        runs.push((false, plain));
    }
    if runs.is_empty() {
        // Empty body: render an empty string (keeps the row shape stable).
        runs.push((false, String::new()));
    }

    let emoji_size = if is_emoji_only(body) { crate::styles::BodyTextSize::EmojiOnly } else { crate::styles::BodyTextSize::Emoji };
    let runs_el: Vec<Element> = runs
        .iter()
        .flat_map(|(is_spoiler, text)| -> Vec<Element> {
            if *is_spoiler {
                // The click-to-reveal spoiler. The generated component fn takes
                // a `Reactive<String>` `text` prop; `String: Into<Reactive<String>>`
                // (the `From<T> for Reactive<T>` impl), so `…into()` bakes a
                // static prop. (The `text = …` named-prop form only parses
                // inside `ui!`; here we call the fn directly, positionally.)
                vec![ui! {
                    view(style = StyleRules {
                        margin_left: Some(px(2.0)),
                        margin_right: Some(px(2.0)),
                        ..Default::default()
                    }) {
                        { crate::spoiler_tag::SpoilerTag(text.to_string().into()) }
                    }
                }]
            } else {
                // Plain run: inline text nodes in the same flow as the
                // spoilers, so the row stays on one visual line when it fits.
                // Emoji pieces get the bigger size.
                // Emoji pieces get the bigger size; mentions become pills.
                crate::mentions::segments(text)
                    .into_iter()
                    .flat_map(|seg| -> Vec<Element> {
                        match seg {
                            discord_server::server_roles::MentionSeg::Text(t) => emoji_runs(&t)
                                .into_iter()
                                .map(|(is_emoji, piece)| ui! {
                                    text(style = crate::styles::BodyText().size(if is_emoji { emoji_size } else { crate::styles::BodyTextSize::Text })) { piece }
                                })
                                .collect(),
                            mention => vec![mention_pill(mention)],
                        }
                    })
                    .collect()
            }
        })
        .collect();

    ui! {
        view(style = crate::styles::MessageBodyFlow()) { runs_el }
    }
}

#[cfg(test)]
mod emoji_size_tests {
    use super::{emoji_runs, is_emoji_only};

    #[test]
    fn emoji_split_from_text_and_multi_codepoint_emoji_stay_whole() {
        assert_eq!(
            emoji_runs("hi 👋 there"),
            vec![(false, "hi ".to_string()), (true, "👋".to_string()), (false, " there".to_string())]
        );
        // A ZWJ family and a flag each stay one emoji run.
        assert_eq!(emoji_runs("👨‍👩‍👧"), vec![(true, "👨‍👩‍👧".to_string())]);
        assert_eq!(emoji_runs("🇺🇸!"), vec![(true, "🇺🇸".to_string()), (false, "!".to_string())]);
    }

    #[test]
    fn emoji_only_means_nothing_but_emoji_and_spaces() {
        assert!(is_emoji_only("😂"));
        assert!(is_emoji_only(" 🔥 ❤️ "));
        assert!(!is_emoji_only("lol 😂"));
        assert!(!is_emoji_only("   "));
        assert!(!is_emoji_only("plain text"));
    }
}

/// Wrap whitespace-delimited tokens starting with `http://`/`https://` in
/// `<…>` autolinks so the SDK renders them as links (Discord linkifies bare
/// URLs; the SDK parser enables strikethrough but not GFM autolinking, and
/// `<url>` is plain CommonMark autolink syntax). URLs inside `[t](u)` or an
/// existing `<u>` never start a token with the scheme, so they're untouched;
/// trailing sentence punctuation is moved outside the link marker. Code is
/// protected per-region, not whole-body: inline `…` spans and ``` fenced blocks
/// pass through verbatim while URLs outside them still linkify, so "`code`
/// https://x" links but "`https://x`" doesn't.
pub(super) fn linkify_bare_urls(body: &str) -> String {
    let bytes = body.as_bytes();
    let n = bytes.len();
    let is_ws = |c: u8| c == b' ' || c == b'\t' || c == b'\n' || c == b'\r';
    let is_tail_punct = |c: u8| c == b'.' || c == b',' || c == b'!' || c == b'?'
        || c == b';' || c == b':' || c == b'\'' || c == b'"';
    // True when the line starting at `pos` opens with ``` (the fence marker),
    // after up to 3 leading spaces (CommonMark's indent allowance).
    let fence_at_line_start = |pos: usize| -> bool {
        let mut j = pos;
        let mut spaces = 0;
        while j < n && spaces < 4 && (bytes[j] == b' ' || bytes[j] == b'\t') {
            j += 1;
            spaces += 1;
        }
        j + 2 < n && bytes[j] == b'`' && bytes[j + 1] == b'`' && bytes[j + 2] == b'`'
    };
    // Index just past the newline ending the line at `pos` (or end of body).
    let line_end = |pos: usize| -> usize {
        if pos >= n {
            return n;
        }
        match body[pos..].find('\n') {
            Some(off) => pos + off + 1,
            None => n
        }
    };
    let mut out = String::new();
    let mut i = 0;
    while i < n {
        let line_start = i == 0 || bytes[i - 1] == b'\n';
        // Fenced code block: copy the opening fence line through the closing
        // fence line verbatim, then resume linkifying after it.
        if line_start && fence_at_line_start(i) {
            let open_end = line_end(i);
            out += &body[i..open_end];
            i = open_end;
            while i < n {
                let next_start = i == 0 || bytes[i - 1] == b'\n';
                if next_start && fence_at_line_start(i) {
                    let close_end = line_end(i);
                    out += &body[i..close_end];
                    i = close_end;
                    break;
                }
                // interior line: verbatim
                let end = line_end(i);
                out += &body[i..end];
                i = end;
            }
            continue;
        }
        // Inline code span: from this backtick through the NEXT backtick on
        // the same line, verbatim. An unmatched backtick falls through to the
        // token loop below and renders as-is, same as pulldown-cmark does.
        if bytes[i] == b'`' {
            let mut k = i + 1;
            while k < n && bytes[k] != b'`' && bytes[k] != b'\n' {
                k += 1;
            }
            if k < n && bytes[k] == b'`' {
                out += &body[i..k + 1];
                i = k + 1;
                continue;
            }
        }
        if is_ws(bytes[i]) {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }
        let start = i;
        while i < n && !is_ws(bytes[i]) {
            i += 1;
        }
        let tok = &body[start..i];
        if tok.starts_with("http://") || tok.starts_with("https://") {
            let scheme_len = if tok.starts_with("https://") { 8 } else { 7 };
            let mut end = i - start;
            while end > scheme_len && is_tail_punct(bytes[start + end - 1]) {
                end -= 1;
            }
            out += "<";
            out += &tok[..end];
            out += ">";
            out += &tok[end..];
        } else {
            out += tok;
        }
    }
    out
}

#[cfg(test)]
mod linkify_tests {
    use super::*;

    /// A bare URL in prose is wrapped in `<…>` autolink syntax.
    #[test]
    fn plain_url_gets_linkified() {
        assert_eq!(linkify_bare_urls("see https://example.com now"),
            "see <https://example.com> now");
    }

    /// Inline code does not veto linkification for the whole body: the
    /// `` `code` `` span passes through verbatim and the URL after it links.
    #[test]
    fn url_outside_inline_code_links() {
        assert_eq!(linkify_bare_urls("`code` https://example.com"),
            "`code` <https://example.com>");
    }

    /// A URL INSIDE an inline code span stays untouched (no markers inserted).
    #[test]
    fn url_inside_inline_code_stays_untouched() {
        assert_eq!(linkify_bare_urls("run `https://example.com` here"),
            "run `https://example.com` here");
    }

    /// A URL inside a ``` fenced block stays untouched — verbatim through the
    /// fence — while a URL after the closing fence still links.
    #[test]
    fn url_inside_fenced_block_stays_untouched() {
        assert_eq!(linkify_bare_urls("```\nhttps://inside.example\n```\nafter https://outside.example"),
            "```\nhttps://inside.example\n```\nafter <https://outside.example>");
    }

    /// Trailing sentence punctuation sits OUTSIDE the autolink marker.
    #[test]
    fn trailing_punctuation_trimmed() {
        assert_eq!(linkify_bare_urls("https://example.com."),
            "<https://example.com>.");
        assert_eq!(linkify_bare_urls("go https://example.com, ok"),
            "go <https://example.com>, ok");
    }
}

/// Chat-body theme for the `markdown` SDK: the dark Discord palette at the
/// message-body text size (15px). `code_bg` sits a hair below `BG_MAIN`
/// (#313338) so inline code reads as a subtle chip rather than a boxy
/// highlight; links use Discord's blurple (`BUTTON`).
pub(super) fn md_chat_theme() -> markdown::MdTheme {
    markdown::MdTheme {
        text: TEXT.to_string(),
        muted: TEXT_MUTED.to_string(),
        heading: TEXT.to_string(),
        link: BUTTON.to_string(),
        code_fg: TEXT.to_string(),
        code_bg: theme::BG_TERTIARY.to_string(),
        quote_fg: TEXT_CHANNEL.to_string(),
        base_size: 15.0,
        heading_scale: [1.6, 1.3, 1.15, 1.05, 1.0, 0.9],
        mono_family: None,
    }
}

/// Cheap super-set test for Markdown worth routing through the SDK: does the
/// body contain an inline sigil (emphasis/code/strike/link brackets) or open
/// a line with a block construct (heading, quote, list bullet, rule)? False
/// positives (a stray `_`, an `[` in prose) are harmless — pulldown-cmark
/// needs well-formed spans, so plain-looking text parses back to one plain
/// run and renders at the same size/color as the `text` node it replaces.
pub(super) fn looks_like_markdown(body: &str) -> bool {
    if body.contains("**")
        || body.contains("__")
        || body.contains("_")
        || body.contains("*")
        || body.contains("~~")
        || body.contains("`")
        || body.contains("](")
        || body.contains("[")
        || body.contains("http://")
        || body.contains("https://")
    {
        return true;
    }
    for line in body.split('\n') {
        let t = line.trim_start();
        if t.starts_with("#") || t.starts_with(">") || t.starts_with("- ")
            || t.starts_with("+ ") || t.starts_with("---")
        {
            return true;
        }
        // Ordered list marker: `1. ` etc. (a digit run then `. `). ASCII, so
        // the byte slice offset equals the char count.
        let ds: Vec<char> = t.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !ds.is_empty() && t[ds.len()..].starts_with(". ") {
            return true;
        }
    }
    false
}

/// Extract a `LinkEmbedPreview` prop pair `(host, mock_title)` from a message
/// body that contains a URL-looking string (`http://…` / `https://…`). Returns
/// `None` for a body with no URL — the caller then renders no preview at all,
/// so this is a content-dependent upgrade (only messages with a link get a
/// card), never a per-row widget. The host is parsed straight from the URL;
/// the title is a small deterministic mock derived from the URL's last path
/// segment (there is no real fetch), so it varies per message.
pub(super) fn link_preview_props(body: &str) -> Option<(String, String)> {
    // Locate the URL substring: from the scheme to the next whitespace (or end).
    let s = body
        .find("http://")
        .or_else(|| body.find("https://"))?;
    let e = body[s..]
        .find(char::is_whitespace)
        .map(|i| s + i)
        .unwrap_or(body.len());
    let url = &body[s..e];
    // Drop the scheme to get "host[/path…][?query…]".
    let after = url.splitn(2, "://").nth(1).unwrap_or("");
    // Host = up to the first `/`, `?`, or `:` (port).
    let host = after
        .split(|c| c == '/' || c == '?' || c == ':')
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    if host.is_empty() {
        return None;
    }
    // Path = everything after the first `/`; the LAST non-empty segment is the
    // mock title source (title-cased) — for `…/owner/repo/…` that's the
    // resource name (Discord's card shows the page's own title, and the repo
    // path is the closest stand-in). `.rev().find` walks from the end, so
    // trailing query/empty segments are skipped. Falls back to a generic label.
    let path = after.splitn(2, "/").nth(1).unwrap_or("");
    let last = path
        .split('/')
        .rev()
        .find(|seg| !seg.trim().is_empty())
        .unwrap_or("");
    let title = if last.trim().is_empty() {
        "Linked page".to_string()
    } else {
        let clean = last.trim();
        let mut chars = clean.chars();
        match chars.next() {
            Some(c) => c.to_uppercase().to_string() + chars.as_str(),
            None => clean.to_string(),
        }
    };
    Some((host, title))
}

/// One message row: avatar chip, then author + "Today at …" line, then body.
/// Takes `&str`s so it serves both the static seed data and runtime-authored
/// messages (the strings are copied into the node, so the returned `Element`
/// is self-contained). `edited` gates the "(edited)" marker: live rows pass
/// the message's real `edited_at.is_some()`; the mock path passes `true` to
/// preserve its previous (always-on placeholder) rendering. `grouped` is the
/// Discord message-grouping form: the row continues the previous message's
/// group (same author within `GROUP_WINDOW_SECS`), so it renders no avatar
/// and no author/time header — just the body, tightly spaced (3px instead of
/// 8px above) and left-aligned under where the name starts (66px = 16px pad
/// + 40px avatar + 10px gap), with no right margin (a body that wide is
/// full-width by construction). `same_group` computes the flag at each call
/// site.
/// A press handler for a message author's avatar and name (the live channel
/// passes one that opens the profile card / user menu; DM and demo rows pass
/// `None`).
pub(crate) type AuthorPress = Rc<dyn Fn(&TouchEvent) -> TouchResponse>;

pub(crate) fn message_row(
    author: &str,
    color: &str,
    body: &str,
    when: String,
    full: String,
    edited: bool,
    grouped: bool,
    on_author: Option<AuthorPress>,
) -> Element {
    let name_color = match crate::mentions::name_color(author) {
        Some(c) => theme::fixed(&c),
        None => col(TEXT),
    };
    let initial = author.chars().next().unwrap_or(' ').to_string();
    // The author's avatar chip: the uploaded image (round-cropped, cover-fit)
    // when the shared client avatar registry (`crate::auth::avatar_for`, fed
    // by login / `list_members` / gateway frames) knows a URL for this author,
    // else the pre-existing colored initial circle. Mock identities are never
    // registered, so guest/demo rows render exactly as before.
    let avatar_chip = if let Some(url) = crate::auth::avatar_for(author) {
        let url = crate::auth::media_url(url.clone());
        ui! {
            image(
                src = url,
                alt = format!("{} avatar", author),
                style = crate::styles::Avatar40(),
            )
        }
    } else {
        ui! {
            view(style = crate::styles::Avatar40().background(col(color))) {
                text(style = crate::styles::AvatarInitial()) { initial }
            }
        }
    };
    if grouped {
        // Grouped form (Discord convention): no avatar, no author/time
        // header. Just the body, tightly spaced above the row it follows
        // (3px vs the 8px of a fresh header row) and left-aligned under
        // where the name would start: 16px pad + 40px avatar + 10px gap.
        // No right padding — at this width the body is full-width anyway.
        return ui! {
            view(style = crate::styles::MessageGroupedBox()) {
                // `message_body` renders markdown-bearing bodies through the
                // SDK; a plain body is one `text` run at the same 15px/TEXT
                // styling, so grouped consecutive rows keep their tight look.
                { message_body(body) }
                if edited { EditedLabel() }
            }
        };
    }
    // Content-dependent link preview: only a body that actually contains a
    // `http(s)://…` string gets a `LinkEmbedPreview` card rendered below it.
    // `link_preview_props` returns `None` for a URL-less body, so no card is
    // rendered for the other rows — this is a deliberate single instance on
    // the one link-bearing seed message, NOT a per-row widget (the graveyard
    // anti-pattern). `site_name` + `title` are parsed from the URL; the
    // description is a small mock line.
    let link = if discord_server::gifs::gif_body(body).is_some() { None } else { link_preview_props(body) };
    // With a press handler the avatar and the name open the author's
    // profile card (click) or user menu (right-click).
    let (avatar_el, name_el): (Element, Element) = match on_author {
        Some(press) => {
            let press2 = press.clone();
            (
                ui! { view { { avatar_chip } }.on_touch(move |ev| (press)(ev)) },
                ui! { view { text(style = crate::styles::AuthorName().color(name_color)) { crate::auth::shown_name(author) } }.on_touch(move |ev| (press2)(ev)) },
            )
        }
        None => (avatar_chip, ui! { text(style = crate::styles::AuthorName().color(name_color)) { crate::auth::shown_name(author) } }),
    };
    ui! {
        view(style = crate::styles::MessageRowBox()) {
            { avatar_el }
            view(style = crate::styles::MessageStack()) {
                view(style = crate::styles::MessageMeta()) {
                    { name_el }
                    if author == "miri" { VerifiedBadge() }
                    // The relative "when" label is a real hover target now:
                    // `message_timestamp_tooltip` shows the short label at rest
                    // and the full absolute date/time (`full`) while hovered.
                    crate::message_timestamp_tooltip::message_timestamp_tooltip(when, full)
                }
                // The body, split on `||…||` spoiler markers: plain runs render
                // as text and each spoiler run as a `SpoilerTag` (blurred,
                // click-to-reveal). Content-dependent — a marker-less body is a
                // single plain run (the previous rendering).
                { message_body(body) }
                if edited { EditedLabel() }
                // The link preview card, only when the body carries a URL.
                if let Some((host, title)) = &link {
                    crate::link_embed_preview::LinkEmbedPreview(
                        host.clone().into(),
                        title.clone().into(),
                        "Shared a link — preview shown automatically.".to_string().into(),
                    )
                }
            }
        }
    }
}

// --- per-row hover action bar + overflow menu (channel message list) --------
// These wrap `message_row` for the CHANNEL message list only (the mock rows
// that also carry a `reaction_area` + the shared reaction `picker`). The wrapper
// adds a `Position::Relative` box around the base row, drives a per-row `hovered`
// signal from the box's `on_hover` (the voice-channel-row idiom), and floats the
// `MessageActionBar` over the row's top-right. DM rows and the live/edit rows
// call `message_row` directly and never reach this, so they keep rendering
// exactly as before (no per-row controls added to a loop that shouldn't have them).

/// The small overflow menu that the action bar's "more" button opens. Rendered
/// only while `open` is true (the `switch` primitive, same show/hide idiom as
/// `MessageActionBar`); it drops just below the bar. `on_pin` shows the pin
/// toast; `on_translate` reveals the message's translate control; every entry
/// closes the menu on tap.
pub(super) fn message_overflow_menu(
    open: super::popups::MenuHandle,
    on_copy: Rc<dyn Fn()>,
    on_pin: Rc<dyn Fn()>,
    on_translate: Rc<dyn Fn()>,
    // Optional owner-only entries (Edit / Delete) — the caller passes them only
    // for the current user's own messages (live rows, `is_mine`); mock rows
    // pass `None` and the menu keeps its two shared entries. When present they
    // render as the menu's last two rows, so the whole message-action set
    // (react / reply / copy / pin / translate / edit / delete) lives in this one
    // hover-gated surface instead of a separate always-visible inline bar.
    on_edit: Option<Rc<dyn Fn()>>,
    on_delete: Option<Rc<dyn Fn()>>,
) -> Element {
    switch(
        move || open.get(),
        move |show: &bool| {
            if !show {
                return ui! {
                    view(style = StyleRules {
                        width: Some(px(0.0)),
                        height: Some(px(0.0)),
                        ..Default::default()
                    }) {}
                };
            }
            let pin = on_pin.clone();
            let translate = on_translate.clone();
            let edit = on_edit.clone();
            let delete = on_delete.clone();
            // A generic tap row: one `Rc<dyn Fn()>` per entry, fired on touch
            // end, then the menu closes. (The earlier `MoreMenuAction` enum
            // hard-coded the two shared actions; Edit/Delete arrived after, so
            // the row now takes its handler directly.)
            let menu_row = |label: &'static str, action: Rc<dyn Fn()>, open: super::popups::MenuHandle| -> Element {
                let row_style = StyleRules {
                    padding_top: Some(px(8.0)),
                    padding_bottom: Some(px(8.0)),
                    padding_left: Some(px(12.0)),
                    padding_right: Some(px(12.0)),
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    ..Default::default()
                };
                let label_style = StyleRules {
                    font_size: Some(px(14.0)),
                    color: Some(col(TEXT)),
                    ..Default::default()
                };
                let open_c = open;
                let touch = move |ev: &runtime_shared::TouchEvent| {
                    if ev.phase == TouchPhase::Ended {
                        action();
                        open_c.set(false);
                    }
                    TouchResponse::CONSUMED
                };
                ui! {
                    view(style = row_style) {
                        text(style = label_style) { label.to_string() }
                    }
                    .on_touch(touch)
                }
            };
            ui! {
                view(style = StyleRules {
                    position: Some(Position::Absolute),
                    top: Some(px(40.0)),
                    right: Some(px(12.0)),
                    width: Some(px(190.0)),
                    z_index: Some(super::popups::POPUP_Z),
                    flex_direction: Some(FlexDirection::Column),
                    // Clip, don't scroll: this is a short fixed list (3–5 rows)
                    // that must never scroll; `hidden` keeps the surface from
                    // showing a scrollbar if a row ever wraps past the box.
                    overflow: Some(Overflow::Hidden),
                    background: Some(col(theme::BG_FLOATING)),
                    border_top_left_radius: Some(px(8.0)),
                    border_top_right_radius: Some(px(8.0)),
                    border_bottom_left_radius: Some(px(8.0)),
                    border_bottom_right_radius: Some(px(8.0)),
                    border_top_width: Some(border_width(1.0)),
                    border_right_width: Some(border_width(1.0)),
                    border_bottom_width: Some(border_width(1.0)),
                    border_left_width: Some(border_width(1.0)),
                    border_top_color: Some(col(BORDER)),
                    border_right_color: Some(col(BORDER)),
                    border_bottom_color: Some(col(BORDER)),
                    border_left_color: Some(col(BORDER)),
                    ..Default::default()
                }) {
                    { menu_row("Copy text", on_copy.clone(), open) }
                    { menu_row("Pin message", pin.clone(), open) }
                    { menu_row("Translate message", translate, open) }
                    // Owner-only entries: present only when the caller supplied
                    // handlers (live rows the current user owns). Each renders a
                    // zero-size placeholder otherwise so the branch still yields
                    // an `Element` for the `ui!` child slot.
                    {
                        if let Some(edit) = edit {
                            menu_row("Edit", edit, open)
                        } else {
                            ui! {
                                view(style = StyleRules {
                                    width: Some(px(0.0)),
                                    height: Some(px(0.0)),
                                    ..Default::default()
                                }) {}
                            }
                        }
                    }
                    {
                        if let Some(delete) = delete {
                            menu_row("Delete", delete, open)
                        } else {
                            ui! {
                                view(style = StyleRules {
                                    width: Some(px(0.0)),
                                    height: Some(px(0.0)),
                                    ..Default::default()
                                }) {}
                            }
                        }
                    }
                }
            }
        },
    )
}

/// A channel message row with the hover `MessageActionBar` + overflow menu.
/// Wraps `message_row` (the base content) in a `Position::Relative` box and
/// floats the action bar over the row's top-right while hovered. React points
/// the SHARED reaction `picker` at this row (the same picker `reaction_area`'s
/// add button opens — no second reaction UI); Reply populates the composer's
/// `reply_ctx` (read by `ReplyPreviewBar`); More opens `message_overflow_menu`
/// whose "Pin message" shows `PinConfirmToast`.
pub(super) fn message_row_with_actions(
    author: &str,
    color: &str,
    body: &str,
    when: String,
    full: String,
    edited: bool,
    grouped: bool,
    key: MessageKey,
    picker: Signal<Option<MessageKey>>,
    reply_ctx: Signal<Option<(String, String)>>,
    // The wire id of the message being replied to. `on_reply` (below) sets it
    // from this row's `key` (a live row carries its real id; a mock row has
    // none), so the composer can attach it to the outbound `Send` frame. The
    // display half (author + snippet) lives in `reply_ctx`, read by the
    // preview bar; this carries only the id the server needs.
    reply_target: Signal<Option<i64>>,
    _pin_visible: Signal<bool>,
    translate_key: Signal<Option<MessageKey>>,
    on_pin: Rc<dyn Fn()>,
    // Optional owner-only menu entries (Edit / Delete). The caller passes them
    // only for the current user's own messages (live rows, `is_mine`); mock
    // rows pass `None` and the menu keeps its three shared entries. When
    // present they render as the menu's last two rows, so the whole
    // message-action set lives in this one hover-gated surface.
    on_edit: Option<Rc<dyn Fn()>>,
    on_delete: Option<Rc<dyn Fn()>>,
    on_author: Option<AuthorPress>,
) -> Element {
    let hovered = signal(false);
    let menu_open = super::popups::MenuHandle { key };
    let base = message_row(author, color, body, when, full, edited, grouped, on_author);

    // Reply context: author name + a short snippet of the body. `Signal` is
    // `Copy`, so each `move` handler captures its own copy; the `String`s are
    // cloned per press so the handlers stay `Fn`.
    let author_owned = author.to_string();
    let snippet: String = if discord_server::gifs::gif_body(body).is_some() {
        "GIF".to_string()
    } else {
        let s: String = body.chars().take(48).collect();
        if body.chars().count() > 48 {
            format!("{s}…")
        } else {
            s
        }
    };
    let reply_c = reply_ctx;
    let author_r = author_owned.clone();
    let snippet_r = snippet.clone();
    // The reply target's wire id, derived from this row's `key`: a live row
    // carries its real id, a mock row has none. Recorded so the composer's
    // outbound `Send` frame can attach it. `key` is `Copy`, so the handler
    // captures its own copy.
    let reply_t = reply_target;
    let key_r = key;
    let on_reply = Rc::new(move || {
        reply_c.set(Some((author_r.clone(), snippet_r.clone())));
        let id = match key_r {
            MessageKey::Live { id } => id,
        };
        reply_t.set(Some(id));
    });

    // React: point the shared picker at this row's `MessageKey` — the exact
    // entry point `reaction_area`'s add button uses. No second UI.
    let picker_c = picker;
    let on_react = Rc::new(move || {
        picker_c.set(Some(key));
    });

    // More: toggle the overflow menu. The reaction picker's full-window
    // backdrop paints over the row area, so opening the menu while the picker
    // is up would bury the menu's rows (the Edit row intercepted by the
    // picker in the last test) — close the picker as the menu opens.
    let menu_c = menu_open;
    let on_more = Rc::new(move || {
        let opening = !menu_c.get();
        menu_c.set(opening);
        if opening {
            picker_c.set(None);
        }
    });

    // "Pin message" (from the overflow menu): the caller supplies `on_pin`. The
    // MOCK rows flash the "Message pinned!" toast (and auto-dismiss it); the
    // LIVE rows call the backend's real `pin_message` RPC (see the live row
    // builder, which also updates the header pin count). The toast's own "x" is
    // the manual dismiss on the mock path.

    // "Translate message" (from the overflow menu): reveal this row's
    // `TranslateToggle` below the message. Content-agnostic — any message can
    // be translated, so the control is a per-row OPTION (the overflow-menu
    // entry), not a persistent widget: the toggle card only mounts while this
    // row's key is the active one. `translate_key` is the shared "which row is
    // translating" signal (the picker idiom), so opening another row's menu
    // and dismissing both close it. Re-tapping "Translate message" on the same
    // row toggles it off.
    let translate_key_c = translate_key;
    let my_key = key;
    let on_translate = Rc::new(move || {
        let k = translate_key_c;
        k.set(if k.get() == Some(my_key) { None } else { Some(my_key) });
    });

    // "Copy text" (from the overflow menu): write this row's message body to
    // the system clipboard. The clipboard SDK (`clipboard::set_text`) is async
    // and per-backend (navigator.clipboard on web, UIPasteboard on Apple,
    // ClipboardManager on Android, arboard on Linux) — the same idiom the
    // `InviteLinkDisplay` Copy button uses — so the work is spawned via
    // `runtime_core::driver::spawn_async` and the menu closes regardless
    // (a failed write is silent, matching the invite-card's no-lie behavior).
    // `body` is an owned clone so the handler is `Fn` across presses.
    let body_owned = body.to_string();
    let on_copy = Rc::new(move || {
        let text = body_owned.clone();
        runtime_core::driver::spawn_async(async move {
            let _ = clipboard::set_text(&text).await;
        });
    });

    let bar = crate::message_action_bar::message_action_bar(hovered, on_react, on_reply, on_more);
    let menu = message_overflow_menu(menu_open, on_copy, on_pin, on_translate, on_edit, on_delete);

    // The translate control: `original` is the message body; `translated` is a
    // mock rendering (this clone has no translation backend — the toggle
    // demonstrates the reveal/dismiss pattern, and the line visibly swaps).
    // Constructed via the generated props (`.into()` → `Reactive::Static`);
    // the card only renders while this row is the active translator, so it is
    // a deliberate single instance, never spliced into every row.
    let orig = body.to_string();
    let trans = format!("Translated (mock): {body}");
    let MessageKey::Live { id: row_id } = key;
    let jumped_to = super::jump::target() == Some(row_id);

    let row = ui! {
        view(style = StyleRules {
            position: Some(Position::Relative),
            flex_direction: Some(FlexDirection::Column),
            ..Default::default()
        }) {
            { base }
            { bar }
            { menu }
            if translate_key.get() == Some(key) {
                view {
                    crate::translate_toggle::TranslateToggle(orig.clone().into(), trans.clone().into())
                }
            }
        }
        .on_hover(move |on| hovered.set(on))
        // Right-click anywhere on the message opens its menu (the same one
        // the hover bar's "more" button opens). Other presses pass through.
        .on_touch(move |ev| {
            if ev.phase == TouchPhase::Began
                && runtime_shared::touch::pointer_button().opens_context_menu()
            {
                hovered.set(true);
                menu_open.set(true);
                return TouchResponse::CONSUMED;
            }
            TouchResponse::IGNORED
        })
    };
    // The message a search result jumped to: highlighted, and measured so
    // the list can scroll to it.
    if jumped_to {
        ui! {
            view(style = StyleRules { background: Some(col(theme::MENTION_ROW)), ..Default::default() }) {
                row
            }
            .bind(super::jump::row_ref())
        }
    } else {
        row
    }
}

/// The composer's "Replying to …" strip: wraps `reply_preview_bar` and backs
/// its "×" cancel with a callback that clears BOTH the shared `reply_ctx`
/// signal (the display author + snippet) and the `reply_target` signal (the
/// wire id). Each is `Copy`, so the `on_cancel` closure captures its own
/// handle; the bar itself reads a separate copy of `reply_ctx` (same signal).
pub(super) fn reply_preview(
    reply_ctx: Signal<Option<(String, String)>>,
    reply_target: Signal<Option<i64>>,
) -> Element {
    let clear = reply_ctx;
    let clear_t = reply_target;
    let on_cancel = Rc::new(move || {
        clear.set(None);
        clear_t.set(None);
    });
    crate::reply_preview_bar::reply_preview_bar(reply_ctx, on_cancel)
}

/// One LIVE (gateway-fed) message row: the `message_row_with_actions` content
/// (the hover `MessageActionBar` + react / reply / pin / translate, exactly
/// like a mock row), plus — for the signed-in user's own messages — the
/// Edit / Delete affordance (inside the overflow menu, hover-gated with the
/// rest) and the inline edit input. Once Edit mode or delete-confirm is
/// actually triggered their Save/Cancel/Delete and Confirm/No rows stay
/// always-visible. The hover bar handles the shared react/reply/pin/translate
/// actions.
///
/// `live_editing` / `live_delete_confirm` / `live_edit_text` are
/// `app_shell`-scope signals that drive per-row edit/delete mode; `gateway` is
/// a `Copy` `UseSocket` handle for sending `EditMessage` / `DeleteMessage`.
/// `key` is the row's `MessageKey` (the real wire `message_id` under `Live`),
/// plus the shared `picker` / `reply_ctx` / `pin_visible` / `translate_key`
/// handles threaded from `app_shell`. The row rebuilds when its state changes
/// because the message-list `switch`'s key folds these signals in (see
/// `sent_messages`).
pub(super) fn live_edit_row(
    message_id: i64,
    author: &str,
    color: &str,
    body: &str,
    when: String,
    full: String,
    edited: bool,
    is_mine: bool,
    grouped: bool,
    live_editing: Signal<Option<i64>>,
    live_edit_text: Signal<String>,
    live_delete_confirm: Signal<Option<i64>>,
    gateway: server::UseSocket<
        discord_server::server_settings::WsServerMsg,
        discord_server::server_settings::WsClientMsg,
    >,
    // Interaction parity with the mock path: the row's identity key (the real
    // wire `message_id` under the `Live` variant) plus the shared picker /
    // reply / pin / translate handles, so the hover action bar and the
    // overflow-menu actions behave identically to a mock row.
    key: MessageKey,
    picker: Signal<Option<MessageKey>>,
    reply_ctx: Signal<Option<(String, String)>>,
    reply_target: Signal<Option<i64>>,
    pin_visible: Signal<bool>,
    on_pin: Rc<dyn Fn()>,
    translate_key: Signal<Option<MessageKey>>,
    on_author: Option<AuthorPress>,
) -> Element {
    // Owner-only overflow-menu entries (Edit / Delete) — the same actions the
    // old always-visible inline Edit/Delete buttons did, now living inside the
    // one hover-gated overflow menu (`message_overflow_menu` renders them as
    // its last two rows when present). Built before the base row so the menu
    // can carry them; `None` for other authors' rows, whose menu keeps only
    // the shared entries. The backend enforces the same ownership — this is
    // the affordance gate (the previous inline buttons used the identical
    // `is_mine` check).
    let (menu_edit, menu_delete): (Option<Rc<dyn Fn()>>, Option<Rc<dyn Fn()>>) = if is_mine {
        let seed = body.to_string();
        let editing_sig = live_editing;
        let edit_text = live_edit_text;
        let confirm_sig = live_delete_confirm;
        let mid = message_id;
        (
            Some(Rc::new(move || {
                // Enter edit mode: seed the draft with the current content and
                // flag this message id; the list rebuild (keyed on
                // `live_editing`) mounts the input for this row.
                editing_sig.set(Some(mid));
                edit_text.set(seed.clone());
            })),
            Some(Rc::new(move || {
                confirm_sig.set(Some(mid));
            })),
        )
    } else {
        (None, None)
    };
    // The base row carries the hover `MessageActionBar` + overflow menu
    // (react / reply / pin / translate — plus Edit / Delete above when
    // `is_mine`) exactly like the mock path. The only differences are the
    // real `(edited)` marker (gated on the message's `edited_at` state) and
    // the live `MessageKey`; when `grouped` it is the headerless condensed
    // form (see `message_row`).
    let base = message_row_with_actions(
        author,
        color,
        body,
        when,
        full,
        edited,
        grouped,
        key,
        picker,
        reply_ctx,
        reply_target,
        pin_visible,
        translate_key,
        on_pin,
        menu_edit,
        menu_delete,
        on_author,
    );
    if !is_mine {
        return base;
    }
    let mid = message_id;
    let editing_sig = live_editing;
    let edit_text = live_edit_text;
    let confirm_sig = live_delete_confirm;
    // --- inline edit input (reuses the composer's `text_input` idiom) -------
    // `text_input` takes a reactive `Signal<String>` for `value`, so the draft
    // binds straight to `live_edit_text`. `value` only drives the NATIVE field
    // from the signal; keystrokes report back through `on_change`, which must
    // write the typed text into `live_edit_text` or Save would read the seed
    // instead (the "edit doesn't stick" bug). Cyclic but stable, same idiom as
    // the composer's `on_change = update_on_type`. `autofocus = true` focuses
    // the field at MOUNT — the input mounts when `live_editing` flips to this
    // row (the list `switch` keys on it), so clicking Edit puts the caret in
    // the seeded text with no extra click. Confirmation is the explicit Save
    // button, plus Enter on the input (Enter already submits in the composer
    // and the password fields, so it submits here too); the input is seeded
    // with the current content on Save-cancel paths by the list rebuild.
    let on_edit_type = move |s: String| { edit_text.set(s); };
    // --- Save (shared by the Save pill and Enter on the edit input) --------
    // Reads the DRAFT signal, which the input's `on_change` keeps in sync with
    // the native field.
    let do_save = move || {
        let content = edit_text.get();
        if !content.trim().is_empty() {
            gateway.send(
                discord_server::server_settings::WsClientMsg::EditMessage {
                    message_id: mid,
                    content,
                },
            );
        }
        editing_sig.set(None);
        edit_text.set(String::new());
    };
    let save_fn: Rc<dyn Fn()> = Rc::new(do_save);
    // `Rc` is not `Copy`: the `small_btn` handlers are `Fn` closures that
    // move-capture and can't call it on every press, so the Save pill arm
    // keeps its own Copy-only inline body; this `Rc` serves the Enter handler.
    let save_for_enter = save_fn.clone();
    let on_enter = move |ev: &KeyEvent| {
        if ev.key == "Enter" && !ev.shift && !ev.ctrl && !ev.alt && !ev.meta {
            (save_for_enter)();
            KeyOutcome::PreventDefault
        } else {
            KeyOutcome::Default
        }
    };
    let edit_input = {
        let input_style = StyleRules {
            font_size: Some(px(15.0)),
            color: Some(col(TEXT)),
            background: Some(col(COMPOSER)),
            padding_top: Some(px(6.0)),
            padding_bottom: Some(px(6.0)),
            padding_left: Some(px(10.0)),
            padding_right: Some(px(10.0)),
            border_top_left_radius: Some(px(8.0)),
            border_top_right_radius: Some(px(8.0)),
            border_bottom_left_radius: Some(px(8.0)),
            border_bottom_right_radius: Some(px(8.0)),
            flex_grow: Some(grow(1.0)),
            ..borderless_input()
        };
        ui! {
            text_input(value = live_edit_text, autofocus = true, on_change = on_edit_type, placeholder = "Edit message", style = input_style, test_id = "edit-input")
                .on_key_down(on_enter)
        }
    };
    // --- per-row control buttons ------------------------------------------
    // Compact text pills for the inline edit/confirm controls. Each button
    // carries a `MsgAction` the shared `on_touch` handler matches on — the
    // action is owned per-button so the closure captures nothing borrowed
    // (avoids the `&str`-lifetime trap of a label-matching closure). Edit and
    // Delete live in the '...' overflow menu, not here.
    #[derive(Clone, Copy, PartialEq)]
    enum MsgAction {
        Delete,
        Save,
        Cancel,
        Confirm,
        No,
    }
    let small_btn: Rc<dyn Fn(MsgAction, &str, &'static str, &'static str) -> Element> =
        Rc::new(move |action: MsgAction, label: &str, bg: &'static str, test_id: &'static str| -> Element {
        let s = StyleRules {
            padding_top: Some(px(4.0)),
            padding_bottom: Some(px(4.0)),
            padding_left: Some(px(10.0)),
            padding_right: Some(px(10.0)),
            background: Some(col(bg)),
            border_top_left_radius: Some(px(8.0)),
            border_top_right_radius: Some(px(8.0)),
            border_bottom_left_radius: Some(px(8.0)),
            border_bottom_right_radius: Some(px(8.0)),
            flex_shrink: Some(grow(0.0)),
            font_size: Some(px(12.0)),
            font_weight: Some(FontWeight::SemiBold),
            color: Some(col(TEXT)),
            ..Default::default()
        };
        let label_style = StyleRules {
            font_size: Some(px(12.0)),
            font_weight: Some(FontWeight::SemiBold),
            color: Some(col(TEXT)),
            ..Default::default()
        };
        let touch = move |ev: &runtime_shared::TouchEvent| {
            if ev.phase != TouchPhase::Ended {
                return TouchResponse::CONSUMED;
            }
            match action {
                MsgAction::Delete => confirm_sig.set(Some(mid)),
                // Same action as Save-from-Enter (`save_for_enter` below); kept
                // inline here because `save_fn` is a non-`Copy` `Rc` that an
                // `Fn` touch handler can't move out of on every press, while
                // every handle this arm touches is `Copy`.
                MsgAction::Save => {
                    let content = edit_text.get();
                    if !content.trim().is_empty() {
                        gateway.send(
                            discord_server::server_settings::WsClientMsg::EditMessage {
                                message_id: mid,
                                content,
                            },
                        );
                    }
                    editing_sig.set(None);
                    edit_text.set(String::new());
                },
                MsgAction::Cancel => {
                    editing_sig.set(None);
                    edit_text.set(String::new());
                }
                MsgAction::Confirm => {
                    gateway.send(discord_server::server_settings::WsClientMsg::DeleteMessage {
                        message_id: mid,
                    });
                    confirm_sig.set(None);
                }
                MsgAction::No => confirm_sig.set(None),
            }
            TouchResponse::CONSUMED
        };
        ui! {
            view(style = s) {
                text(style = label_style) { label }
            }
            .on_touch(touch)
            .test_id(test_id)
        }
    });
    // Two mutually-exclusive per-row control states, each rendered beneath the
    // base row in place of nothing else: the edit input (Save / Cancel / Delete)
    // and the delete-confirm strip (Delete this message? / Confirm / No). Edit
    // and Delete are entered from the '...' overflow menu, not inline — see
    // `message_overflow_menu`. The idle row has no inline controls.
    let edit_mode = editing_sig.get() == Some(mid);
    let delete_mode = confirm_sig.get() == Some(mid);
    let save_btn = small_btn(MsgAction::Save, "Save", BUTTON, "save-edit");
    let cancel_btn = small_btn(MsgAction::Cancel, "Cancel", COMPOSER, "cancel-edit");
    let edit_delete_btn = small_btn(MsgAction::Delete, "Delete", BADGE, "row-delete");
    let confirm_btn = small_btn(MsgAction::Confirm, "Confirm", BADGE, "confirm-delete");
    let no_btn = small_btn(MsgAction::No, "No", COMPOSER, "cancel-delete");
    ui! {
        view(style = StyleRules {
            flex_direction: Some(FlexDirection::Column),
            gap: Some(px(6.0)),
            ..Default::default()
        }) {
            { base }
            if edit_mode {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Column),
                    gap: Some(px(4.0)),
                    ..Default::default()
                }) {
                    { edit_input }
                    view(style = StyleRules {
                        flex_direction: Some(FlexDirection::Row),
                        gap: Some(px(6.0)),
                        ..Default::default()
                    }) {
                        { save_btn }
                        { cancel_btn }
                        { edit_delete_btn }
                    }
                }
            } else if delete_mode {
                view(style = StyleRules {
                    flex_direction: Some(FlexDirection::Row),
                    align_items: Some(AlignItems::Center),
                    gap: Some(px(6.0)),
                    ..Default::default()
                }) {
                    text(style = StyleRules {
                        font_size: Some(px(12.0)),
                        color: Some(col(TEXT_MUTED)),
                        flex_grow: Some(grow(1.0)),
                        ..Default::default()
                    }) { "Delete this message?".to_string() }
                    { confirm_btn }
                    { no_btn }
                }
            }
        }
    }
}

#[cfg(test)]
mod grouping_tests {
    use super::*;

    /// Discord convention: same author within the 7-minute window groups.
    #[test]
    fn same_author_within_window_groups() {
        assert!(same_group("alice", 1_000_000, "alice", 1_000_000 + 419));
        assert!(same_group("alice", 1_000_000, "alice", 1_000_000));
    }

    /// Same author past the window → fresh header.
    #[test]
    fn same_author_past_window_breaks() {
        assert!(!same_group("alice", 1_000_000, "alice", 1_000_000 + 420));
    }

    /// Different author → always breaks, regardless of timing.
    #[test]
    fn different_author_breaks() {
        assert!(!same_group("alice", 1_000_000, "bob", 1_000_001));
    }

    /// A message older than the previous one (out of order) never groups.
    #[test]
    fn out_of_order_never_groups() {
        assert!(!same_group("alice", 1_000_000, "alice", 999_999));
    }
}
