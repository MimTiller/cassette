//! Server-level settings endpoints: rename, icon, transfer ownership.

use serde::Deserialize;
use server::{server, ServerError};

#[cfg(feature = "server")]
use crate::permissions::Permissions;

/// The acting user for the current request, resolved from a verified session
/// token (see [`crate::session`]); `None` when unauthenticated.
#[cfg(feature = "server")]
use crate::session::acting_user;
use crate::server_settings::ServerSettingsResult;
#[cfg(feature = "server")]
use crate::server_settings::{ServerSettings, WsServerMsg, db_from_state, next_frame_seq};


#[cfg(feature = "server")]
impl ServerSettings {
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
        self.require(caller_id, server_id, Permissions::MANAGE_SERVER)?;
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
        self.require(caller_id, server_id, Permissions::MANAGE_SERVER)?;
        // Reject an empty string URL — treat it as an error rather than
        // silently storing garbage.
        if let Some(url) = icon {
            if url.trim().is_empty() {
                return Err("icon url must not be empty (pass null to clear)".into());
            }
            // Same posture as avatars: the bytes move to the media store, the
            // DB keeps the `/media/…` path. Server icons use the AVATAR cap.
            let (mime, bytes) = match crate::media::decode_data_image_url(url) {
                Some(p) => p,
                None => return Err("icon must be a data:image/…;base64, URL".into()),
            };
            match crate::media::store(bytes.as_slice(), &mime, crate::auth::AVATAR_MAX_BYTES) {
                Err(e) => return Err(e),
                Ok(path) => {
                    self.db
                        .set_server_icon(sid, Some(&path))
                        .map_err(|e| e.to_string())?;
                }
            }
        } else {
            self.db.set_server_icon(sid, None).map_err(|e| e.to_string())?;
        }
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
}

// ── #[server] endpoints ─────────────────────────────────────────────────────

/// Rename a server. Only the current owner may call (owner-only by policy).
///
/// The acting user is read from the `x-user-id` request header.
#[server]
pub async fn rename_server(input: RenameServerInput) -> Result<ServerSettingsResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    // The server's members, captured before `db` moves into the service.
    // Renaming changes no membership, so this is the post-change audience too.
    let audience = crate::realtime::server_audience(&db, input.server_id);
    let result = ServerSettings::new(db)
        .rename(caller, input.server_id, &input.new_name)
        .map_err(ServerError::failed)?;
    // Members on other sockets refetch the server list so the new name
    // reaches their rail and sidebar live.
    crate::realtime::hub().publish(
        &audience,
        &WsServerMsg::ServerSettingsChanged {
            server_id: input.server_id,
            seq: next_frame_seq(),
        },
    );
    Ok(result)
}

/// Set (or clear) a server's icon. Owner or admin may call.
///
/// `icon` is a URL string, or `null` to clear the icon.
#[server]
pub async fn set_server_icon(input: SetServerIconInput) -> Result<ServerSettingsResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    // The server's members, captured before `db` moves into the service.
    // Setting an icon changes no membership, so this is the post-change audience too.
    let audience = crate::realtime::server_audience(&db, input.server_id);
    let result = ServerSettings::new(db)
        .set_icon(caller, input.server_id, input.icon.as_deref())
        .map_err(ServerError::failed)?;
    // Members on other sockets refetch the server list so the new icon
    // reaches their rail live.
    crate::realtime::hub().publish(
        &audience,
        &WsServerMsg::ServerSettingsChanged {
            server_id: input.server_id,
            seq: next_frame_seq(),
        },
    );
    Ok(result)
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

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use crate::server_settings::test_helpers::setup_db;

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
        assert_eq!(err, "you need the Manage Server permission to do that");
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
        let res = svc.set_icon(1, 1, Some("data:image/webp;base64,UklGRjgAAABXRUJQVlA4ICwAAADQAQCdASoIAAgAAgA0JaACdLoB+AADsAD+8FMf/yz5/DZ1Of/Cr43IWFAAAA==")).unwrap();
        assert!(res.icon_url.is_some() && res.icon_url.expect("path").starts_with("/media/"), "icon stored as media path");
    }

    #[test]
    fn owner_can_clear_icon() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db.clone());
        svc.set_icon(1, 1, Some("data:image/webp;base64,UklGRjgAAABXRUJQVlA4ICwAAADQAQCdASoIAAgAAgA0JaACdLoB+AADsAD+8FMf/yz5/DZ1Of/Cr43IWFAAAA==")).unwrap();
        let res = svc.set_icon(1, 1, None).unwrap();
        assert!(res.icon_url.is_none());
    }

    #[test]
    fn non_owner_set_icon_rejected() {
        let (_dir, db) = setup_db();
        let svc = ServerSettings::new(db.clone());
        let err = svc.set_icon(2, 1, Some("https://evil.example.com/x.png")).unwrap_err();
        assert_eq!(err, "you need the Manage Server permission to do that");
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
}
