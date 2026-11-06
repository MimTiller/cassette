//! Emoji reactions on channel messages: persist and fan out the post-state
//! count.

use crate::db::Db;
use crate::permissions::Permissions;
use crate::server_settings::{WsServerMsg, next_frame_seq};
use crate::realtime::{hub, authorize_text_channel, channel_audience, now_millis};

/// A reaction is one emoji (at most 16 chars), or a custom emoji's
/// `<:name:id>` token.
fn reaction_ok(emoji: &str) -> bool {
    let custom = emoji.starts_with("<:") && emoji.ends_with('>') && emoji.len() <= 64;
    !emoji.trim().is_empty() && (emoji.chars().count() <= 16 || custom)
}

/// Persist that `user_id` reacted with `emoji` to `message_id`, then fan out a
/// `ReactionAdd` frame carrying the new per-emoji count to everyone who can
/// see the channel (the reactor's own echo included). The count is read back after the
/// write, never incremented, so a duplicate add converges to the truth.
/// Returns the resolved `(channel_id, count)` for the endpoint's result.
///
/// Authorization: the message must exist in a text channel where the caller
/// holds Add Reactions.
pub fn add_channel_reaction(
    db: &Db,
    user_id: i64,
    message_id: i64,
    emoji: &str,
) -> Result<(i64, u32), String> {
    if !reaction_ok(emoji) {
        return Err("invalid reaction emoji".into());
    }
    let (_author_id, channel_id) = match db.message_row(message_id) {
        Some(pair) => pair,
        None => return Err("message not found".into()),
    };
    authorize_text_channel(db, user_id, channel_id, Permissions::ADD_REACTIONS)?;
    db.add_reaction(message_id, user_id, emoji, now_millis())
        .map_err(|e| format!("failed to store reaction: {e}"))?;
    let count = reaction_count_for(db, message_id, emoji);
    hub().publish(
        &channel_audience(db, channel_id),
        &WsServerMsg::ReactionAdd {
            message_id,
            channel_id,
            user_id,
            emoji: emoji.to_string(),
            count,
            seq: next_frame_seq(),
        },
    );
    Ok((channel_id, count))
}

/// Drop `user_id`'s `emoji` reaction from `message_id` and fan out a
/// `ReactionRemove` frame with the post-state count. Taking back one's own
/// reaction only needs to see the channel; idempotent (removing an absent reaction is a
/// no-op that still broadcasts the unchanged count, so clients converge).
pub fn remove_channel_reaction(
    db: &Db,
    user_id: i64,
    message_id: i64,
    emoji: &str,
) -> Result<(i64, u32), String> {
    if !reaction_ok(emoji) {
        return Err("invalid reaction emoji".into());
    }
    let (_author_id, channel_id) = match db.message_row(message_id) {
        Some(pair) => pair,
        None => return Err("message not found".into()),
    };
    authorize_text_channel(db, user_id, channel_id, Permissions::VIEW_CHANNEL)?;
    let removed = db
        .remove_reaction(message_id, user_id, emoji)
        .map_err(|e| format!("failed to remove reaction: {e}"))?;
    // Whether a row was actually dropped does not gate the broadcast: a no-op
    // removal still reports the post-state count so clients converge.
    let _ = removed;
    let count = reaction_count_for(db, message_id, emoji);
    hub().publish(
        &channel_audience(db, channel_id),
        &WsServerMsg::ReactionRemove {
            message_id,
            channel_id,
            user_id,
            emoji: emoji.to_string(),
            count,
            seq: next_frame_seq(),
        },
    );
    Ok((channel_id, count))
}

