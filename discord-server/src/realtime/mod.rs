//! Realtime fan-out and gateway authorization (server-only).
//!
//! A process-wide [`Hub`] tracks every open gateway socket per user. Anything
//! that produces an event (a channel message, a DM, a presence change) works
//! out WHO should hear it from the database at delivery time -- the members of
//! the server, the participants of the DM -- and hands the list to the hub.
//! Because the audience is recomputed per event there is no per-socket
//! subscription state to keep in sync: join a server and you start hearing it,
//! leave and you stop.
//!
//! Authorization for the gateway also lives here: a socket may only read or
//! post in a text channel of a server the (token-authenticated) user belongs
//! to; posts are size-limited and rate-limited per user.

// The module is split by feature; every public item is re-exported here so
// `crate::realtime::X` paths keep working.

mod hub;
mod audience;
mod messages;
mod reactions;
#[cfg(test)]
mod gateway;

pub use hub::{ConnId, Hub, hub};
pub use audience::{server_audience, channel_audience, presence_audience, announce_presence, shown_presence, refresh_presence, announce_membership};
pub use messages::{MAX_MESSAGE_CHARS, MAX_ATTACHMENTS_PER_MESSAGE, authorize_text_channel, resolve_mentions, post_channel_message, edit_channel_message, delete_channel_message, history_for};
pub use reactions::{add_channel_reaction, remove_channel_reaction};

/// The current wall-clock milliseconds since the Unix epoch.
pub(crate) fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod test_fixtures {
    use std::path::Path;
    use crate::db::Db;

    /// Users 1..=4. Server 1: owner 1 + member 2, text channel 1, voice
    /// channel 2. Server 2: owner 3, text channel 3. User 4 is in no server.
    pub fn setup() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let migrations = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let db = Db::migrate(&dir.path().join("t.db"), &migrations).unwrap();
        db.lock()
            .unwrap()
            .execute_batch(
                "INSERT INTO users (id, username, email, created_at) VALUES
                    (1,'alice','a@x',1),(2,'bob','b@x',1),(3,'carol','c@x',1),(4,'dave','d@x',1);
                 INSERT INTO servers (id, name, owner_id, icon_url, created_at) VALUES
                    (1,'S1',1,NULL,1),(2,'S2',3,NULL,1);
                 INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES
                    (1,1,'owner',1),(1,2,'member',1),(2,3,'owner',1);
                 INSERT INTO channels (id, server_id, name, kind, position, parent_channel_id, created_at) VALUES
                    (1,1,'general','text',0,NULL,1),
                    (2,1,'lounge','voice',1,NULL,1),
                    (3,2,'other','text',0,NULL,1);",
            )
            .unwrap();
        (dir, db)
    }

    /// Channel 4 in server 1 is private: @everyone can't see it, bob can't.
    pub fn with_private_channel(db: &Db) {
        db.lock()
            .unwrap()
            .execute_batch(
                "INSERT INTO channels (id, server_id, name, kind, position, created_at) VALUES (4,1,'secret','text',2,1);
                 INSERT INTO channel_overwrites (channel_id, target_kind, target_id, allow, deny)
                    VALUES (4,'everyone',0,0,1);",
            )
            .unwrap();
    }
}
