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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use chat_model::{Id};

    fn uid(n: u128) -> Id {
        Id::from_u128(n)
    }

    #[test]
    fn a_window_pages_by_wire_ids() {
        // `Window::before(id)` is what history paging sends: a `Before`
        // window tagged `at`, the id serde'd as its UUID string.
        let value = serde_json::to_value(Window::before(uid(42))).unwrap_or(Value::Null);
        let held = serde_json::to_string(&value).unwrap_or_default();
        assert_eq!(held.contains("\"before\""), true);
        assert_eq!(held.contains("id"), true);
        assert_eq!(Window::latest() == Window::Latest, true);
    }

    #[test]
    fn a_round_trip_window_survives_the_wire() {
        // Recorded JSON from a session: decode it back into the selector.
        let text = "{\"at\":\"around\",\"id\":\"00000000-0000-0000-0000-00000000002a\"}";
        let parsed = serde_json::from_str::<Window>(text).unwrap_or(Window::Latest);
        assert_eq!(parsed == Window::Around { id: uid(42) }, true);
    }

    #[test]
    fn a_slot_decodes_from_the_begin_upload_answer() {
        // `begin_upload` answers `Slot{id, put_url, content_type}`.
        let text = "{\"id\":\"00000000-0000-0000-0000-000000000007\",\"put_url\":\"https://files.example/presigned/abc\",\"content_type\":\"image/png\"}";
        let slot = serde_json::from_str::<Slot>(text).unwrap_or(Slot {
            id: Id::nil(),
            put_url: "".to_string(),
            content_type: "".to_string(),
        });
        assert_eq!(slot.put_url, "https://files.example/presigned/abc".to_string());
        assert_eq!(slot.content_type, "image/png".to_string());
        assert_eq!(slot.id == uid(7), true);
    }

    #[test]
    fn an_unknown_error_kind_still_reads_as_a_refusal() {
        // Wire study §1: errors ride {"e":{"<kind>":{"message":…}}} — an
        // unlisted kind keeps the text rather than losing the answer.
        let banned: Value = serde_json::from_str(
            "{\"e\":{\"banned\":{\"message\":\"this account is off the roster\"}}}"
        ).unwrap_or(serde_json::Value::Null);
        let decoded = decode_error(banned);
        assert_eq!(crate::errors::friendly(&decoded) == "this account is off the roster".to_string(), true);
        let empty: Value = serde_json::from_str("{}").unwrap_or(serde_json::Value::Null);
        assert_eq!(crate::errors::friendly(&decode_error(empty)) == "the server refused the request".to_string(), true);
    }

    #[test]
    fn a_version_mismatch_names_itself() {
        let body: Value = serde_json::from_str(
            "{\"e\":{\"incompatible_version\":null}}"
        ).unwrap_or(Value::Null);
        assert_eq!(crate::errors::friendly(&decode_error(body)) == "the server speaks a different schema".to_string(), true);
    }
}
