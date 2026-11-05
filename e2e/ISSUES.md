# App issues found by the e2e suite

Worst first. Each one is marked KNOWN in the suite, so the suite stays green
until it's fixed; when it is, flip the `known_bug` back to a `check`.
Screenshots are in `/root/discord-ref/e2e-issues-2026-10-31/` on the dev box.

Also seen: the page carries hidden demo components
(overlays.rs, components/delete_message_confirm.rs, a moderation gallery)
whose texts duplicate real controls, which is what tripped earlier test runs.
