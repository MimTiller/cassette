//! Named roles over the idea-chat RPCs: the snapshot and the edits translate
//! through `perms_map`, the colour pair below, and the `mentionable` flag, which
//! rides the wire both ways on this branch. There is no reordering verb — a
//! role's position counts UP its ladder (lower is more senior), so RWND's
//! position (higher first) is the negated idea-chat one, and a reorder refuses
//! with a sentence naming what this server orders by. Colours go the other way:
//! idea-chat's `Option<u32>` (0xRRGGBB) and RWND's `""`/`#rrggbb` string map
//! through one conversion pair both ways.

use crate::backend::*;
use crate::backend::idea_chat::{client, live, perms_map};
use chat_model::{Id, Role, Space};
use discord_server::server_roles::{validate_color, CustomRole};

impl RolesApi for IdeaChatBackend {
    // The reducer state carries the space's role records and every member's
    // held ids; the snapshot maps them through the shared permission table.
    async fn server_roles(server_id: i64) -> Result<RolesSnapshot, ServerError> {
        let world = live::snapshot();
        let space_id = live::wire_of(server_id);
        match world.spaces.get(&space_id) {
            None => Err(ServerError::failed("that space is not on this server")),
            Some(space) => Ok(roles_snapshot_of(space_id, space.clone())),
        }
    }

    // `create_role(space, name, permissions, colour, mentionable)` answers the
    // fresh role. The new role starts with the empty permission set — RWND sets
    // powers through `update_role` after creating — and its colour goes over as
    // idea-chat's `Option<u32>`; a malformed RWND colour refuses with a
    // sentence. RWND's `mentionable` rides the wire as idea-chat's own flag.
    async fn create_role(
        server_id: i64,
        name: String,
        color: String,
        mentionable: bool,
    ) -> Result<CustomRole, ServerError> {
        let colour = match idea_color_of(color) {
            Err(why) => return Err(ServerError::failed(why)),
            Ok(colour) => colour,
        };
        let mut args = Vec::new();
        args.push(client::arg(live::wire_of(server_id)));
        args.push(client::arg(&name));
        args.push(client::arg::<u64>(0));
        args.push(client::arg(colour));
        args.push(client::arg(mentionable));
        match client::call_as::<Role>("create_role", args).await {
            Err(e) => Err(e),
            Ok(role) => Ok(custom_role_of(server_id, role.clone())),
        }
    }

    // `update_role(space, role, name, permissions, colour, mentionable)` answers
    // unit: RWND's `None` permissions leave them as they are, so the current set
    // comes back through unchanged; the colour and `mentionable` are set to
    // exactly what is sent, like the name. The fresh record is read off the
    // snapshot the `RolesChanged` event updates.
    async fn update_role(
        role_id: i64,
        name: String,
        color: String,
        mentionable: bool,
        permissions: Option<Permissions>,
    ) -> Result<CustomRole, ServerError> {
        let role_wire = live::wire_of(role_id);
        let world = live::snapshot();
        match space_of_role(world.clone(), role_wire) {
            None => Err(ServerError::failed("that role is not on this server")),
            Some(space_id) => {
                let held = match world.spaces.get(&space_id).and_then(move |space| space.roles.get(&role_wire)) {
                    None => chat_model::perms::EVERYONE,
                    Some(role) => role.permissions,
                };
                let new_perms = match permissions {
                    None => held,
                    Some(permissions) => perms_map::to_idea(permissions),
                };
                let colour = match idea_color_of(color) {
                    Err(why) => return Err(ServerError::failed(why)),
                    Ok(colour) => colour,
                };
                let mut args = Vec::new();
                args.push(client::arg(space_id));
                args.push(client::arg(role_wire));
                args.push(client::arg(&name));
                args.push(client::arg::<u64>(new_perms));
                args.push(client::arg(colour));
                args.push(client::arg(mentionable));
                match client::post("update_role", args).await {
                    Err(e) => Err(e),
                    Ok(_) => match live::snapshot().spaces.get(&space_id).and_then(move |space| space.roles.get(&role_wire)) {
                        None => Err(ServerError::failed("the role is not on this server")),
                        Some(role) => Ok(custom_role_of(live::see(space_id), role.clone())),
                    },
                }
            },
        }
    }

    // Roles carry a position but the wire has no reordering verb: the
    // position comes from a role's rank in the ladder, not a free slot.
    async fn move_role(_role_id: i64, _position: i64) -> Result<CustomRole, ServerError> {
        Err(ServerError::failed("this server orders roles by rank, not by position"))
    }

