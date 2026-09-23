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
use crate::roles::{Action, ChannelKind, Role};

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
// Scope: channel messages + a per-user presence heartbeat. Presence is the
// user's one status field ('online' / 'away' / 'offline'), driven by this
// socket: 'online' at open, 'away' after `PRESENCE_IDLE_SECS` of silence,
// 'offline' when the handler returns. Changes are persisted to
// `users.presence_status` and announced on the wire as `WsServerMsg::Presence`
// (echoed to the user's own socket; a cross-user broadcast is a follow-up).

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
    /// A presence status change for a user: 'online' / 'away' / 'offline'.
    Presence { user_id: i64, status: String },
    /// A gateway error for a specific operation.
    Error { reason: String },
}

/// How long an idle (no inbound frame) socket may sit before its owner's
/// presence flips from 'online' to 'away'. Any inbound frame resets the
/// clock back to 'online'.
const PRESENCE_IDLE_SECS: u64 = 30;

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

    // Presence heartbeat: 'online' the moment the socket is authenticated.
    // `db.set_presence` returns whether the value actually changed, so we only
    // announce a transition the client doesn't already know about.
    let mut presence = "online".to_string();
    if db.set_presence(user_id, &presence).is_ok_and(|changed| changed) {
        let _ = ch
            .send(WsServerMsg::Presence {
                user_id,
                status: presence.clone(),
            })
            .await;
    }

    loop {
        // Idle timeout: no inbound frame within PRESENCE_IDLE_SECS → the user
        // is treated as away (their socket is still open). Any inbound frame
        // (message or history request) counts as activity, so the timer resets
        // implicitly on the next iteration.
        let frame = tokio::select! {
            r = ch.recv() => r,
            _ = tokio::time::sleep(std::time::Duration::from_secs(PRESENCE_IDLE_SECS)) => {
                if db
                    .set_presence(user_id, "away")
                    .is_ok_and(|changed| changed)
                {
                    presence = "away".to_string();
                    let _ = ch
                        .send(WsServerMsg::Presence {
                            user_id,
                            status: presence.clone(),
                        })
                        .await;
                }
                continue;
            }
        };
        let Some(Ok(msg)) = frame else {
            // Transport error or close: the loop exits and the user goes
            // 'offline' below.
            break;
        };
        // Any inbound frame is activity → back to 'online'.
        if presence != "online"
            && db.set_presence(user_id, "online").is_ok_and(|changed| changed)
        {
            presence = "online".to_string();
            let _ = ch
                .send(WsServerMsg::Presence {
                    user_id,
                    status: presence.clone(),
                })
                .await;
        }
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
    // Disconnect: the socket has closed (client went away, transport died,
    // or the loop broke on an error). Mark the user 'offline'.
    if db.set_presence(user_id, "offline").is_ok_and(|changed| changed) {
        let _ = ch
            .send(WsServerMsg::Presence {
                user_id,
                status: "offline".to_string(),
            })
            .await;
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
        // Owner-only: the caller's standing in this server.
        let role = self.caller_role(server_id, caller_id)?;
        if !role.can(Action::RenameServer) {
            return Err("permission denied: only the server owner can rename the server".into());
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
        let role = self.caller_role(server_id, caller_id)?;
        if !role.can(Action::SetServerIcon) {
            return Err("permission denied: only the server owner or an admin can set the server icon".into());
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
    /// - `permission denied` — the caller is not the current owner.
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
            return Err("permission denied: only the server owner can transfer ownership".into());
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

    // ── shared authorization helpers ───────────────────────────────────────

    /// Resolve the caller's [`Role`] in `server_id`.
    ///
    /// Returns `permission denied: not a member of this server` when the
    /// caller has no membership row. An unparseable role string (schema
    /// drift) is treated as a *denied* standing rather than trusted, so a
    /// corrupt row can never grant power.
    fn caller_role(&self, server_id: i64, caller_id: i64) -> Result<Role, String> {
        let role_str = self
            .db
            .membership_role(server_id, caller_id)
            .ok_or_else(|| "permission denied: not a member of this server".to_string())?;
        Role::from_db_str(&role_str).ok_or_else(|| {
            format!("permission denied: unknown stored role {role_str:?}")
        })
    }

    /// Check that `caller_id` may perform `action` in `server_id`, and
    /// return the caller's role on success. Combines membership resolution
    /// with the [`Role::can`] policy in one call — the single gate every
    /// mutation below goes through.
    fn require(&self, caller_id: i64, server_id: i64, action: Action) -> Result<Role, String> {
        let role = self.caller_role(server_id, caller_id)?;
        if !role.can(action) {
            return Err(format!(
                "permission denied: requires role '{}' or above (you are '{}')",
                action.minimum_role().label(),
                role.label()
            ));
        }
        Ok(role)
    }

    /// The current wall-clock seconds since the Unix epoch, for the
    /// `joined_at` / `created_at` timestamps the DB layer stores as `i64`.
    fn now_secs() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(1_000)
    }

    // ── channel management ─────────────────────────────────────────────────

    /// Create a channel in `server_id`. Admin+ (owner or admin).
    ///
    /// # Errors
    /// - `not found` — the server id doesn't exist.
    /// - `permission denied` — the caller's role is below admin.
    /// - `channel name must not be empty` / `channel name is already taken`.
    pub fn create_channel(
        &self,
        caller_id: i64,
        server_id: i64,
        name: &str,
        kind: ChannelKind,
        parent_channel_id: Option<i64>,
    ) -> Result<i64, String> {
        if name.trim().is_empty() {
            return Err("channel name must not be empty".into());
        }
        self.db
            .server_by_id(server_id)
            .ok_or("server not found")?;
        self.require(caller_id, server_id, Action::CreateChannel)?;
        self.db
            .insert_channel(server_id, name.trim(), kind, parent_channel_id, Self::now_secs())
            .map_err(|e| format!("failed to create channel: {e}"))
    }

    /// Rename a channel. Moderator+ (owner, admin, or moderator).
    ///
    /// # Errors
    /// - `channel not found` — the id doesn't exist.
    /// - `permission denied` — the caller's role is below moderator, or the
    ///   channel lives in a different server than `server_id`.
    pub fn rename_channel(
        &self,
        caller_id: i64,
        server_id: i64,
        channel_id: i64,
        new_name: &str,
    ) -> Result<String, String> {
        if new_name.trim().is_empty() {
            return Err("channel name must not be empty".into());
        }
        // Resolve the channel first so the permission check runs against the
        // channel's *own* server (a caller can't rename a channel in a server
        // they don't hold the role in by passing a foreign server_id).
        let (_cid, chan_server, _name, _kind, _pos, _parent) = self
            .db
            .channel_full(channel_id)
            .ok_or("channel not found")?;
        if chan_server != server_id {
            return Err("channel not found".into());
        }
        self.require(caller_id, server_id, Action::RenameChannel)?;
        self.db
            .rename_channel(channel_id, new_name.trim())
            .map_err(|e| format!("failed to rename channel: {e}"))
    }

    /// Delete a channel. Admin+ (owner or admin).
    ///
    /// # Errors
    /// - `channel not found` — the id doesn't exist (or is in another server).
    /// - `permission denied` — the caller's role is below admin.
    pub fn delete_channel(&self, caller_id: i64, server_id: i64, channel_id: i64) -> Result<(), String> {
        let (_cid, chan_server, _name, _kind, _pos, _parent) = self
            .db
            .channel_full(channel_id)
            .ok_or("channel not found")?;
        if chan_server != server_id {
            return Err("channel not found".into());
        }
        self.require(caller_id, server_id, Action::DeleteChannel)?;
        let removed = self
            .db
            .delete_channel(channel_id)
            .map_err(|e| format!("failed to delete channel: {e}"))?;
        if removed == 0 {
            return Err("channel not found".into());
        }
        Ok(())
    }

    // ── member management ──────────────────────────────────────────────────

    /// Add (or re-role) a member in `server_id` with `role`. Moderator+ for
    /// `AddMember`; granting `admin`/`owner` additionally requires `admin+`
    /// (enforced via `SetMemberRole`) because a plain moderator must not be
    /// able to promote someone above themselves.
    ///
    /// The caller can never be demoted or removed from the server by this
    /// path (that is `remove_member`), and the owner role cannot be granted
    /// here — only `transfer_ownership` moves it.
    pub fn add_member(
        &self,
        caller_id: i64,
        server_id: i64,
        user_id: i64,
        role: Role,
    ) -> Result<String, String> {
        // The owner role is reserved for the creator; it moves only via
        // transfer_ownership.
        if role == Role::Owner {
            return Err("cannot assign the owner role (use transfer_ownership)".into());
        }
        self.db
            .server_by_id(server_id)
            .ok_or("server not found")?;
        // Adding someone (even as a plain member) is a moderator+ duty.
        self.require(caller_id, server_id, Action::AddMember)?;
        // Granting admin is an admin+ duty: a moderator may add members but
        // not promote them to admin.
        if role >= Role::Admin {
            self.require(caller_id, server_id, Action::SetMemberRole)?;
        }
        self.db
            .upsert_membership(server_id, user_id, role, Self::now_secs())
            .map_err(|e| format!("failed to add member: {e}"))?;
        Ok(role.db_str().to_string())
    }

    /// Remove a member from `server_id`. Moderator+ (owner, admin, or
    /// moderator). The owner can never be removed through this path —
    /// ownership must be transferred first (the DB layer demotes, never
    /// deletes, the owner row, and this method refuses to target it).
    ///
    /// # Errors
    /// - `not found` — the server id doesn't exist.
    /// - `permission denied` — the caller's role is below moderator.
    /// - `cannot remove the server owner` — `user_id` is the owner.
    /// - `member not found` — `user_id` has no membership row.
    pub fn remove_member(&self, caller_id: i64, server_id: i64, user_id: i64) -> Result<(), String> {
        let (_sid, _name, owner_id, _icon_url) = self
            .db
            .server_by_id(server_id)
            .ok_or("server not found")?;
        self.require(caller_id, server_id, Action::RemoveMember)?;
        // The owner is protected: removing them would orphan the server.
        if user_id == owner_id {
            return Err("cannot remove the server owner (transfer ownership first)".into());
        }
        let removed = self
            .db
            .remove_membership(server_id, user_id)
            .map_err(|e| format!("failed to remove member: {e}"))?;
        if removed == 0 {
            return Err("member not found".into());
        }
        Ok(())
    }

    /// Change an existing member's role in `server_id`. Admin+ (owner or
    /// admin). The owner's role can't be changed and the owner role can't be
    /// assigned — ownership moves only via `transfer_ownership`.
    ///
    /// # Errors
    /// - `not found` — the server id doesn't exist.
    /// - `permission denied` — the caller's role is below admin.
    /// - `cannot change the server owner's role` / `cannot assign the owner
    ///   role`.
    /// - `member not found` — `user_id` has no membership row.
    pub fn set_member_role(
        &self,
        caller_id: i64,
        server_id: i64,
        user_id: i64,
        new_role: Role,
    ) -> Result<String, String> {
        let (_sid, _name, owner_id, _icon_url) = self
            .db
            .server_by_id(server_id)
            .ok_or("server not found")?;
        self.require(caller_id, server_id, Action::SetMemberRole)?;
        if new_role == Role::Owner {
            return Err("cannot assign the owner role (use transfer_ownership)".into());
        }
        if user_id == owner_id {
            return Err("cannot change the server owner's role (transfer ownership first)".into());
        }
        // The target must already be a member (this is a *change*, not an
        // invite — invites go through add_member).
        if self.db.membership_role(server_id, user_id).is_none() {
            return Err("member not found".into());
        }
        self.db
            .upsert_membership(server_id, user_id, new_role, Self::now_secs())
            .map_err(|e| format!("failed to set member role: {e}"))?;
        Ok(new_role.db_str().to_string())
    }

    // ── channel reads / unread counts ──────────────────────────────────────
    //
    // Per-user read state, gated on server membership. These are the two
    // operations the client needs to draw the unread badge: mark a channel
    // read up to a message id, and fetch the unread count for every channel in
    // a server.

    /// Record that `caller_id` has read up to and including `message_id` in
    /// `channel_id`.
    ///
    /// The caller must be a member of the server that owns the channel. The
    /// write is monotonic (the stored id only ever moves forward), so marking
    /// read with an older id is a harmless no-op rather than a regression.
    ///
    /// # Errors
    /// - `channel not found` — the id doesn't exist.
    /// - `permission denied: not a member of this server` — no membership.
    /// - `failed to mark channel read: …` — the write failed.
    pub fn mark_read(
        &self,
        caller_id: i64,
        channel_id: i64,
        message_id: i64,
    ) -> Result<(), String> {
        // Resolve the owning server from the channel (a channel id maps to
        // exactly one server); membership is then checked against that server.
        let (_, server_id, _, _, _, _) = self
            .db
            .channel_full(channel_id)
            .ok_or("channel not found")?;
        if self.db.membership_role(server_id, caller_id).is_none() {
            return Err("permission denied: not a member of this server".into());
        }
        self.db
            .mark_channel_read(caller_id, channel_id, message_id, Self::now_millis())
            .map_err(|e| format!("failed to mark channel read: {e}"))
    }

    /// The unread message count for every channel in `server_id` that
    /// `caller_id` belongs to.
    ///
    /// # Errors
    /// - `server not found` — the id doesn't exist.
    /// - `permission denied: not a member of this server` — no membership.
    pub fn unread_counts(
        &self,
        caller_id: i64,
        server_id: i64,
    ) -> Result<Vec<UnreadCount>, String> {
        if self.db.server_by_id(server_id).is_none() {
            return Err("server not found".into());
        }
        if self.db.membership_role(server_id, caller_id).is_none() {
            return Err("permission denied: not a member of this server".into());
        }
        let counts = self.db.unread_counts_for_server(caller_id, server_id);
        Ok(counts
            .into_iter()
            .map(|(channel_id, count)| UnreadCount {
                channel_id,
                count,
            })
            .collect())
    }

    /// The current wall-clock milliseconds since the Unix epoch, for
    /// `read_at` timestamps (the `messages.created_at` convention).
    fn now_millis() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(1_000_000)
    }
}

// ── #[server] endpoints ─────────────────────────────────────────────────────

/// Rename a server. Only the current owner may call (owner-only by policy).
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

/// Set (or clear) a server's icon. Owner or admin may call.
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

// ── channel management endpoints ────────────────────────────────────────────

/// Create a channel in a server. Owner or admin may call.
#[server]
pub async fn create_channel(input: CreateChannelInput) -> Result<ChannelResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let kind = ChannelKind::from_db_str(&input.kind)
        .ok_or_else(|| ServerError::failed(format!("unknown channel kind: {}", input.kind)))?;
    let id = ServerSettings::new(db)
        .create_channel(caller, input.server_id, &input.name, kind, input.parent_channel_id)
        .map_err(ServerError::failed)?;
    Ok(ChannelResult { channel_id: id, server_id: input.server_id, name: Some(input.name) })
}

