//! The idea-chat vocabulary for the bridge: what both halves of the second
//! backend agree on.
//!
//! The nouns (`Space`, `Channel`, `User`, `Me`, `Message`, `Invite`, `Id`
//! = UUIDv7 — a string on the wire, `Millis` = epoch milliseconds), the live
//! world (`State`, `Event`, `Action`, the reducer) and the timeline (`Page`)
//! come from `chat-model` — a pinned git dependency, the commit
//! `docs/idea-chat-wire.md` studies — re-exported verbatim. The frame enums
//! (`Down`/`Up`/`Cursor`) come from `idea-state` beside it. The few wire
//! types that live in the `chat-api` crate — which this crate never links,
//! it is built on `server` 3.0 — are restated here with the serde shapes the
//! pinned rev fixes: `Window`, `SearchScope`, `Purpose`, `Slot`, `Session`
//! and `SignedUp`.

use serde::{Deserialize, Serialize};

use chat_model::Id;

use server::ServerError;

// chat-model's nouns plus its `live` module (`State`, `Event`, `Action`, the
// reducer) and `timeline` (`Page`): re-exported so the mapping code reads
// like the wire study.
pub use chat_model::*;

/// The reducer's frame enums, shared with the server.
pub use idea_state as frame;

/// The server→client frame vocabulary for this app's state/event types.
pub type Down = frame::Down<chat_model::live::State, chat_model::live::Event>;
/// The client→server frame vocabulary.
pub type Up = frame::Up<chat_model::live::Action>;

/// A `history` page selector. Restated from `chat-api/src/messages.rs` at the
/// pinned rev (chat-api is not a dependency — its transport is `server` 3.0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "at", rename_all = "snake_case")]
pub enum Window {
    Latest,
    Before { id: Id },
    After { id: Id },
    Around { id: Id },
}

impl Window {
    pub fn latest() -> Window {
        Window::Latest
    }
    pub fn before(id: Id) -> Window {
        Window::Before { id }
    }
}

/// A search scope. Restated from `chat-api/src/messages.rs` at the pinned
/// rev (its serde tag is `in`). Spelled `SearchScope` because `chat_model`
/// re-exports its own `Scope` (the notification scope) above.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "in", rename_all = "snake_case")]
pub enum SearchScope {
    Space { id: Id },
    Channel { id: Id },
    Conversations,
}

impl SearchScope {
    pub fn space(id: Id) -> SearchScope {
        SearchScope::Space { id }
    }
}

/// An upload's use. Restated from `chat-api/src/files.rs` at the pinned rev.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    Message,
    Avatar,
    Icon,
}

/// Where an upload's bytes go: `begin_upload` answers this, the PUT runs
/// against `put_url`, then `finish_upload(id, …)` closes the triplet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Slot {
    pub id: Id,
    /// A presigned PUT, good for ten minutes. Send `Content-Type` as given.
    pub put_url: String,
    pub content_type: String,
}

/// One signed session as the wire returns it: `sign_in`, `change_password`,
/// `reset_password` answer `Session` — the bearer token for native clients
/// (`None` on web, where an httpOnly `session` cookie carries the session)
/// plus the viewer's private half. Restated from `chat-api/auth.rs`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub token: Option<String>,
    pub me: Me,
}

/// Signup finished; a verification code is on its way to `email`. Restated
/// from `chat-api/auth.rs`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignedUp {
    pub email: String,
}

// --- the kit's error envelope ------------------------------------------------
//
// `server-kit` 3.0 `ApiError` on the wire: `{"e":{"failed":{"message":"…"}}}`.
// The variants `failed`, `invalid`, `forbidden`, `unauthorized`, `unavailable`,
// `too_many_requests` carry a `{message}` object; `incompatible_version` is
// null. Decoded here (chat-api is not a dependency) into the framework's own
// `server::ServerError`, which `crate::errors::friendly` renders verbatim.

/// Decode an idea-chat error body (`{"e":{"<kind>":…}}`) into a `ServerError`
/// the accessors can render. Unknown kinds become `failed`.
pub(crate) fn decode_error(body: serde_json::Value) -> ServerError {
    let Some(variants) = body.get("e");
    let Some(object) = variants.as_object();
    let Some((kind, detail)) = object.iter().next();
    if kind == "incompatible_version" {
        return ServerError::Codec("the server speaks a different schema");
    }
    let message = if let Some(fields) = detail.as_object(), let Some(m) = fields.get("message") {
        m.as_str().unwrap_or(kind)
    } else {
        kind
    };
    ServerError::failed(message.to_string())
}

/// The `not_supported` refusal every RWND-only surface answers. Shown to the
/// user verbatim (`crate::errors::friendly`), so it reads as a sentence.
pub(crate) function not_supported() -> ServerError {
    ServerError::failed("not supported on this server")
}
