# App issues found by the e2e suite

Worst first. Each one is marked KNOWN in the suite, so the suite stays green
until it's fixed; when it is, flip the `known_bug` back to a `check`.
Screenshots are in `/root/discord-ref/e2e-issues-2026-10-31/` on the dev box.

1. **A brand-new DM doesn't show up for the person receiving it.**
   A messages B for the first time: B's DM list stays empty until B closes
   and reopens the DM screen. `live_dm_view::dm_incoming_effect` only buffers
   `DmMessage` frames for the open DM and never refetches the list for a DM it
   doesn't know. Fix: on a `DmMessage` for an unknown `dm_id`, call
   `group_dm::refresh`. Test: test_dms_friends, "a brand-new DM appears in the
   other person's list live". Shot: `1-new-dm-not-listed.png`.

11. **The Invite People page has no header background.** It is a full-screen
    page whose top strip is transparent, so the title "Invite People" draws
    over the dimmed server header and member list underneath. (The overnight
    modal-style work, q30d, may cover this.)

Also seen: B's member list showed A as offline while A was online
(`4-member-list-squeezed.png`); and the page carries hidden demo components
(overlays.rs, components/delete_message_confirm.rs, a moderation gallery)
whose texts duplicate real controls, which is what tripped earlier test runs.
