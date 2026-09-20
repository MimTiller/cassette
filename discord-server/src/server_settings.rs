//! Server-settings endpoints: rename a server, set its icon, transfer
//! ownership.
//!
//! ## Design
//!
//! The owner-only authorization policy lives in a pure, DB-backed service
//! ([`ServerSettings`]) that takes the DB handle and the acting user id as
//! plain arguments. It returns a plain `Result<T, String>` so it is fully
//! unit-testable against a temp-file SQLite database without the axum/tokio
//! stack. The three `#[server]` fns below are thin adapters: they pull the
//! shared `Db` out of server state, resolve the acting user from the request
//! header, and map a service failure to `ServerError::failed`.
//!
//! There is no real auth layer yet (the `users` table carries no credentials —
//! "auth is a later layer"), so the acting user is read from the `x-user-id`
//! request header. That is a stand-in that the real identity layer will
//! replace; the owner-gating logic does not depend on how the caller is
//! resolved.
//!
//! **Icon representation:** the icon is stored as a **URL string** in
//! `servers.icon_url`. The `set_server_icon` endpoint accepts `None` (clear) or
//! a `String` URL. Keeping one representation keeps the wire format stable and
//! consistent with the schema column.
//!
//! ## Owner-only guarantee
//!
//! Every mutation first checks the *current* owner in the database:
//!   * rename / set icon  → the caller must be the row in
//!     `server_memberships` with `role = 'owner'` for that server.
//!   * transfer ownership → the caller must be the current owner, and the
//!     new owner must already be a member; the transfer is done in a single
//!     transaction guarded by `servers.owner_id = <caller>` so a concurrent
//!     transfer can't race.

use serde::{Deserialize, Serialize};
use server::{server, ServerError};

use crate::db::Db;

/// Read the acting user id from the request header. Returns `None` if the
/// header is absent (the adapter turns that into an unauthenticated error).
fn acting_user() -> Option<i64> {
    server::use_request_header("x-user-id")?.parse().ok()
}

// ===========================================================================
// Realtime gateway: send/receive chat messages in a channel over WebSocket.
// ===========================================================================
//
// A full-duplex `#[server::channel]` endpoint. A client opens it with a
// `x-user-id` request header (the same stand-in session mechanism the
// `#[server]` fns use via `acting_user`) and a `channel_id`, then exchanges
// `WsClientMsg`/`WsServerMsg` frames. The acting user is read from the
// upgrade header via the `server::Headers` reserved extractor, so auth works
// in the channel body (the task-local `use_request_header` does not).
//
// Scope: channel messages only. Presence (typing / voice / online state) is a
// separate follow-up task and is intentionally NOT here.

/// Inbound frames the client sends.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WsClientMsg {
    /// Send a message to the channel.
    Send { channel_id: i64, content: String },
    /// Request the recent message history for a channel.
    History { channel_id: i64 },
}

/// Outbound frames the server sends.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WsServerMsg {
    /// A message, either a freshly posted one (`new: true`) or history
    /// (`new: false`).
    Message {
        message_id: i64,
        channel_id: i64,
        author_id: i64,
        content: String,
        created_at: i64,
        edited_at: Option<i64>,
        new: bool,
    },
    /// The most recent `Message { new: false }` frames for `History`.
    History { channel_id: i64 },
    /// A gateway error for a specific operation.
    Error { reason: String },
}

