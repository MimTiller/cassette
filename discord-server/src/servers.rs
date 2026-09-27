//! Server + channel discovery: create a server, list the caller's
//! servers, join a server, list a server's channels.
//!
//! Same shape as auth.rs: this file is NOT feature-gated so the
//! `#[server]` fns compile to client RPC stubs by default; only the
//! DB-touching pieces are gated `#[cfg(feature = "server")]`.

use serde::{Deserialize, Serialize};
use server::{server, ServerError};

#[cfg(feature = "server")]
use crate::db::Db;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateServerInput {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerSummary {
    pub id: i64,
    pub name: String,
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JoinServerInput {
    pub server_id: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListChannelsInput {
    pub server_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelSummary {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub position: i64,
    pub parent_channel_id: Option<i64>,
}

#[cfg(feature = "server")]
use crate::session::acting_user;

#[cfg(feature = "server")]
fn db_from_state() -> Result<Db, ServerError> {
    server::use_state::<Db>().ok_or_else(|| ServerError::failed("database not installed"))
}

/// Create a new server, owned by the caller (who becomes its first
/// member with role='owner').
#[server]
pub async fn create_server(input: CreateServerInput) -> Result<ServerSummary, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    if input.name.trim().is_empty() {
        return Err(ServerError::failed("server name must not be empty"));
    }
    let id = db
        .insert_server(caller, &input.name)
        .map_err(|e| ServerError::failed(e.to_string()))?;
    Ok(ServerSummary {
        id,
        name: input.name,
        icon_url: None,
    })
}

/// List the servers the caller is a member of.
#[server]
pub async fn list_servers(_input: ()) -> Result<Vec<ServerSummary>, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    Ok(db
        .servers_for_user(caller)
        .into_iter()
        .map(|(id, name, icon_url)| ServerSummary { id, name, icon_url })
        .collect())
}

/// Join an existing server as a plain member.
#[server]
pub async fn join_server(input: JoinServerInput) -> Result<(), ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    db.insert_membership(input.server_id, caller)
        .map_err(|e| ServerError::failed(e.to_string()))?;
    Ok(())
}

/// List a server's channels, in position order.
#[server]
pub async fn list_channels(
    input: ListChannelsInput,
) -> Result<Vec<ChannelSummary>, ServerError> {
    let db = db_from_state()?;
    Ok(db
        .channels_for_server(input.server_id)
        .into_iter()
        .map(|(id, name, kind, position, parent_channel_id)| ChannelSummary {
            id,
            name,
            kind,
            position,
            parent_channel_id,
        })
        .collect())
}

#[cfg(test)]
#[cfg(feature = "server")]
mod tests {
    use super::*;
    use std::path::Path;

    /// Build a temp-file database with all migrations applied, and seed
    /// two users (ids 1 and 2) so server/member FKs can reference them.
    fn setup_db() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let db = Db::migrate(&db_path, &migrations_dir).unwrap();

        let c = db.lock().unwrap();
        c.execute_batch(
            "INSERT INTO users (id, username, email, created_at) VALUES
                (1, 'owner',  'owner@example.com',  1000),
                (2, 'joiner', 'joiner@example.com', 1000);",
        )
        .unwrap();
        drop(c);
        (dir, db)
    }

    #[test]
    fn create_then_list_shows_owner() {
        let (_dir, db) = setup_db();
        let id = db.insert_server(1, "Test Server").unwrap();
        let servers = db.servers_for_user(1);
        assert!(
            servers
                .iter()
                .any(|(sid, name, _)| *sid == id && name == "Test Server")
        );
    }

    #[test]
    fn join_then_list_shows_for_joiner() {
        let (_dir, db) = setup_db();
        let id = db.insert_server(1, "Test Server").unwrap();
        db.insert_membership(id, 2).unwrap();
        let servers = db.servers_for_user(2);
        assert!(servers.iter().any(|(sid, _, _)| *sid == id));
    }

    #[test]
    fn join_twice_fails() {
        let (_dir, db) = setup_db();
        let id = db.insert_server(1, "Test Server").unwrap();
        db.insert_membership(id, 2).unwrap();
        assert!(db.insert_membership(id, 2).is_err());
    }

    #[test]
    fn list_channels_in_position_order() {
        let (_dir, db) = setup_db();
        let id = db.insert_server(1, "Test Server").unwrap();
        let chans = db.channels_for_server(id);
        let positions: Vec<i64> = chans.iter().map(|(_, _, _, p, _)| *p).collect();
        let mut sorted = positions.clone();
        sorted.sort();
        assert_eq!(positions, sorted);
    }
}
