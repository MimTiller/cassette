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

2. **Denied Send Messages leaves the composer open.** The owner denies Send
   Messages for @everyone in #general; the member's composer still reads
   "Message #general", typing works, and Enter silently does nothing (the
   server refuses it). The no-permission bar never shows, live or after
   switching channels. Test: test_permissions, "the composer shows the
   no-permission bar". Shot: `2-denied-composer-open.png`.

3. **Delete in the message menu does nothing visible.** Menu → Delete closes
   the menu, but the inline "Delete this message? Confirm / No" strip never
   appears and the message stays. (Suspected; the app exposes no signals to
   MCP, so `live_delete_confirm` couldn't be read directly.) Test:
   test_server, "delete: the message disappears". Shot:
   `3-delete-no-confirm.png`.

4. **A long wrapped message squeezes the member list.** After a ~200-word
   message the chat column grows and the member list shrinks to a ~70px
   sliver ("ONLINE — 1" wraps, names cut off) on every client. Seen in
   test_messages. Shot: `4-member-list-squeezed.png`.

6. **No new-messages bar in the channel a server opens on.** Entering a
   server opens #general before `unread_counts` returns, so its count reads 0
   and the "N new messages" bar never shows there (it works for a channel
   opened afterwards). Test: test_notifications. Shot:
   `6-no-unread-bar-on-entry.png`.

7. **Pinning a live message gives no confirmation.** Nothing sets
   `pin_visible` for live rows (only the removed mock rows did), so the
   "Message pinned!" toast never shows; only the header count changes. Test:
   test_messages, "pin: the pin toast confirms".

8. **Search jump doesn't highlight the message** (carried over from the
   earlier run, not re-checked). Test: test_search.

9. **Blocking hides messages** is still KNOWN in test_dms_friends (right-click
   → Block didn't collapse the blocked user's DM text); not investigated yet.

11. **The Invite People page has no header background.** It is a full-screen
    page whose top strip is transparent, so the title "Invite People" draws
    over the dimmed server header and member list underneath. (The overnight
    modal-style work, q30d, may cover this.)

Also seen: B's member list showed A as offline while A was online
(`4-member-list-squeezed.png`); and the page carries hidden demo components
(overlays.rs, components/delete_message_confirm.rs, a moderation gallery)
whose texts duplicate real controls, which is what tripped earlier test runs.
