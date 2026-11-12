//! Link previews: the title, description, site name and image of pages
//! linked in a message, shown as cards under it.
//!
//! The server fetches the pages, so it must not become a way to reach
//! machines on its own network (SSRF). Every fetch, and every redirect hop,
//! resolves the host first and refuses it if any address is internal (see
//! [`ip_allowed`]), then connects to exactly the address it checked. Pages are
//! read for at most 512 KB, images for at most 2 MB, everything within 5 s,
//! and a page's image is copied into the media store so viewers never load
//! third-party URLs.

use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// One preview card.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Embed {
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub site_name: Option<String>,
    /// A `/media/…` path.
    pub image: Option<String>,
    /// The image's pixel size, so a client can lay it out before it loads.
    #[serde(default)]
    pub image_size: Option<(u32, u32)>,
}

/// At most this many links in one message get a card.
pub const MAX_LINKS: usize = 3;
const MAX_TITLE: usize = 256;
const MAX_DESCRIPTION: usize = 400;

/// Whether the server may connect to `ip`: public unicast only. Loopback,
/// private, link-local, unique-local, CGNAT, multicast, unspecified,
/// broadcast, documentation and reserved ranges are refused, including when
/// written as an IPv4-mapped or IPv4-compatible IPv6 address.
pub fn ip_allowed(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4_allowed(v4),
        IpAddr::V6(v6) => v6_allowed(v6),
    }
}

fn v4_allowed(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_documentation()
        || a == 0
        || (a == 100 && (64..128).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 198 && (b == 18 || b == 19))
        || a >= 240)
}

fn v6_allowed(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return v4_allowed(v4);
    }
    let s = ip.segments();
    // ::a.b.c.d (IPv4-compatible) and 64:ff9b::/96 (NAT64) both reach IPv4.
    if s[..6] == [0, 0, 0, 0, 0, 0] && !ip.is_unspecified() && !ip.is_loopback() {
        return v4_allowed(Ipv4Addr::new((s[6] >> 8) as u8, s[6] as u8, (s[7] >> 8) as u8, s[7] as u8));
    }
    if s[0] == 0x64 && s[1] == 0xff9b {
        return false;
    }
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (s[0] & 0xffc0) == 0xfe80
        || (s[0] & 0xfe00) == 0xfc00
        || (s[0] == 0x2001 && s[1] == 0x0db8))
}

/// The links in `text` worth a card: `http(s)` URLs outside `code`, the
/// first [`MAX_LINKS`] distinct ones, trailing punctuation trimmed.
pub fn extract_links(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    // Drop code: ``` blocks first, then `inline` spans.
    let mut plain = String::new();
    for (i, part) in text.split("```").enumerate() {
        if i % 2 == 0 {
            for (j, piece) in part.split('`').enumerate() {
                if j % 2 == 0 {
                    plain.push_str(piece);
                    plain.push(' ');
                }
            }
        }
    }
    for word in plain.split_whitespace() {
        let word = word.trim_start_matches(['<', '(']);
        if !(word.starts_with("https://") || word.starts_with("http://")) {
            continue;
        }
        let url = word.trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']', '}', '>', '\'', '"']);
        if url.len() > 2048 || url.len() <= "https://".len() || out.iter().any(|u| u == url) {
            continue;
        }
        out.push(url.to_string());
        if out.len() == MAX_LINKS {
            break;
        }
    }
    out
}

/// What a page says about itself: OpenGraph tags, else `<title>` and the
/// description meta tag.
#[derive(Debug, Default, PartialEq)]
pub struct PageMeta {
    pub title: Option<String>,
    pub description: Option<String>,
    pub site_name: Option<String>,
    pub image: Option<String>,
}

