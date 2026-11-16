//! What the active backend can do: the capability report the UI follows.
//!
//! The screens used to assume the RWND server's feature set everywhere; with
//! idea-chat as the backend several of those features do not exist (search
//! over ciphertext, server-fetched link previews, the GIF service, custom
//! emoji, stickers, named roles, invites-as-codes, sync'd custom themes,
//! calls). Each backend answers the same `Capabilities` shape from its own
//! model — no RPC, nothing to await — and the seam caches the answer in one
//! signal that login/connect refresh. The UI reads the signal's fields, so a
//! screen re-renders exactly when a capability it shows changes; features the
//! backend lacks are hidden, not broken buttons.
//!
//! A test hook (`cassette.force_capabilities`, see `crate::robot_hooks`)
//! overrides the cached report at runtime through `apply_override` here, so
//! the robot suite can watch a control hide or show without rebuilding the
//! app against the other backend.

use super::*;
use std::cell::Cell;

/// Where search runs: the server reads plaintext (Cassette), only the device
/// holding the decrypted messages can (idea-chat's search RPC answers the
/// reader's own client-side search; see `docs/idea-chat-gaps.md`), or neither.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SearchSource {
    Server,
    OnDevice,
    None,
}

/// Who builds a link-preview card: the server fetches the URL (Cassette's
/// cached fetch), the message's sender carried the card data at send time,
/// or the feature is off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LinkPreviewSource {
    Server,
    Sender,
    None,
}

/// The call provider carrying voice media, matching `call_media`'s Provider
/// names; `None` means this backend has no voice at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VoiceProvider {
    None,
    IdeaChat,
}

/// The shape every backend reports. One field per feature family the UI can
/// hide; a new feature family gets a new field here, not a new accessor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capabilities {
    pub search: SearchSource,
    pub link_previews: LinkPreviewSource,
    /// Whether the wire is end-to-end encrypted: the server cannot read
    /// message content, so content-driven features move client-side or off.
    pub e2ee: bool,
    pub voice: VoiceProvider,
    pub custom_emoji: bool,
    pub stickers: bool,
    pub themes_sync: bool,
    pub gifs: bool,
    pub roles: bool,
    pub invites: bool,
    pub pins: bool,
}

/// What today's `discord-server` can do: every feature the screens ship
/// against — server search, server-fetched link previews, the GIF service,
/// custom emoji, stickers, named roles, invite codes, the per-account theme
/// store, and idea-chat voice. Nothing encrypted: `e2ee` is false.
pub(crate) const CASSETTE_CAPABILITIES: Capabilities = Capabilities {
    search: SearchSource::Server,
    link_previews: LinkPreviewSource::Server,
    e2ee: false,
    voice: VoiceProvider::IdeaChat,
    custom_emoji: true,
    stickers: true,
    themes_sync: true,
    gifs: true,
    roles: true,
    invites: true,
    pins: true,
};

/// The seam-level capability trait: the active backend answers what it
/// offers, synchronously.
pub trait CapabilitiesApi {
    fn capabilities() -> Capabilities;
}

/// The sentence a voice row shows when the backend has no call provider at
/// all — the control is not a broken button, the row says why.
pub(crate) const VOICE_NOT_ENABLED: &str = "voice is not enabled on this server";

impl CapabilitiesApi for CassetteBackend {
    fn capabilities() -> Capabilities {
        CASSETTE_CAPABILITIES
    }
}

#[cfg(feature = "idea-chat")]
impl CapabilitiesApi for IdeaChatBackend {
    fn capabilities() -> Capabilities {
        Capabilities {
            // The `search` RPC answers the reader's own client-side search
            // over decrypted text (the server never sees plaintext): OnDevice.
            search: SearchSource::OnDevice,
            // No fetcher anywhere and senders carry no card data: off.
            link_previews: LinkPreviewSource::None,
            // The ledger's commits are Sigil-encrypted: true.
            e2ee: true,
            // chat-model speaks VoiceJoined/KeyDelivered, but this build has
            // no bridge to it yet (call_media refuses), so: none.
            voice: VoiceProvider::None,
            // EmojiApi, StickersApi, ThemesApi, GifApi, RolesApi and the
            // per-channel permission overwrites refuse (`not_supported`);
            // invites exist (`create_invite` / `invite_preview` + `join_space`)
            // and pins do not.
            custom_emoji: false,
            stickers: false,
            themes_sync: false,
            gifs: false,
            roles: false,
            invites: true,
            pins: false,
        }
    }
}