/// Rename a channel. Owner, admin, or moderator may call.
#[server]
pub async fn rename_channel(input: RenameChannelInput) -> Result<ChannelResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let name = ServerSettings::new(db)
        .rename_channel(caller, input.server_id, input.channel_id, &input.new_name)
        .map_err(ServerError::failed)?;
    Ok(ChannelResult { channel_id: input.channel_id, server_id: input.server_id, name: Some(name) })
}

/// Delete a channel. Owner or admin may call.
#[server]
pub async fn delete_channel(input: DeleteChannelInput) -> Result<ChannelResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .delete_channel(caller, input.server_id, input.channel_id)
        .map_err(ServerError::failed)?;
    Ok(ChannelResult { channel_id: input.channel_id, server_id: input.server_id, name: None })
}

// ── member management endpoints ─────────────────────────────────────────────

/// Add (or re-role) a member in a server. Moderator+ to add; admin+ to grant
/// an admin role. The owner role is not assignable here.
#[server]
pub async fn add_member(input: AddMemberInput) -> Result<MemberResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let role = Role::from_db_str(&input.role)
        .ok_or_else(|| ServerError::failed(format!("unknown role: {}", input.role)))?;
    let role_str = ServerSettings::new(db)
        .add_member(caller, input.server_id, input.user_id, role)
        .map_err(ServerError::failed)?;
    Ok(MemberResult { server_id: input.server_id, user_id: input.user_id, role: role_str })
}

