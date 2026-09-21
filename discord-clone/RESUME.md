# discord-clone — resume notes

A Discord-style layout on the Idealyst runtime (Rust → WebAssembly). Pure
platform-agnostic Rust; the entry point is `src/main.rs`, one
`idealyst::entry!(discord_clone)` line. The app is composed in `src/app.rs`;
the mock data lives in `src/data.rs`.

## Current state (working)

- **Theme / layout**: Discord's classic dark palette (`#1e1f22` rail,
  `#2b2d31` sidebar, `#313338` chat, `#404249` hover/active, blurple
  `#5865f2` accent). Proportions match Discord: 72px rail with 48px
  squircle server icons + white active-server pill (4x20, rail's left
  edge, `opacity`-toggled per active server); 240px sidebar with
  server-name header + chevron and 48px rounded channel rows (8px inset);
  240px member panel with right border; user profile on a fixed darker
  strip (`#232428`) at the sidebar bottom. (Commit `baf0a8f0`.)
- **Server rail** (left): 3 servers — Rust & Beers, Pixel Forge, Trailhead.
- **Channel sidebar** (left): per-server list, grouped into `Category`
  sections — an uppercase "TEXT CHANNELS" / "VOICE CHANNELS" header followed
  by its text-channel rows (`#name`) and voice rows. Text rows are
  interactive (select → main view rebuilds). Voice rows are a speaker glyph
  (`🔊 name`) + the connected members listed beneath (avatar + name, indented);
  presence only, not interactive. User profile pinned at the bottom (avatar
  with green Online dot, "you", "Online").
- **Main view** (middle): header (channel name + topic), message list (seed
  messages + any sent this session), composer. Each message shows a
  Discord-style time line above the body: "Today at 10:14 AM", "Yesterday at
  9:02 PM", or "m/d/yyyy" when older than a day (see timestamps below).
- **Message timestamps**: seed `Message`s store an `age_secs` offset (not a
  frozen Unix time); at render, `Message::ts_secs(now_secs())` resolves it
  against the live wall clock so "Today at …" stays true whenever the app is
  opened. `now_secs()` reads `runtime_shared::time::epoch_millis()` (the web
  backend installs a `js Date`-backed source). Day math is pure integer
  (`local_day_clock` + Hinnant `civil_ymd_from_day`); no `chrono`.
- **Voice-channel join** (commit `c7496cb6`): voice rows are interactive.
  Click one and "you" join — `joined_voice: Signal<Option<(u32, u32)>>`
  (server, channel). The row's speaker stack gains your avatar, and the
  profile strip turns into a green "Voice Connected <channel>" bar with a
  Leave pill. The sidebar `switch` key is
  `(active_server, active_channel, joined_voice)` so voice rows rebuild on
  join/leave; switching servers clears the join.
- **Signed-in user in the data model** (commit `c7496cb6`): `data::USER` is
  static identity (name + avatar color); live presence is app-level
  `user_status: Signal<Status>`. `Status::name()` (strip casing) vs
  `label()` (group headers), `next()` for cycling. The member panel groups
  "you" into the live-status group (rebuilds on status change); clicking
  the profile avatar cycles Online → Idle → DND → Online.
- **Member panel** (right, 240px): "MEMBER COUNT — N" header, then members
  grouped by status (ONLINE → IDLE → DO NOT DISTURB → OFFLINE), each group
  with a count. Rebuilds when the active server OR the user's status
  changes.
- **Composer**: type a message, send via the **Send button** OR the **Enter
  key**. Shift/Ctrl/Alt/Meta+Enter fall through to the platform default.
- **Mock login screen**: `app()` gates on `authed: Signal<bool>` (a root
  `switch`) — false → `login_screen(authed)`, true → the full app tree
  (renamed `app_shell`). The login screen is a centered card on the rail
  background: D logo, "Welcome back!", email + password fields (`text_input`,
  `secure = true` on the password), a Sign In `button`, and a "Continue as
  guest" `view.on_touch` pill. No backend: any non-empty email+password signs
  in (guest skips the fields); empty submit shows an inline red
  "Email and password are required." error that is reactive — the form is a
  nested `switch` keyed on `error` (see pitfall 8). A **Log Out** pill was
  added to `user_panel` (it now takes `authed`) to round-trip back to login;
  re-login resets the fields to empty. Verified in-browser (CDP input):
  render, masked password, empty→error, valid→app, Log Out→login, guest→app.
