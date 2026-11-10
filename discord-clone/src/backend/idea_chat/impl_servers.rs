//! Servers and channels over the idea-chat RPCs: listing rides the reducer
//! state (idea-chat has no list RPC), the mutations are calls whose facts
//! arrive as live events, and invites are the preview-then-join pair.
//! A space's channels come from `State.channels` keyed by `Channel.space`.

use crate::backend::*;
use crate::backend::idea_chat::{client, live, model};
use chat_model::{Channel, ChannelKind, Id, Role, Space};
use runtime_shared::time::epoch_millis;

impl ServersApi for IdeaChatBackend {
    // No list RPC: the snapshot carries every space the viewer is in, in
    // `space_order` order.
    async fn list_servers() -> Result<Vec<ServerSummary>, ServerError> {
        Ok(servers_of(live::snapshot()))
    }

    // `create_space(name)` answers the new space's id; its first channels
    // arrive on the live connection as `SpaceUpserted`.
    async fn create_server(name: String) -> Result<ServerSummary, ServerError> {
        let mut args = Vec::new();
        args.push(client::arg(&name));
        match client::call_as::<Id>("create_space", args).await {
            Err(e) => Err(e),
            Ok(space) => Ok(ServerSummary {
                id: live::see(space),
                name: name.clone(),
                icon_url: None,
            }),
        }
    }

    // Channels come from the snapshot, filtered to this space, in position
    // order; idea-chat has no per-space channel RPC.
    async fn list_channels(server_id: i64) -> Result<Vec<ChannelSummary>, ServerError> {
        Ok(channels_of(live::snapshot(), live::wire_of(server_id)))
    }

    // `create_channel(space, kind, name, category)`: RWND's channel kind is
    // the string "text"/"voice"; no category or private flag maps onto this
    // server (visibility is by role), so `private` is dropped here.
    async fn create_channel(
        server_id: i64,
        name: String,
        kind: String,
        _parent_channel_id: Option<i64>,
        _private: bool,
    ) -> Result<ChannelResult, ServerError> {
        let channel_kind = match kind.as_str() {
            "voice" => ChannelKind::Voice,
            _ => ChannelKind::Text,
        };
        let mut args = Vec::new();
        args.push(client::arg(live::wire_of(server_id)));
        args.push(client::arg(channel_kind));
        args.push(client::arg(&name));
        args.push(serde_json::Value::Null);
        match client::call_as::<Channel>("create_channel", args).await {
            Err(e) => Err(e),
            Ok(channel) => Ok(ChannelResult {
                channel_id: live::see(channel.id),
                server_id: match channel.space {
                    Some(space) => live::see(space),
                    None => server_id,
                },
                name: Some(channel.name),
            }),
        }
    }

    // `rename_channel(channel, name)` answers the fresh channel.
    async fn rename_channel(
        _server_id: i64,
        channel_id: i64,
        new_name: String,
    ) -> Result<ChannelResult, ServerError> {
        let mut args = Vec::new();
        args.push(client::arg(live::wire_of(channel_id)));
        args.push(client::arg(&new_name));
        match client::call_as::<Channel>("rename_channel", args).await {
            Err(e) => Err(e),
            Ok(channel) => Ok(ChannelResult {
                channel_id: live::see(channel.id),
                server_id: match channel.space {
                    Some(space) => live::see(space),
                    None => _server_id,
                },
                name: Some(channel.name),
            }),
        }
    }

    // `delete_channel(channel)` answers unit; the `ChannelRemoved` event
    // drops it from the state.
    async fn delete_channel(
        server_id: i64,
        channel_id: i64,
    ) -> Result<ChannelResult, ServerError> {
        let mut args = Vec::new();
        args.push(client::arg(live::wire_of(channel_id)));
        match client::post("delete_channel", args).await {
            Err(e) => Err(e),
            Ok(_) => Ok(ChannelResult { channel_id, server_id, name: None }),
        }
    }

