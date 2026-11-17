//! What the caller may do, over the idea-chat reducer: the server-wide set
//! is `perms::effective` translated through the shared table, the per-channel
//! sets are `perms::at_channel`, and a channel's per-role overwrites ride the
//! channel record's `overwrites` map (role-keyed only — RWND's everyone/member
//! targets have no counterpart here, so they refuse with a sentence naming
//! which kind this server takes).

use crate::backend::*;
use crate::backend::idea_chat::{client, live, perms_map};
use chat_model::{Channel, Id};
use discord_server::permissions::Rank;

impl PermissionsApi for IdeaChatBackend {
    // No RPC: the answer is computed from the snapshot with the wire's own
    // permission rules, then translated back to RWND's flags. Only channels
    // the caller can read are listed, matching the server's filter.
    async fn my_permissions(server_id: i64) -> Result<MyPermissions, ServerError> {
        let world = live::snapshot();
        let space_id = live::wire_of(server_id);
        let self_id = match world.me.clone() {
            None => return Err(ServerError::failed("nobody is signed in")),
            Some(me) => me.user.id,
        };
        match world.spaces.get(&space_id) {
            None => Err(ServerError::failed("that space is not on this server")),
            Some(space) => match chat_model::perms::effective(space, self_id) {
                None => Err(ServerError::failed("you are not a member of this space")),
                Some(server_bits) => {
                    let mut channels: Vec<(i64, Permissions)> = Vec::new();
                    for (channel_id, channel) in world.channels.iter() {
                        match channel.space {
                            None => continue,
                            Some(space_of) => {
                                if space_of != space_id {
                                    continue
                                }
                            },
                        }
                        match chat_model::perms::at_channel(self_id, channel, space) {
                            None => continue,
                            Some(bits) => {
                                let translated = perms_map::to_rwnd(bits);
                                if translated.contains(Permissions::VIEW_CHANNEL) {
                                    channels.push((live::see(*channel_id), translated))
                                }
                            },
                        }
                    }
                    Ok(MyPermissions {
                        server_id: server_id,
                        server: perms_map::to_rwnd(server_bits),
                        channels: channels,
                        rank: rank_of(world.clone(), space_id, self_id),
                    })
                }
            },
        }
    }

    // The channel record carries its per-role overwrites; both sides of each
    // overwrite go back through the shared table.
    async fn channel_overwrites(channel_id: i64) -> Result<Vec<Overwrite>, ServerError> {
        let channel_wire = live::wire_of(channel_id);
        match live::snapshot().channels.get(&channel_wire) {
            None => Err(ServerError::failed("that channel is not on this server")),
            Some(channel) => Ok(overwrites_of(channel.clone())),
        }
    }

    // `set_overwrite(channel, role, allow, deny)` is role-targeted: RWND's
    // everyone and member targets have no counterpart, so they refuse. Allowing
    // and denying nothing is the wire's `clear_overwrite`. The channel's fresh
    // overwrites read back off the snapshot the `ChannelUpserted` event updates.
    async fn set_channel_overwrite(
        channel_id: i64,
        target: Target,
        allow: Permissions,
        deny: Permissions,
    ) -> Result<Vec<Overwrite>, ServerError> {
        let role_wire = match target {
            Target::Role(role_id) => live::wire_of(role_id),
            Target::Everyone => return Err(ServerError::failed("only per-role overwrites exist on this server")),
            Target::Member(_) => return Err(ServerError::failed("only per-role overwrites exist on this server")),
        };
        let allow_bits = perms_map::to_idea(allow);
        let deny_bits = perms_map::to_idea(deny);
        let channel_wire = live::wire_of(channel_id);
        let cleared = allow_bits == 0 && deny_bits == 0;
        let mut args = Vec::new();
        args.push(client::arg(channel_wire));
        args.push(client::arg(role_wire));
        if !cleared {
            args.push(client::arg::<u64>(allow_bits));
            args.push(client::arg::<u64>(deny_bits))
        }
        let rpc = match cleared { true => "clear_overwrite", false => "set_overwrite" };
        match client::post(rpc, args).await {
            Err(e) => Err(e),
            Ok(_) => Ok(overwrites_of(fresh_channel(channel_wire))),
        }
    }

    // `set_everyone(space, permissions)` answers unit; the translated set goes
    // back through the table so what reads out is exactly what was written.
    async fn set_everyone_permissions(server_id: i64, permissions: Permissions) -> Result<Permissions, ServerError> {
        let mut args = Vec::new();
        args.push(client::arg(live::wire_of(server_id)));
        args.push(client::arg::<u64>(perms_map::to_idea(permissions)));
        match client::post("set_everyone", args).await {
            Err(e) => Err(e),
            Ok(_) => Ok(permissions),
        }
    }
}

// --- helpers this area shares -------------------------------------------