/// Scan `html` for the tags [`PageMeta`] needs. Only looks at `<meta>` and
/// `<title>`, which sit in the head, so a truncated page still works.
pub fn parse_html(html: &str) -> PageMeta {
    let lower = html.to_ascii_lowercase();
    let mut meta = PageMeta::default();
    let (mut og_title, mut og_desc, mut desc) = (None, None, None);
    let mut at = 0;
    while let Some(i) = lower[at..].find("<meta") {
        let start = at + i;
        let Some(len) = lower[start..].find('>') else { break };
        let tag = &html[start..start + len];
        at = start + len;
        let key = attr(tag, "property").or_else(|| attr(tag, "name")).map(|k| k.to_ascii_lowercase());
        let Some(content) = attr(tag, "content") else { continue };
        let content = decode_entities(&content);
        // An empty tag (say `og:title content=""`) says nothing; fall back.
        if content.is_empty() {
            continue;
        }
        match key.as_deref() {
            Some("og:title") => og_title = og_title.or(Some(content)),
            Some("og:description") => og_desc = og_desc.or(Some(content)),
            Some("og:site_name") => meta.site_name = meta.site_name.or(Some(content)),
            Some("og:image") | Some("og:image:url") => meta.image = meta.image.or(Some(content)),
            Some("description") => desc = desc.or(Some(content)),
            _ => {}
        }
    }
    let title_tag = lower.find("<title").and_then(|i| {
        let open = i + lower[i..].find('>')? + 1;
        let close = open + lower[open..].find("</title")?;
        Some(decode_entities(&html[open..close]))
    });
    meta.title = og_title.or(title_tag).map(|t| clip(&t, MAX_TITLE)).filter(|t| !t.is_empty());
    meta.description = og_desc.or(desc).map(|d| clip(&d, MAX_DESCRIPTION)).filter(|d| !d.is_empty());
    meta.site_name = meta.site_name.map(|s| clip(&s, 100)).filter(|s| !s.is_empty());
    meta
}

/// The value of attribute `name` in one tag's source (quoted either way, or
/// bare).
fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(name) {
        let start = from + i;
        from = start + name.len();
        let before_ok = start == 0 || lower.as_bytes()[start - 1].is_ascii_whitespace();
        let rest = lower[from..].trim_start();
        if !before_ok || !rest.starts_with('=') {
            continue;
        }
        let value_at = tag.len() - rest.len() + 1;
        let value = tag[value_at..].trim_start();
        return Some(match value.chars().next() {
            Some(q @ ('"' | '\'')) => value[1..].split(q).next().unwrap_or("").to_string(),
            _ => value.split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("").to_string(),
        });
    }
    None
}

fn decode_entities(s: &str) -> String {
    let s = s
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&");
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clip(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max - 1).collect();
    out.push('…');
    out
}

#[cfg(feature = "server")]
pub use fetch::{embeds_for, schedule, hop_allowed, all_addrs_allowed, chain_ok, throttle_previews};

#[cfg(feature = "server")]
mod fetch {
    use super::*;
    use crate::db::Db;
    use crate::rate_limit::RateLimiter;
    use std::sync::OnceLock;
    use std::time::Duration;

    const PAGE_LIMIT: usize = 512 * 1024;
    const IMAGE_LIMIT: usize = 2 * 1024 * 1024;
    const MAX_REDIRECTS: usize = 3;
    const CACHE_MS: i64 = 24 * 60 * 60 * 1000;
    const PREVIEWS_PER_WINDOW: usize = 30;
    const PREVIEW_WINDOW: Duration = Duration::from_secs(120);

    /// Whether one URL of a redirect chain may be connected at all, checked
    /// purely and before any DNS: `http`/`https` only, only on the scheme's
    /// own standard port (a preview fetcher that honored arbitrary ports
    /// could be aimed at daemons on any port a local box runs), no
    /// credentials smuggled into the URL, and a host written as a literal
    /// address must be public. Hostnames are covered by the resolver check
    /// in [`guarded_get`].
    pub fn hop_allowed(url: &reqwest::Url) -> bool {
        if !matches!(url.scheme(), "http" | "https") {
            return false;
        }
        if !url.username().is_empty() || url.password().is_some() {
            return false;
        }
        let standard = if url.scheme() == "https" { 443 } else { 80 };
        if url.port_or_known_default() != Some(standard) {
            return false;
        }
        let Some(host) = url.host_str() else { return false };
        host_literal_allowed(host)
    }

    /// A host written as an address — dotted IPv4, or IPv6 bracketed or
    /// bare, including the IPv4-mapped forms reqwest normalizes to — must
    /// be public per [`ip_allowed`]. A plain hostname isn't an address and
    /// passes here; the resolver check covers it.
    fn host_literal_allowed(host: &str) -> bool {
        let h = if host.starts_with('[') && host.ends_with(']') {
            &host[1..host.len() - 1]
        } else {
            host
        };
        h.parse::<IpAddr>().map(ip_allowed).unwrap_or(true)
    }

