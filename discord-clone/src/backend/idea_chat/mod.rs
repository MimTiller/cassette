//! The SECOND backend: RWND's UI, talked to over the idea-chat wire instead
//! of today's discord-server. Built ONLY with the `idea-chat` cargo feature;
//! the switch point in the parent (`crate::backend`) picks
//! [`IdeaChatBackend`] over `CassetteBackend` when the feature is on, so
//! every area trait above gains a second impl here and the accessors,
//! Gateway verbs, and event stream recompile against it with no UI change.
//!
//! Everything here speaks the wire format documented in
//! `docs/idea-chat-wire.md`: `POST /_srv/<fn>` with a JSON array of the
//! non-extractor args in declaration order, errors as
//! `{"e":{"<kind>":{"message":…}}}`, and one live `/_srv/_ws/live`
//! WebSocket carrying `idea_state` Down/Up frames. The message vocabulary
//! (nouns, live state, events, actions) is `chat_model`'s, pulled as a
//! pinned git dependency; the `chat-api` crate that sits beside it is NOT
//! a dependency (it is built on `server` 3.0, this app is on 4.0.3) — we
//! reproduce only the call sites we need.
//!
//! There is no idea-chat server running anywhere yet, so this half is
//! exercised offline: unit tests feed recorded JSON (from the wire study's
//! examples and `chat_model`'s own serde) through the decoder, the live-frame
//! handling, and the mapping code. The features RWND has that idea-chat does
//! not (pins, link previews, custom emoji, GIFs, stickers, custom roles,
//! notification settings, bans/timeouts, channel permissions, avatar
//! decorations, username change) answer `Err("not supported on this server")`
//! rather than fake data.

/// The `chat_model` vocabulary, plus the few chat-api wire types that live in
/// the `chat-api` crate (which RWND never links) mirrored here — kept in
/// `model.rs` so the mapping code below speaks `model::Space`, `model::State`
/// etc. exactly as the wire study names them.
pub(crate) mod model;

/// Our own small HTTP client: POST /_srv/<fn>, JSON-array args, the kit's
/// error envelope decoded into `server::ServerError`, bearer credentials on
/// native and the browser's `session` cookie on web, configurable base URL.
pub(crate) mod client;

/// The live connection: `live_ticket` → `/_srv/_ws/live`, Down-frame decode
/// (snapshot / event with per-connection seq / refused / pong), Up frames
/// (ping / typing), a `chat_model::live::State` applying events in `Cursor`
/// order — a seq gap drops the state and waits for the next snapshot. Live
/// events are turned into the `WsServerMsg` frames the UI already listens to,
/// and the id-table helpers every area impl shares.
pub(crate) mod live;

/// The Uuid ↔ i64 table: first seen, first numbered, persisted in browser
/// storage on the web (so ids survive a reload) — one table per signed-in
/// account.
pub(crate) mod idmap;

// The area traits' second implementations.
pub(crate) mod realtime;
pub(crate) mod impl_account;
pub(crate) mod impl_servers;
pub(crate) mod impl_members;
pub(crate) mod impl_dms;
pub(crate) mod impl_social;
pub(crate) mod impl_messages;
pub(crate) mod impl_stubs;