/// How many users reacted with `emoji` on `message_id` — the authoritative
/// post-operation total for one emoji (the grouped store in
/// [`crate::db::Db::reactions_for_message`] returns per-emoji counts).
fn reaction_count_for(db: &Db, message_id: i64, emoji: &str) -> u32 {
    db.reactions_for_message(message_id, 0)
        .iter()
        .find(|(e, _, _)| e == emoji)
        .map(|(_, c, _)| *c)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::realtime::test_fixtures::{setup};
    use tokio::sync::mpsc;

    #[test]
    fn reaction_persists_and_bounces_to_the_server_audience() {
        // Regression for the non-persistent reaction gap: a member's react must
        // write a `message_reactions` row AND broadcast the post-state count to
        // every socket of the channel's server (the reactor's echo included).
        let _guard = crate::e2e_lock::guard();
        let (_d, db) = setup();
        let hub = crate::realtime::hub();
        hub.reset();
        let (_id, mut rx) = hub.register(1); // alice — the reactor
        let (_id2, mut rx2) = hub.register(2); // bob — co-member (viewer)

        let mid = db.insert_message(1, 1, "reactable", 100, None).unwrap();

        // A non-member of the channel's server cannot react.
        assert_eq!(
            add_channel_reaction(&db, 4, mid, "🎉").unwrap_err(),
            "not a member of this server"
        );
        // No row was written for the refused attempt.
        assert!(db.reactions_for_message(mid, 1).is_empty(), "refused add wrote nothing");

        // Alice reacts with 👍: resolved channel + post-state count 1,
        // read back from the DB (never a per-client increment).
        let (channel_id, count) = add_channel_reaction(&db, 1, mid, "👍").unwrap();
        assert_eq!((channel_id, count), (1, 1u32), "resolved channel + post-state count");

        // PERSISTENCE: the grouped store now shows one 👍 reacted by alice (1).
        // This is the row a cold load reads back via `reactions_for_message`,
        // so a reload restores the pill without re-clicking.
        let pills = db.reactions_for_message(mid, 1);
        assert_eq!(pills, vec![("👍".to_string(), 1u32, true)], "the reaction row persisted");

        // Bob reacting with the SAME emoji bumps the authoritative total to 2.
        let (_cid, count2) = add_channel_reaction(&db, 2, mid, "👍").unwrap();
        assert_eq!(count2, 2u32, "the count is the DB total after the write");

        // DROPPING alice's 👍 leaves bob's, so the total returns to 1.
        let (_cid, count3) = remove_channel_reaction(&db, 1, mid, "👍").unwrap();
        assert_eq!(count3, 1u32, "post-state total after removing alice's reaction");
        // A second remove of alice's now-absent reaction is a no-op at count 1.
        let (_cid, count4) = remove_channel_reaction(&db, 1, mid, "👍").unwrap();
        assert_eq!(count4, 1u32, "idempotent: removing an absent reaction keeps 1");
        // PERSISTENCE: alice's 👍 row is gone, bob's remains.
        let pills2 = db.reactions_for_message(mid, 1);
        assert_eq!(pills2, vec![("👍".to_string(), 1u32, false)], "bob still reacts, alice does not");

        // The FIRST reaction (alice's 👍, count 1) bounced to BOTH sockets:
        // alice's own echo plus bob's copy, each with the post-state count 1.
        // Other modules' tests publish to the shared hub without the e2e lock
        // (friends, profiles, ...), so skip to the reaction frame itself.
        let first_reaction = |rx: &mut mpsc::Receiver<WsServerMsg>| loop {
            match rx.try_recv() {
                Ok(m @ WsServerMsg::ReactionAdd { .. }) => break Some(m),
                Ok(_) => continue,
                Err(_) => break None,
            }
        };
        match first_reaction(&mut rx).expect("reactor echo") {
            WsServerMsg::ReactionAdd { message_id, channel_id, user_id, emoji, count, .. } => {
                assert_eq!((message_id, channel_id, user_id, count), (mid, 1, 1, 1u32));
                assert_eq!(emoji, "👍", "the echo names the emoji");
            }
            other => panic!("expected ReactionAdd, got {other:?}"),
        }
        assert!(
            matches!(
                first_reaction(&mut rx2).expect("co-member sees the reaction live"),
                WsServerMsg::ReactionAdd { message_id, emoji, count, .. }
                    if message_id == mid && emoji == "👍" && count == 1u32
            ),
            "the reaction fans out to the whole server, not just the reactor"
        );
    }
}
