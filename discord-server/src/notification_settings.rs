//! Per-server and per-channel notification levels and mutes, picked by each
//! user. A row's `level` of `None` inherits (a channel from its server, a
//! server from the user's default in User Settings); `muted_until` is `None`
//! (not muted), [`MUTED_UNTIL_TURNED_OFF`], or an epoch-ms deadline.

use serde::{Deserialize, Serialize};
use server::{server, ServerError};

#[cfg(feature = "server")]
use crate::db::Db;
#[cfg(feature = "server")]
use crate::permissions::{self, Permissions};

/// `muted_until` for a mute with no end.
pub const MUTED_UNTIL_TURNED_OFF: i64 = -1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Scope {
    Server,
    Channel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Level {
    All,
    Mentions,
    Nothing,
}

impl Scope {
    pub fn db_str(self) -> &'static str {
        match self {
            Scope::Server => "server",
            Scope::Channel => "channel",
        }
    }
}

impl Level {
    pub fn db_str(self) -> &'static str {
        match self {
            Level::All => "all",
            Level::Mentions => "mentions",
            Level::Nothing => "nothing",
        }
    }

    pub fn from_db(s: &str) -> Option<Self> {
        [Level::All, Level::Mentions, Level::Nothing].into_iter().find(|l| l.db_str() == s)
    }
}

/// One server's or channel's setting for the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationSetting {
    pub scope: Scope,
    pub scope_id: i64,
    pub level: Option<Level>,
    pub muted_until: Option<i64>,
}

/// Whether a mute is in force at `now_ms`.
pub fn is_muted(muted_until: Option<i64>, now_ms: i64) -> bool {
    match muted_until {
        Some(MUTED_UNTIL_TURNED_OFF) => true,
        Some(until) => until > now_ms,
        None => false,
    }
}

/// `server_id` = one server's settings (its own and its channels'); `None` =
/// all of the caller's.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationSettingsInput {
    #[serde(default)]
    pub server_id: Option<i64>,
}

/// Set (or, with no level and no mute, clear) one setting.
pub type SetNotificationInput = NotificationSetting;

