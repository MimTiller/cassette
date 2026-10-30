# Working on Cassette

Rules for anyone changing this repo, human or agent. Read
[ARCHITECTURE.md](ARCHITECTURE.md) first for how the pieces fit together.

## Before you call a change done

Run these from the repo root. All must pass, with no new warnings:

```bash
(cd discord-server && cargo check --features server && cargo test --features server)
(cd discord-clone && cargo check && cargo test)
```

For a UI change, also look at it in a browser (see "Testing in a browser"),
driving it through the idealyst MCP server (see "Driving the app").

## Rules

### Server

- **Every permission check happens on the server**, in the feature's
  service, against `crate::permissions` (see ARCHITECTURE.md). Check a
  channel's permissions, not the server's, for anything about its messages,
  and never send a channel's data to someone without `VIEW_CHANNEL` in it.
  A new privileged operation reuses a permission bit where one fits, and
  gets a test for both the allowed and the refused caller.
  Client-side checks only hide controls; they never protect anything.
- **Refusals are user-facing text.** `ServerError::failed("…")` messages are
  shown to the user verbatim, so write them as plain sentences ("only admins
  and the owner can manage roles"), never debug output.
- **Broadcast changes others can see.** If a `#[server]` function changes
  data other users have on screen, publish a `WsServerMsg` frame and handle
  it in `discord-clone/src/app/gateway_effects.rs`. New fields on an existing
  frame get `#[serde(default)]` so older clients don't break.
- **Migrations:** add a new file with the next unused number
  (`NNNN_what.sql`). Never reuse a number, and never edit a migration that
  has been merged. Use foreign keys with `ON DELETE CASCADE` so deleting a
  user, server or channel leaves no orphans.
- **Test the service, not just the handler.** Add unit tests for pure logic
  and `db_tests` for anything touching SQL.

### Client

- **Colours come from the theme:** `col(theme::ROLE)` for UI,
  `theme::live(theme::ROLE)` for icons, and `theme::fixed("#hex")` only for
  data colours (a role's colour, a swatch). Never add hex constants to a file.
- **Every `text_input` needs `on_change`.** `value = sig` only fills the box;
  without `on_change = move |v: String| sig.set(v)` the signal never sees
  what was typed.
- **Show server errors with `crate::errors::friendly(&e)`.**
- **`switch` keys must include everything the subtree reads** that can
  change. Dialogs are built once and hidden, so anything that must be fresh
  when one opens goes in a `switch` keyed on its open flag.
- **Build `Element`s inside render closures.** Elements aren't `Copy`; one
  built outside a `switch` render closure is moved out on the first render.
- **Calling a PascalCase component with positional arguments:** inside
  `ui!` that parses as props, so use the full path
  (`crate::module::Name(..)`).
- **Timers:** handles from `after_ms` and `after_animation_frame` cancel when
  dropped. Use `after_ms_detached` for fire-and-forget.
- **Server calls: `spawn_then_in`, never a raw `spawn_async` that writes
  signals.** Put the call in the future and every signal write in the
  callback; the callback is skipped if its scope has gone. Anchor it with
  `runtime_core::ScopeAlive::current()` taken in the component body (a
  handler's own node dies when its first write rebuilds it), or with
  `crate::app::shell_alive()` for app-shell state changed from a menu or
  popup that closes. `idealyst lint discord-clone` flags the unsafe form
  (`signal-across-await`).
- **New shared state goes through `ShellState`** (see ARCHITECTURE.md).

### Both

- **No mock or demo data.** Everything on screen comes from the server.
- **Delete dead code instead of hiding it.** No `#[allow(dead_code)]` or
  `#[allow(unused)]` to silence a warning about something unused.
- **Comments describe what the code does now.** No history ("was", "now",
  "used to", "[Feature 16]", "after the refactor"). That belongs in commit
  messages.
- **Match the surrounding code:** its naming, comment density and idioms.
- **Never commit secrets or data:** `*.db`, `*.session-secret`, `giphy.key`,
  `media/`. They are gitignored; keep it that way.

### Out of scope

Don't build threads, polls or scheduled events; they aren't wanted. Voice
audio comes last, after everything else.

## Commits

- One logical change per commit, with an imperative summary line ("Add role
  colours to member names").
- The body says what changed and why, briefly.
- Don't mix a refactor with a behaviour change in one commit.

## Testing in a browser

Start a dev server from the **repo root**, with the crate as an argument:

```bash
idealyst dev --web --local --port 8086 discord-clone
```

- **Start it from the repo root.** Started from inside `discord-clone/`, it
  opens a fresh empty `discord.db` there, and logins fail.
- **Use a free port.** :8080 is the live site.
- **Hot patches can leave old code running.** If the page contradicts the
  code, stop the server and move `discord-clone/target/idealyst-web-*/idealyst-hotpatch`
  (and `target/idealyst-web-*/idealyst-hotpatch`) aside, then restart. The
  `[build-web] timing` line should show a `cargo` step.
- **Stopping a server:** by PID only. Never use `pkill -f` or `pgrep -f`
  with a pattern inside an `ssh '…'` command; it matches the shell running
  it and kills that shell.

## Driving the app

Use the idealyst MCP server to drive and inspect the running app. It's what
the framework provides for development, and it sees the app's own element
tree.

- **Start it:** `idealyst mcp --project-root <repo>/discord-clone`. Most
  agent tools take this as an MCP server entry.
- **Find the app:** with a dev server running and its page open in a
  browser, the app registers with it. `list_apps` / `wait_for_app` list one
  app per dev server, with its `project_root`; pick yours by that. Keep one
  tab per dev server, since several tabs don't show up as separate apps; for
  a second user, script that side (or run a second dev server).
- **Drive it:** `find_element` / `find_all_elements` (by `test_id`, label or
  kind), `click` or `tap_native`, `type_text`, `get_state`, `get_snapshot`,
  `screenshot`, `get_logs`, `read_signal`, and `call_function` for
  `#[robot_fn]` hooks such as `avatar.upload`.
- **`test_id`s live in the app's tree, not the web page's DOM.** Look them
  up through MCP. Its results say whether each match is `visible` and
  `in_viewport`; text and position guesses in the DOM can hit hidden copies
  of a control.
- **Look things up:** for idealyst questions, use `search`, `read_guide`,
  `describe_primitive`, `describe_component` and `list_recipes`.
- **Fall back to scripts only for gaps.** Drive Chrome over the DevTools
  protocol only for what MCP can't do, and say which gap you hit.

