//! Message pins: pin, unpin, and the pinned view / count.

use serde::Deserialize;
use server::{server, ServerError};

#[cfg(feature = "server")]
use crate::permissions::Permissions;

/// The acting user for the current request, resolved from a verified session
/// token (see [`crate::session`]); `None` when unauthenticated.
#[cfg(feature = "server")]
use crate::session::acting_user;
#[cfg(feature = "server")]
use crate::server_settings::{ServerSettings, db_from_state};

#[cfg(feature = "server")]
impl ServerSettings {
    // ── message pins ───────────────────────────────────────────────────────
    //
    // Pinning a message in a channel. Pinning/unpinning needs Manage Messages
    // in the channel; reading the pinned view needs only to see it. The
    // channel is resolved from the message id, so a caller cannot pin a
    // message in a channel they can't manage by passing a foreign id.

    /// Pin `message_id` in its channel. Needs Manage Messages there.
    /// Idempotent: a second pin is a no-op.
    ///
    /// # Errors
    /// - `message not found` — the id is not a stored guild message.
    /// - `channel not found` — the message's channel is gone.
    /// - `you need the Manage Messages permission to do that`, or the caller
    ///   is not a member of the owning server.
    pub fn pin_message(&self, caller_id: i64, message_id: i64) -> Result<PinResult, String> {
        let (_author_id, channel_id) = self
            .db
            .message_row(message_id)
            .ok_or("message not found")?;
        self.require_in_channel(caller_id, channel_id, Permissions::MANAGE_MESSAGES)?;
        self.db
            .pin_message(message_id, caller_id, Self::now_millis())
            .map_err(|e| format!("failed to pin message: {e}"))?;
        Ok(PinResult {
            message_id,
            channel_id,
            pinned: true,
            pinned_count: self.db.pinned_count(channel_id),
        })
    }

    /// Unpin `message_id` from its channel. Same permission as
    /// [`Self::pin_message`]. Idempotent: unpinning an unpinned message is a
    /// no-op (`pinned: false`) rather than an error.
    ///
    /// # Errors
    /// - `message not found` — the id is not a stored guild message.
    /// - `channel not found` — the message's channel is gone.
    /// - `you need the Manage Messages permission to do that`, or the caller
    ///   is not a member of the owning server.
    pub fn unpin_message(&self, caller_id: i64, message_id: i64) -> Result<PinResult, String> {
        let (_author_id, channel_id) = self
            .db
            .message_row(message_id)
            .ok_or("message not found")?;
        self.require_in_channel(caller_id, channel_id, Permissions::MANAGE_MESSAGES)?;
        let removed = self
            .db
            .unpin_message(message_id)
            .map_err(|e| format!("failed to unpin message: {e}"))?;
        // After an unpin the message is *not* pinned — whether a row was just
        // removed or it was already unpinned (a no-op), the post-state is the
        // same. `pinned: false` unconditionally; `removed` only distinguishes
        // "we did the work" from "already gone" but that is not surfaced.
        let _ = removed;
        Ok(PinResult {
            message_id,
            channel_id,
            pinned: false,
            pinned_count: self.db.pinned_count(channel_id),
        })
    }

    /// Every pinned message in `channel_id`, in the order they were pinned
    /// (oldest pin first). Any member of the channel's server may read the
    /// list — pinning is moderator-only, but reading is not.
    ///
    /// # Errors
    /// - `channel not found` — the id doesn't exist.
    /// - `you're not a member of this server` — no membership.
    pub fn list_pinned(&self, caller_id: i64, channel_id: i64) -> Result<Vec<PinnedMessage>, String> {
        self.require_in_channel(caller_id, channel_id, Permissions::VIEW_CHANNEL)?;
        Ok(self
            .db
            .pinned_messages(channel_id)
            .into_iter()
            .map(|(m, pinned_by)| PinnedMessage {
                message_id: m.id,
                channel_id: m.channel_id,
                author_id: m.author_id,
                author_username: self
                    .db
                    .user_by_id(m.author_id)
                    .map(|(_, n)| n)
                    .unwrap_or_default(),
                content: m.content,
                created_at: m.created_at,
                edited_at: m.edited_at,
                mentioned_user_ids: m.mentioned_user_ids,
                pinned_by,
            })
            .collect())
    }

    /// The number of pinned messages in `channel_id`. Any member of the
    /// channel's server may read it (this backs the header pin-count badge).
    ///
    /// # Errors
    /// - `channel not found` — the id doesn't exist.
    /// - `you're not a member of this server` — no membership.
    pub fn pinned_count(&self, caller_id: i64, channel_id: i64) -> Result<i64, String> {
        self.require_in_channel(caller_id, channel_id, Permissions::VIEW_CHANNEL)?;
        Ok(self.db.pinned_count(channel_id))
    }
}