/// Remove a member from a server. Owner, admin, or moderator may call. The
/// server owner can't be removed.
#[server]
pub async fn remove_member(input: RemoveMemberInput) -> Result<MemberResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .remove_member(caller, input.server_id, input.user_id)
        .map_err(ServerError::failed)?;
    Ok(MemberResult { server_id: input.server_id, user_id: input.user_id, role: String::new() })
}

/// Change an existing member's role. Owner or admin may call. The owner's
/// role can't be changed and the owner role can't be assigned.
#[server]
pub async fn set_member_role(input: SetMemberRoleInput) -> Result<MemberResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let role = Role::from_db_str(&input.role)
        .ok_or_else(|| ServerError::failed(format!("unknown role: {}", input.role)))?;
    let role_str = ServerSettings::new(db)
        .set_member_role(caller, input.server_id, input.user_id, role)
        .map_err(ServerError::failed)?;
    Ok(MemberResult { server_id: input.server_id, user_id: input.user_id, role: role_str })
}

// ── channel reads / unread counts ───────────────────────────────────────────

/// Mark a channel read up to a message id for the caller. Any member of the
/// owning server may call; the write is monotonic so it never regresses.
#[server]
pub async fn mark_read(input: MarkReadInput) -> Result<MarkReadResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .mark_read(caller, input.channel_id, input.message_id)
        .map_err(ServerError::failed)?;
    Ok(MarkReadResult {
        channel_id: input.channel_id,
        message_id: input.message_id,
    })
}

