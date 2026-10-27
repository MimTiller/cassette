//! GIF search for the composer's GIF picker, backed by Giphy.
//!
//! ## Design
//!
//! The browser never talks to Giphy's API: [`gif_search`] runs the search on
//! the server, so the API key stays here. The key comes from `GIPHY_API_KEY`,
//! or else the file `giphy.key` next to the database (`DISCORD_DB`); with
//! neither, the fn answers "GIF search isn't set up" and the picker says so.
//!
//! A Giphy development key allows 100 calls an hour, so answers are cached
//! per `(query, offset)` for [`CACHE_SECS`]: reopening the picker or
//! re-typing a recent search costs nothing.
//!
//! A picked GIF is sent as an ordinary message whose whole body is the GIF's
//! media URL with its pixel size in the fragment (`…/giphy.gif#480x270`).
//! [`gif_body`] recognizes such a body (or any pasted Giphy media link) so the
//! client renders it as an image; only Giphy's media hosts qualify, so a
//! message can't make every reader's browser fetch an arbitrary URL.

use serde::{Deserialize, Serialize};
use server::{server, ServerError};

#[cfg(feature = "server")]
use crate::session::acting_user;

/// Results per page.
pub const PAGE_SIZE: u32 = 30;

/// How long a cached answer is reused, in seconds.
#[cfg(feature = "server")]
const CACHE_SECS: u64 = 600;

/// Input for [`gif_search`]: an empty `query` means Giphy's trending GIFs.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GifSearchInput {
    pub query: String,
    #[serde(default)]
    pub offset: u32,
}

/// One GIF: a small rendition for the picker grid and a bigger one to send.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Gif {
    pub id: String,
    pub title: String,
    pub preview_url: String,
    pub preview_width: u32,
    pub preview_height: u32,
    pub url: String,
    pub width: u32,
    pub height: u32,
}

/// One page of results; `next_offset` is `None` on the last page.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct GifPage {
    pub gifs: Vec<Gif>,
    pub next_offset: Option<u32>,
}