    // `delete_role(space, role)` answers unit; the `RolesChanged` event drops
    // it from the state and the holders from their rosters.
    async fn delete_role(role_id: i64) -> Result<(), ServerError> {
        let role_wire = live::wire_of(role_id);
        match space_of_role(live::snapshot(), role_wire) {
            None => Err(ServerError::failed("that role is not on this server")),
            Some(space_id) => {
                let mut args = Vec::new();
                args.push(client::arg(space_id));
                args.push(client::arg(role_wire));
                match client::post("delete_role", args).await {
                    Err(e) => Err(e),
                    Ok(_) => Ok(()),
                }
            },
        }
    }

    // `set_roles(space, member, roles)` restates the whole list, so giving
    // one role is the member's current list plus it, and taking one is the
    // list without it. The server gates the act itself (Manage Roles, only
    // roles below the caller's own).
    async fn assign_role(role_id: i64, user_id: i64, assigned: bool) -> Result<(), ServerError> {
        let role_wire = live::wire_of(role_id);
        let member_wire = live::wire_of(user_id);
        match space_of_role(live::snapshot(), role_wire) {
            None => Err(ServerError::failed("that role is not on this server")),
            Some(space_id) => {
                let current: Vec<Id> = match live::snapshot().spaces.get(&space_id).and_then(move |space| space.members.get(&member_wire)) {
                    None => Vec::new(),
                    Some(member) => member.roles.clone(),
                };
                let roles: Vec<Id> = match assigned {
                    true => {
                        let mut roles = current;
                        if !roles.contains(&role_wire) {
                            roles.push(role_wire)
                        }
                        roles
                    }
                    false => current.iter().filter(move |held| **held != role_wire).copied().collect(),
                };
                let mut args = Vec::new();
                args.push(client::arg(space_id));
                args.push(client::arg(member_wire));
                args.push(client::arg(roles));
                match client::post("set_roles", args).await {
                    Err(e) => Err(e),
                    Ok(_) => Ok(()),
                }
            },
        }
    }
}

// --- helpers this area shares -------------------------------------------

/// RWND's role row from one idea-chat record: its colour translated to RWND's
/// hex string (the neutral `""` for no colour), its `mentionable` flag carried
/// across, and the ladder position negated so RWND's higher-first ordering
/// matches idea-chat's senior-lowest one.
pub(crate) fn custom_role_of(server_id: i64, role: Role) -> CustomRole {
    CustomRole {
        id: live::see(role.id),
        server_id: server_id,
        name: role.name.clone(),
        color: rwnd_color_of(role.colour),
        mentionable: role.mentionable,
        position: -(role.position as i64),
        permissions: perms_map::to_rwnd(role.permissions),
    }
}

/// idea-chat's `Option<u32>` colour from RWND's string: `""` (no colour) is
/// `None`, and a well-formed `#rrggbb` becomes the 0xRRGGBB number. Anything
/// else — "red", `#12345`, a number without `#` — refuses with RWND's own
/// message rather than guessing or panicking.
pub(crate) fn idea_color_of(color: String) -> Result<Option<u32>, String> {
    match validate_color(&color) {
        Err(why) => Err(why.clone()),
        Ok(c) if c.is_empty() => Ok(None),
        // `validate_color` already guaranteed exactly six hex digits after the
        // `#`, so the parse cannot fail; the arms keep it total anyway.
        Ok(c) => match c.strip_prefix("#") {
            None => Err("a role colour must look like #a1b2c3".to_string()),
            Some(hex) => match u32::from_str_radix(hex, 16) {
                Err(_) => Err("a role colour must look like #a1b2c3".to_string()),
                Ok(value) => Ok(Some(value)),
            },
        },
    }
}

/// RWND's colour string from idea-chat's `Option<u32>`: no colour is the
/// neutral `""`, otherwise the lower-case `#rrggbb` RWND draws.
pub(crate) fn rwnd_color_of(colour: Option<u32>) -> String {
    match colour {
        None => "".to_string(),
        Some(c) => format!("#{:06x}", c),
    }
}

/// The whole snapshot: role records (highest RWND position first), the
/// `(user, role)` pairs of who holds what, and the translated baseline.
pub(crate) fn roles_snapshot_of(space_id: Id, space: Space) -> RolesSnapshot {
    let mut roles: Vec<CustomRole> = space.roles.values().map(move |role| custom_role_of(live::see(space_id), role.clone())).collect();
    roles.sort_by_key(move |role| -role.position);
    let mut assignments: Vec<(i64, i64)> = Vec::new();
    for (user_id, member) in space.members.iter() {
        for role_id in member.roles.iter() {
            assignments.push((live::see(*user_id), live::see(*role_id)))
        }
    }
    RolesSnapshot {
        server_id: live::see(space_id),
        roles: roles,
        assignments: assignments,
        everyone_permissions: perms_map::to_rwnd(space.everyone),
    }
}

