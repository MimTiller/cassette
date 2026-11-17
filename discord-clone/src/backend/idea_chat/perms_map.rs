//! One translation table between RWND's permission bits and idea-chat's,
//! used both ways: writing a role, an overwrite or the baseline goes RWND →
//! idea-chat, reading what the caller may do comes back idea-chat → RWND.
//!
//! RWND's bits are `discord_server::permissions::Permissions` (the `u64`
//! wrapper with its `pub const` flags); idea-chat's are `chat_model::perms`'s
//! `Permission::bit` positions. The two lists differ, so a pair either maps
//! or has no counterpart: unmapped RWND bits are dropped on write (a role
//! here can carry no power this server cannot enforce) and never reported on
//! read (idea-chat's own bits with no RWND flag do not become a flag the UI
//! would draw).
//!
//! | RWND flag | idea-chat `Permission` | note |
//! | --- | --- | --- |
//! | VIEW_CHANNEL | ViewChannel | |
//! | SEND_MESSAGES | SendMessage | |
//! | ADD_REACTIONS | AddReaction | |
//! | ATTACH_FILES | AttachFile | |
//! | EMBED_LINKS | EmbedLink | |
//! | MENTION_EVERYONE | MentionEveryone | |
//! | MANAGE_MESSAGES (pin/unpin) | ManagePins | idea-chat defines the bit but has no pin endpoint to gate it, so a holder of it sees no pinned control here |
//! | CREATE_INVITE | ManageInvites | |
//! | KICK_MEMBERS | ManageMembers | |
//! | BAN_MEMBERS | BanMembers | |
//! | MODERATE_MEMBERS (timeout) | TimeoutMembers | |
//! | MANAGE_CHANNELS | ManageChannels | |
//! | MANAGE_ROLES | ManageRoles | |
//! | MANAGE_SERVER | ManageSpaceSettings | |
//! | ADMINISTRATOR | IsAdmin (tag) | tag: expands to everything on either side |
//! | CONNECT | VoiceConnect | |
//! | SPEAK | VoiceSpeak | |
//!
//! RWND flags with no counterpart: MANAGE_EXPRESSIONS — idea-chat has no
//! custom emoji anywhere in its model.
//! idea-chat permissions with no RWND flag: EditOwnMessage, DeleteOwnMessage,
//! DeleteOthersMessage, VoiceVideo, VoiceShareScreen, VoiceForceMute, VoiceMoveMember,
//! ManageChannelRoles (RWND's channel-role setting rides MANAGE_CHANNELS);
//! they are dropped on write and never reported on read.

use chat_model::perms::Permission;
use discord_server::permissions::Permissions;

/// `(RWND flag, idea-chat bit)` pairs — the one table, read both ways. The
/// idea-chat side is `Permission::bit`, so an idea-chat flag renamed upstream
/// moves here rather than silently drifting.
const PAIRS: [(Permissions, u64); 17] = [
    (Permissions::VIEW_CHANNEL, Permission::ViewChannel.bit()),
    (Permissions::SEND_MESSAGES, Permission::SendMessage.bit()),
    (Permissions::ADD_REACTIONS, Permission::AddReaction.bit()),
    (Permissions::ATTACH_FILES, Permission::AttachFile.bit()),
    (Permissions::EMBED_LINKS, Permission::EmbedLink.bit()),
    (Permissions::MENTION_EVERYONE, Permission::MentionEveryone.bit()),
    (Permissions::MANAGE_MESSAGES, Permission::ManagePins.bit()),
    (Permissions::CREATE_INVITE, Permission::ManageInvites.bit()),
    (Permissions::KICK_MEMBERS, Permission::ManageMembers.bit()),
    (Permissions::BAN_MEMBERS, Permission::BanMembers.bit()),
    (Permissions::MODERATE_MEMBERS, Permission::TimeoutMembers.bit()),
    (Permissions::MANAGE_CHANNELS, Permission::ManageChannels.bit()),
    (Permissions::MANAGE_ROLES, Permission::ManageRoles.bit()),
    (Permissions::MANAGE_SERVER, Permission::ManageSpaceSettings.bit()),
    (Permissions::ADMINISTRATOR, Permission::IsAdmin.bit()),
    (Permissions::CONNECT, Permission::VoiceConnect.bit()),
    (Permissions::SPEAK, Permission::VoiceSpeak.bit()),
];

/// RWND's bitset → idea-chat's. Unmapped RWND bits drop out: a role, an
/// overwrite or the baseline this server stores can only carry a power it
/// knows how to enforce.
pub(crate) fn to_idea(permissions: Permissions) -> u64 {
    let held = Permissions(permissions.0).known();
    PAIRS.iter().fold(0u64, move |acc, (flag, bit)| {
        if held.contains(*flag) {
            acc | bit
        } else {
            acc
        }
    })
}

/// idea-chat's bitset → RWND's. Unmapped idea-chat bits are never reported:
/// a power this server has no RWND flag for does not become a flag the UI
/// would draw.
pub(crate) fn to_rwnd(permissions: u64) -> Permissions {
    PAIRS.iter().fold(Permissions::NONE, move |acc, (flag, bit)| {
        if chat_model::perms::covers(permissions, *bit) {
            acc.union(*flag)
        } else {
            acc
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chat_model::perms::Permission;

    #[test]
    fn every_mapped_pair_round_trips_both_ways() {
        for (flag, bit) in PAIRS.iter() {
            assert_eq!(to_rwnd(to_idea(*flag)).contains(*flag), true);
            assert_eq!(chat_model::perms::covers(to_idea(to_rwnd(*bit)), *bit), true);
        }
    }

    #[test]
    fn an_unmapped_rwnd_bit_drops_on_write() {
        // MANAGE_EXPRESSIONS has no counterpart: a role written with only it
        // carries nothing this server can enforce.
        assert_eq!(to_idea(Permissions::MANAGE_EXPRESSIONS), 0);
    }

    #[test]
    fn an_unmapped_idea_bit_is_never_reported_on_read() {
        // VoiceVideo has no RWND flag: reading a set that holds only it
        // answers no flags at all.
        assert_eq!(to_rwnd(Permission::VoiceVideo.bit()) == Permissions::NONE, true);
    }
}