thread_local! {
    /// The cached report, minted once inside the app shell (see `install`).
    static CAPABILITIES: Cell<Option<Signal<Capabilities>>> = const { Cell::new(None) };
}

/// Mint and cache the report's signal, once, in the app root's scope
/// (`app()` calls it before the login/shell branch, so the signal survives
/// log-out/log-in cycles: a signal minted inside the shell would die on the
/// first log out and every later login's `refresh` write would trap).
pub(crate) fn install() {
    let _ = CAPABILITIES.with(|c| {
        if let Some(existing) = c.get() {
            let _ = existing;
            return;
        }
        c.set(Some(runtime_core::signal(report())));
    });
}

/// The active backend's own report (uncached): the seed and the refresh.
pub(crate) fn report() -> Capabilities {
    if chat_idea_chat() {
        IdeaChatBackend::capabilities()
    } else {
        CassetteBackend::capabilities()
    }
}

/// The report's signal for UI sites: read `.get()` (or a field) inside a
/// render closure or `switch` key so the screen rebuilds when a capability
/// changes. Falls back to a fresh uncached signal before `install` runs
/// (pre-login screens show no capability rows, so nothing misses the update).
pub(crate) fn capabilities_signal() -> Signal<Capabilities> {
    CAPABILITIES.with(|c| c.get()).unwrap_or_else(move || runtime_core::signal(report()))
}

/// The cached report itself (not reactive): backend-internal logic. UI sites
/// should read `capabilities_signal()` so the reactive dependency is tracked.
pub(crate) fn capabilities() -> Capabilities {
    capabilities_signal().get()
}

/// Refresh the cached report from the active backend (login / connect).
pub(crate) fn refresh() {
    let _ = CAPABILITIES.with(|c| {
        if let Some(s) = c.get() {
            s.set(report());
        }
    });
}

/// Replace the cached report with the active backend's report overridden per
/// `spec`: the force hook's path.
pub(crate) fn force(spec: String) {
    let _ = CAPABILITIES.with(|c| {
        if let Some(s) = c.get() {
            s.set(apply_override(report(), spec));
        }
    });
}

/// The report the cache currently holds, as `key=value` clauses the robot
/// hook's answer can compare field by field.
pub(crate) fn describe() -> String {
    let caps = capabilities();
    let search = match caps.search {
        SearchSource::Server => "server",
        SearchSource::OnDevice => "on_device",
        SearchSource::None => "none",
    };
    let previews = match caps.link_previews {
        LinkPreviewSource::Server => "server",
        LinkPreviewSource::Sender => "sender",
        LinkPreviewSource::None => "none",
    };
    let voice = match caps.voice {
        VoiceProvider::None => "none",
        VoiceProvider::IdeaChat => "idea_chat",
    };
    let e2ee = on_off(caps.e2ee);
    let emoji = on_off(caps.custom_emoji);
    let stickers = on_off(caps.stickers);
    let themes = on_off(caps.themes_sync);
    let gifs = on_off(caps.gifs);
    let roles = on_off(caps.roles);
    let invites = on_off(caps.invites);
    let pins = on_off(caps.pins);
    format!("search={search} link_previews={previews} e2ee={e2ee} voice={voice} custom_emoji={emoji} stickers={stickers} themes_sync={themes} gifs={gifs} roles={roles} invites={invites} pins={pins}")
}

/// A boolean as `true`/`false` for the report's text form.
pub(crate) fn on_off(flag: bool) -> String {
    if flag {
        "true".to_string()
    } else {
        "false".to_string()
    }
}

