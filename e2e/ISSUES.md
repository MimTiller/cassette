# App issues found by the robot suite

Each open one is marked KNOWN in the suite (via `crate::support::known_bug`),
so the suite stays green until it is fixed; when it is, flip the `known_bug`
back to an assertion and drop the entry here. A run's `[KNOWN]` lines name the
check and reason; screenshots are listed as `[shot] label: path` lines in the
run's output.

No open bugs: a full run prints no `[KNOWN]` lines (last run: 32 passed, 0
failed).

Also seen: the page carries hidden demo components whose texts duplicate real
controls (overlays.rs, components/delete_message_confirm.rs, a moderation
gallery). Locator "no match"/"mounted but not visible" failures often mean a
hidden copy, not a missing one.