// ── message pins ─────────────────────────────────────────────────────────────
//
// Pin/unpin a message in a channel (Manage Messages), and read a channel's
// pinned view + count (anyone who can see it). The acting user comes from the verified session
// token; the channel is resolved from the message on the pin/unpin path.

/// Pin a message in its channel. Needs Manage Messages. Idempotent.
#[server]
pub async fn pin_message(input: PinInput) -> Result<PinResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .pin_message(caller, input.message_id)
        .map_err(ServerError::failed)
}

/// Unpin a message from its channel. Same permission as `pin_message`.
/// Idempotent — unpinning an unpinned message is a no-op.
#[server]
pub async fn unpin_message(input: PinInput) -> Result<PinResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .unpin_message(caller, input.message_id)
        .map_err(ServerError::failed)
}

/// Every pinned message in a channel, in pin order (oldest first). Any member
/// of the owning server may read the view.
#[server]
pub async fn list_pinned(input: PinnedChannelInput) -> Result<Vec<PinnedMessage>, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .list_pinned(caller, input.channel_id)
        .map_err(ServerError::failed)
}

/// The number of pinned messages in a channel (backs the header pin badge).
/// Any member of the owning server may read it.
#[server]
pub async fn pinned_count(input: PinnedChannelInput) -> Result<i64, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    ServerSettings::new(db)
        .pinned_count(caller, input.channel_id)
        .map_err(ServerError::failed)
}

// ── message pins ─────────────────────────────────────────────────────────────

/// A single message to pin or unpin. The channel is resolved from the message
/// (a message belongs to exactly one channel), so the caller only supplies the
/// message id — there is no separate `channel_id` to keep in sync.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct PinInput {
    pub message_id: i64,
}

/// The channel whose pinned messages (or pin count) the caller wants.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct PinnedChannelInput {
    pub channel_id: i64,
}

/// The outcome of a pin or unpin. `pinned` is `true` when the message is now
/// pinned (the pin succeeded, or it was already pinned) and `false` when it is
/// now *not* pinned (an unpin). `pinned_count` is the channel's total pinned
/// messages after the operation — the client updates its header badge from this
/// without a second round trip.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct PinResult {
    pub message_id: i64,
    pub channel_id: i64,
    pub pinned: bool,
    pub pinned_count: i64,
}

