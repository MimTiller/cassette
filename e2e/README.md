# Cassette E2E tests: the Rust robot suite

The end-to-end suite lives in `discord-clone/tests/robot/` (registered in
`discord-clone/tests/robot.rs`). It drives the running app through the
idealyst robot bridge — `test_id`s and visible labels — against the run's
own isolated server and database. Python suites are retired: `e2e/deferred/`
holds the two tests (search, permissions) whose areas are being redesigned
before porting.

## Running it

From this worktree's root:

```sh
sh /root/bin/robot-test            # the whole suite
sh /root/bin/robot-test server::   # one module (filter = <module>::)
```

The wrapper runs `idealyst test --web discord-clone` with:
- `--env DISCORD_DB=<throwaway .db>` — the run gets a fresh database, so
  tests never see each other's state or the staging server's.
- `IDEALYST_CHROME=/root/bin/chrome-robot` — a headless-Chrome wrapper this
  host needs (`--no-sandbox`); `users::open_browser` launches extra
  browsers with the same variable.
- It first deletes the isolated target's `idealyst-hotpatch` dirs, because a
  hot patch can leave the test session running a stale build whose
  `robot_fn`s and `test_id`s the suite needs are missing.

The run prints ok/FAIL lines, one `[KNOWN]` line per still-broken check, and
`[shot] label: path` per screenshot. A full run costs ~4 minutes; one module, ~30 s.

## Writing a test

- One module per area: `tests/robot/<area>.rs`, appended to the module list
  in `tests/robot.rs`. Mark each test `#[robot_test]`.
- Shared steps live in `tests/robot/support.rs`: `Account::new(prefix)`
  (fresh account through the server), `fresh_name`, `sign_up`/`sign_in_as`,
  `signed_out` (start every test with it), `eventually`, `WAIT`, and
  `known_bug` — a check that fails for a bug in `e2e/ISSUES.md`.
- Direct server calls for setup the UI cannot script: `Account.call("create_server", ...)`
  against `/_srv/<fn>` on `IDEALYST_TEST_PORT`.
- App elements must carry a `test_id` for the robot to act on; add one in
  `discord-clone/src` where nothing identifies the control, and expose
  computed state as a read-only `#[runtime_core::robot_fn]` in
  `discord-clone/src/robot_hooks.rs` (register it in that file's
  `register()` — forgetting silently blanks the hook).
- Keep tests independent: fresh accounts/server/channel per test via
  `Account::new(prefix)` / `fresh_name(prefix)` so test_ids never collide.

## Multi-user tests

`idealyst test` drives one browser; a second user gets a second real browser
through `crate::users::open_browser(label)` (`tests/robot/users.rs`), which
returns a `Browser` with its own full driver (`second.app`). The robot relay
routes to the newest tab that dials it, so the extra browser starts its own
relay and, over the DevTools protocol before the page loads, wraps the page's
`WebSocket` to dial that relay instead of the run's. Screenshots from an
extra browser land in `/root/.idealyst/screenshots/cassette-<label>-*.png`.
End tests that leave a picked status (Idle/DND) with an explicit `sign_out`
so the next test's `signed_out` can still find the login screen.