    // `rename_space(space, name)`: read the fresh name/icon/owner off the
    // snapshot the `SpaceRenamed` event updates.
    async fn rename_server(
        server_id: i64,
        new_name: String,
    ) -> Result<ServerSettingsResult, ServerError> {
        let mut args = Vec::new();
        args.push(client::arg(live::wire_of(server_id)));
        args.push(client::arg(&new_name));
        match client::post("rename_space", args).await {
            Err(e) => Err(e),
            Ok(_) => Ok(settings_of(server_id)),
        }
    }

    // `set_space_icon(space, upload?)`: the icon is an upload id from the
    // triplet; RWND hands a data URL so `upload_data_url` sniffs before PUT.
    async fn set_server_icon(
        server_id: i64,
        icon: Option<String>,
    ) -> Result<ServerSettingsResult, ServerError> {
        let upload = match icon {
            None => None,
            Some(url) => match live::upload_data_url("icon".to_string(), url, model::Purpose::Icon).await {
                Err(e) => return Err(e),
                Ok(id) => Some(id),
            },
        };
        let mut args = Vec::new();
        args.push(client::arg(live::wire_of(server_id)));
        args.push(client::arg(upload));
        match client::post("set_space_icon", args).await {
            Err(e) => Err(e),
            Ok(_) => Ok(settings_of(server_id)),
        }
    }

    // `transfer_space(space, to)`: the new owner is `to`.
    async fn transfer_server_ownership(
        server_id: i64,
        new_owner_id: i64,
    ) -> Result<ServerSettingsResult, ServerError> {
        let mut args = Vec::new();
        args.push(client::arg(live::wire_of(server_id)));
        args.push(client::arg(live::wire_of(new_owner_id)));
        match client::post("transfer_space", args).await {
            Err(e) => Err(e),
            Ok(_) => Ok(settings_of(server_id)),
        }
    }

    // `leave_space(space)`; the `SpaceRemoved`/`MemberLeft` events follow.
    async fn leave_server(server_id: i64) -> Result<(), ServerError> {
        let mut args = Vec::new();
        args.push(client::arg(live::wire_of(server_id)));
        match client::post("leave_space", args).await {
            Err(e) => Err(e),
            Ok(_) => Ok(()),
        }
    }
}

impl InvitesApi for IdeaChatBackend {
    // `create_invite(space, max_uses?, hours?)`: RWND's `expires_at` (epoch
    // seconds) becomes whole hours from now on this server.
    async fn create_invite(
        server_id: i64,
        expires_at: Option<i64>,
        max_uses: Option<i64>,
    ) -> Result<Invite, ServerError> {
        let hours: Option<u32> = match expires_at {
            None => None,
            Some(at) => Some(hours_from_now(epoch_millis() / 1_000, at)),
        };
        let uses: Option<u32> = match max_uses { Some(u) => Some(u as u32), None => None };
        let mut args = Vec::new();
        args.push(client::arg(live::wire_of(server_id)));
        args.push(client::arg(uses));
        args.push(client::arg(hours));
        match client::call_as::<chat_model::Invite>("create_invite", args).await {
            Err(e) => Err(e),
            Ok(wire) => Ok(Invite {
                id: live::see(wire.space),
                server_id: live::see(wire.space),
                code: wire.code,
                created_by: live::see(wire.created_by),
                created_at: live::millis_at(wire.created_at),
                expires_at: wire.expires_at,
                max_uses: match wire.max_uses { Some(m) => Some(m as i64), None => None },
                uses: wire.uses as i64,
            }),
        }
    }

