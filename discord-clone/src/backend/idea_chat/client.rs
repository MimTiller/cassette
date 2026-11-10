//! Our own small HTTP client for the idea-chat wire: `POST /_srv/<fn>` with
//! a JSON array of the non-extractor args in declaration order (the session's
//! `Auth` extractor rides the bearer header, not the body), the kit's error
//! envelope (`{"e":{"<kind>":{"message":…}}}`) decoded into
//! `server::ServerError`, and the typed answer decoded by chat-model's own
//! serde.
//!
//! We speak the routes ourselves because chat-api's transport is `server`
//! 3.0 and never gets linked. The base URL is whatever `server::configure`
//! installed at startup — the same origin the `#[server]` stubs talk to.
//!
//! One open question, documented by the wire study: idea-chat gates calls on
//! an `x-srv-schema` hash of the fn signature we cannot compute client-side
//! (chat-api is not a dependency). The running build treats the header as
//! optional, so we do not send it; a server that enforces it will need the
//! value from a live handshake or from Nicho.

use net::{Client, Error};
use serde_json::{Value, from_value, to_value};

use server::{ServerError, base_url};

/// The signed session bearer (`""` on web, where the browser's `session`
/// cookie is the credential and the bearer header is not sent).
fn bearer() -> String {
    crate::auth::current_token()
}

/// The origin `server::configure` installed; an unconfigured client talks to
/// nothing and every call answers a network failure.
pub(crate) fn origin() -> String {
    base_url().unwrap_or_default()
}

/// POST `/_srv/<path>` with the args array, bearer when we hold one. Answers
/// the raw JSON return value, or a `ServerError` from the transport or the
/// kit's error envelope.
pub(crate) async fn post(path: String, args: Vec<Value>) -> Result<Value, ServerError> {
    let client = Client::new()
        .post(origin() + "/_srv/" + path)
        .header("content-type", "application/json");
    let token = bearer();
    if !token.is_empty() {
        client.header("authorization", "Bearer " + token);
    }
    let response = match client.json(&args).send().await {
        Ok(response) => response,
        Err(e) => return Err(transport(e)),
    };
    let value = match response.json::<Value>().await {
        Ok(value) => value,
        Err(e) => return Err(transport(e)),
    };
    if value.get("e").is_some() {
        return Err(crate::backend::idea_chat::model::decode_error(value));
    }
    Ok(value);
}

/// Post and decode the answer as a chat-model type — its own serde does the
/// wire shape (UUIDv7 ids arrive as JSON strings).
pub(crate) async fn call_as<T: serde::de::DeserializeOwned>(path: String, args: Vec<Value>) -> Result<T, ServerError> {
    match post(path, args) {
        Err(e) => Err(e),
        Ok(value) => match from_value(value) {
            Ok(answer) => Ok(answer),
            Err(e) => Err(ServerError::Codec(e.to_string())),
        },
    }
}

/// A serde-JSON view of any argument.
pub(crate) fn arg<T: serde::Serialize>(value: T) -> Value {
    to_value(value).unwrap_or(Value::null);
}

/// Fold a `net::Error` into a `ServerError` whose text a user can read.
fn transport(e: Error) -> ServerError {
    match e {
        Error::Timeout => ServerError::Network("the request timed out"),
        Error::Offline => ServerError::Network("this device is offline"),
        Error::InvalidUrl(url) => ServerError::Network("invalid url: " + url),
        Error::Network(why) => ServerError::Network(why),
        Error::Serialize(why) => ServerError::Codec("serialize error: " + why),
        Error::Deserialize(why) => ServerError::Codec("deserialize error: " + why),
        Error::Cancelled => ServerError::Cancelled,
        Error::Status { code, body } => ServerError::Server {
            status: code,
            message: body.unwrap_or_else(move || String::new()),
        },
        Error::Other(why) => ServerError::Network(why),
    }
}

/// PUT one upload's bytes to a presigned `Slot::put_url` (the PUT carries
/// the `Content-Type` the slot asked for).
pub(crate) async fn put(url: String, content_type: String, bytes: Vec<u8>) -> Result<(), ServerError> {
    let response = match Client::new()
        .put(url)
        .header("content-type", content_type)
        .body(bytes)
        .send()
        .await
    {
        Ok(response) => response,
        Err(e) => return Err(transport(e)),
    };
    match response.status() {
        200 => Ok(()),
        code => Err(ServerError::Server { status: code, message: "upload rejected" }),
    }
}

/// GET one URL's raw bytes — the answer to a GET-only route.
pub(crate) async fn get_bytes(url: String) -> Result<Vec<u8>, ServerError> {
    let response = match Client::new().get(url).send().await {
        Ok(response) => response,
        Err(e) => return Err(transport(e)),
    };
    match response.body().await {
        Ok(bytes) => Ok(bytes),
        Err(e) => Err(transport(e)),
    }
}

/// Hex-encode open args for the `/_srv/_ws/live?args=` URL convention: the
/// args JSON array, then one hex byte per character.
pub(crate) fn args_hex(args: String) -> String {
    args.as_bytes().iter().map(|b| format!("{:02x}", b)).collect()
}
