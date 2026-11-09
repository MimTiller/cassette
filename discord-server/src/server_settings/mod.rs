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

// The module is split by feature; every public item is re-exported here so
// `crate::server_settings::X` paths keep working.

#[cfg(feature = "server")]
use server::{ServerError};

#[cfg(feature = "server")]
use crate::db::Db;
#[cfg(feature = "server")]
use crate::permissions::{self, Member, Permissions, ServerContext};

mod gateway;
#[path = "server.rs"] mod settings;
mod channels;
mod members;
mod reads;
mod pins;
mod reactions;

pub use gateway::{AttachmentInput, MessageAttachment, StickerCopy, WsClientMsg, WsServerMsg, next_frame_seq, message_gateway};
pub use settings::{RenameServerInput, SetServerIconInput, TransferOwnershipInput, rename_server, set_server_icon, transfer_server_ownership};
pub use channels::{CreateChannelInput, RenameChannelInput, DeleteChannelInput, ChannelResult, create_channel, rename_channel, delete_channel};
pub use members::{AddMemberInput, LeaveServerInput, RemoveMemberInput, BanMemberInput, TimeoutMemberInput, ListMembersInput, MemberInfo, MemberResult, add_member, remove_member, leave_server, ban_member, timeout_member, list_members};
pub use reads::{MarkReadInput, UnreadCountsInput, MarkReadResult, UnreadCount, mark_read, unread_counts};
pub use pins::{PinInput, PinnedChannelInput, PinResult, PinnedMessage, pin_message, unpin_message, list_pinned, pinned_count};
pub use reactions::{ReactionInput, ReactionResult, add_reaction, remove_reaction};

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

/// The refusal for the old owner/admin/moderator/member levels, which roles
/// replaced.
#[cfg(feature = "server")]

/// The server-settings service. Wraps a `Db` handle.
///
/// Every method takes the acting user id explicitly so the authorization is
/// testable without any request context.
#[cfg(feature = "server")]
pub struct ServerSettings {
    db: Db,
}

#[cfg(feature = "server")]
impl ServerSettings {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    // ── shared authorization helpers ───────────────────────────────────────

    /// Check that `caller_id` is a member of `server_id` holding `perm`
    /// server-wide. Returns the loaded server and the caller.
    fn require(&self, caller_id: i64, server_id: i64, perm: Permissions) -> Result<(ServerContext, Member), String> {
        let (ctx, member) = permissions::member_in(&self.db, server_id, caller_id)?;
        if !member.server.contains(perm) {
            return Err(permissions::missing(perm));
        }
        Ok((ctx, member))
    }

    /// Check that `caller_id` holds `perm` in `channel_id`, and return the
    /// channel's server id. A channel the caller can't see is "not found".
    fn require_in_channel(&self, caller_id: i64, channel_id: i64, perm: Permissions) -> Result<i64, String> {
        let server_id = self.db.channel_full(channel_id).ok_or("channel not found")?.1;
        let (ctx, member) = permissions::member_in(&self.db, server_id, caller_id)?;
        let perms = ctx.in_channel(&member, channel_id);
        if !perms.contains(Permissions::VIEW_CHANNEL) {
            return Err("channel not found".into());
        }
        if !perms.contains(perm) {
            return Err(permissions::missing(perm));
        }
        Ok(server_id)
    }

    /// Check that `caller` outranks member `user_id`; `what` is the verb for
    /// the refusal ("kick", "ban", …).
    fn require_outranks(ctx: &ServerContext, caller: &Member, user_id: i64, what: &str) -> Result<(), String> {
        let target = ctx.member(user_id).ok_or("member not found")?;
        match permissions::refuse_outranked(caller.rank, target.rank, what) {
            Some(refusal) => Err(refusal),
            None => Ok(()),
        }
    }

    /// The current wall-clock seconds since the Unix epoch, for the
    /// `joined_at` / `created_at` timestamps the DB layer stores as `i64`.
    fn now_secs() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(1_000)
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

/// Pull the shared `Db` out of server state, installed at startup by the
/// server bin via `server::install_state(db)`. `Db` is cheap to clone (it's
/// just an `Arc<Mutex<Connection>>`), so each request gets its own handle.
#[cfg(feature = "server")]
fn db_from_state() -> Result<Db, ServerError> {
    server::use_state::<Db>()
        .ok_or_else(|| ServerError::failed("database not installed"))
}

// ── tests ────────────────────────────────────────────────────────────────────
//
// The tests exercise the owner-only policy directly against the `ServerSettings`
// service (no HTTP / no server state needed). A rejection case is included for
// each mutation.

#[cfg(all(test, feature = "server"))]
mod test_helpers {
    use super::*;

    use std::path::Path;
    /// A boxed future — used by the raw-HTTP e2e client below so the closure
    /// can name its return type without a `-> impl Future` annotation.
    pub type BoxFuture<T> = std::pin::Pin<
        Box<dyn std::future::Future<Output = T> + Send>,
    >;

    /// Build a temp-file database, run migrations, seed one server with an
    /// owner (user 1) and two members (users 2 and 3), and return the `Db`.
    pub fn setup_db() -> (tempfile::TempDir, Db) {
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
    pub fn setup_db_with_channels() -> (tempfile::TempDir, Db) {
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
    pub fn post_messages(db: &Db, channel_id: i64, author_id: i64, count: i64) -> i64 {
        let mut last = 0i64;
        for i in 0..count {
            last = db
                .insert_message(channel_id, author_id, &format!("msg {i}"), 1000 + i, None)
                .unwrap();
        }
        last
    }

    /// Look up one channel's unread count in an `unread_counts` result,
    /// panicking if the channel is absent (the contract is that every member
    /// channel appears, even with count 0).
    pub fn count_for(counts: &[UnreadCount], channel_id: i64) -> i64 {
        counts
            .iter()
            .find(|c| c.channel_id == channel_id)
            .unwrap_or_else(|| panic!("channel {channel_id} missing from unread list: {counts:?}"))
            .count
    }

    /// Acquire the e2e serialization guard; hold it for the whole test body.
    /// (Poisoned if a prior holder panicked — recover rather than re-panic.)
    /// Delegates to the crate-level [`crate::e2e_lock`] so the HTTP/WS e2e
    /// tests here share ONE lock with the DM e2e test in `dms`.
    pub fn e2e_guard() -> std::sync::MutexGuard<'static, ()> {
        crate::e2e_lock::guard()
    }
}