/// RWND's rank the caller acts at in one space: the owner is `Owner`, a
/// holder's highest role (idea-chat positions count up, RWND's count down) is
/// `Role`, and the baseline seat is `Everyone`.
pub(crate) fn rank_of(world: chat_model::live::State, space_id: Id, user: Id) -> Rank {
    match world.spaces.get(&space_id) {
        None => Rank::Everyone,
        Some(space) => {
            if user == space.owner {
                return Rank::Owner
            }
            match chat_model::perms::top_position(space, user) {
                None => Rank::Everyone,
                Some(position) => Rank::Role(-(position as i64)),
            }
        }
    }
}

/// One channel's overwrites as RWND rows: idea-chat keys them by role, so
/// every target is `Target::Role`, translated back through the table.
pub(crate) fn overwrites_of(channel: Channel) -> Vec<Overwrite> {
    channel.overwrites.iter().map(move |(role, overwrite)| Overwrite {
        target: Target::Role(live::see(*role)),
        allow: perms_map::to_rwnd(overwrite.allow),
        deny: perms_map::to_rwnd(overwrite.deny),
    }).collect()
}

/// One channel the caller is about to see after a write: from the snapshot,
/// or an empty-overwrite stand-in if the record is not (yet) there.
fn fresh_channel(id: Id) -> Channel {
    match live::snapshot().channels.get(&id) {
        None => channel_placeholder(id),
        Some(channel) => channel.clone(),
    }
}

/// A channel the snapshot no longer has: empty overwrites, so the caller sees
/// the write land without the record.
fn channel_placeholder(id: Id) -> Channel {
    Channel {
        id: id,
        kind: chat_model::ChannelKind::Text,
        space: None,
        name: "".to_string(),
        topic: "".to_string(),
        category: None,
        position: 0,
        parent: None,
        starter: None,
        view: 0,
        post: 0,
        members: Vec::new(),
        archived: false,
        overwrites: std::collections::BTreeMap::new(),
        last_message: None,
        message_count: 0,
        created_by: Id::nil(),
        created_at: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chat_model::{Id, Channel, ChannelKind};
    use chat_model::live::State;
    use chat_model::perms;
    use chat_model::Space;

    fn uid(n: u128) -> Id {
        Id::from_u128(n)
    }

    #[test]
    fn a_role_overwrite_round_trips_through_the_table() {
        // The rows the UI draws must carry RWND flags, not raw idea-chat bits.
        let mut row = std::collections::BTreeMap::new();
        row.insert(uid(9), chat_model::Overwrite {
            allow: perms::Permission::ManageMembers.bit(),
            deny: perms::Permission::ViewChannel.bit(),
        });
        let channel = Channel {
            id: uid(11),
            kind: ChannelKind::Text,
            space: Some(uid(3)),
            name: "officers".to_string(),
            topic: "".to_string(),
            category: None,
            position: 0,
            parent: None,
            starter: None,
            view: 0,
            post: 0,
            members: Vec::new(),
            archived: false,
            overwrites: row,
            last_message: None,
            message_count: 0,
            created_by: Id::nil(),
            created_at: 0,
        };
        let rows = overwrites_of(channel);
        assert_eq!(rows.len(), 1);
        assert_eq!(matches!(rows[0].target, Target::Role(_)), true);
        assert_eq!(rows[0].allow.contains(Permissions::KICK_MEMBERS), true);
        assert_eq!(rows[0].deny.contains(Permissions::VIEW_CHANNEL), true);
    }

    #[test]
    fn a_non_member_reads_as_no_permission_set() {
        // `effective` answers None for someone off the roster, and my_permissions
        // turns that into a refusal rather than an empty row.
        let space = Space {
            id: uid(3), name: "a".to_string(), icon: None, owner: uid(7),
            members: empty_members(), roles: empty_roles(),
            everyone: perms::EVERYONE, categories: Vec::new(), created_at: 0,
        };
        assert_eq!(chat_model::perms::effective(&space, uid(8)).is_some(), false);
    }

    #[test]
    fn a_holders_rank_comes_from_their_top_role() {
        // The ladder position negates: the most senior role (idea-chat position
        // 0) is RWND's top-ranked role row.
        let mut roles = std::collections::BTreeMap::new();
        roles.insert(uid(9), chat_model::Role {
            id: uid(9), name: "Mods".to_string(), permissions: perms::ADMIN, position: 0,
        });
        let mut members = std::collections::BTreeMap::new();
        members.insert(uid(8), chat_model::Member { roles: vec!(uid(9)), joined_at: 0 });
        let space = Space {
            id: uid(3), name: "a".to_string(), icon: None, owner: uid(7),
            members: members, roles: roles, everyone: perms::EVERYONE,
            categories: Vec::new(), created_at: 0,
        };
        let mut world = State::default();
        world.spaces.insert(uid(3), space);
        assert_eq!(rank_of(world.clone(), uid(3), uid(7)) == Rank::Owner, true);
        assert_eq!(rank_of(world.clone(), uid(3), uid(8)) == Rank::Role(0), true);
        assert_eq!(rank_of(world, uid(3), uid(99)) == Rank::Everyone, true);
    }

    fn empty_members() -> std::collections::BTreeMap<Id, chat_model::Member> {
        std::collections::BTreeMap::new()
    }

    fn empty_roles() -> std::collections::BTreeMap<Id, chat_model::Role> {
        std::collections::BTreeMap::new()
    }
}