#[cfg(feature = "server")]
pub fn list(db: &Db, caller: i64, server_id: Option<i64>) -> Result<Vec<NotificationSetting>, String> {
    if let Some(sid) = server_id {
        permissions::member_in(db, sid, caller)?;
    }
    let c = db.lock().map_err(|e| e.to_string())?;
    let mut stmt = c
        .prepare(
            "SELECT scope, scope_id, level, muted_until FROM notification_settings
             WHERE user_id = ?1 AND (?2 IS NULL
                OR (scope = 'server' AND scope_id = ?2)
                OR (scope = 'channel' AND scope_id IN (SELECT id FROM channels WHERE server_id = ?2)))
             ORDER BY scope, scope_id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![caller, server_id], |r| {
            let scope: String = r.get(0)?;
            let level: Option<String> = r.get(2)?;
            Ok(NotificationSetting {
                scope: if scope == "server" { Scope::Server } else { Scope::Channel },
                scope_id: r.get(1)?,
                level: level.as_deref().and_then(Level::from_db),
                muted_until: r.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.flatten().collect())
}

#[cfg(feature = "server")]
pub fn set(db: &Db, caller: i64, s: NotificationSetting) -> Result<NotificationSetting, String> {
    match s.scope {
        Scope::Server => {
            permissions::member_in(db, s.scope_id, caller)?;
        }
        Scope::Channel => {
            let visible = permissions::for_channel(db, caller, s.scope_id)
                .is_some_and(|(_, p)| p.contains(Permissions::VIEW_CHANNEL));
            if !visible {
                return Err("channel not found".into());
            }
        }
    }
    if matches!(s.muted_until, Some(t) if t != MUTED_UNTIL_TURNED_OFF && t <= 0) {
        return Err("invalid mute end".into());
    }
    let c = db.lock().map_err(|e| e.to_string())?;
    if s.level.is_none() && s.muted_until.is_none() {
        c.execute(
            "DELETE FROM notification_settings WHERE user_id = ?1 AND scope = ?2 AND scope_id = ?3",
            rusqlite::params![caller, s.scope.db_str(), s.scope_id],
        )
    } else {
        c.execute(
            "INSERT OR REPLACE INTO notification_settings (user_id, scope, scope_id, level, muted_until)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![caller, s.scope.db_str(), s.scope_id, s.level.map(Level::db_str), s.muted_until],
        )
    }
    .map_err(|e| format!("failed to save the notification setting: {e}"))?;
    Ok(s)
}

#[cfg(feature = "server")]
fn db_from_state() -> Result<Db, ServerError> {
    server::use_state::<Db>().ok_or_else(|| ServerError::failed("database not installed"))
}

#[cfg(feature = "server")]
fn caller() -> Result<i64, ServerError> {
    crate::session::acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))
}

/// The caller's notification settings (one server's, or all).
#[server]
pub async fn notification_settings(input: NotificationSettingsInput) -> Result<Vec<NotificationSetting>, ServerError> {
    list(&db_from_state()?, caller()?, input.server_id).map_err(ServerError::failed)
}

/// Change one of the caller's notification settings. Needs membership of
/// the server, or to be able to see the channel.
#[server]
pub async fn set_notification_setting(input: SetNotificationInput) -> Result<NotificationSetting, ServerError> {
    set(&db_from_state()?, caller()?, input).map_err(ServerError::failed)
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use std::path::Path;

    const MIGRATIONS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/migrations");

    /// alice (1) owns server 1 with #general (10) and #staff (11, hidden from
    /// @everyone); bob (2) is a member; carol (3) is not.
    fn setup() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::migrate(&dir.path().join("t.db"), Path::new(MIGRATIONS)).unwrap();
        db.lock()
            .unwrap()
            .execute_batch(
                "INSERT INTO users (id, username, email, created_at) VALUES (1,'alice','a@x',1),(2,'bob','b@x',1),(3,'carol','c@x',1);
                 INSERT INTO servers (id, name, owner_id, created_at) VALUES (1,'one',1,1);
                 INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES (1,1,'owner',1),(1,2,'member',1);
                 INSERT INTO channels (id, server_id, name, kind, position, created_at) VALUES (10,1,'general','text',0,1),(11,1,'staff','text',1,1);
                 INSERT INTO channel_overwrites (channel_id, target_kind, target_id, allow, deny) VALUES (11,'everyone',0,0,1);",
            )
            .unwrap();
        (dir, db)
    }

    fn row(scope: Scope, scope_id: i64, level: Option<Level>, muted_until: Option<i64>) -> NotificationSetting {
        NotificationSetting { scope, scope_id, level, muted_until }
    }

    #[test]
    fn set_list_and_clear() {
        let (_dir, db) = setup();
        set(&db, 2, row(Scope::Server, 1, Some(Level::Mentions), None)).unwrap();
        set(&db, 2, row(Scope::Channel, 10, None, Some(MUTED_UNTIL_TURNED_OFF))).unwrap();
        assert_eq!(list(&db, 2, Some(1)).unwrap().len(), 2);
        assert_eq!(list(&db, 2, None).unwrap().len(), 2);
        assert!(list(&db, 1, Some(1)).unwrap().is_empty(), "settings are per user");
        // Replacing, then clearing.
        set(&db, 2, row(Scope::Server, 1, Some(Level::Nothing), None)).unwrap();
        let rows = list(&db, 2, Some(1)).unwrap();
        assert!(rows.contains(&row(Scope::Server, 1, Some(Level::Nothing), None)));
        set(&db, 2, row(Scope::Channel, 10, None, None)).unwrap();
        assert_eq!(list(&db, 2, Some(1)).unwrap(), vec![row(Scope::Server, 1, Some(Level::Nothing), None)]);
    }

    #[test]
    fn access_is_checked() {
        let (_dir, db) = setup();
        assert!(set(&db, 3, row(Scope::Server, 1, Some(Level::All), None)).is_err(), "not a member");
        assert!(list(&db, 3, Some(1)).is_err());
        assert!(set(&db, 2, row(Scope::Channel, 11, Some(Level::All), None)).is_err(), "can't see #staff");
        assert!(set(&db, 1, row(Scope::Channel, 11, Some(Level::All), None)).is_ok(), "the owner can");
        assert!(set(&db, 2, row(Scope::Channel, 10, None, Some(0))).is_err(), "bad mute end");
    }

    #[test]
    fn mutes_expire() {
        assert!(is_muted(Some(MUTED_UNTIL_TURNED_OFF), 5));
        assert!(is_muted(Some(10), 5));
        assert!(!is_muted(Some(10), 10));
        assert!(!is_muted(None, 5));
    }
}