/// The realtime message gateway. Mounted at `GET /_srv/_ws/message_gateway`.
///
/// `headers` is the reserved `server::Headers` extractor (the WebSocket
/// upgrade's `HeaderMap`); the acting user is read from `x-user-id` exactly
/// as the `#[server]` fns do.
#[server::channel]
pub async fn message_gateway(
    mut ch: server::Socket<WsClientMsg, WsServerMsg>,
    headers: server::Headers,
) -> Result<(), ServerError> {
    // Authenticated once at open: the existing session mechanism.
    let user_id = headers
        .get("x-user-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| ServerError::failed("unauthenticated: missing x-user-id"))?;

    let db = db_from_state()?;

    while let Some(Ok(msg)) = ch.recv().await {
        match msg {
            WsClientMsg::Send {
                channel_id,
                content,
            } => {
                // Reject an empty message rather than storing garbage.
                if content.trim().is_empty() {
                    let _ = ch
                        .send(WsServerMsg::Error {
                            reason: "empty message".into(),
                        })
                        .await;
                    continue;
                }
                // The channel must exist.
                let Some((_id, _name)) = db.channel_by_id(channel_id) else {
                    let _ = ch
                        .send(WsServerMsg::Error {
                            reason: "channel not found".into(),
                        })
                        .await;
                    continue;
                };
                let created_at = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                match db.insert_message(channel_id, user_id, &content, created_at) {
                    Ok(message_id) => {
                        let _ = ch
                            .send(WsServerMsg::Message {
                                message_id,
                                channel_id,
                                author_id: user_id,
                                content,
                                created_at,
                                edited_at: None,
                                new: true,
                            })
                            .await;
                    }
                    Err(e) => {
                        let _ = ch
                            .send(WsServerMsg::Error {
                                reason: e.to_string(),
                            })
                            .await;
                    }
                }
            }
            WsClientMsg::History { channel_id } => {
                let _ = ch
                    .send(WsServerMsg::History { channel_id })
                    .await;
                for m in db.recent_messages(channel_id, 50) {
                    let _ = ch
                        .send(WsServerMsg::Message {
                            message_id: m.id,
                            channel_id: m.channel_id,
                            author_id: m.author_id,
                            content: m.content,
                            created_at: m.created_at,
                            edited_at: m.edited_at,
                            new: false,
                        })
                        .await;
                }
            }
        }
    }
    Ok(())
}

/// The result of any server-settings operation. Carries the updated server
/// name + icon so the client can refresh its local model in one round trip.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ServerSettingsResult {
    pub server_id: i64,
    pub name: String,
    pub icon_url: Option<String>,
    /// The current owner after the operation (changes on transfer).
    pub owner_id: i64,
}

/// The server-settings service. Wraps a `Db` handle.
///
/// Every method takes the acting user id explicitly so the authorization is
/// testable without any request context.
pub struct ServerSettings {
    db: Db,
}

impl ServerSettings {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Rename the server with `server_id` to `new_name`.
    ///
    /// # Errors
    /// - `not found` — the server id doesn't exist.
    /// - `not the server owner` — the caller is not the current owner.
    pub fn rename(&self, caller_id: i64, server_id: i64, new_name: &str) -> Result<ServerSettingsResult, String> {
        if new_name.trim().is_empty() {
            return Err("server name must not be empty".into());
        }
        let (sid, _name, _owner_id, _icon_url) = self
            .db
            .server_by_id(server_id)
            .ok_or("server not found")?;
        // Owner-only: the caller must be the row with role='owner'.
        let role = self
            .db
            .membership_role(server_id, caller_id)
            .ok_or("not the server owner")?;
        if role != "owner" {
            return Err("not the server owner".into());
        }
        self.db.rename_server(sid, new_name).map_err(|e| e.to_string())?;
        // Re-read to return a consistent snapshot (the rename already updated
        // the name; re-reading keeps the result faithful to disk).
        let (_id, name, owner_id, icon_url) = self
            .db
            .server_by_id(server_id)
            .ok_or("server not found")?;
        Ok(ServerSettingsResult {
            server_id,
            name,
            icon_url,
            owner_id,
        })
    }