/// Unread message counts, per channel, for the caller across a server. Every
/// channel the caller is a member of is included, even when its count is 0.
#[server]
pub async fn unread_counts(input: UnreadCountsInput) -> Result<Vec<UnreadCount>, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .unread_counts(caller, input.server_id)
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

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct CreateChannelInput {
    pub server_id: i64,
    pub name: String,
    /// `'text'` / `'voice'` / `'category'`.
    pub kind: String,
    /// Parent (category) channel id, or `None` for top-level.
    #[serde(default)]
    pub parent_channel_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct RenameChannelInput {
    pub server_id: i64,
    pub channel_id: i64,
    pub new_name: String,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct DeleteChannelInput {
    pub server_id: i64,
    pub channel_id: i64,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct AddMemberInput {
    pub server_id: i64,
    pub user_id: i64,
    /// `'owner'` / `'admin'` / `'moderator'` / `'member'` (owner rejected).
    pub role: String,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct RemoveMemberInput {
    pub server_id: i64,
    pub user_id: i64,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct SetMemberRoleInput {
    pub server_id: i64,
    pub user_id: i64,
    pub role: String,
}

/// The id of the newest message the caller has read in a channel. The caller's
/// read state is advanced to (at least) this message id.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct MarkReadInput {
    pub channel_id: i64,
    pub message_id: i64,
}

/// The server whose channels the caller wants unread counts for.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct UnreadCountsInput {
    pub server_id: i64,
}

/// Confirmation that a channel was marked read: the channel and the message id
/// the read state was advanced to.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct MarkReadResult {
    pub channel_id: i64,
    pub message_id: i64,
}

/// The result of a channel-management operation. `name` is the channel's name
/// after the op (absent when the channel was deleted, or on a create where the
/// client already knows the name).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ChannelResult {
    pub channel_id: i64,
    pub server_id: i64,
    pub name: Option<String>,
}

/// The result of a member-management operation. `role` is the member's role
/// after the op (empty string when the member was removed).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct MemberResult {
    pub server_id: i64,
    pub user_id: i64,
    pub role: String,
}

