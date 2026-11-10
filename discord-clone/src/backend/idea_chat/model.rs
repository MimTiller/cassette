//! The idea-chat vocabulary for the bridge: what both halves of the second
//! backend agree on.
//!
//! The nouns (`Space`, `Channel`, `User`, `Me`, `Message`, `Invite`, `Id` =
//! UUIDv7 — a string on the wire, `Millis` = epoch milliseconds), the live
//! world (`State`, `Event`, `Action`, the reducer) and the timeline (`Page`)
//! come from `chat_model` — a pinned git dependency, the commit
//! `docs/idea-chat-wire.md` studies — re-exported verbatim. The frame enums
//! (`Down`/`Up`/`Cursor`) come from `idea_state` beside it. The few wire
//! types that live in the `chat-api` crate — which this crate never links,
//! it is built on `server` 3.0 — are restated here with the serde shapes the
//! pinned rev fixes: `Window`, `SearchScope`, `Purpose`, `Slot`, `Session`
//! and `SignedUp`.

use serde::{Deserialize, Serialize};
use server::ServerError;

// chat-model's nouns plus its `live` module (`State`, `Event`, `Action`) and
// `timeline` (`Page`), re-exported so the mapping code below reads like the
// wire study.
pub use chat_model::*;

/// The reducer's frame vocabulary, shared with the server.
pub use idea_state as frame;

/// The server→client frame vocabulary for this app's state and events.
pub type Down = frame::Down<chat_model::live::State, chat_model::live::Event>;
/// The client→server frame vocabulary.
pub type Up = frame::Up<chat_model::live::Action>;

/// A `history` page selector. Restated from `chat-api/src/messages.rs` at the
/// pinned rev (chat-api is not a dependency — its transport is `server` 3.0);
/// its serde tag is `at`.
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
/// rev (its serde tag is `in`); spelled `SearchScope` because `chat_model`
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
    pub put_url: String,
    pub content_type: String,
}

/// One signed session as the wire returns it: `sign_in`, `change_password`,
/// `reset_password`, `verify_email` answer `Session` — the bearer token for
/// native clients (`None` on web, where an httpOnly `session` cookie carries
/// the session) plus the viewer's private half.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub token: Option<String>,
    pub me: Me,
}

/// Decode an idea-chat error body (`{"e":{"<kind>":{"message":…}}}`) into a
/// `ServerError` the accessors can render. Unknown kinds become `failed`.
pub(crate) fn decode_error(body: serde_json::Value) -> ServerError {
    match body.get("e").and_then(|variants| variants.as_object()).and_then(|object| object.iter().next()) {
        None => ServerError::failed("the server refused the request"),
        Some((kind, detail)) => {
            if kind == "incompatible_version" {
                return ServerError::failed("the server speaks a different schema");
            }
            let message = match detail.as_object().and_then(|fields| fields.get("message")) {
                Some(value) => value.as_str().unwrap_or(kind),
                None => kind,
            };
            ServerError::failed(message.to_string())
        }
    }
}

/// The `not_supported` refusal every RWND-only surface answers.
pub(crate) fn not_supported() -> ServerError {
    ServerError::failed("not supported on this server")
}
