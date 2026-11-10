//! Channel management endpoints: create, rename, delete.

use serde::Deserialize;
use server::{server, ServerError};

#[cfg(feature = "server")]
use crate::permissions::{self, Permissions};
#[cfg(feature = "server")]
use crate::roles::{ChannelKind};

/// The acting user for the current request, resolved from a verified session
/// token (see [`crate::session`]); `None` when unauthenticated.
#[cfg(feature = "server")]
use crate::session::acting_user;
#[cfg(feature = "server")]
use crate::server_settings::{ServerSettings, db_from_state};

#[cfg(feature = "server")]
impl ServerSettings {
    // ── channel management ─────────────────────────────────────────────────

    /// Create a channel in `server_id`. Needs Manage Channels, in the
    /// category too when there is one.
    ///
    /// # Errors
    /// - `server not found` — the server id doesn't exist.
    /// - `you need the Manage Channels permission to do that`.
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
        self.require(caller_id, server_id, Permissions::MANAGE_CHANNELS)?;
        if let Some(parent) = parent_channel_id {
            if self.require_in_channel(caller_id, parent, Permissions::MANAGE_CHANNELS)? != server_id {
                return Err("channel not found".into());
            }
        }
        self.db
            .insert_channel(server_id, name.trim(), kind, parent_channel_id, Self::now_secs())
            .map_err(|e| format!("failed to create channel: {e}"))
    }

    /// Rename a channel. Needs Manage Channels in it.
    ///
    /// # Errors
    /// - `channel not found` — the id doesn't exist, the caller can't see it,
    ///   or it lives in a different server than `server_id`.
    /// - `you need the Manage Channels permission to do that`.
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
        self.require_in_channel(caller_id, channel_id, Permissions::MANAGE_CHANNELS)?;
        self.db
            .rename_channel(channel_id, new_name.trim())
            .map_err(|e| format!("failed to rename channel: {e}"))
    }

    /// Delete a channel. Needs Manage Channels in it.
    ///
    /// # Errors
    /// - `channel not found` — the id doesn't exist (or is in another server).
    /// - `you need the Manage Channels permission to do that`.
    pub fn delete_channel(&self, caller_id: i64, server_id: i64, channel_id: i64) -> Result<(), String> {
        let (_cid, chan_server, _name, _kind, _pos, _parent) = self
            .db
            .channel_full(channel_id)
            .ok_or("channel not found")?;
        if chan_server != server_id {
            return Err("channel not found".into());
        }
        self.require_in_channel(caller_id, channel_id, Permissions::MANAGE_CHANNELS)?;
        let removed = self
            .db
            .delete_channel(channel_id)
            .map_err(|e| format!("failed to delete channel: {e}"))?;
        if removed == 0 {
            return Err("channel not found".into());
        }
        Ok(())
    }
}

// ── channel management endpoints ────────────────────────────────────────────

/// Create a channel in a server. Owner or admin may call.
#[server]
pub async fn create_channel(input: CreateChannelInput) -> Result<ChannelResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let kind = ChannelKind::from_db_str(&input.kind)
        .ok_or_else(|| ServerError::failed(format!("unknown channel kind: {}", input.kind)))?;
    let id = ServerSettings::new(db.clone())
        .create_channel(caller, input.server_id, &input.name, kind, input.parent_channel_id)
        .map_err(ServerError::failed)?;
    if input.private {
        permissions::make_private(&db, id, caller).map_err(ServerError::failed)?;
    }
    // The new channel's permissions are part of every member's snapshot, so
    // tell the server's members to refetch (they re-evaluate the composer's
    // no-permission cover against the fresh channel).
    crate::membership::access_changed(
        &db,
        input.server_id,
        crate::membership::AccessChange::ChannelOverridesChanged { channel_id: id },
    );
    Ok(ChannelResult { channel_id: id, server_id: input.server_id, name: Some(input.name) })
}

/// Rename a channel. Needs Manage Channels in it.
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
    ServerSettings::new(db.clone())
        .delete_channel(caller, input.server_id, input.channel_id)
        .map_err(ServerError::failed)?;
    // A deleted channel changes what members can see and do: refetch.
    crate::membership::access_changed(
        &db,
        input.server_id,
        crate::membership::AccessChange::ChannelOverridesChanged { channel_id: input.channel_id },
    );
    Ok(ChannelResult { channel_id: input.channel_id, server_id: input.server_id, name: None })
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
    /// Hide the channel from @everyone; only its creator (and anyone given
    /// access later) can see it.
    #[serde(default)]
    pub private: bool,
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

/// The result of a channel-management operation. `name` is the channel's name
/// after the op (absent when the channel was deleted, or on a create where the
/// client already knows the name).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ChannelResult {
    pub channel_id: i64,
    pub server_id: i64,
    pub name: Option<String>,
}
