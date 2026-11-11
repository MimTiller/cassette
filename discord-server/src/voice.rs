//! Voice calls: the provider-agnostic call seam.
//!
//! Whoever hosts a Cassette server picks the voice-call provider for it:
//! today LiveKit, later Nicho's idea-chat calls (QUIC relay + SFrame E2EE +
//! signed grants, see `docs/idea-chat-wire.md`). Joining a voice channel
//! returns a [`CallTicket`] naming the provider and everything its client
//! needs; the UI talks only to that ticket and the client's `CallApi` area,
//! never to a provider directly.
//!
//! The provider comes from the environment, read on every call (the
//! server's own choice, not the client's): `CALL_PROVIDER` = `livekit` |
//! `none` (default `none`; `idea-chat` is reserved and refused until its
//! wire lands), plus `LIVEKIT_URL`, `LIVEKIT_API_KEY` and `LIVEKIT_API_SECRET`
//! for LiveKit. With `none` a join refuses, and the voice UI shows the
//! not-enabled state instead of a connect.
//!
//! A LiveKit ticket carries the socket URL and an access token: a JWT
//! (HS256) signed with the API secret, claims `iss` = the API key, `sub` =
//! the user id, `name` = the display name, `iat`/`nbf`/`exp` (short-lived),
//! and a `video` grant object for the channel's room (`rwnd-ch-<id>`), whose
//! `canPublish` follows the Speak permission. Joining requires the Connect
//! permission, checked in the channel (a voice channel's permission
//! question is the channel's, never the server's).
//!
//! Who is in which voice channel lives on the server - per-session state,
//! no DB column - and rides the gateway as `WsServerMsg::VoiceRoster`
//! whole-list frames to everyone who can see the channel, including the
//! socket itself (sent on connect with the initial state). Leaving a
//! channel, joining another, or a user's last gateway socket closing
//! clears their entry.

use server::{server, ServerError};

#[cfg(feature = "server")]
use crate::db::Db;
#[cfg(feature = "server")]
use crate::permissions::{Permissions, ServerContext};
#[cfg(feature = "server")]
use crate::roles::ChannelKind;
#[cfg(feature = "server")]
use crate::session::acting_user;
#[cfg(feature = "server")]
use crate::realtime::{channel_audience, hub};
#[cfg(feature = "server")]
use crate::server_settings::{next_frame_seq, WsServerMsg};

#[cfg(feature = "server")]
use hmac::{Hmac, Mac};
#[cfg(feature = "server")]
use sha2::Sha256;

use serde::{Deserialize, Serialize};

/// How long a LiveKit access token stays valid: short, since it only
/// authorizes a connection the user is about to make.
#[cfg(feature = "server")]
const TOKEN_TTL_SECS: i64 = 10 * 60;

/// The refusal when voice is disabled: shown to the user verbatim.
#[cfg(feature = "server")]
const NOT_ENABLED: &'static str = "voice is not enabled on this server";

// ── the wire types (both builds) ───────────────────────────────────────────

/// A call ticket: what joining a voice channel hands the client's call
/// provider. Each variant names a provider and everything it needs to
/// connect; the client treats it as opaque provider data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CallTicket {
    /// A LiveKit call: the server's socket URL and an access token for the
    /// viewer's identity, plus whether they may publish audio (their Speak
    /// permission, which the client's mic control follows).
    LiveKit {
        url: String,
        token: String,
        can_publish: bool,
    },
    /// Reserved: Nicho's idea-chat calls (QUIC relay + SFrame E2EE + signed
    /// grants - see `docs/idea-chat-wire.md`). Not issued until that wire
    /// lands; a server configured for it refuses for now.
    IdeaChat {
        relay: String,
        grant: String,
    },
}

/// One connected user's display card in a voice roster: who to draw under
/// the channel row, without the client looking anyone up. Mute/deafen live
/// on the server (a browser's own toggle is invisible to peers) so every
/// client draws the same icons, and the flags ride the whole-list frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceUser {
    pub user_id: i64,
    pub username: String,
    /// Their avatar (a `/media/…` path), `None` = default initial circle.
    #[serde(default)]
    pub avatar_url: Option<String>,
    /// Their mic is off (published audio is suppressed).
    #[serde(default)]
    pub muted: bool,
    /// Their incoming audio is off (they hear nobody).
    #[serde(default)]
    pub deafened: bool,
    /// Their camera is on (they publish a video track). Server-side like
    /// the mute flags: one browser's own grant is invisible to peers, so
    /// every client draws the same tiles.
    #[serde(default)]
    pub camera: bool,
    /// They are screen-sharing (a screen track plus its audio where the
    /// browser allows it).
    #[serde(default)]
    pub screen_share: bool,
}

