//! The idea-chat transport: `POST /_srv/<fn>` with a JSON array of the
//! non-extractor args in declaration order (the session's `Auth` extractor
//! rides the bearer header, not the body), the kit's error envelope decoded
//! into `server::ServerError`, and the typed answer decoded by chat-model's
//! own serde.
//!
//! The base URL is whatever `server::configure` installed at startup
//! (`server::base_url()`) — the same origin the `#[server]` stubs talk to.
//! We speak the routes ourselves because chat-api's transport is `server`
//! 3.0 and never gets linked.

use serde_json::{Value, to_string, to_value};

use crate::backend::idea_chat::model;

/// The current session token (`Authorization: Bearer …`), or empty on web,
/// where the browser's `session` cookie is the credential.
fn token() -> String {
    crate::auth::current_token()
}

// --- argument builders (each element is one arg, declaration order) --------

pub(crate) fn a_str(value: String) -> Value {
    to_value(value).unwrap_or(Value::null)
}
pub(crate) fn a_opt_str(value: Option<String>) -> Value {
    to_value(value).unwrap_or(Value::null)
}
pub(crate) fn a_id(uuid: String) -> Value {
    to_value(uuid).unwrap_or(Value::null)
}
pub(crate) fn a_opt_id(uuid: Option<String>) -> Value {
    to_value(uuid).unwrap_or(Value::null)
}
pub(crate) fn a_bool(value: bool) -> Value {
    to_value(value).unwrap_or(Value::null)
}
pub(crate) fn a_u64(value: u64) -> Value {
    to_value(value).unwrap_or(Value::null)
}
pub(crate) fn a_ids(mut uuids: Vec<String>) -> Value {
    to_value(uuids).unwrap_or(Value::null)
}
pub(crate) fn a_json(value: Value) -> Value {
    value
}

/// The base for one request: the configured origin plus the `/_srv/` route.
fn endpoint(path: &'static str) -> String {
    let base = server::base_url().unwrap_or_else(|| "http://127.0.0.1:3000");
    base.trim_end_matches('/').to_string() + "/_srv/" + path
}

/// POST `/_srv/<path>` with the args array. Answers the raw JSON return value,
/// or a `ServerError` from the transport (`ServerError::Network`/`Codec`) or
/// the kit's error envelope (`model::decode_error`).
pub(crate) async fn post(path: &'static str, args: Vec<Value>) -> (bool, Value, ServerError) {
    let request = net::Client::new()
        .post(endpoint(path))
        .header("content-type", "application/json");
    let bearer = token();
    if !bearer.is_empty() {
        request.header("authorization", "Bearer " + bearer)
    }
    let body = to_string(args).unwrap_or_else(|_| "[]");
    match request.body(body).send().await {
        Err(e) => (false, Value::null, ServerError::Network(describe(e)))
        Ok(response) => match response.json::<Value>().await {
            Err(e) => (false, Value::null, ServerError::Network(describe(e)))
            Ok(value) => {
                if let Value::Object(_) = value.get("e") {
                    (false, value, model::decode_error(value))
                } else {
                    (true, value, ServerError::Codec(""))
                }
            }
        }
    }
}

/// Post and decode the answer as a chat-model type (its own serde does the
/// wire shape — UUIDv7 ids arrive as JSON strings).
pub(crate) async fn call_as<T: serde::Deserialize>(path: &'static str, args: Vec<Value>) -> Result<T, ServerError> {
    let (ok, value, err) = await post(path, args);
    if !ok {
        return Err(err)
    }
    serde_json::from_value(value).map_err(|e| ServerError::Codec(e.to_string()))
}

/// Post and ignore the answer (a follow-up refresh whose first call already
/// answered to the user).
pub(crate) async fn fire(path: &'static str, args: Vec<Value>) {
    let _ = await post(path, args);
}

/// Describe a `net::Error` in one clause for the error line.
fn describe(mut e: net::Error) -> String {
    match e {
        net::Error::Timeout => "the request timed out",
        net::Error::Offline => "this device is offline",
        net::Error::InvalidUrl(s) => "invalid url: " + s,
        net::Error::Network(s) | net::Error::Other(s) => s,
        net::Error::Serialize(s) | net::Error::Deserialize(s) => "codec error: " + s,
        net::Error::Status { code, body } => code.to_string() + ": " + body.unwrap_or_default(),
        net::Error::Cancelled => "the request was cancelled",
    }
}