- **State**: `authed` (login gate, `bool`), `active_server`, `active_channel`,
  `composer` (text), `sent`
  (per-session log of `(channel_id, author, body, ts_secs)` — user-sent
  messages get a real `now_secs()` stamp), `revision` (bumped on every send
  so the message list re-renders), `joined_voice`
  (`Option<(server, channel)>`), `user_status` (`Status`).

## Build / run

```sh
# Dev (hot-reload, livereload server at http://127.0.0.1:8080/)
cd /root/idealyst-native/discord-clone
export PATH="$HOME/.cargo/bin:/root/idealyst-native/target/debug:$PATH"
idealyst dev --web --local        # log: /tmp/idealyst-dev.log

# Prod (serves at http://127.0.0.1:8010/)
idealyst build --web
```

`discord-clone/` is its own Cargo workspace (own `Cargo.toml`), detached from
the parent. Path deps on `/root/idealyst-native/crates/{idealyst,
runtime-core, runtime-vocabulary, runtime-scene}` — 100% idealyst-native, no
React or external frameworks.

## Repo conventions

- No AI-attribution trailers in commits.
- No `git stash`.
- `discord-clone/` is **tracked** in the parent repo (its own Cargo workspace,
  but committed under `/root/idealyst-native` — `git log -- discord-clone`
  works from the parent).

## Framework pitfalls (learned the hard way)

### 1. `switch` dedups on its scrutinee VALUE
`runtime_core::switch(key_fn, render)` rebuilds its subtree only when the
key's **value** changes. Re-setting a signal to its current value does NOT
trigger a rebuild (the equal scrutinee keeps the mounted arm). This bit us
when sending a message: the old "re-set `active_channel` to force a rebuild"
trick was a no-op. **Fix**: add a `revision: Signal<u32>` to the key tuple
and bump it in the send handler. The key is now
`(active_server, active_channel, revision)`.

### 2. `ui!` macro `text_input` doesn't expose `on_key_down`
The `ui!` macro's `text_input` prop list only supports `value`, `on_change`,
`placeholder`, `secure`. But the macro **forwards a method chain verbatim**
after the primitive, and `GlueTextInput::on_key_down` exists — so you can
write `text_input(…).on_key_down(on_enter)` and it works. The closure
signature is `|ev: &KeyEvent| -> KeyOutcome` (both re-exported from
`runtime_core`). Return `KeyOutcome::PreventDefault` to stop the input's
default Enter handling.

### 3. `Element` is not `Clone`
`runtime_scene::Element` is an enum over `Box<dyn Any>` — no `Clone` impl.
You can't build a subtree once and splice it into multiple `switch` branches.
Each branch must build its own instance (or pass in the data and call the
builder fn per branch). This is why `member_panel(server)` takes `&Server`
and is called inside the `switch` render closure.

### 4. Testing interactions from the browser: use CDP input, not DOM events
The rail / channel rows / composer are wired with `on_touch` (and the
`text_input` `on_key_down`), not `on_click`. A JS `element.click()` or a
synthetic `TouchEvent` dispatched via `dispatchEvent` does NOT reach the
runtime's touch handling, so the selection never updates. In the browser tool,
`click_at_xy(x, y)` (CDP-level `Input.dispatchMouseEvent`) is what actually
triggers the handlers: read the target's `getBoundingClientRect()` with
`js(...)` and pass the center to `click_at_xy`. The composer's Enter path is
reached by `fill_input(...)` then dispatching a real `keydown` `Enter`
`KeyboardEvent` (that one *does* fire `on_key_down`).

### 5. `opacity` is `Tokenized<f32>`, and absolute children anchor to the nearest *non-static* ancestor
`StyleRules.opacity` takes `Tokenized<f32>` — a bare `f32` is a type error;
wrap with `Tokenized::Literal(1.0)`. And an `Position::Absolute` child
anchors to the nearest positioned (non-Static) ancestor: an icon row that
is *supposed* to host an absolute pill must set `position: Relative`
itself, or the pill escapes to the viewport (rendered off-screen at
negative x). Both bit us on the rail's active-server pill.