/// Join the voice channel `channel_id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JoinVoiceInput {
    pub channel_id: i64,
}

/// Leave the voice channel `channel_id` the caller is in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeaveVoiceInput {
    pub channel_id: i64,
}

/// What leaving answered: whether the user was in a voice channel at all.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeaveVoiceResult {
    pub was_in: bool,
}

/// Ask the server what its voice state is: the enabled call provider
/// (`livekit` | `none` - for the not-enabled UI state; secret material is
/// never handed out) and, when the caller is already in `channel_id`, the
/// ticket that re-joins them, so a fresh browser restores its voice bar
/// without a new click.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceStateInput {
    pub channel_id: i64,
}

/// Set the caller's mute/deafen flags for the voice channel they are in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceFlagsInput {
    pub channel_id: i64,
    pub muted: bool,
    pub deafened: bool,
    /// The caller's camera is on: they publish a video track alongside
    /// their audio. Gated like publishing audio (Speak) — RWND has no
    /// separate Stream bit, so Speak is the publish permission.
    #[serde(default)]
    pub camera: bool,
    /// The caller is screen-sharing (a screen track plus its audio where
    /// the browser allows it).
    #[serde(default)]
    pub screen_share: bool,
}

/// The voice-state answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceStateResult {
    pub provider: String,
    pub ticket: Option<CallTicket>,
    /// The caller's own mute/deafen/camera/screen-share flags for this
    /// channel, so a fresh browser restores its icons and tiles without a
    /// new click.
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub deafened: bool,
    #[serde(default)]
    pub camera: bool,
    #[serde(default)]
    pub screen_share: bool,
}

// ── the provider config ────────────────────────────────────────────────────

/// The call-provider settings: which provider answers joins, and LiveKit's
/// credentials when it does. Read from the environment on every join, so
/// a server can't be steered by its clients.
#[cfg(feature = "server")]
pub struct CallConfig {
    /// `livekit` | `none` (| `idea-chat`, reserved).
    pub provider: String,
    pub livekit_url: String,
    pub livekit_api_key: String,
    pub livekit_api_secret: String,
}

#[cfg(feature = "server")]
impl CallConfig {
    pub fn from_env() -> Self {
        CallConfig {
            provider: std::env::var("CALL_PROVIDER").unwrap_or_else(|_| "none".to_string()),
            livekit_url: std::env::var("LIVEKIT_URL").unwrap_or_default(),
            livekit_api_key: std::env::var("LIVEKIT_API_KEY").unwrap_or_default(),
            livekit_api_secret: std::env::var("LIVEKIT_API_SECRET").unwrap_or_default(),
        }
    }

    /// The refusal for a join under this config, or `None` when a provider
    /// answers. `idea-chat` is reserved: the wire is specced but not
    /// implemented, so a server configured for it says so plainly.
    pub fn refusal(&self) -> Option<String> {
        match self.provider.as_str() {
            "livekit" =>
                if self.livekit_url.is_empty()
                    || self.livekit_api_key.is_empty()
                    || self.livekit_api_secret.is_empty()
                {
                    Some(
                        "livekit is configured but LIVEKIT_URL, LIVEKIT_API_KEY or LIVEKIT_API_SECRET is missing".into(),
                    )
                } else {
                    None
                }
            "idea-chat" => Some("voice calls over idea-chat are not available yet".into()),
            _ => Some(NOT_ENABLED.to_string()),
        }
    }
}

/// The provider name the client compares [`VoiceStateResult.provider`]
/// against to decide between the live and the not-enabled voice UI.
pub const PROVIDER_LIVEKIT: &'static str = "livekit";
pub const PROVIDER_NONE: &'static str = "none";

// ── the JWT ────────────────────────────────────────────────────────────────

/// The LiveKit room for a channel: one room per channel id.
#[cfg(feature = "server")]
pub fn room_for(channel_id: i64) -> String {
    format!("rwnd-ch-{channel_id}")
}