/// One entry in a channel's pinned view: the full message plus who pinned it.
/// `mentioned_user_ids` is populated (resolved `@mention` ids), exactly as in
/// [`crate::realtime`]'s history path, so a pinned view can render mentions.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct PinnedMessage {
    pub message_id: i64,
    pub channel_id: i64,
    pub author_id: i64,
    pub author_username: String,
    pub content: String,
    pub created_at: i64,
    pub edited_at: Option<i64>,
    #[serde(default)]
    pub mentioned_user_ids: Vec<i64>,
    /// The user id of whoever pinned this message (not necessarily its author).
    pub pinned_by: i64,
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use crate::server_settings::test_helpers::{setup_db, setup_db_with_channels, post_messages};
    use crate::db::Db;

    // ── message pins ───────────────────────────────────────────────────────
    //
    // Pinning a message needs Manage Messages;
    // reading the pinned view / count is any member. The pin state lives in
    // `message_pins` and the channel is resolved from the message, so these
    // tests exercise: the happy path (owner pins), the authz matrix (member
    // can read but not pin; moderator can pin), idempotence (re-pin / re-unpin
    // are no-ops), pin-order preservation, the count, and the "unknown
    // message" rejection.

    /// Give user 3 (bob) the old moderator level's powers in server 1.
    fn make_user3_moderator(db: &Db) {
        let _ = crate::permissions::grant(db, 1, 3, crate::permissions::MODERATOR_LEVEL, 1);
    }

    #[test]
    fn owner_can_pin_a_message() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        let msg = post_messages(&db, 1, 1, 3); // channel 1: ids 1..=3
        let res = svc.pin_message(1, msg).unwrap();
        assert_eq!(res.message_id, msg);
        assert_eq!(res.channel_id, 1, "the channel is resolved from the message");
        assert!(res.pinned, "pin reports the message as pinned");
        assert_eq!(res.pinned_count, 1, "one message is now pinned in channel 1");
        assert!(db.message_is_pinned(msg), "the pin is persisted");
    }

    #[test]
    fn pin_is_idempotent() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        let msg = post_messages(&db, 1, 1, 2);
        svc.pin_message(1, msg).unwrap();
        // Pinning the same message again is a no-op, not an error, and the
        // count stays at 1 (no duplicate pin row).
        let res = svc.pin_message(1, msg).unwrap();
        assert!(res.pinned);
        assert_eq!(res.pinned_count, 1, "a second pin must not double-count");
        assert_eq!(svc.pinned_count(1, 1).unwrap(), 1);
    }

    #[test]
    fn plain_member_cannot_pin_but_can_read() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        let msg = post_messages(&db, 1, 1, 2);
        // alice (2) is a plain member: pinning is refused…
        let err = svc.pin_message(2, msg).unwrap_err();
        assert_eq!(err, "you need the Manage Messages permission to do that");
        assert!(!db.message_is_pinned(msg), "no pin row was written");
        // …but she may still read the (empty) pinned view and count.
        assert!(svc.list_pinned(2, 1).unwrap().is_empty());
        assert_eq!(svc.pinned_count(2, 1).unwrap(), 0);
    }

    #[test]
    fn moderator_can_pin_and_unpin() {
        let (_dir, db) = setup_db_with_channels();
        make_user3_moderator(&db);
        let svc = ServerSettings::new(db.clone());
        let a = post_messages(&db, 1, 1, 1); // id 1
        let b = post_messages(&db, 1, 1, 1); // id 2
        // bob (3, moderator) pins two messages in order a then b.
        svc.pin_message(3, a).unwrap();
        let res = svc.pin_message(3, b).unwrap();
        assert_eq!(res.pinned_count, 2);
        // The view is in pin order (oldest pin first): a, then b.
        let view = svc.list_pinned(3, 1).unwrap();
        let order: Vec<i64> = view.iter().map(|m| m.message_id).collect();
        assert_eq!(order, vec![a, b], "pinned view preserves pin order");
        // Each entry carries the author's resolved username and who pinned it.
        assert_eq!(view[0].author_username, "owner", "author username resolved");
        assert_eq!(view[0].pinned_by, 3, "pinned_by is the moderator, not the author");
        // Unpin the first: idempotent and reflected in the count + view.
        let res = svc.unpin_message(3, a).unwrap();
        assert!(!res.pinned, "after unpin the message is not pinned");
        assert_eq!(res.pinned_count, 1);
        assert!(!db.message_is_pinned(a));
        let view = svc.list_pinned(3, 1).unwrap();
        assert_eq!(view.iter().map(|m| m.message_id).collect::<Vec<_>>(), vec![b]);
        // Unpinning an already-unpinned message is a no-op, not an error.
        let res = svc.unpin_message(3, a).unwrap();
        assert!(!res.pinned);
        assert_eq!(res.pinned_count, 1);
    }

    #[test]
    fn plain_member_cannot_unpin() {
        let (_dir, db) = setup_db_with_channels();
        make_user3_moderator(&db);
        let svc = ServerSettings::new(db.clone());
        let msg = post_messages(&db, 1, 1, 1);
        svc.pin_message(3, msg).unwrap();
        assert!(db.message_is_pinned(msg));
        // alice (2, plain member) cannot unpin it.
        let err = svc.unpin_message(2, msg).unwrap_err();
        assert_eq!(err, "you need the Manage Messages permission to do that");
        assert!(db.message_is_pinned(msg), "the pin is untouched");
    }

    #[test]
    fn pinned_view_rejects_non_member_and_unknown_channel() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        // A non-member cannot read the view or count.
        assert!(svc.list_pinned(999, 1).unwrap_err().contains("not a member"));
        assert!(svc.pinned_count(999, 1).unwrap_err().contains("not a member"));
        // An unknown channel is rejected even for a member.
        assert!(svc.list_pinned(1, 999).unwrap_err().contains("channel not found"));
        assert!(svc.pinned_count(1, 999).unwrap_err().contains("channel not found"));
    }

    #[test]
    fn pin_unknown_message_rejected() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db);
        // Even the owner cannot pin a message that does not exist.
        let err = svc.pin_message(1, 999_999).unwrap_err();
        assert!(err.contains("message not found"), "got: {err}");
    }

    #[test]
    fn pinned_view_is_per_channel() {
        let (_dir, db) = setup_db_with_channels();
        let svc = ServerSettings::new(db.clone());
        let m1 = post_messages(&db, 1, 1, 1); // channel 1
        let _m2 = post_messages(&db, 2, 1, 1); // channel 2
        svc.pin_message(1, m1).unwrap();
        // Pinning in one channel does not leak into the other.
        assert_eq!(svc.pinned_count(1, 1).unwrap(), 1);
        assert_eq!(svc.pinned_count(1, 2).unwrap(), 0);
        let view2 = svc.list_pinned(1, 2).unwrap();
        assert!(view2.is_empty(), "channel 2 has no pins");
        // Unpinning the channel-1 pin zeroes that channel's count only.
        svc.unpin_message(1, m1).unwrap();
        assert_eq!(svc.pinned_count(1, 1).unwrap(), 0);
    }
}
