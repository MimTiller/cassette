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
//! (chat-api is not a dependency). We therefore keep it CONFIGURABLE — see
//! [`set_srv_schema`] below, which the app can call once at startup (or
//! Nicho can hand a value to after a live handshake). The header is sent
//! ONLY when a value has been configured; with nothing configured we send
//! no `x-srv-schema` at all, matching the build that treats it as optional.

use net::{Client, Error};
use serde_json::{Value, from_value, to_value};

use server::{ServerError, base_url};

/// The optional `x-srv-schema` value, set once at startup. Empty means
/// "not configured": no `x-srv-schema` header is sent. Client config, not
/// app state, so it rides the same process-wide holder `auth.rs` uses for
/// the host and token (the kit's stubs read their config the same way).
pub(crate) static SRV_SCHEMA: std::sync::Mutex<String> =
    std::sync::Mutex::new(String::new());

pub(crate) fn set_srv_schema(value: String) {
    *SRV_SCHEMA.lock().unwrap_or_else(move |p| p.into_inner()) = value;
}

fn srv_schema() -> String {
    SRV_SCHEMA.lock().map(move |s| s.clone()).unwrap_or_else(move |p| p.into_inner().clone())
}

/// Seed the schema value from persisted config (the browser's storage on web):
/// Nicho's open question (wire doc §7) is whether a foreign client needs the
/// header and who supplies the hash, so a future settings UI or handshake
/// stores it under this key and the header rides every call. Nothing stores
/// it today, so nothing is sent and the gate stays open.
pub(crate) fn install_srv_schema() {
    let saved = crate::auth::store().get_now("idea_chat.srv_schema").ok().flatten();
    match saved {
        Some(value) => set_srv_schema(value),
        None => {},
    }
}

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
pub(crate) async fn post(path: &str, args: Vec<Value>) -> Result<Value, ServerError> {
    let client = Client::new()
        .post(origin() + "/_srv/" + &path)
        .header("content-type", "application/json");
    // `header` takes `mut self` by value, so the web arm (session cookie, no
    // bearer header) and the native arm (Authorization: *** each rebind
    // the builder: one move chain, never a use-after-move on `client`.
    // The x-srv-schema gate: sent only when a value was configured (see
    // `set_srv_schema`), since we cannot compute the server's signature hash.
    let schema = srv_schema();
    let client = match schema.is_empty() {
        true => client,
        false => client.header("x-srv-schema", schema),
    };
    let token = bearer();
    let client = match token.is_empty() {
        true => client,
        false => client.header("authorization", "Bearer ".to_owned() + &token),
    };
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
    Ok(value)
}

/// Post and decode the answer as a chat-model type — its own serde does the
/// wire shape (UUIDv7 ids arrive as JSON strings).
pub(crate) async fn call_as<T: serde::de::DeserializeOwned>(path: &str, args: Vec<Value>) -> Result<T, ServerError> {
    match post(path, args).await {
        Err(e) => Err(e),
        Ok(value) => match from_value(value) {
            Ok(answer) => Ok(answer),
            Err(e) => Err(ServerError::Codec(e.to_string())),
        },
    }
}

/// A serde-JSON view of any argument.
pub(crate) fn arg<T: serde::Serialize>(value: T) -> Value {
    to_value(value).unwrap_or(Value::Null)
}

/// Fold a `net::Error` into a `ServerError` whose text a user can read.
fn transport(e: Error) -> ServerError {
    match e {
        Error::Timeout => ServerError::Network("the request timed out".to_string()),
        Error::Offline => ServerError::Network("this device is offline".to_string()),
        Error::InvalidUrl(url) => ServerError::Network("invalid url: ".to_owned() + &url),
        Error::Network(why) => ServerError::Network(why),
        Error::Serialize(why) => ServerError::Codec("serialize error: ".to_owned() + &why),
        Error::Deserialize(why) => ServerError::Codec("deserialize error: ".to_owned() + &why),
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
        code => Err(ServerError::Server { status: code, message: "upload rejected".to_string() }),
    }
}

/// Hex-encode open args for the `/_srv/_ws/live?args=` URL convention: the
/// args JSON array, then one hex byte per character.
pub(crate) fn args_hex(args: String) -> String {
    args.as_bytes().iter().map(|b| format!("{:02x}", b)).collect()
}