/// Base64url (no padding), for a JWT's header/claims segments.
#[cfg(feature = "server")]
pub fn b64url(bytes: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = *chunk.first().unwrap_or(&0) as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        out.push(A[((b0 >> 2) & 63) as usize] as char);
        out.push(A[((b0 << 4 | b1 >> 4) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(A[((b1 << 2 | b2 >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(A[(b2 & 63) as usize] as char);
        }
    }
    out
}

/// The header segment every token here carries: `{"alg":"HS256","typ":"JWT"}`,
/// base64url, no padding.
#[cfg(feature = "server")]
const JWT_HEADER: &'static str = "eyJhbG...VCJ9";

#[cfg(feature = "server")]
type HmacSha256 = Hmac<Sha256>;

/// Sign a JWT's header/claims body with `secret`: the signature segment.
#[cfg(feature = "server")]
fn jwt_sign(body: &[u8], secret: &[u8]) -> String {
    // HMAC accepts any key length; the error arm is unreachable.
    let mut mac = HmacSha256::new_from_slice(secret).expect("hmac accepts any key length");
    mac.update(body);
    b64url(&mac.finalize().into_bytes())
}

/// The JWT claims JSON for a LiveKit access token: `iss` names the issuing
/// service, `sub` is the identity, `name` its display name, `iat`/`nbf`/`exp`
/// the short validity window pinned to `now`, and the `video` grant object
/// opens room `room` (`canPublish` from the Speak permission).
#[cfg(feature = "server")]
pub fn livekit_claims(api_key: String, identity: i64, name: String, room: String, can_publish: bool, now: i64) -> String {
    let exp = now + TOKEN_TTL_SECS;
    let nbf = now - TOKEN_TTL_SECS;
    format!(
        "{{\"iss\":\"{api_key}\",\"sub\":\"{identity}\",\"name\":\"{name}\",\"iat\":{now},\"nbf\":{nbf},\"exp\":{exp},\"video\":{{\"room\":\"{room}\",\"roomJoin\":true,\"canPublish\":{can_publish},\"canSubscribe\":true,\"canPublishData\":{can_publish}}}}}"
    )
}

#[cfg(feature = "server")]
pub fn issue_access_token_at(
    api_key: String,
    secret: String,
    identity: i64,
    name: String,
    room: String,
    can_publish: bool,
    now: i64,
) -> String {
    let claims = livekit_claims(api_key, identity, name, room, can_publish, now);
    let claims_b64 = b64url(claims.as_bytes());
    let body = format!("{JWT_HEADER}.{claims_b64}");
    let sig = jwt_sign(body.as_bytes(), secret.as_bytes());
    format!("{body}.{sig}")
}

/// A LiveKit access token, live now.
#[cfg(feature = "server")]
pub fn issue_access_token(
    api_key: String,
    secret: String,
    identity: i64,
    name: String,
    room: String,
    can_publish: bool,
) -> String {
    issue_access_token_at(api_key, secret, identity, name, room, can_publish, now_secs())
}

// ── the connected-voice roster (process state, no DB column) ───────────────────────────────

/// Who is in which voice channel: `user_id -> channel_id`, one channel per
/// user (the last one they joined). Kept in process memory like presence,
/// a session state, not persisted data.
#[cfg(feature = "server")]
static VOICE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<i64, i64>>> =
    std::sync::OnceLock::new();

/// Fresh empty map for the static, off the static-initializer path (which
#[cfg(feature = "server")]
/// only allows const calls).
fn new_voice_map() -> std::sync::Mutex<std::collections::HashMap<i64, i64>> {
    std::sync::Mutex::new(std::collections::HashMap::new())
}

#[cfg(feature = "server")]
/// The voice channel the user is in, or `None`.
pub fn voice_of(user_id: i64) -> Option<i64> {
    let voice = VOICE.get_or_init(new_voice_map)
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    voice.get(&user_id).copied()
}

#[cfg(feature = "server")]
/// The ids connected to `channel_id`, ordered (a stable display order).
fn roster_ids(channel_id: i64) -> Vec<i64> {
    let mut ids = VOICE.get_or_init(new_voice_map)
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .iter()
        .filter(move |(_, c)| **c == channel_id)
        .map(move |(uid, _)| *uid)
        .collect::<Vec<i64>>();
    ids.sort_unstable();
    ids
}
#[cfg(feature = "server")]
/// Per-user mute/deafen/camera/screen-share flags for their current voice
/// connection: process state like the roster itself, cleared when they leave
/// the channel. LiveKit only ever sees one browser's own toggles and grants,
/// so the flags live server-side and ride every VoiceRoster frame — that is
/// what lets a peer draw the muted icon and a video tile, and what a fresh
/// client restores from `voice_state`.
static VOICE_FLAGS: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<i64, (bool, bool, bool, bool)>>> =
    std::sync::OnceLock::new();
#[cfg(feature = "server")]
fn flags_of(user_id: i64) -> (bool, bool, bool, bool) {
    let m = VOICE_FLAGS.get_or_init(new_voice_map_flags)
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    m.get(&user_id).copied().unwrap_or((false, false, false, false))
}
#[cfg(feature = "server")]
fn new_voice_map_flags() -> std::sync::Mutex<std::collections::HashMap<i64, (bool, bool, bool, bool)>> {
    std::sync::Mutex::new(std::collections::HashMap::new())
}

#[cfg(feature = "server")]
/// The (user, muted, deafened) cards for `channel_id`, ordered by user id
/// (a stable display order): the pure read both the roster frames and the
/// tests go through.
pub fn roster_entries(db: Db, channel_id: i64) -> Vec<VoiceUser> {
    roster_ids(channel_id)
        .into_iter()
        .map(move |uid| {
            let (muted, deafened, camera, screen_share) = flags_of(uid);
            VoiceUser {
                user_id: uid,
                username: db.user_by_id(uid).map(|(_, u)| u).unwrap_or_default(),
                avatar_url: db.user_avatar(uid),
                muted,
                deafened,
                camera,
                screen_share,
            }
        })
        .collect::<Vec<VoiceUser>>()
}

#[cfg(feature = "server")]
/// Make `user_id`'s voice channel `channel_id` (`None` clears it) and push
/// the whole rosters of the old and new channels to their viewers, so every
/// client that can see them - the user's own socket included - replaces the
/// lists in one frame.
pub fn set_voice(db: Db, user_id: i64, channel_id: Option<i64>) {
    let old = {
        let mut voice = VOICE.get_or_init(new_voice_map)
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let old = voice.remove(&user_id);
        if let Some(cid) = channel_id {
            voice.insert(user_id, cid);
        }
        old
    };
    if channel_id.is_none() {
        // Leaving drops the connection entirely: the flags (mute, deafen,
        // camera, screen share) go with it, so a later join starts clean.
        VOICE_FLAGS
            .get_or_init(new_voice_map_flags)
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&user_id);
    }
    if old == channel_id {
        return;
    }
    for cid in [old, channel_id].into_iter().flatten() {
        announce_roster(db.clone(), cid);
    }
}

#[cfg(feature = "server")]
/// Push `channel_id`'s current roster (ordered by user id, a stable display
/// order) to the channel's viewers. An empty roster is a frame too: a client
/// must be able to drop stale rows.
pub fn announce_roster(db: Db, channel_id: i64) {
    let audience = channel_audience(&db, channel_id);
    hub().publish(
        &audience,
        &WsServerMsg::VoiceRoster {
            channel_id,
            entries: roster_entries(db, channel_id),
            seq: next_frame_seq(),
        },
    );
}

#[cfg(feature = "server")]
/// A new gateway socket whose user still holds a voice channel (another
/// socket is live) gets that channel's roster with its initial state, so
/// the fresh client shows who is connected without waiting for a change.
pub fn announce_voice_on_connect(db: Db, user_id: i64) {
    if let Some(cid) = voice_of(user_id) {
        announce_roster(db.clone(), cid);
    }
}

#[cfg(feature = "server")]
/// The user's last gateway socket closed: they are no longer connected,
/// so their voice row drops and the channel's viewers get the frame.
pub fn clear_voice_on_disconnect(db: Db, user_id: i64) {
    if voice_of(user_id).is_some() {
        set_voice(db.clone(), user_id, None);
    }
}

// ── the service: the rules, testable without HTTP ──────────────────────────

/// The voice-call service: joins answer with a provider ticket and record
/// the roster; permission questions go through the channel's own
/// permission layer (`ServerContext::in_channel`), never the server's.
#[cfg(feature = "server")]
pub struct VoiceService {
    db: Db,
}

#[cfg(feature = "server")]
impl VoiceService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// The caller's channel permissions in `channel_id`, after checking the
    /// channel exists, is a voice channel, is visible, and holds Connect:
    /// the join gate minus the provider check, so a grant re-ask answers
    /// identically.
    pub fn authorize(&self, caller: i64, channel_id: i64) -> Result<Permissions, String> {
        let (_id, server_id, _name, kind, _pos, _parent) =
            self.db.channel_full(channel_id).ok_or("channel not found")?;
        if ChannelKind::from_db_str(&kind) != Some(ChannelKind::Voice) {
            return Err("not a voice channel".into());
        }
        let ctx = ServerContext::load(&self.db, server_id).ok_or("channel not found")?;
        let member = ctx.member(caller).ok_or("not a member of this server")?;
        let perms = ctx.in_channel(&member, channel_id);
        if !perms.contains(Permissions::VIEW_CHANNEL) {
            return Err("channel not found".into());
        }
        if !perms.contains(Permissions::CONNECT) {
            return Err("only members who can connect to voice may join a voice channel".into());
        }
        Ok(perms)
    }

    /// The provider ticket for an authorized join: LiveKit's URL plus an
    /// access token whose `video` grant opens the channel's room, canPublish
    /// from Speak. The provider check runs first, so a server with voice
    /// disabled says so plainly even for a channel the caller could join.
    pub fn ticket(&self, caller: i64, channel_id: i64, config: CallConfig) -> Result<CallTicket, String> {
        if let Some(reason) = config.refusal() {
            return Err(reason);
        }
        let perms = self.authorize(caller, channel_id)?;
        let can_publish = perms.contains(Permissions::SPEAK);
        let name = self.db.user_display_name(caller)
            .unwrap_or_else(|| self.db.user_by_id(caller).map(|(_, u)| u).unwrap_or_default());
        Ok(CallTicket::LiveKit {
            url: config.livekit_url,
            token: issue_access_token(
                config.livekit_api_key,
                config.livekit_api_secret,
                caller,
                name,
                room_for(channel_id),
                can_publish,
            ),
            can_publish,
        })
    }

    /// Join: authorize and issue, then record (and broadcast) the roster.
    pub fn join(&self, caller: i64, channel_id: i64, config: CallConfig) -> Result<CallTicket, String> {
        let ticket = self.ticket(caller, channel_id, config);
        if ticket.is_ok() {
            set_voice(self.db.clone(), caller, Some(channel_id));
        }
        ticket
    }

    /// Set the caller's mute/deafen/camera/screen-share flags for `channel_id`
    /// (they must be in it) and rebroadcast the channel's roster, so every
    /// viewer's copy shows the icons and tiles. Mute gates publishing, deafen
    /// gates playback; publishing a camera or a screen share is gated like
    /// publishing audio — Speak (RWND has no separate Stream bit), so a
    /// caller without Speak who taps camera is refused with a plain sentence.
    pub fn set_flags(
        &self,
        caller: i64,
        channel_id: i64,
        muted: bool,
        deafened: bool,
        camera: bool,
        screen_share: bool,
    ) -> Result<(), String> {
        if voice_of(caller) != Some(channel_id) {
            return Err("not connected to this voice channel".into());
        }
        let perms = self.authorize(caller, channel_id)?;
        if camera || screen_share {
            if !perms.contains(Permissions::SPEAK) {
                return Err("only members who can speak may turn on the camera or share their screen".into());
            }
        }
        VOICE_FLAGS
            .get_or_init(new_voice_map_flags)
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(caller, (muted, deafened, camera, screen_share));
        announce_roster(self.db.clone(), channel_id);
        Ok(())
    }

    /// Leave: clear the caller's voice entry if they are in this channel.
    pub fn leave(&self, caller: i64, channel_id: i64) -> Result<LeaveVoiceResult, String> {
        let was_in = voice_of(caller) == Some(channel_id);
        if was_in {
            set_voice(self.db.clone(), caller, None);
        }
        Ok(LeaveVoiceResult { was_in })
    }

    /// The server's call provider name - `livekit` | `none` - for clients
    /// that ask, so secret material is never handed out.
    pub fn provider(&self) -> String {
        CallConfig::from_env().provider
    }
}

// ── endpoints ──────────────────────────────────────────────────────────────

#[cfg(feature = "server")]
fn db_from_state() -> Result<Db, ServerError> {
    server::use_state::<Db>()
        .ok_or_else(|| ServerError::failed("database not installed"))
}

#[cfg(feature = "server")]
fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(1_000)
}

/// Join a voice channel: check the channel, its Connect permission, and
/// the server's provider, answer with the provider's call ticket, and
/// record (and broadcast) the roster. A refusal is the provider's own
/// clear "not enabled" sentence, shown verbatim.
#[server]
pub async fn join_voice(input: JoinVoiceInput) -> Result<CallTicket, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let ticket = VoiceService::new(db.clone())
        .join(caller, input.channel_id, CallConfig::from_env())
        .map_err(ServerError::failed)?;
    Ok(ticket)
}

/// Leave the voice channel the caller is in: clear their roster entry.
#[server]
pub async fn leave_voice(input: LeaveVoiceInput) -> Result<LeaveVoiceResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let left = VoiceService::new(db.clone())
        .leave(caller, input.channel_id)
        .map_err(ServerError::failed)?;
    Ok(left)
}