/// Which space defines the role `role`, if any of the viewer's do.
pub(crate) fn space_of_role(world: chat_model::live::State, role: Id) -> Option<Id> {
    for (space_id, space) in world.spaces.iter() {
        if space.roles.contains_key(&role) {
            return Some(*space_id)
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use chat_model::live::State;
    use chat_model::perms;
    use chat_model::{Member, Role};
    use std::collections::BTreeMap;

    fn uid(n: u128) -> Id {
        Id::from_u128(n)
    }

    #[test]
    fn a_roles_snapshot_translates_masks_and_negates_positions() {
        // Offline: the mapping is pure over the records, so hand it a space
        // with one role (view + kick powers, senior position 2) and one
        // holder, and check the RWND row.
        let mut roles: BTreeMap<Id, Role> = BTreeMap::new();
        roles.insert(uid(9), Role {
            id: uid(9),
            name: "Helpers".to_string(),
            permissions: perms::Permission::ViewChannel.bit() | perms::Permission::ManageMembers.bit(),
            position: 2,
            colour: Some(0xE74C3C),
            mentionable: true,
        });
        let mut members: BTreeMap<Id, Member> = BTreeMap::new();
        members.insert(uid(8), Member { roles: vec!(uid(9)), joined_at: 0 });
        let space = Space {
            id: uid(3),
            name: "Alpha".to_string(),
            icon: None,
            owner: uid(7),
            members: members,
            roles: roles,
            everyone: perms::DEFAULT_EVERYONE,
            categories: Vec::new(),
            created_at: 0,
        };
        let snap = roles_snapshot_of(uid(3), space);
        assert_eq!(snap.roles.len(), 1);
        assert_eq!(snap.roles[0].name, "Helpers".to_string());
        assert_eq!(snap.roles[0].position, -2);
        assert_eq!(snap.roles[0].color, "#e74c3c".to_string());
        assert_eq!(snap.roles[0].mentionable, true);
        assert_eq!(snap.roles[0].permissions.contains(Permissions::VIEW_CHANNEL), true);
        assert_eq!(snap.roles[0].permissions.contains(Permissions::KICK_MEMBERS), true);
        assert_eq!(snap.roles[0].permissions.contains(Permissions::BAN_MEMBERS), false);
        assert_eq!(snap.assignments.len(), 1);
        assert_eq!(snap.everyone_permissions.contains(Permissions::SEND_MESSAGES), true);
        assert_eq!(snap.everyone_permissions.contains(Permissions::MANAGE_ROLES), false);
    }

    #[test]
    fn a_role_edit_carries_the_callers_flags_once() {
        // The update path re-encodes RWND's flags through the table, so the
        // idea-chat set it writes holds exactly the mapped bits.
        let rw = Permissions::VIEW_CHANNEL.union(Permissions::MANAGE_ROLES);
        assert_eq!(perms_map::to_idea(rw), perms::Permission::ViewChannel.bit() | perms::Permission::ManageRoles.bit());
    }

    #[test]
    fn a_role_colour_round_trips_both_ways() {
        // RWND's "#e74c3c" goes over as 0xE74C3C and back as the same string,
        // upper-case input normalising to RWND's lower-case form.
        assert_eq!(idea_color_of("#e74C3C".to_string()), Ok(Some(0xE74C3C)));
        assert_eq!(rwnd_color_of(idea_color_of("#e74c3c".to_string()).unwrap()), "#e74c3c".to_string());
        assert_eq!(idea_color_of(rwnd_color_of(Some(0x2ECC71))).unwrap(), Some(0x2ECC71));
    }

    #[test]
    fn no_colour_maps_to_rwnd_s_neutral_default() {
        // idea-chat's None reads as RWND's "" (which its roles screen and the
        // member-name colour check treat as no colour), and "" writes back as
        // None, clearing the role.
        assert_eq!(rwnd_color_of(None), "".to_string());
        assert_eq!(idea_color_of("".to_string()), Ok(None));
    }

    #[test]
    fn a_mentionable_flag_maps_both_ways() {
        // The role row reads idea-chat's flag straight across, and a role
        // written without it reads back without it, so RWND's mention picker
        // and the wire agree on the same roles.
        let role = Role {
            id: uid(9),
            name: "Helpers".to_string(),
            permissions: 0,
            position: 1,
            colour: None,
            mentionable: true,
        };
        assert_eq!(custom_role_of(1, role.clone()).mentionable, true);
        assert_eq!(custom_role_of(1, Role { mentionable: false, ..role }).mentionable, false);
    }

    #[test]
    fn a_malformed_rwnd_colour_refuses_with_a_sentence() {
        // "red", a short hex, and a number without the `#` all answer RWND's
        // own colour message, which the callers wrap in a ServerError::failed —
        // no panic.
        assert_eq!(idea_color_of("red".to_string()), Err("colour must look like #a1b2c3".to_string()));
        assert_eq!(idea_color_of("#12345".to_string()), Err("colour must look like #a1b2c3".to_string()));
        assert_eq!(idea_color_of("1abc9c".to_string()), Err("colour must look like #a1b2c3".to_string()));
    }
}