/// Parse an override spec like `gifs=false e2ee=true search=on_device` into
/// the report with those fields changed, so the force hook (and unit tests)
/// can stage a backend's report without a build. Unknown keys and values are
/// ignored; a failed field keeps the previous value.
pub(crate) fn apply_override(base: Capabilities, spec: String) -> Capabilities {
    let mut out = base;
    for clause in spec.trim().split(' ') {
        if clause.is_empty() {
            continue;
        }
        let pair = clause.split_once('=');
        if pair.is_none() {
            continue;
        }
        let (key, value) = pair.unwrap();
        match (key, value) {
            ("gifs", "true") => out.gifs = true,
            ("gifs", "false") => out.gifs = false,
            ("e2ee", "true") => out.e2ee = true,
            ("e2ee", "false") => out.e2ee = false,
            ("custom_emoji", "true") => out.custom_emoji = true,
            ("custom_emoji", "false") => out.custom_emoji = false,
            ("stickers", "true") => out.stickers = true,
            ("stickers", "false") => out.stickers = false,
            ("themes_sync", "true") => out.themes_sync = true,
            ("themes_sync", "false") => out.themes_sync = false,
            ("roles", "true") => out.roles = true,
            ("roles", "false") => out.roles = false,
            ("invites", "true") => out.invites = true,
            ("invites", "false") => out.invites = false,
            ("pins", "true") => out.pins = true,
            ("pins", "false") => out.pins = false,
            ("link_previews", "server") => out.link_previews = LinkPreviewSource::Server,
            ("link_previews", "sender") => out.link_previews = LinkPreviewSource::Sender,
            ("link_previews", "none") => out.link_previews = LinkPreviewSource::None,
            ("search", "server") => out.search = SearchSource::Server,
            ("search", "on_device") => out.search = SearchSource::OnDevice,
            ("search", "none") => out.search = SearchSource::None,
            ("voice", "none") => out.voice = VoiceProvider::None,
            ("voice", "idea_chat") => out.voice = VoiceProvider::IdeaChat,
            _ => {}
        };
    }
    out
}

/// The search placeholder for a server named `server_name`, per a backend's
/// search source: on-device search says so in the label; `None` (no search
/// anywhere) says the feature is off. Cassette's "Search {name}" unchanged.
pub(crate) fn search_label(server_name: String, search: SearchSource) -> String {
    let base = if server_name.is_empty() {
        "Search".to_string()
    } else {
        format!("Search {server_name}")
    };
    match search {
        SearchSource::Server => base,
        SearchSource::OnDevice => format!("{base} (searches this device)"),
        SearchSource::None => "Search is not available on this server".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cassette_reports_the_full_feature_set() {
        let caps = CassetteBackend::capabilities();
        assert_eq!(caps.search == SearchSource::Server, true);
        assert_eq!(caps.link_previews == LinkPreviewSource::Server, true);
        assert_eq!(caps.e2ee, false);
        assert_eq!(caps.voice == VoiceProvider::IdeaChat, true);
        assert_eq!(caps.custom_emoji, true);
        assert_eq!(caps.stickers, true);
        assert_eq!(caps.themes_sync, true);
        assert_eq!(caps.gifs, true);
        assert_eq!(caps.roles, true);
        assert_eq!(caps.invites, true);
        assert_eq!(caps.pins, true);
    }

    #[cfg(feature = "idea-chat")]
    #[test]
    fn ideachat_reports_the_encrypted_wire_set() {
        let caps = IdeaChatBackend::capabilities();
        assert_eq!(caps.e2ee, true);
        assert_eq!(caps.search == SearchSource::OnDevice, true);
        assert_eq!(caps.link_previews == LinkPreviewSource::None, true);
        assert_eq!(caps.voice == VoiceProvider::None, true);
        assert_eq!(caps.gifs, false);
        assert_eq!(caps.invites, true);
    }

    #[test]
    fn an_override_flips_named_fields_only() {
        let forced = apply_override(CassetteBackend::capabilities(), "gifs=false e2ee=true".to_string());
        assert_eq!(forced.gifs, false);
        assert_eq!(forced.e2ee, true);
        assert_eq!(forced.roles, true);
    }

    #[test]
    fn a_bad_or_unknown_clause_keeps_the_previous_value() {
        let forced = apply_override(CassetteBackend::capabilities(), "gifs=maybe nonsense=1 e2ee=".to_string());
        assert_eq!(forced.gifs, true);
        assert_eq!(forced.e2ee, false);
    }

    #[test]
    fn repeated_clauses_last_one_wins() {
        let forced = apply_override(CassetteBackend::capabilities(), "gifs=false gifs=true".to_string());
        assert_eq!(forced.gifs, true);
    }

    #[test]
    fn the_search_label_names_the_search_source() {
        assert_eq!(search_label("Acme".to_string(), SearchSource::Server), "Search Acme".to_string());
        assert_eq!(search_label("".to_string(), SearchSource::Server), "Search".to_string());
        assert_eq!(
            search_label("Acme".to_string(), SearchSource::OnDevice),
            "Search Acme (searches this device)".to_string(),
        );
    }
}