/// Parse a Giphy search/trending response. GIFs missing a usable rendition
/// are skipped.
pub fn parse_giphy(json: &serde_json::Value) -> GifPage {
    // Giphy reports sizes as strings ("200"); accept numbers too.
    fn num(v: &serde_json::Value) -> u32 {
        v.as_str().and_then(|s| s.parse().ok()).or(v.as_u64().map(|n| n as u32)).unwrap_or(0)
    }
    fn rendition(images: &serde_json::Value, name: &str) -> Option<(String, u32, u32)> {
        let r = images.get(name)?;
        let url = r.get("url")?.as_str()?.to_string();
        let (w, h) = (num(&r["width"]), num(&r["height"]));
        (url.starts_with("https://") && w > 0 && h > 0).then_some((url, w, h))
    }
    let gifs: Vec<Gif> = json["data"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|g| {
                    let images = &g["images"];
                    let (preview_url, preview_width, preview_height) = rendition(images, "fixed_width")?;
                    let (url, width, height) = rendition(images, "downsized")
                        .or_else(|| rendition(images, "original"))
                        .unwrap_or_else(|| (preview_url.clone(), preview_width, preview_height));
                    Some(Gif {
                        id: g["id"].as_str()?.to_string(),
                        title: g["title"].as_str().unwrap_or_default().to_string(),
                        preview_url,
                        preview_width,
                        preview_height,
                        url,
                        width,
                        height,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let p = &json["pagination"];
    let (offset, count, total) = (num(&p["offset"]), num(&p["count"]), num(&p["total_count"]));
    let next = offset + count;
    GifPage { next_offset: (count > 0 && next < total).then_some(next), gifs }
}

/// The message body that sends `gif`: its URL with the size in the fragment.
pub fn gif_message(gif: &Gif) -> String {
    let base = gif.url.split('#').next().unwrap_or_default();
    format!("{base}#{}x{}", gif.width, gif.height)
}

/// When a message body is just a Giphy media link, the image URL and the
/// size from its fragment (if any). Anything else is `None`.
pub fn gif_body(body: &str) -> Option<(String, Option<(u32, u32)>)> {
    let body = body.trim();
    if body.contains(char::is_whitespace) {
        return None;
    }
    let rest = body.strip_prefix("https://")?;
    let (host, path) = rest.split_once('/')?;
    let giphy_host = host == "i.giphy.com"
        || host == "media.giphy.com"
        || (host.starts_with("media") && host.ends_with(".giphy.com") && host[5..host.len() - 10].chars().all(|c| c.is_ascii_digit()));
    if !giphy_host {
        return None;
    }
    let (path, frag) = match path.split_once('#') {
        Some((p, f)) => (p, Some(f)),
        None => (path, None),
    };
    let file = path.split('?').next().unwrap_or_default();
    if !(file.ends_with(".gif") || file.ends_with(".webp")) {
        return None;
    }
    let size = frag.and_then(|f| {
        let (w, h) = f.split_once('x')?;
        Some((w.parse().ok()?, h.parse().ok()?))
    });
    Some((format!("https://{host}/{path}"), size))
}

/// The Giphy key, from `GIPHY_API_KEY` or `giphy.key` beside the database.
#[cfg(feature = "server")]
fn api_key() -> Option<String> {
    if let Ok(k) = std::env::var("GIPHY_API_KEY") {
        if !k.trim().is_empty() {
            return Some(k.trim().to_string());
        }
    }
    let db = std::env::var("DISCORD_DB").unwrap_or_else(|_| "discord.db".to_string());
    let path = std::path::Path::new(&db).with_file_name("giphy.key");
    let k = std::fs::read_to_string(path).ok()?;
    let k = k.trim();
    (!k.is_empty()).then(|| k.to_string())
}

#[cfg(feature = "server")]
type Cache = std::collections::HashMap<(String, u32), (std::time::Instant, GifPage)>;

#[cfg(feature = "server")]
fn cache() -> &'static std::sync::Mutex<Cache> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<Cache>> = std::sync::OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Search Giphy (or list trending GIFs for an empty query). Signed-in users
/// only; rated PG-13 and below.
#[server]
pub async fn gif_search(input: GifSearchInput) -> Result<GifPage, ServerError> {
    acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let query: String = input.query.trim().chars().take(50).collect::<String>().to_lowercase();
    let key = (query.clone(), input.offset);
    {
        let mut c = cache().lock().unwrap();
        c.retain(|_, (at, _)| at.elapsed().as_secs() < CACHE_SECS);
        if let Some((_, page)) = c.get(&key) {
            return Ok(page.clone());
        }
    }
    let api_key = api_key().ok_or_else(|| ServerError::failed("GIF search isn't set up yet (no Giphy key)"))?;
    let endpoint = if query.is_empty() { "trending" } else { "search" };
    let limit = PAGE_SIZE.to_string();
    let offset = input.offset.to_string();
    let mut params = vec![
        ("api_key", api_key.as_str()),
        ("limit", limit.as_str()),
        ("offset", offset.as_str()),
        ("rating", "pg-13"),
    ];
    if !query.is_empty() {
        params.push(("q", query.as_str()));
    }
    let resp = reqwest::Client::new()
        .get(format!("https://api.giphy.com/v1/gifs/{endpoint}"))
        .query(&params)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|_| ServerError::failed("Couldn't reach Giphy"))?;
    if !resp.status().is_success() {
        let msg = if resp.status().as_u16() == 429 {
            "Giphy's rate limit was hit; try again later".to_string()
        } else {
            format!("Giphy answered {}", resp.status().as_u16())
        };
        return Err(ServerError::failed(msg));
    }
    let json: serde_json::Value = resp.json().await.map_err(|_| ServerError::failed("Giphy sent a bad answer"))?;
    let page = parse_giphy(&json);
    let mut c = cache().lock().unwrap();
    if c.len() >= 200 {
        c.clear();
    }
    c.insert(key, (std::time::Instant::now(), page.clone()));
    Ok(page)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_giphy_page() {
        let json = serde_json::json!({
            "data": [
                { "id": "abc", "title": "cat GIF", "images": {
                    "fixed_width": { "url": "https://media1.giphy.com/media/abc/200w.gif", "width": "200", "height": "150" },
                    "downsized": { "url": "https://media1.giphy.com/media/abc/giphy.gif", "width": "480", "height": "360" }
                }},
                { "id": "bad", "images": {} }
            ],
            "pagination": { "offset": 0, "count": 30, "total_count": 100 }
        });
        let page = parse_giphy(&json);
        assert_eq!(page.gifs.len(), 1);
        let g = &page.gifs[0];
        assert_eq!((g.preview_width, g.preview_height, g.width, g.height), (200, 150, 480, 360));
        assert_eq!(page.next_offset, Some(30));
        assert_eq!(gif_message(g), "https://media1.giphy.com/media/abc/giphy.gif#480x360");
    }

    #[test]
    fn last_page_has_no_next_offset() {
        let json = serde_json::json!({ "data": [], "pagination": { "offset": 90, "count": 10, "total_count": 100 } });
        assert_eq!(parse_giphy(&json).next_offset, None);
    }

    #[test]
    fn recognizes_only_giphy_media_bodies() {
        assert_eq!(
            gif_body("https://media2.giphy.com/media/abc/giphy.gif#480x360"),
            Some(("https://media2.giphy.com/media/abc/giphy.gif".to_string(), Some((480, 360))))
        );
        assert_eq!(gif_body(" https://i.giphy.com/abc.webp "), Some(("https://i.giphy.com/abc.webp".to_string(), None)));
        assert_eq!(gif_body("https://evil.com/x.gif"), None);
        assert_eq!(gif_body("https://media.giphy.com.evil.com/x.gif"), None);
        assert_eq!(gif_body("https://mediax.giphy.com/x.gif"), None);
        assert_eq!(gif_body("http://media.giphy.com/x.gif"), None);
        assert_eq!(gif_body("look https://media.giphy.com/x.gif"), None);
        assert_eq!(gif_body("https://giphy.com/gifs/cat-abc"), None);
    }
}
