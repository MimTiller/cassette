"""e2e browser suite for Cassette (the discord-clone app).

How it works
- Each test_*.py is standalone: run it as
    /usr/bin/python3 test_NAME.py http://127.0.0.1:8085 out
  it prints one `check`/`known_bug` line per assertion and ends with a
  `RESULTS {...}` JSON line that run.py parses.
- run.py runs every test_*.py in sorted order and reports ok/KNOWN/FAIL per
  test; a check that the test itself marked known_bug prints KNOWN and does
  not fail the suite.
- All tests drive the shared test browser over CDP (127.0.0.1:9341) and
  close every tab they open (finally blocks + run.py closes strays).
- cdp.py is the toolkit: Session (open/close tabs), paint checks (visible/
  painted/text_pos — only text actually painted on screen passes), clicking
  (click_text/click_starts/center), typing (type_ph_start/input helpers),
  waiting (wait_for/until), seeding (signup/login/send_message) and the
  check/known_bug accounting.
- srv_helper.py: POST /_srv/<fn> with a JSON array body + Bearer token, for
  seeding server data (accounts, roles, emoji) that the UI cannot script
  directly. UI assertions still look at what the browser paints.
- Screenshots land in /tmp/cassette-e2e, outside the repo: the dev server
  watches the repo, so writing there reloads the app mid-test. Run a test by
  hand with `python3 -B`. ISSUES.md logs every glaring app
  problem found while running the suite — do not fix app code from here.

Tests
- test_auth      signup/login/logout round trip
- test_server    create server/channels, rename/delete, roles, members panel
- test_settings  user settings sections + server settings sections render
- test_two_users two tabs, live messages both ways, presence
- test_messages  reply, reaction add/remove, pin + pins panel, Markdown,
                 long-message wrap, jump-to-bottom, typing indicator
- test_dms_friends friend request/accept, live DMs, group DM (3 people),
                   block hides the other user's messages
- test_search    search finds the hit, clicking jumps and highlights
- test_permissions no edit controls without Manage Channels, private
                   channel hidden until role allowed, Send Messages denied
                   shows the composer no-permission bar
- test_notifications mute from channel right-click dims it, Only @mentions
                   stops toasts, 12+ unread shows the new-messages bar
- test_profile_status display name/about-me visible on the other user's
                   card, DND red dot cross-user, tab icon picker
Not written yet: test_emoji_gifs (custom emoji, pickers close with Escape),
test_phone (390x844 layout), test_ui_sweep (every menu/dialog, overlap checks).

Things that bite when writing a test
- Work out selectors with the idealyst MCP server (AGENTS.md, "Driving the
  app") before writing any; test_ids are only visible there, not in the DOM.
- The page carries hidden demo components with the same texts as real
  controls; only painted text counts (painted/text_pos), and bound x/y to the
  column you mean: rail 0-72, channel list 72-312, chat from 312. "general"
  also matches the "# general" channel header.
- Direct Messages is a full-screen overlay opened by the chat-bubble icon at
  the top of the rail (click (36, 36)); its list column is x 0-240.
- The server name in the sidebar header opens the server menu: press Escape
  after clicking it.
- `until()` needs a JS boolean: wrap element expressions as "!!" + expr, and
  use painted(...) (a JS string), not visible(...) (a coroutine).
- Emoji are two UTF-16 units in JS; match them by text, not length === 1.
- Give a freshly logged-in tab a few seconds before typing: frames sent
  before its gateway socket is up are dropped.
- Session.shot brings the tab to the front first; a background tab never
  answers Page.captureScreenshot.

Timeouts: a test may set `TIMEOUT = N` at top level (seconds) for run.py.
"""