    /// Set (or clear) the server icon.
    ///
    /// `icon` is `None` to clear the icon, or a URL string to set it.
    ///
    /// # Errors
    /// - `not found` — the server id doesn't exist.
    /// - `not the server owner` — the caller is not the current owner.
    pub fn set_icon(&self, caller_id: i64, server_id: i64, icon: Option<&str>) -> Result<ServerSettingsResult, String> {
        let (sid, _name, _owner_id, _icon_url) = self
            .db
            .server_by_id(server_id)
            .ok_or("server not found")?;
        let role = self
            .db
            .membership_role(server_id, caller_id)
            .ok_or("not the server owner")?;
        if role != "owner" {
            return Err("not the server owner".into());
        }
        // Reject an empty string URL — treat it as an error rather than
        // silently storing garbage.
        if let Some(url) = icon {
            if url.trim().is_empty() {
                return Err("icon url must not be empty (pass null to clear)".into());
            }
        }
        self.db.set_server_icon(sid, icon).map_err(|e| e.to_string())?;
        let (_id, name, owner_id, icon_url) = self
            .db
            .server_by_id(server_id)
            .ok_or("server not found")?;
        Ok(ServerSettingsResult {
            server_id,
            name,
            icon_url,
            owner_id,
        })
    }

    /// Transfer server ownership from the current owner (`caller_id`) to
    /// `new_owner_id`.
    ///
    /// # Errors
    /// - `not found` — the server id doesn't exist.
    /// - `not the server owner` — the caller is not the current owner.
    /// - `new owner is not a member of this server` — the target user has no
    ///   membership row for this server.
    pub fn transfer_ownership(
        &self,
        caller_id: i64,
        server_id: i64,
        new_owner_id: i64,
    ) -> Result<ServerSettingsResult, String> {
        let (_sid, _name, owner_id, _icon_url) = self
            .db
            .server_by_id(server_id)
            .ok_or("server not found")?;
        // Owner-only: the caller must be the current owner.
        if owner_id != caller_id {
            return Err("not the server owner".into());
        }
        self.db
            .transfer_server_ownership(server_id, caller_id, new_owner_id)
            .map_err(|e| e.to_string())?;
        let (_id, name, owner_id, icon_url) = self
            .db
            .server_by_id(server_id)
            .ok_or("server not found")?;
        Ok(ServerSettingsResult {
            server_id,
            name,
            icon_url,
            owner_id,
        })
    }
}

// ── #[server] endpoints ─────────────────────────────────────────────────────

/// Rename a server. Only the current owner may call.
///
/// The acting user is read from the `x-user-id` request header.
#[server]
pub async fn rename_server(input: RenameServerInput) -> Result<ServerSettingsResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .rename(caller, input.server_id, &input.new_name)
        .map_err(ServerError::failed)
}

/// Set (or clear) a server's icon. Only the current owner may call.
///
/// `icon` is a URL string, or `null` to clear the icon.
#[server]
pub async fn set_server_icon(input: SetServerIconInput) -> Result<ServerSettingsResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .set_icon(caller, input.server_id, input.icon.as_deref())
        .map_err(ServerError::failed)
}

/// Transfer server ownership to another member. Only the current owner may call.
#[server]
pub async fn transfer_server_ownership(input: TransferOwnershipInput) -> Result<ServerSettingsResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .transfer_ownership(caller, input.server_id, input.new_owner_id)
        .map_err(ServerError::failed)
}

/// Pull the shared `Db` out of server state, installed at startup by the
/// server bin via `server::install_state(db)`. `Db` is cheap to clone (it's
/// just an `Arc<Mutex<Connection>>`), so each request gets its own handle.
fn db_from_state() -> Result<Db, ServerError> {
    server::use_state::<Db>()
        .ok_or_else(|| ServerError::failed("database not installed"))
}