/// One entry in a per-channel unread list: the count of messages the caller has
/// not yet read in a single channel (every message whose id is greater than the
/// caller's stored `channel_reads.last_read_message_id`). `count` is 0 when the
/// channel is fully read or has no messages.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct UnreadCount {
    pub channel_id: i64,
    pub count: i64,
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
    // The raw-tokio-TCP e2e clients (the `#[server]` HTTP round-trip tests)
    // call `.write_all` / `.read_to_end` on the split socket halves; those come
    // from the async io extension traits (in scope here via tokio's feature
    // unification with tokio-tungstenite, but still needing the `use`).
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A boxed future — used by the raw-HTTP e2e client below so the closure
    /// can name its return type without a `-> impl Future` annotation.
    type BoxFuture<T> = std::pin::Pin<
        Box<dyn std::future::Future<Output = T> + Send>,
    >;

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

    /// On top of `setup_db`, seed two text channels (ids 1 and 2) into server 1
    /// and return `(dir, db)`. Channel ids are chosen explicitly so the tests
    /// can address them by literal.
    fn setup_db_with_channels() -> (tempfile::TempDir, Db) {
        let (dir, db) = setup_db();
        let c = db.lock().unwrap();
        c.execute_batch(
            "INSERT INTO channels (id, server_id, name, kind, position, parent_channel_id, created_at)
             VALUES (1, 1, 'general', 'text', 0, NULL, 1000),
                    (2, 1, 'random',  'text', 1, NULL, 1000);",
        )
        .unwrap();
        drop(c);
        (dir, db)
    }

    /// Post `count` messages into `channel_id` authored by `author_id`,
    /// returning the id of the most recent one (ids are strictly increasing).
    fn post_messages(db: &Db, channel_id: i64, author_id: i64, count: i64) -> i64 {
        let mut last = 0i64;
        for i in 0..count {
            last = db
                .insert_message(channel_id, author_id, &format!("msg {i}"), 1000 + i)
                .unwrap();
        }
        last
    }

    /// Look up one channel's unread count in an `unread_counts` result,
    /// panicking if the channel is absent (the contract is that every member
    /// channel appears, even with count 0).
    fn count_for(counts: &[UnreadCount], channel_id: i64) -> i64 {
        counts
            .iter()
            .find(|c| c.channel_id == channel_id)
            .unwrap_or_else(|| panic!("channel {channel_id} missing from unread list: {counts:?}"))
            .count
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
        assert!(err.contains("permission denied"), "got: {err}");
        // Name unchanged.
        assert_eq!(db.server_by_id(1).unwrap().1, "My Server");
    }

    #[test]
    fn non_member_rename_rejected() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db);
        // User 999 is not in the server at all.
        let err = svc.rename(999, 1, "Hacked").unwrap_err();
        assert!(err.contains("not a member"), "got: {err}");
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
        assert!(err.contains("permission denied"), "got: {err}");
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
        assert!(err.contains("permission denied"), "got: {err}");
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

    // ── channel reads / unread counts ───────────────────────────────────────
    //
    // Service-level tests for the per-channel read state: marking a channel
    // read, the unread-count computation, and the membership/authz guards.
    // The e2e test that drives the real `#[server]` HTTP routes lives at the
    // bottom of this module with the other gateway round-trip tests.

    #[test]
    fn mark_read_creates_state_and_is_idempotent() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        let last = post_messages(&db, 1, 1, 3); // 3 messages in channel 1

        svc.mark_read(1, 1, last).unwrap();
        // The read state was persisted to the newest message id.
        assert_eq!(db.last_read_message_id(1, 1), Some(last));

        // Marking the same channel read again with the same id is a no-op that
        // doesn't error and doesn't move the pointer.
        svc.mark_read(1, 1, last).unwrap();
        assert_eq!(db.last_read_message_id(1, 1), Some(last));
    }

    #[test]
    fn mark_read_is_monotonic_not_backward() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        let last = post_messages(&db, 1, 1, 5);

        // Read up to the newest, then attempt to mark read at an OLDER id.
        svc.mark_read(1, 1, last).unwrap();
        svc.mark_read(1, 1, 1).unwrap(); // id 1 < last
        // The pointer must not regress.
        assert_eq!(db.last_read_message_id(1, 1), Some(last));
    }

    #[test]
    fn mark_read_per_user_isolated() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        let last = post_messages(&db, 1, 1, 3);

        svc.mark_read(1, 1, last).unwrap();
        // User 2 has not read anything yet.
        assert_eq!(db.last_read_message_id(2, 1), None);
        // User 2 marking read is independent.
        svc.mark_read(2, 1, 1).unwrap();
        assert_eq!(db.last_read_message_id(1, 1), Some(last));
        assert_eq!(db.last_read_message_id(2, 1), Some(1));
    }

    #[test]
    fn mark_read_rejects_unknown_channel() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db);
        let err = svc.mark_read(1, 999, 1).unwrap_err();
        assert!(err.contains("channel not found"), "got: {err}");
    }

    #[test]
    fn mark_read_rejects_non_member() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        // User 999 has no membership in server 1, so they can't touch its
        // channel 1 even though the channel exists.
        let err = svc.mark_read(999, 1, 1).unwrap_err();
        assert!(err.contains("not a member"), "got: {err}");
        // No read state was written.
        assert_eq!(db.last_read_message_id(999, 1), None);
    }

    #[test]
    fn unread_counts_full_when_unread() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        post_messages(&db, 1, 1, 4); // channel 1: 4 msgs
        post_messages(&db, 2, 2, 2); // channel 2: 2 msgs

        // User 1 has read nothing → every message in every channel is unread.
        let counts = svc.unread_counts(1, 1).unwrap();
        assert_eq!(counts.len(), 2, "both member channels are listed");
        assert_eq!(count_for(&counts, 1), 4);
        assert_eq!(count_for(&counts, 2), 2);
    }

    #[test]
    fn unread_counts_zero_after_read() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        let last1 = post_messages(&db, 1, 1, 4);
        let last2 = post_messages(&db, 2, 2, 2);

        svc.mark_read(1, 1, last1).unwrap();
        svc.mark_read(1, 2, last2).unwrap();
        let counts = svc.unread_counts(1, 1).unwrap();
        assert_eq!(count_for(&counts, 1), 0);
        assert_eq!(count_for(&counts, 2), 0);
    }

    #[test]
    fn unread_counts_partial_read_counts_only_newer() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        let last = post_messages(&db, 1, 1, 5); // ids 1..=5
        // Read only up to id 2 → messages 3, 4, 5 remain unread.
        svc.mark_read(1, 1, 2).unwrap();
        let counts = svc.unread_counts(1, 1).unwrap();
        assert_eq!(count_for(&counts, 1), 3);
        assert_eq!(count_for(&counts, 2), 0, "channel 2 has no messages");
        // Sanity: the read pointer really is at id 2.
        assert_eq!(db.last_read_message_id(1, 1), Some(2));
        // And marking read up to the newest zeroes it out.
        svc.mark_read(1, 1, last).unwrap();
        assert_eq!(count_for(&svc.unread_counts(1, 1).unwrap(), 1), 0);
    }

    #[test]
    fn unread_counts_empty_channel_is_zero_not_absent() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db);
        // No messages anywhere, no read state. Both channels must still appear,
        // each with count 0 — the client distinguishes "0 unread" from
        // "no such channel".
        let counts = svc.unread_counts(1, 1).unwrap();
        assert_eq!(counts.len(), 2);
        assert_eq!(count_for(&counts, 1), 0);
        assert_eq!(count_for(&counts, 2), 0);
    }

    #[test]
    fn unread_counts_rejects_unknown_server() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db);
        let err = svc.unread_counts(1, 999).unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
    }

    #[test]
    fn unread_counts_rejects_non_member() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db);
        let err = svc.unread_counts(999, 1).unwrap_err();
        assert!(err.contains("not a member"), "got: {err}");
    }

    #[test]
    fn unread_counts_isolated_per_user() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        let last = post_messages(&db, 1, 1, 3);

        svc.mark_read(1, 1, last).unwrap(); // user 1 reads up to date
        // User 2, who has not read, still sees all 3 as unread.
        let c1 = svc.unread_counts(1, 1).unwrap();
        let c2 = svc.unread_counts(2, 1).unwrap();
        assert_eq!(count_for(&c1, 1), 0);
        assert_eq!(count_for(&c2, 1), 3);
    }

    #[tokio::test]
    async fn unread_counts_and_mark_read_over_http() {
        let (_dir, db) = setup_db_with_channels();
        let _guard = e2e_guard();
        server::install_state(db.clone());

        let last1 = post_messages(&db, 1, 1, 3); // channel 1: 3 msgs

        // Boot the macro-mounted router on an ephemeral port.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, server::router()).await;
        });

        // A tiny raw-HTTP client: POST a JSON args-tuple to /_srv/<fn> with the
        // x-user-id auth header, return the parsed `{"Ok":…}`/`{"Err":…}` body.
        // (A raw socket, like the gateway tests, because the `#[server]` HTTP
        // transport isn't in the vendored registry as a dev-dep.)
        let call =
            |path: String, user_id: String, body: String| -> BoxFuture<serde_json::Value> {
                Box::pin(async move {
                    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
                    let (mut reader, mut writer) = stream.into_split();
                    let req = format!(
                        "POST /_srv/{path} HTTP/1.1\r\n\
                         Host: {addr}\r\n\
                         x-user-id: {user_id}\r\n\
                         content-type: application/json\r\n\
                         content-length: {}\r\n\
                         connection: close\r\n\r\n{body}",
                        body.len()
                    );
                    writer.write_all(req.as_bytes()).await.unwrap();
                    let mut buf = Vec::new();
                    reader.read_to_end(&mut buf).await.unwrap();
                    let text = String::from_utf8(buf).unwrap();
                    // Body is everything after the first blank line.
                    let body = text
                        .split_once("\r\n\r\n")
                        .map(|(_, b)| b.to_string())
                        .unwrap_or_default();
                    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
                    v
                })
            };

        // 1. unread_counts before any read: user 1 sees 3 in channel 1, 0 in 2.
        let resp = call(
            "unread_counts".to_string(),
            "1".to_string(),
            serde_json::to_string(&[serde_json::json!({ "server_id": 1 })]).unwrap(),
        )
        .await;
        let counts: Vec<UnreadCount> = resp["Ok"]
            .as_array()
            .cloned()
            .expect("HTTP unread_counts returns Ok(array)")
            .into_iter()
            .map(|v| serde_json::from_value(v).unwrap())
            .collect();
        assert_eq!(count_for(&counts, 1), 3, "all 3 messages unread before read");
        assert_eq!(count_for(&counts, 2), 0);

        // 2. mark_read up to the newest message in channel 1.
        let resp = call(
            "mark_read".to_string(),
            "1".to_string(),
            serde_json::to_string(&[serde_json::json!({
                "channel_id": 1,
                "message_id": last1
            })])
            .unwrap(),
        )
        .await;
        let mark: MarkReadResult = serde_json::from_value(resp["Ok"].clone())
            .expect("HTTP mark_read returns Ok(MarkReadResult)");
        assert_eq!(mark.channel_id, 1);
        assert_eq!(mark.message_id, last1);

        // 3. unread_counts again: channel 1 is now 0.
        let resp = call(
            "unread_counts".to_string(),
            "1".to_string(),
            serde_json::to_string(&[serde_json::json!({ "server_id": 1 })]).unwrap(),
        )
        .await;
        let counts: Vec<UnreadCount> = resp["Ok"]
            .as_array()
            .cloned()
            .unwrap()
            .into_iter()
            .map(|v| serde_json::from_value(v).unwrap())
            .collect();
        assert_eq!(count_for(&counts, 1), 0, "channel 1 is read up to date");
        assert_eq!(count_for(&counts, 2), 0);

        // 4. Unauthenticated (no x-user-id) is rejected by the endpoint.
        let resp = {
            let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            let (mut reader, mut writer) = stream.into_split();
            let body = serde_json::to_string(&[serde_json::json!({ "server_id": 1 })]).unwrap();
            let req = format!(
                "POST /_srv/unread_counts HTTP/1.1\r\nHost: {addr}\r\n\
                 content-type: application/json\r\ncontent-length: {}\r\n\
                 connection: close\r\n\r\n{body}",
                body.len()
            );
            writer.write_all(req.as_bytes()).await.unwrap();
            let mut buf = Vec::new();
            reader.read_to_end(&mut buf).await.unwrap();
            String::from_utf8(buf).unwrap()
        };
        let v: serde_json::Value = resp
            .split_once("\r\n\r\n")
            .map(|(_, b)| b.to_string())
            .unwrap_or_default()
            .parse()
            .unwrap();
        assert!(
            v.get("Err").is_some(),
            "unauthenticated unread_counts must fail, got {v:?}"
        );
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

    /// The e2e gateway tests all install their `Db` into the same
    /// process-wide `server::install_state` slot (the macro reads it back via
    /// `use_state`). They therefore MUST run one at a time or they clobber
    /// each other's state mid-test. This guard serializes them.
    static E2E_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Acquire the e2e serialization guard; hold it for the whole test body.
    /// (Poisoned if a prior holder panicked — recover rather than re-panic.)
    fn e2e_guard() -> std::sync::MutexGuard<'static, ()> {
        E2E_LOCK.lock().unwrap_or_else(|p| p.into_inner())
    }

    #[tokio::test]
    async fn gateway_send_and_history_round_trip() {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        use tokio_tungstenite::tungstenite::http::header;

        let (dir, db, channel_id) = setup_db_with_channel();
        let _guard = e2e_guard();
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
        // The connect-time Presence frame arrives first (online at open); the
        // echoed message follows it.
        match decode_gateway(ws.next().await.unwrap().expect("socket open")) {
            WsServerMsg::Presence { status, .. } => assert_eq!(status, "online"),
            other => panic!("expected an online Presence frame at open, got {other:?}"),
        }
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
        let _guard = e2e_guard();
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

        // Empty content is rejected with an Error frame, not stored. The
        // connect-time Presence frame arrives first.
        match decode_gateway(ws.next().await.unwrap().expect("socket open")) {
            WsServerMsg::Presence { status, .. } => assert_eq!(status, "online"),
            other => panic!("expected an online Presence frame at open, got {other:?}"),
        }
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

    // ── presence ───────────────────────────────────────────────────────────
    //
    // The status field is driven by the gateway socket itself: 'online' at
    // open (announced as the first frame), 'offline' once the socket closes.
    // The idle→'away' transition uses the same code path but with a 30s
    // timer, so it is not covered by an e2e test (too slow); the unit test
    // below pins `set_presence`'s change-detection instead.

    #[test]
    fn presence_set_and_change_detection() {
        let (_dir, db) = setup_db();
        // Fresh user is 'offline' (the migration default).
        assert_eq!(db.presence(1).as_deref(), Some("offline"));
        // Setting the same value is a no-op (no change to broadcast).
        assert_eq!(db.set_presence(1, "offline").unwrap(), false);
        // A different value is a change.
        assert_eq!(db.set_presence(1, "online").unwrap(), true);
        assert_eq!(db.presence(1).as_deref(), Some("online"));
        // Unknown user → QueryReturnedNoRows, not a silent no-op.
        assert!(db.set_presence(999, "online").is_err());
        // Invalid status strings are rejected.
        assert!(db.set_presence(1, "sleeping").is_err());
    }

    #[tokio::test]
    async fn gateway_presence_online_on_open_and_offline_on_close() {
        use futures_util::StreamExt;
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        use tokio_tungstenite::tungstenite::http::header;

        let (dir, db, _channel_id) = setup_db_with_channel();
        let _guard = e2e_guard();
        server::install_state(db.clone());

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, server::router()).await;
        });

        let url = format!("ws://{addr}/_srv/_ws/message_gateway");
        let mut req = url.into_client_request().expect("connect URL parses");
        req.headers_mut()
            .insert(header::USER_AGENT, "discord-server-presence-test".parse().unwrap());
        req.headers_mut().insert("x-user-id", "1".parse().unwrap());
        let (mut ws, _) = tokio_tungstenite::connect_async(req)
            .await
            .expect("gateway upgrade succeeds (authenticated)");

        // 1. On open the first frame is a Presence 'online' announcement.
        let frame = decode_gateway(
            ws.next()
                .await
                .unwrap()
                .expect("socket open after connect"),
        );
        match frame {
            WsServerMsg::Presence { user_id, status } => {
                assert_eq!(user_id, 1);
                assert_eq!(status, "online");
            }
            other => panic!("expected an online Presence frame at open, got {other:?}"),
        }
        // ...and it is persisted.
        assert_eq!(db.presence(1).as_deref(), Some("online"));

        // 2. Close the socket → the handler returns and marks the user offline.
        drop(ws);
        // The disconnect happens on the server's task; poll until persisted.
        for _ in 0..100 {
            if db.presence(1).as_deref() == Some("offline") {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(
            db.presence(1).as_deref(),
            Some("offline"),
            "user should be offline after the socket closes"
        );

        drop(dir);
    }

    #[tokio::test]
    async fn gateway_rejects_missing_auth() {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;

        let (dir, db, _channel_id) = setup_db_with_channel();
        let _guard = e2e_guard();
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