    /// The resolver's addresses are usable for a connect: at least one, and
    /// every one public. A host that resolves to a mix is refused outright.
    pub fn all_addrs_allowed(addrs: &[std::net::SocketAddr]) -> bool {
        !addrs.is_empty() && addrs.iter().all(|a| ip_allowed(a.ip()))
    }

    /// A redirect chain from the requested URL to the final one: non-empty,
    /// within the hop cap, and every hop re-allowed by [`hop_allowed`].
    pub fn chain_ok(chain: &[reqwest::Url]) -> bool {
        !chain.is_empty()
            && chain.len() <= MAX_REDIRECTS + 1
            && chain.iter().all(|u| hop_allowed(u))
    }

    /// Refuse a preview-fetch run once `key` (the message's author) has used
    /// up its slice of the budget. The limiter is a parameter so tests can
    /// drive a tiny budget; production uses [`preview_limiter`].
    pub fn throttle_previews(limiter: &RateLimiter, key: &str) -> Result<(), String> {
        crate::rate_limit::throttle(limiter, key)
    }

    fn preview_limiter() -> &'static RateLimiter {
        static L: OnceLock<RateLimiter> = OnceLock::new();
        L.get_or_init(|| RateLimiter::new(PREVIEWS_PER_WINDOW, PREVIEW_WINDOW))
    }

    /// Fetch `url` and follow redirects by hand, checking every hop's
    /// addresses. Returns the final URL and the response, which is only
    /// returned when its content type starts with `want`.
    async fn guarded_get(url: &str, want: &str) -> Option<(reqwest::Url, reqwest::Response)> {
        let mut url = reqwest::Url::parse(url).ok()?;
        if !hop_allowed(&url) {
            return None;
        }
        let mut chain: Vec<reqwest::Url> = vec![url.clone()];
        for _ in 0..=MAX_REDIRECTS {
            let host = url.host_str()?.to_string();
            let port = url.port_or_known_default()?;
            let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host.as_str(), port)).await.ok()?.collect();
            if !all_addrs_allowed(&addrs) {
                return None;
            }
            // Connect to the address just checked, so DNS can't change in
            // between.
            let client = reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .no_proxy()
                .resolve(&host, addrs[0])
                .timeout(Duration::from_secs(5))
                .user_agent("Mozilla/5.0 (compatible; CassetteBot/1.0; link previews)")
                .build()
                .ok()?;
            let resp = client.get(url.clone()).send().await.ok()?;
            if resp.status().is_redirection() {
                let next = resp.headers().get(reqwest::header::LOCATION)?.to_str().ok()?;
                url = url.join(next).ok()?;
                chain.push(url.clone());
                if !chain_ok(&chain) {
                    return None;
                }
                continue;
            }
            if !resp.status().is_success() {
                return None;
            }
            let kind = resp.headers().get(reqwest::header::CONTENT_TYPE)?.to_str().ok()?.to_ascii_lowercase();
            if !kind.starts_with(want) {
                return None;
            }
            return Some((url, resp));
        }
        None
    }

    /// Read at most `limit` bytes of the body.
    async fn read_capped(mut resp: reqwest::Response, limit: usize) -> Vec<u8> {
        let mut out = Vec::new();
        while let Ok(Some(chunk)) = resp.chunk().await {
            out.extend_from_slice(&chunk);
            if out.len() >= limit {
                out.truncate(limit);
                break;
            }
        }
        out
    }

    /// The card for one page, fetched now.
    async fn fetch_embed(url: &str) -> Option<Embed> {
        let (final_url, resp) = guarded_get(url, "text/html").await?;
        let html = read_capped(resp, PAGE_LIMIT).await;
        let meta = parse_html(&String::from_utf8_lossy(&html));
        let mut image = None;
        if let Some(src) = meta.image.as_deref().and_then(|s| final_url.join(s).ok()) {
            if let Some((_, resp)) = guarded_get(src.as_str(), "image/").await {
                let mime = resp
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .map(|v| v.split(';').next().unwrap_or("").trim().to_ascii_lowercase())
                    .unwrap_or_default();
                let mime = if mime == "image/jpg" { "image/jpeg".to_string() } else { mime };
                let bytes = read_capped(resp, IMAGE_LIMIT + 1).await;
                if bytes.len() <= IMAGE_LIMIT {
                    image = crate::media::store(&bytes, &mime, IMAGE_LIMIT).ok();
                }
            }
        }
        let embed = Embed {
            url: url.to_string(),
            title: meta.title,
            description: meta.description,
            site_name: meta.site_name.or_else(|| final_url.host_str().map(str::to_string)),
            image,
            image_size: None,
        };
        (embed.title.is_some() || embed.description.is_some() || embed.image.is_some()).then_some(embed)
    }

    /// The card for `url`, from the day-long cache when it's there. A page
    /// that had nothing to show is cached too, so it isn't refetched.
    async fn embed_for(db: &Db, url: &str) -> Option<Embed> {
        let now = crate::realtime::now_millis();
        let cached = db.lock().ok().and_then(|c| {
            c.query_row(
                "SELECT fetched_at, title, description, site_name, image_path FROM link_preview_cache WHERE url = ?1",
                rusqlite::params![url],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        Embed {
                            url: url.to_string(),
                            title: r.get(1)?,
                            description: r.get(2)?,
                            site_name: r.get(3)?,
                            image: r.get(4)?,
                            image_size: None,
                        },
                    ))
                },
            )
            .ok()
        });
        let embed = match cached {
            Some((at, e)) if now - at < CACHE_MS => Some(e),
            _ => {
                let fetched = tokio::time::timeout(Duration::from_secs(5), fetch_embed(url)).await.ok().flatten();
                let e = fetched.clone().unwrap_or_else(|| Embed { url: url.to_string(), ..Default::default() });
                if let Ok(c) = db.lock() {
                    let _ = c.execute(
                        "INSERT OR REPLACE INTO link_preview_cache (url, fetched_at, title, description, site_name, image_path)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        rusqlite::params![url, now, e.title, e.description, e.site_name, e.image],
                    );
                }
                return fetched;
            }
        };
        embed.filter(|e| e.title.is_some() || e.description.is_some() || e.image.is_some())
    }

    /// The stored cards of `message_id`, in order.
    pub fn embeds_for(db: &Db, message_id: i64) -> Vec<Embed> {
        let Ok(c) = db.lock() else { return Vec::new() };
        let Ok(mut stmt) = c.prepare(
            "SELECT url, title, description, site_name, image_path FROM message_embeds
             WHERE message_id = ?1 ORDER BY position",
        ) else {
            return Vec::new();
        };
        stmt.query_map(rusqlite::params![message_id], |r| {
            Ok(Embed { url: r.get(0)?, title: r.get(1)?, description: r.get(2)?, site_name: r.get(3)?, image: r.get(4)?, image_size: None })
        })
        .map(|rows| rows.flatten().map(with_size).collect())
        .unwrap_or_default()
    }

    fn with_size(mut e: Embed) -> Embed {
        e.image_size = e.image.as_deref().and_then(crate::media::dimensions);
        e
    }

    fn store_embeds(db: &Db, message_id: i64, embeds: &[Embed]) -> Result<(), String> {
        let mut c = db.lock().map_err(|e| e.to_string())?;
        let tx = c.transaction().map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM message_embeds WHERE message_id = ?1", rusqlite::params![message_id])
            .map_err(|e| e.to_string())?;
        for (i, e) in embeds.iter().enumerate() {
            tx.execute(
                "INSERT INTO message_embeds (message_id, position, url, title, description, site_name, image_path)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![message_id, i as i64, e.url, e.title, e.description, e.site_name, e.image],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())
    }

    /// After a post or an edit: work out the message's cards in the
    /// background, store them and tell the channel. `allowed` is whether the
    /// author has Embed Links there; without it (or without links) the
    /// message's cards are cleared. Nothing runs outside a Tokio runtime.
    pub fn schedule(db: &Db, channel_id: i64, message_id: i64, content: &str, allowed: bool) {
        let links = if allowed { extract_links(content) } else { Vec::new() };
        let had = !embeds_for(db, message_id).is_empty();
        if links.is_empty() && !had {
            return;
        }
        // One author can't hammer the fetcher forever: the budget is per author,
        // and a refusal just leaves the message without cards.
        if !links.is_empty() {
            let author = db.lock().ok().and_then(|c| {
                c.query_row(
                    "SELECT author_id FROM messages WHERE id = ?1",
                    rusqlite::params![message_id],
                    |r| r.get::<_, i64>(0),
                ).ok()
            }).unwrap_or(0);
            if throttle_previews(preview_limiter(), &format!("preview-{author}")).is_err() {
                return;
            }
        }
        let Ok(rt) = tokio::runtime::Handle::try_current() else { return };
        let db = db.clone();
        rt.spawn(async move {
            let mut embeds = Vec::new();
            for url in &links {
                if let Some(e) = embed_for(&db, url).await {
                    embeds.push(with_size(e));
                }
            }
            if embeds.is_empty() && !had {
                return;
            }
            if store_embeds(&db, message_id, &embeds).is_err() {
                return; // deleted meanwhile
            }
            crate::realtime::hub().publish(
                &crate::realtime::channel_audience(&db, channel_id),
                &crate::server_settings::WsServerMsg::MessageEmbeds {
                message_id, channel_id, embeds, seq: crate::server_settings::next_frame_seq(),
            },
            );
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(s: &str) -> bool {
        ip_allowed(s.parse().unwrap())
    }

    #[test]
    fn internal_addresses_are_refused() {
        for bad in [
            "127.0.0.1", "127.8.9.10", "10.0.0.1", "10.255.255.255", "172.16.0.1", "172.31.255.254",
            "192.168.0.50", "192.168.1.1", "169.254.169.254", "100.64.0.1", "100.127.255.255",
            "224.0.0.1", "239.255.255.250", "0.0.0.0", "0.1.2.3", "255.255.255.255", "240.0.0.1",
            "192.0.0.8", "198.18.0.1", "192.0.2.1", "::1", "::", "fe80::1", "fe80::abcd:1",
            "fc00::1", "fd12:3456::1", "ff02::1", "::ffff:127.0.0.1", "::ffff:10.0.0.1",
            "::ffff:192.168.0.50", "::ffff:169.254.169.254", "::127.0.0.1", "::10.1.2.3",
            "64:ff9b::a00:1", "2001:db8::1",
        ] {
            assert!(!ok(bad), "{bad} should be refused");
        }
    }

    #[test]
    fn public_addresses_pass() {
        for good in ["8.8.8.8", "1.1.1.1", "151.101.1.69", "172.32.0.1", "100.128.0.1", "2606:4700::1111", "::ffff:8.8.8.8"] {
            assert!(ok(good), "{good} should pass");
        }
    }

    #[test]
    fn links_skip_code_and_repeats() {
        let text = "see https://a.example/x, and `https://code.example` then\n```\nhttps://block.example\n```\n(https://b.example/y) https://a.example/x https://c.example https://d.example";
        assert_eq!(extract_links(text), vec!["https://a.example/x", "https://b.example/y", "https://c.example"]);
        assert!(extract_links("ftp://x.example http://").is_empty());
    }

    #[test]
    fn og_tags_win_over_title() {
        let html = r#"<html><head><title>Plain &amp; simple</title>
            <meta name="description" content="fallback">
            <meta property="og:title" content="Rust &quot;Lang&quot;">
            <meta content='Fast and safe.' property='og:description'>
            <meta property="og:site_name" content="rust-lang.org">
            <meta property="og:image" content="/static/logo.png"></head>"#;
        let m = parse_html(html);
        assert_eq!(m.title.as_deref(), Some("Rust \"Lang\""));
        assert_eq!(m.description.as_deref(), Some("Fast and safe."));
        assert_eq!(m.site_name.as_deref(), Some("rust-lang.org"));
        assert_eq!(m.image.as_deref(), Some("/static/logo.png"));
        let empty_og = parse_html(r#"<meta property="og:title" content="" /><title>Real title</title>"#);
        assert_eq!(empty_og.title.as_deref(), Some("Real title"));
        let plain = parse_html("<title>  Just a title </title><meta name=description content=hi>");
        assert_eq!(plain.title.as_deref(), Some("Just a title"));
        assert_eq!(plain.description.as_deref(), Some("hi"));
    }

    #[test]
    fn long_text_is_clipped() {
        let html = format!("<title>{}</title>", "x".repeat(300));
        assert_eq!(parse_html(&html).title.unwrap().chars().count(), MAX_TITLE);
    }

    #[cfg(feature = "server")]
    fn url(s: &str) -> reqwest::Url {
        reqwest::Url::parse(s).unwrap()
    }

    #[cfg(feature = "server")]
    #[test]
    fn hop_refuses_non_http_schemes() {
        for bad in ["file:///etc/passwd", "ftp://a.example/", "data:text/html,hi", "javascript:alert(1)"] {
            assert!(!hop_allowed(&url(bad)), "{bad} must not be fetched");
        }
        assert!(hop_allowed(&url("http://8.8.8.8/")));
    }

    #[cfg(feature = "server")]
    #[test]
    fn hop_refuses_nonstandard_ports() {
        for bad in ["http://8.8.8.8:8080/", "https://8.8.8.8:8443/", "http://example.com:0/", "http://example.com:1024/"] {
            assert!(!hop_allowed(&url(bad)), "a fetcher may not be aimed at an arbitrary port");
        }
        assert!(hop_allowed(&url("http://example.com:80/x")));
        assert!(hop_allowed(&url("https://example.com:443/x")));
    }

    #[cfg(feature = "server")]
    #[test]
    fn hop_refuses_credentials_in_the_url() {
        for bad in ["http://user@127.0.0.1/", "http://user:pw@8.8.8.8/", "https://x:y@example.com/"] {
            assert!(!hop_allowed(&url(bad)), "credentials smuggled into a preview URL are refused");
        }
    }

    #[cfg(feature = "server")]
    #[test]
    fn hop_refuses_literal_internal_hosts() {
        for bad in ["http://169.254.169.254/", "http://127.0.0.1/", "http://0.0.0.0/",
                    "http://255.255.255.255/", "http://[::1]/", "http://[0:0:0:0:0:0:0:1]/",
                    "http://[fe80::1]/", "http://[fc00::1]/", "http://[::ffff:127.0.0.1]/",
                    "http://[::ffff:127.0.0.1]/", "http://[::127.0.0.1]/", "http://[::]:80/"] {
            assert!(!hop_allowed(&url(bad)), "an internal literal host must not connect");
        }
        for good in ["http://8.8.8.8/", "http://151.101.1.69/", "http://[::ffff:8.8.8.8]/"] {
            assert!(hop_allowed(&url(good)), "{good} is a public literal host");
        }
    }

    #[cfg(feature = "server")]
    #[test]
    fn hop_accepts_plain_hostnames() {
        assert!(hop_allowed(&url("http://localhost/")));
        assert!(hop_allowed(&url("https://example.com/x")));
    }

    #[cfg(feature = "server")]
    #[test]
    fn resolver_refuses_empty_or_mixed_addresses() {
        use std::net::{Ipv4Addr, SocketAddr};
        let a = |ip: (u8, u8, u8, u8), port: u16| SocketAddr::from((Ipv4Addr::new(ip.0, ip.1, ip.2, ip.3), port));
        assert!(!all_addrs_allowed(&[]));
        assert!(!all_addrs_allowed(&[a((127, 0, 0, 1), 80)]));
        assert!(!all_addrs_allowed(&[a((8, 8, 8, 8), 80), a((10, 0, 0, 1), 80)]));
        assert!(all_addrs_allowed(&[a((8, 8, 8, 8), 80), a((151, 101, 1, 69), 443)]));
    }

    #[cfg(feature = "server")]
    #[test]
    fn chain_caps_hops_and_rechecks_each_one() {
        let chain = [url("http://a.example/"), url("http://b.example/"), url("http://c.example/")];
        assert!(chain_ok(&chain));
        let too_long = [url("http://a.example/"), url("http://b.example/"),
                        url("http://c.example/"), url("http://d.example/"), url("http://e.example/")];
        assert!(!chain_ok(&too_long), "one hop over the cap is refused");
        assert!(!chain_ok(&[]));
        let poisoned = [url("http://a.example/"), url("http://169.254.169.254/")];
        assert!(!chain_ok(&poisoned), "a redirect that lands on an internal host is refused");
    }

    #[cfg(feature = "server")]
    #[test]
    fn preview_budget_trips_and_slides() {
        use crate::rate_limit::RateLimiter;
        let limiter = RateLimiter::new(2, std::time::Duration::from_secs(60));
        assert!(throttle_previews(&limiter, "author-1").is_ok());
        assert!(throttle_previews(&limiter, "author-1").is_ok());
        let e = throttle_previews(&limiter, "author-1").unwrap_err();
        assert!(e.contains("seconds"), "{e} must be user-facing text");
        limiter.set_now(std::time::Duration::from_secs(61));
        assert!(throttle_previews(&limiter, "author-1").is_ok(), "the window slid past the oldest attempt");
        assert!(throttle_previews(&limiter, "author-2").is_ok(), "another author has its own budget");
    }
}