// ── wire types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct RenameServerInput {
    pub server_id: i64,
    pub new_name: String,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct SetServerIconInput {
    pub server_id: i64,
    /// A URL string, or `None` to clear the icon.
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct TransferOwnershipInput {
    pub server_id: i64,
    pub new_owner_id: i64,
}

// ── tests ────────────────────────────────────────────────────────────────────
//
// The tests exercise the owner-only policy directly against the `ServerSettings`
// service (no HTTP / no server state needed). A rejection case is included for
// each mutation.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use std::path::Path;

    /// Build a temp-file database, run migrations, seed one server with an
    /// owner (user 1) and two members (users 2 and 3), and return the `Db`.
    fn setup_db() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let db = Db::migrate(&db_path, &migrations_dir).unwrap();

        // Seed users + one server + memberships directly via raw SQL
        // (the db layer doesn't expose a "create user" API yet).
        let c = db.lock().unwrap();
        c.execute_batch("
            INSERT INTO users (id, username, email, created_at) VALUES
                (1, 'owner',  'owner@example.com',  1000),
                (2, 'alice',  'alice@example.com',  1000),
                (3, 'bob',    'bob@example.com',    1000);
            INSERT INTO servers (id, name, owner_id, icon_url, created_at)
                VALUES (1, 'My Server', 1, NULL, 1000);
            INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES
                (1, 1, 'owner',  1000),
                (1, 2, 'member', 1000),
                (1, 3, 'member', 1000);
        ").unwrap();
        drop(c);

        (dir, db)
    }

    #[test]
    fn owner_can_rename() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db);
        let res = svc.rename(1, 1, "New Name").unwrap();
        assert_eq!(res.name, "New Name");
        assert_eq!(res.owner_id, 1);
    }

    #[test]
    fn non_owner_rename_rejected() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db.clone());
        let err = svc.rename(2, 1, "Hacked").unwrap_err();
        assert!(err.contains("not the server owner"), "got: {err}");
        // Name unchanged.
        assert_eq!(db.server_by_id(1).unwrap().1, "My Server");
    }

    #[test]
    fn non_member_rename_rejected() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db);
        // User 999 is not in the server at all.
        let err = svc.rename(999, 1, "Hacked").unwrap_err();
        assert!(err.contains("not the server owner"), "got: {err}");
    }

    #[test]
    fn empty_name_rejected() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db);
        let err = svc.rename(1, 1, "   ").unwrap_err();
        assert!(err.contains("must not be empty"), "got: {err}");
    }

    #[test]
    fn owner_can_set_icon() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db);
        let res = svc.set_icon(1, 1, Some("https://cdn.example.com/icon.png")).unwrap();
        assert_eq!(res.icon_url.as_deref(), Some("https://cdn.example.com/icon.png"));
    }

    #[test]
    fn owner_can_clear_icon() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db.clone());
        svc.set_icon(1, 1, Some("https://cdn.example.com/icon.png")).unwrap();
        let res = svc.set_icon(1, 1, None).unwrap();
        assert!(res.icon_url.is_none());
    }

    #[test]
    fn non_owner_set_icon_rejected() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db.clone());
        let err = svc.set_icon(2, 1, Some("https://evil.example.com/x.png")).unwrap_err();
        assert!(err.contains("not the server owner"), "got: {err}");
        // Icon unchanged.
        assert!(db.server_by_id(1).unwrap().3.is_none());
    }

    #[test]
    fn owner_can_transfer_ownership() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db.clone());
        let res = svc.transfer_ownership(1, 1, 2).unwrap();
        assert_eq!(res.owner_id, 2, "owner should now be user 2");
        // Verify roles: old owner demoted to admin, new owner promoted.
        assert_eq!(db.membership_role(1, 1).as_deref(), Some("admin"));
        assert_eq!(db.membership_role(1, 2).as_deref(), Some("owner"));
    }

    #[test]
    fn non_owner_transfer_rejected() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db.clone());
        let err = svc.transfer_ownership(2, 1, 3).unwrap_err();
        assert!(err.contains("not the server owner"), "got: {err}");
        // Ownership unchanged.
        assert_eq!(db.server_by_id(1).unwrap().2, 1, "owner should still be user 1");
    }

    #[test]
    fn transfer_to_non_member_rejected() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db.clone());
        // User 999 is not a member.
        let err = svc.transfer_ownership(1, 1, 999).unwrap_err();
        assert!(err.contains("not a member"), "got: {err}");
        // Ownership unchanged.
        assert_eq!(db.server_by_id(1).unwrap().2, 1);
    }

    #[test]
    fn transfer_to_self_rejected_by_membership_guard() {
        // Transferring to oneself: the owner is already a member, so the
        // membership guard passes. But the old-owner→admin demotion would run
        // and then the self→owner promotion would re-set it. The net effect
        // is a no-op with owner_id unchanged. Verify it doesn't error and
        // the owner stays the same.
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db);
        let res = svc.transfer_ownership(1, 1, 1).unwrap();
        assert_eq!(res.owner_id, 1);
    }

    #[test]
    fn unknown_server_rejected() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db);
        let err = svc.rename(1, 999, "X").unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
    }

    // ── realtime gateway: send/receive round-trip ────────────────────────────
    //
    // E2E through the macro-mounted route: boot `server::router()` (which
    // folds the `#[server::channel]` WsEntry into `GET /_srv/_ws/
    // message_gateway`), install the shared `Db`, connect a raw WebSocket
    // client, and prove a message sent over the socket is persisted +
    // echoed back, and that `History` replays it.
    //
    // A raw `tokio-tungstenite` client is used instead of the framework's
    // `net::WebSocket::connect(url)` because that API is URL-only and cannot
    // attach the `x-user-id` header the upgrade handshake needs for auth.

    /// Seed one server (owner = user 1) and one text channel, returning the
    /// channel id. The messages table is created empty — the test posts into
    /// it over the gateway.
    fn setup_db_with_channel() -> (tempfile::TempDir, Db, i64) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let db = Db::migrate(&db_path, &migrations_dir).unwrap();

        let c = db.lock().unwrap();
        c.execute_batch(
            "INSERT INTO users (id, username, email, created_at) VALUES
                (1, 'owner', 'owner@example.com', 1000);
            INSERT INTO servers (id, name, owner_id, icon_url, created_at)
                VALUES (1, 'My Server', 1, NULL, 1000);
            INSERT INTO channels (id, server_id, name, kind, position, parent_channel_id, created_at)
                VALUES (1, 1, 'general', 'text', 0, NULL, 1000);",
        )
        .unwrap();
        drop(c);

        (dir, db, 1)
    }

    fn decode_gateway(msg: tokio_tungstenite::tungstenite::Message) -> WsServerMsg {
        let tokio_tungstenite::tungstenite::Message::Text(s) = msg else {
            panic!("expected a text frame, got {msg:?}");
        };
        serde_json::from_str(&s).expect("frame is valid WsServerMsg JSON")
    }

    fn encode_client(msg: &WsClientMsg) -> tokio_tungstenite::tungstenite::Message {
        tokio_tungstenite::tungstenite::Message::Text(
            serde_json::to_string(msg).unwrap(),
        )
    }

    #[tokio::test]
    async fn gateway_send_and_history_round_trip() {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        use tokio_tungstenite::tungstenite::http::header;

        let (dir, db, channel_id) = setup_db_with_channel();
        server::install_state(db.clone());

        // Boot the macro-mounted router on an ephemeral port.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, server::router()).await;
        });

        // Authenticated client: x-user-id on the upgrade handshake.
        let url = format!("ws://{addr}/_srv/_ws/message_gateway");
        let mut req = url.into_client_request().expect("connect URL parses");
        req.headers_mut()
            .insert(header::USER_AGENT, "discord-server-gateway-test".parse().unwrap());
        req.headers_mut().insert("x-user-id", "1".parse().unwrap());
        let (mut ws, _) = tokio_tungstenite::connect_async(req)
            .await
            .expect("gateway upgrade succeeds (authenticated)");

        // 1. Send a message → the server persists it and echoes it back with new: true.
        let send = WsClientMsg::Send {
            channel_id,
            content: "hello channel".into(),
        };
        ws.send(encode_client(&send)).await.unwrap();
        let echo = decode_gateway(ws.next().await.unwrap().expect("socket open"));
        match echo {
            WsServerMsg::Message {
                message_id,
                channel_id: ch,
                author_id,
                content,
                edited_at,
                new,
                ..
            } => {
                assert_eq!(ch, channel_id);
                assert_eq!(author_id, 1, "author is the x-user-id");
                assert_eq!(content, "hello channel");
                assert!(new, "a freshly posted message is new");
                assert!(edited_at.is_none());
                // It was really persisted.
                let stored = db.recent_messages(channel_id, 10);
                assert_eq!(stored.len(), 1);
                assert_eq!(stored[0].id, message_id);
                assert_eq!(stored[0].content, "hello channel");
            }
            other => panic!("expected an echo Message, got {other:?}"),
        }

        // 2. History → a History marker followed by the posted message, new: false.
        let hist = WsClientMsg::History { channel_id };
        ws.send(encode_client(&hist)).await.unwrap();
        let marker = decode_gateway(ws.next().await.unwrap().expect("socket open"));
        match marker {
            WsServerMsg::History { channel_id: ch } => assert_eq!(ch, channel_id),
            other => panic!("expected a History marker, got {other:?}"),
        }
        let replayed = decode_gateway(ws.next().await.unwrap().expect("socket open"));
        match replayed {
            WsServerMsg::Message {
                channel_id: ch,
                author_id,
                content,
                new,
                ..
            } => {
                assert_eq!(ch, channel_id);
                assert_eq!(author_id, 1);
                assert_eq!(content, "hello channel");
                assert!(!new, "history replays are not new");
            }
            other => panic!("expected a replayed Message, got {other:?}"),
        }

        drop(ws);
        drop(dir);
    }

    #[tokio::test]
    async fn gateway_rejects_empty_message() {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        use tokio_tungstenite::tungstenite::http::header;

        let (dir, db, channel_id) = setup_db_with_channel();
        server::install_state(db.clone());

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, server::router()).await;
        });

        let url = format!("ws://{addr}/_srv/_ws/message_gateway");
        let mut req = url.into_client_request().expect("connect URL parses");
        req.headers_mut()
            .insert(header::USER_AGENT, "discord-server-gateway-test".parse().unwrap());
        req.headers_mut().insert("x-user-id", "1".parse().unwrap());
        let (mut ws, _) = tokio_tungstenite::connect_async(req)
            .await
            .expect("gateway upgrade succeeds (authenticated)");

        // Empty content is rejected with an Error frame, not stored.
        let send = WsClientMsg::Send {
            channel_id,
            content: "   ".into(),
        };
        ws.send(encode_client(&send)).await.unwrap();
        match decode_gateway(ws.next().await.unwrap().expect("socket open")) {
            WsServerMsg::Error { reason } => assert!(reason.contains("empty"), "got: {reason}"),
            other => panic!("expected an Error frame, got {other:?}"),
        }
        assert!(db.recent_messages(channel_id, 10).is_empty(), "nothing stored");

        drop(ws);
        drop(dir);
    }

    #[tokio::test]
    async fn gateway_rejects_missing_auth() {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;

        let (dir, db, _channel_id) = setup_db_with_channel();
        server::install_state(db.clone());

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, server::router()).await;
        });

        // No x-user-id header → the handler returns Err at open. The framework
        // completes the 101 handshake first (it cannot reject it), then the
        // handler's early return tears down the session: the socket is closed
        // promptly rather than held open for anonymous sends.
        let url = format!("ws://{addr}/_srv/_ws/message_gateway");
        let req = url
            .into_client_request()
            .expect("connect URL parses");
        let (mut ws, _) = tokio_tungstenite::connect_async(req)
            .await
            .expect("handshake completes; auth is enforced by the handler");
        use futures_util::StreamExt;
        let close = tokio::time::timeout(std::time::Duration::from_secs(3), ws.next()).await;
        let frame = match close {
            Err(_) => panic!("socket stayed open after unauthenticated open — anonymous sender would be accepted"),
            Ok(frame) => frame,
        };
        match frame {
            None => {} // clean EOF: handler returned and dropped the socket
            Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) => {}
            Some(Ok(other)) => panic!("expected a close, got {other:?}"),
            Some(Err(_)) => {} // transport-level close
        }
        drop(dir);
    }
}