/// Set the caller's mute/deafen flags for the voice channel they are in and
/// rebroadcast the channel's roster, so every viewer draws the same icons.
#[server]
pub async fn set_voice_flags(input: VoiceFlagsInput) -> Result<(), ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    VoiceService::new(db.clone())
        .set_flags(
            caller,
            input.channel_id,
            input.muted,
            input.deafened,
            input.camera,
            input.screen_share,
        )
        .map_err(ServerError::failed)?;
    Ok(())
}

/// The server's voice state: its enabled call provider, plus the ticket for
/// `channel_id` when the caller is already in it, so a fresh client restores
/// its voice bar without a new click. Never leaks provider secrets.
#[server]
pub async fn voice_state(input: VoiceStateInput) -> Result<VoiceStateResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let config = CallConfig::from_env();
    let service = VoiceService::new(db.clone());
    let provider = service.provider();
    let connected = Some(input.channel_id) == voice_of(caller);
    let ticket =
        if connected {
            service.ticket(caller, input.channel_id, config).ok()
        } else {
            None
        };
    let (muted, deafened, camera, screen_share) =
        if connected { flags_of(caller) } else { (false, false, false, false) };
    Ok(
        VoiceStateResult {
            provider,
            ticket,
            muted,
            deafened,
            camera,
            screen_share,
        },
    )
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use std::path::Path;

    const MIGRATIONS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/migrations");

    fn exec(db: &Db, sql: &str) {
        db.lock().unwrap().execute_batch(sql).unwrap();
    }

    /// A temp DB: alice (1) owns server 1 (owners bypass overwrites, like
    /// Discord), dave (4) is a plain member. @everyone (31 | Connect | Speak)
    /// grants view, connect and speak; channel 20 is a joinable voice room, 21
    /// denies Connect to @everyone, and 10 is a text channel.
    fn fixture() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("t.db");
        let db = Db::migrate(&db_path, Path::new(MIGRATIONS)).unwrap();
        exec(
            &db,
            r"INSERT INTO users (id, username, email, created_at, display_name) VALUES
                (1,'alice','a@x',1,'Alice'),(4,'dave','d@x',1,'Dave');
             INSERT INTO servers (id, name, owner_id, created_at, everyone_permissions)
                VALUES (1,'one',1,1,31 | 65536 | 131072);
             INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES
                (1,1,'owner',1),(1,4,'member',1);
             INSERT INTO channels (id, server_id, name, kind, position, created_at) VALUES
                (10,1,'general','text',0,1),(20,1,'meet','voice',1,1),(21,1,'locked','voice',2,1),(22,1,'viewonly','voice',3,1);
             INSERT INTO channel_overwrites (channel_id, target_kind, target_id, allow, deny)
                VALUES (21,'everyone',0,0,65536),(22,'everyone',0,0,131072);",
        );
        (dir, db)
    }

    fn live_config() -> CallConfig {
        CallConfig {
            provider: "livekit".to_string(),
            livekit_url: "ws://127.0.0.1:7880".to_string(),
            livekit_api_key: "devkey".to_string(),
            livekit_api_secret: "secret".to_string(),
        }
    }

    fn none_config() -> CallConfig {
        CallConfig {
            provider: "none".to_string(),
            livekit_url: "".to_string(),
            livekit_api_key: "".to_string(),
            livekit_api_secret: "".to_string(),
        }
    }

    fn idea_chat_config() -> CallConfig {
        CallConfig {
            provider: "idea-chat".to_string(),
            livekit_url: "".to_string(),
            livekit_api_key: "".to_string(),
            livekit_api_secret: "".to_string(),
        }
    }

    #[test]
    fn connect_required_to_join() {
        let (_dir, db) = fixture();
        let svc = VoiceService::new(db.clone());
        assert!(svc.authorize(1, 20).is_ok(), "the owner holds Connect");
        assert!(svc.authorize(4, 20).is_ok(), "dave holds Connect via @everyone");
        assert_eq!(
            svc.authorize(4, 21).unwrap_err(),
            "only members who can connect to voice may join a voice channel",
            "channel 21 denies Connect to @everyone",
        );
        assert_eq!(svc.authorize(4, 10).unwrap_err(), "not a voice channel");
    }

    #[test]
    fn provider_refusals_are_clear_text() {
        let (_dir, db) = fixture();
        let svc = VoiceService::new(db.clone());
        assert_eq!(
            svc.join(4, 20, none_config()).unwrap_err(),
            NOT_ENABLED,
            "the not-enabled sentence is shown verbatim",
        );
        assert_eq!(
            svc.join(4, 20, idea_chat_config()).unwrap_err(),
            "voice calls over idea-chat are not available yet",
            "the reserved provider says so plainly",
        );
        assert_eq!(
            svc.join(4, 20, CallConfig {
                provider: "livekit".to_string(),
                livekit_url: "".to_string(),
                livekit_api_key: "".to_string(),
                livekit_api_secret: "".to_string(),
            }).unwrap_err(),
            "livekit is configured but LIVEKIT_URL, LIVEKIT_API_KEY or LIVEKIT_API_SECRET is missing",
            "a half-configured provider refuses with its own sentence",
        );
    }

    #[test]
    fn livekit_ticket_shape() {
        let (_dir, db) = fixture();
        let svc = VoiceService::new(db.clone());
        match svc.ticket(4, 20, live_config()) {
            Ok(CallTicket::LiveKit { url, token, can_publish }) => {
                assert_eq!(url, "ws://127.0.0.1:7880");
                assert!(can_publish, "dave has Speak via @everyone");
                let (body, sig) = token.rsplit_once('.').unwrap();
                let (header, claims_b64) = body.rsplit_once('.').unwrap();
                assert_eq!(header, JWT_HEADER, "alg HS256, typ JWT");
                assert!(!sig.is_empty(), "three non-empty segments");
                assert!(!claims_b64.is_empty(), "the claims segment is there");
            }
            Ok(other) => panic!("livekit config issued {other:?}"),
            Err(reason) => panic!("{reason}"),
        }
    }

    #[test]
    fn token_claims_and_signature_verify() {
        // Re-signing the exact header/claims body with the API secret must
        // reproduce a pinned token byte-for-byte: that pins the signature,
        // and the claims JSON pins iss/sub/name, the short nbf/exp window,
        // and the video grant for the room derived from the channel id.
        let now = 1_700_000_000;
        let exp = now + TOKEN_TTL_SECS;
        let claims = livekit_claims(
            "devkey".to_string(),
            7,
            "Bob".to_string(),
            room_for(20),
            false,
            now,
        );
        assert!(claims.contains("\"iss\":\"devkey\""), "iss is the api key");
        assert!(claims.contains("\"sub\":\"7\""), "sub is the identity");
        assert!(claims.contains("\"name\":\"Bob\""), "name is the display name");
        let nbf_at = now - TOKEN_TTL_SECS;
        let nbf = format!("\"nbf\":{nbf_at}");
        assert!(
            claims.contains(nbf.as_str()),
            "not valid before a TTL-width window",
        );
        let exp_needle = format!("\"exp\":{exp}");
        assert!(
            claims.contains(exp_needle.as_str()),
            "expires after the short TTL window",
        );
        assert!(claims.contains("\"room\":\"rwnd-ch-20\""), "room from the channel id");
        assert!(claims.contains("\"canPublish\":false"), "no Speak -> cannot publish");
        let token = issue_access_token_at(
            "devkey".to_string(),
            "secret".to_string(),
            7,
            "Bob".to_string(),
            room_for(20),
            false,
            now,
        );
        let claims_b64 = b64url(claims.as_bytes());
        let body = format!("{JWT_HEADER}.{claims_b64}");
        let sig = jwt_sign(body.as_bytes(), "secret".as_bytes());
        let rebuilt = format!("{body}.{sig}");
        assert_eq!(rebuilt, token, "the HMAC of the body is the token");
    }

    #[test]
    fn set_voice_flags_makes_the_roster_show_it() {
        // Allowed caller: joining then muting lands the flag on the roster
        // cards; a caller who is not connected to the channel is refused
        // with a plain sentence, and so is one whose channel differs.
        let (_dir, db) = fixture();
        let svc = VoiceService::new(db.clone());
        assert!(svc.join(4, 20, live_config()).is_ok(), "dave joins");
        assert!(svc.set_flags(4, 20, true, false, false, false).is_ok(), "dave mutes");
        let cards = roster_entries(db.clone(), 20);
        assert_eq!(cards.len(), 1, "one entry");
        assert!(cards[0].muted, "the card carries the mute");
        assert!(!cards[0].deafened, "deafened untouched");
        assert!(!cards[0].camera, "camera untouched");
        assert!(!cards[0].screen_share, "screen share untouched");
        assert_eq!(
            svc.set_flags(1, 20, true, true, false, false).unwrap_err(),
            "not connected to this voice channel",
            "alice never joined"
        );
        assert!(svc.leave(4, 20).unwrap().was_in, "dave leaves");
        assert!(roster_entries(db, 20).is_empty(), "the roster is gone");
    }

    #[test]
    fn camera_and_share_follow_speak() {
        // Publishing a camera or a screen share is gated like publishing
        // audio: Speak. Channel 22 denies Speak to @everyone, so dave may
        // join and mute there but any tap that turns video on is refused
        // with a plain sentence; on the open channel 20 it lands on the
        // roster card, which is what a peer's client draws the tile from.
        let (_dir, db) = fixture();
        let svc = VoiceService::new(db.clone());
        assert!(svc.join(4, 22, live_config()).is_ok(), "dave holds Connect on 22");
        assert_eq!(
            svc.set_flags(4, 22, false, false, true, false).unwrap_err(),
            "only members who can speak may turn on the camera or share their screen",
            "camera refused without Speak",
        );
        assert_eq!(
            svc.set_flags(4, 22, false, false, false, true).unwrap_err(),
            "only members who can speak may turn on the camera or share their screen",
            "share refused without Speak",
        );
        assert!(svc.set_flags(4, 22, true, false, false, false).is_ok(), "mute alone stays allowed");
        assert!(svc.leave(4, 22).unwrap().was_in, "dave leaves 22");
        assert!(svc.join(4, 20, live_config()).is_ok(), "dave joins the open room");
        assert!(svc.set_flags(4, 20, false, false, true, true).is_ok(), "dave opens both");
        let cards = roster_entries(db.clone(), 20);
        assert_eq!(cards.len(), 1, "one entry");
        assert!(cards[0].camera && cards[0].screen_share, "the card carries both");
        assert!(svc.leave(4, 20).unwrap().was_in, "dave leaves");
        assert!(roster_entries(db, 20).is_empty(), "leaving clears the card");
    }

    #[test]
    fn join_records_and_leave_clears_the_roster() {
        let (_dir, db) = fixture();
        let svc = VoiceService::new(db.clone());
        assert!(svc.join(4, 20, live_config()).is_ok(), "dave joins the open room");
        assert_eq!(voice_of(4), Some(20), "the join records the entry");
        assert_eq!(roster_ids(20), vec![4], "dave is listed");
        assert!(svc.leave(4, 20).unwrap().was_in, "leave reports they were in");
        assert_eq!(voice_of(4), None, "leave clears it");
        assert!(roster_ids(20).is_empty(), "an empty roster frame still clears rows");
        assert_eq!(svc.leave(4, 20).unwrap().was_in, false, "not in any voice channel now");
    }

    #[test]
    fn last_voice_channel_wins() {
        // One channel per user: the last one joined replaces the old entry,
        // and the old channel drops the row. User 9 stands in for a
        // reconnect into a different room, so ids stay isolated from the
        // tests above (the roster is process state shared across tests).
        let (_dir, db) = fixture();
        set_voice(db.clone(), 9, Some(30));
        assert_eq!(voice_of(9), Some(30));
        set_voice(db.clone(), 9, Some(31));
        assert_eq!(voice_of(9), Some(31), "one channel per user, the last set wins");
        assert!(roster_ids(30).is_empty(), "the old channel drops the row");
        set_voice(db.clone(), 9, None);
        assert_eq!(voice_of(9), None, "None clears the entry");
    }
}