### 6. `button(label, on_click)` has no style arg — but `ui!` lifts `style` onto any element
The vocabulary's `button` takes exactly two args (label + action); there's no
style *parameter*. However the `ui!` macro (crates/runtime/macros/src/jsx.rs)
pulls a `style = …` prop out of the prop list on ANY element and emits
`.with_style(…)` — so `button("Send", on_click, style = send_style)` compiles
and works (the composer's Send pill does exactly this). For a fully custom
pill you may still prefer `view(style=…).on_touch(…)` + child `text(…)`
(the voice-bar Leave button uses that for consistency with the row-click
idiom). Either is fine.

### 8. `ui!` trees are one-shot — reactivity only comes from `switch` keys
A `ui!` block evaluates its children (including `signal.get()` reads and
`if let` splices) ONCE, at build time. Setting a captured signal afterwards
does NOT re-run it — the live unit of reactivity is `switch(key, render)`.
Concrete bite: the login screen's inline error was a `let err_el = …error.get()`
spliced with `if let` — an empty submit set `error`, the button handler
ran, but the error text never appeared. Fix: wrap the form in
`switch(move || error.get(), move |err: &String| …)` and build the error
element inside the render closure. Corollary: `switch`'s render is `Fn`
(rebuilds many times), so it must not *move* captured non-`Copy` values —
build pre-made `Element`s (guest pill, error row) INSIDE the render closure
on each rebuild; `StyleRules` (which IS `Clone`) can be captured +
`.clone()`d per use (the sidebar idiom).

### 9. Stale dead `python http.server` on port 8000 from old sessions
Before serving, `ss -tlnp | grep 8000` — a dead (stuck, returns
ERR_EMPTY_RESPONSE) server from a previous session may hold the port and
make the fresh `http.server` exit with "Address already in use". Kill the
old pid (`ls -la /proc/<pid>/cwd` shows what it's serving).

## Files

- `src/app.rs` — the app: server rail, channel sidebar (with user profile),
  header, message list, composer, member panel. All the `ui!` trees and the
  `switch` closures live here.
- `src/data.rs` — mock data: 3 servers, `Category` groups (text + voice
  channels), messages (`age_secs` offsets), members (with `Status` enum:
  Online/Idle/DoNotDisturb/Offline). All `&'static`. Exposes `now_secs()`
  (runtime wall clock) for the composer's message stamps.
- `src/lib.rs` — crate root: `mod data; mod style_helpers; mod app;` + the
  SDK-handler registration seams (empty for this app).
- `src/main.rs` — entry point (`idealyst::entry!(discord_clone)`).
- `src/style_helpers.rs` — `px()`, `pct()`, `col()` tokenizers.

## Server side (discord-server/, branch discord-server-clone)

The backend lives in the sibling `../discord-server` crate (its own
`server` cargo feature; `src/db` + `migrations` + `src/bin/server.rs`).

- **WebSocket message gateway — done** (commit `1ac7f48d`):
  `#[server::channel] message_gateway` in `src/server_settings.rs`, mounted
  at `GET /_srv/_ws/message_gateway`. Send/receive only, **no presence**
  (separate follow-up). Wire types `WsClientMsg::{Send,History}` →
  `WsServerMsg::{Message,History,Error}`. Auth = the reserved
  `server::Headers` extractor reading `x-user-id` (the same stand-in
  identity the `#[server]` fns use) — the framework completes the 101
  handshake, then the handler's early `?` tears the session down if the
  header is missing. 3 e2e tests (raw tokio-tungstenite client, booted
  `server::router()` on an ephemeral port) pass; 15 total green.
  NOTE: `net::WebSocket::connect(url)` is URL-only (no header injection),
  so tests use a raw `tokio-tungstenite` client with `x-user-id` set on the
  upgrade request; read frames with `ws.next()` (StreamExt), not `ws.recv()`.
- **Orphaned / not yet wired**: `src/rate_limit.rs` exists in the tree but is
  NOT declared in `lib.rs` (not compiled) and its IP window limiter is not
  hooked into login yet. Separate concern.

## Not yet done

- **Presence** over the gateway — the explicitly deferred follow-up to the
  message gateway.
- Wire the orphaned `rate_limit.rs` (declare in `lib.rs`, hook into login).
- ~~Commit~~ — done (first commit `6670cb6d` added the app; `ef98615c` added
  categories/voice/timestamps). `discord-clone/` is now tracked in the parent.
- ~~Channel categories~~, ~~voice channels~~, ~~message timestamps~~ — done
  (see "Current state").
- ~~Voice-channel join~~ — done (commit `c7496cb6`, see "Current state").
- ~~Signed-in user as a data-model entry with editable status~~ — done
  (commit `c7496cb6`).
- ~~Mock login screen~~ — done (see "Current state"). Round-trips: login →
  app, Log Out → login, guest → app.
- Member-list interactions (mute / hide-activity / kick) — the one remaining
  item from the plan; lowest impact, mostly scaffolding.