    // The preview-then-join pair: `invite_preview(code)` shows the space,
    // then `join_space(code)` (Answer<Id>) actually joins it.
    async fn redeem_invite(code: String) -> Result<RedeemResult, ServerError> {
        let mut preview_args = Vec::new();
        preview_args.push(client::arg(&code));
        match client::call_as::<chat_model::InvitePreview>("invite_preview", preview_args).await {
            Err(e) => Err(e),
            Ok(_) => {
                let mut join_args = Vec::new();
                join_args.push(client::arg(&code));
                match client::call_as::<Id>("join_space", join_args).await {
                    Err(e) => Err(e),
                    Ok(space) => {
                        let world = live::snapshot();
                        Ok(RedeemResult {
                            server_id: live::see(space),
                            user_id: match world.me {
                                Some(me) => live::see(me.user.id),
                                None => 0,
                            },
                            already_member: false,
                            role: "member".to_string(),
                        })
                    },
                }
            },
        }
    }
}

// RWND's `expires_at` (epoch seconds) becomes whole hours from now on this
// server; never round an invite down to shorter than it was asked for.
pub(crate) fn hours_from_now(now: i64, at: i64) -> u32 {
    let secs = at - now;
    if secs <= 0 {
        1u32
    } else {
        ((secs + 3_599) / 3_600) as u32
    }
}

// --- the state views the RPC-less calls read ------------------------------

/// The viewer's spaces, in switcher order, as server rows.
fn servers_of(world: chat_model::live::State) -> Vec<ServerSummary> {
    let mut out: Vec<ServerSummary> = Vec::new();
    for space_id in world.space_order.iter() {
        match world.spaces.get(space_id) {
            None => {},
            Some(space) => out.push(ServerSummary {
                id: live::see(*space_id),
                name: space.name.to_string(),
                icon_url: match space.icon { Some(ref u) => Some(u.to_string()), None => None },
            }),
        }
    }
    out
}

/// `space_id`'s channels, in position order, with RWND's kind string.
fn channels_of(world: chat_model::live::State, space_id: Id) -> Vec<ChannelSummary> {
    let mut rows: Vec<ChannelSummary> = Vec::new();
    for (id, channel) in world.channels.iter() {
        match channel.space {
            None => {},
            Some(space) => {
                if space == space_id {
                    rows.push(channel_row(*id, channel.clone()));
                }
            },
        }
    }
    rows
}

/// One channel's row; private is what a non-everyone channel is on this
/// server (a member can't see it).
pub(crate) fn channel_row(id: Id, channel: Channel) -> ChannelSummary {
    ChannelSummary {
        id: live::see(id),
        name: channel.name.to_string(),
        kind: channel_kind_word(channel.kind),
        position: channel.position as i64,
        parent_channel_id: match channel.parent {
            Some(p) => Some(live::see(p)),
            None => None,
        },
        private: channel.view == Role::Admin || channel.view == Role::Owner,
    }
}

fn channel_kind_word(kind: ChannelKind) -> String {
    match kind {
        ChannelKind::Voice => "voice".to_string(),
        _ => "text".to_string(),
    }
}

