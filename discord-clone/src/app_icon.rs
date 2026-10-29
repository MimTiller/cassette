//! The cassette in the browser tab. Five looks, picked under Appearance and
//! saved like the theme; `index.html` starts with Sunset inline so the tab
//! has an icon before the app loads.

const KEY_ICON: &str = "idealyst.discord.appIcon";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppIcon {
    Sunset,
    Memphis,
    Miami,
    Bubblegum,
    Classic,
}

impl AppIcon {
    pub const ALL: [AppIcon; 5] = [AppIcon::Sunset, AppIcon::Memphis, AppIcon::Miami, AppIcon::Bubblegum, AppIcon::Classic];

    pub fn key(self) -> &'static str {
        match self {
            AppIcon::Sunset => "sunset",
            AppIcon::Memphis => "memphis",
            AppIcon::Miami => "miami",
            AppIcon::Bubblegum => "bubblegum",
            AppIcon::Classic => "classic",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            AppIcon::Sunset => "Sunset",
            AppIcon::Memphis => "Memphis",
            AppIcon::Miami => "Miami",
            AppIcon::Bubblegum => "Bubblegum",
            AppIcon::Classic => "Classic",
        }
    }

    pub fn from_key(key: &str) -> Option<AppIcon> {
        AppIcon::ALL.into_iter().find(|i| i.key() == key)
    }

    fn svg(self) -> &'static str {
        match self {
            AppIcon::Sunset => include_str!("../icons/sunset.svg"),
            AppIcon::Memphis => include_str!("../icons/memphis.svg"),
            AppIcon::Miami => include_str!("../icons/miami.svg"),
            AppIcon::Bubblegum => include_str!("../icons/bubblegum.svg"),
            AppIcon::Classic => include_str!("../icons/classic.svg"),
        }
    }

    /// The icon as a `data:` URL, for the tab and for `image(src = …)`.
    pub fn data_url(self) -> String {
        svg_data_url(self.svg())
    }
}

/// `svg` as a `data:image/svg+xml,…` URL: double quotes become single ones
/// and anything a URL can't carry as-is is percent-encoded.
fn svg_data_url(svg: &str) -> String {
    let mut out = String::from("data:image/svg+xml,");
    for c in svg.replace('"', "'").split_whitespace().collect::<Vec<_>>().join(" ").chars() {
        if c.is_ascii_alphanumeric() || " '=/:,.()-".contains(c) {
            out.push(c);
        } else {
            let mut buf = [0u8; 4];
            for b in c.encode_utf8(&mut buf).bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        }
    }
    out
}

/// The saved icon, Sunset if none.
pub fn current() -> AppIcon {
    crate::auth::store()
        .get_now(KEY_ICON)
        .ok()
        .flatten()
        .and_then(|k| AppIcon::from_key(&k))
        .unwrap_or(AppIcon::Sunset)
}

/// Save `icon` and show it in the tab.
pub fn choose(icon: AppIcon) {
    let _ = crate::auth::store().set_now(KEY_ICON, icon.key());
    show(icon);
}

/// Point the page's `<link rel="icon">` at `icon` (web only).
pub fn show(icon: AppIcon) {
    #[cfg(target_arch = "wasm32")]
    if let Some(link) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.query_selector("link[rel=icon]").ok().flatten())
    {
        let _ = link.set_attribute("href", &icon.data_url());
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = icon;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip() {
        for i in AppIcon::ALL {
            assert_eq!(AppIcon::from_key(i.key()), Some(i));
        }
        assert_eq!(AppIcon::from_key("walkman"), None);
    }

    #[test]
    fn data_url_escapes_what_it_must() {
        let url = svg_data_url("<svg a=\"#fff\">\n  <g/>\n</svg>");
        assert_eq!(url, "data:image/svg+xml,%3Csvg a='%23fff'%3E %3Cg/%3E %3C/svg%3E");
        assert!(AppIcon::Sunset.data_url().contains("%23ff7a00"));
    }
}