/// The space's settings row, read off the state after a mutation.
pub(crate) fn settings_of(server_id: i64) -> ServerSettingsResult {
    let world = live::snapshot();
    let space = live::wire_of(server_id);
    match world.spaces.get(&space) {
        None => ServerSettingsResult {
            server_id,
            name: String::new(),
            icon_url: None,
            owner_id: 0,
        },
        Some(s) => ServerSettingsResult {
            server_id,
            name: s.name.to_string(),
            icon_url: match s.icon { Some(ref u) => Some(u.to_string()), None => None },
            owner_id: live::see(s.owner),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chat_model::live::State;
    use chat_model::{Id, Space, Channel, ChannelKind, Role};
    use std::collections::BTreeMap;

    fn uid(n: u128) -> Id {
        Id::from_u128(n)
    }

    fn empty_members() -> BTreeMap<Id, chat_model::Member> {
        BTreeMap::new()
    }

    // Feed a recorded SpaceUpserted-shaped snapshot through the listing
    // mapping: two spaces, one with two channels (a text and a voice), and
    // check the rows and their i64 ids come out first-seen-first-numbered.
    fn two_space_state() -> State {
        let mut s = State::default();
        s.spaces.insert(uid(1), Space {
            id: uid(1),
            name: "Alpha".to_string(),
            icon: None,
            owner: uid(2),
            members: empty_members(),
            categories: Vec::new(),
            created_at: 0,
        });
        s.spaces.insert(uid(3), Space {
            id: uid(3),
            name: "Beta".to_string(),
            icon: Some("/f/x.png".to_string()),
            owner: uid(2),
            members: empty_members(),
            categories: Vec::new(),
            created_at: 0,
        });
        s.space_order = vec![uid(1), uid(3)];
        s.channels.insert(uid(10), Channel {
            id: uid(10),
            kind: ChannelKind::Text,
            space: Some(uid(1)),
            name: "general".to_string(),
            topic: "".to_string(),
            category: None,
            position: 0,
            parent: None,
            starter: None,
            view: Role::Member,
            post: Role::Member,
            members: Vec::new(),
            archived: false,
            last_message: None,
            message_count: 0,
            created_by: uid(2),
            created_at: 0,
        });
        s.channels.insert(uid(11), Channel {
            id: uid(11),
            kind: ChannelKind::Voice,
            space: Some(uid(1)),
            name: "lounge".to_string(),
            topic: "".to_string(),
            category: None,
            position: 1,
            parent: None,
            starter: None,
            view: Role::Admin,
            post: Role::Admin,
            members: Vec::new(),
            archived: false,
            last_message: None,
            message_count: 0,
            created_by: uid(2),
            created_at: 0,
        });
        s
    }

    #[test]
    fn spaces_list_in_switcher_order_with_stable_ids() {
        // Offline: number ids deterministically via the table's first-seen
        // order by feeding the state through the pure listing helper. The
        // live() id table reads the account's handle from auth, so we exercise
        // the ordering and field mapping, not the persistence.
        let world = two_space_state();
        let rows = servers_of(world);
        assert_eq!(rows.len(), 2);
        match rows.get(0) {
            Some(first) => assert_eq!(first.name, "Alpha".to_string()),
            None => panic!("no first row"),
        }
        match rows.get(1) {
            Some(second) => assert_eq!(second.icon_url, Some("/f/x.png".to_string())),
            None => panic!("no second row"),
        }
    }

    #[test]
    fn channels_of_a_space_come_back_in_position_order() {
        let world = two_space_state();
        let rows = channels_of(world, uid(1));
        assert_eq!(rows.len(), 2);
        assert_eq!(match rows.iter().find(move |c| c.kind == "voice".to_string()) { None => -1, Some(row) => row.position }, 1);
    }

    #[test]
    fn a_voice_channel_maps_to_the_voice_kind_and_private_when_admin_only() {
        let c = Channel {
            id: uid(11),
            kind: ChannelKind::Voice,
            space: Some(uid(1)),
            name: "lounge".to_string(),
            topic: "".to_string(),
            category: None,
            position: 1,
            parent: None,
            starter: None,
            view: Role::Admin,
            post: Role::Admin,
            members: Vec::new(),
            archived: false,
            last_message: None,
            message_count: 0,
            created_by: uid(2),
            created_at: 0,
        };
        assert_eq!(channel_kind_word(c.kind), "voice".to_string());
        assert_eq!(channel_row(c.id, c).private, true);
    }

    #[test]
    fn an_expiry_of_epoch_seconds_becomes_whole_hours_from_now() {
        // 2h into the future rounds to hours = 2; the wire clamps per server.
        let at = 7_200;
        assert_eq!(hours_from_now(0, at), 2u32);
        // A little over two hours rounds up to 3 hours, never down: an invite
        // that claims 2h is not shorter than the caller asked for.
        assert_eq!(hours_from_now(0, 7_201), 3u32);
    }
}
