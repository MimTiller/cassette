# discord-clone — resume notes

A Discord-style layout on the Idealyst runtime (Rust → WebAssembly). Pure
platform-agnostic Rust; the entry point is `src/main.rs`, one
`idealyst::entry!(discord_clone)` line. The app is composed in `src/app.rs`;
the mock data lives in `src/data.rs`.

## Current state (working)

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
- **Member panel** (right, 240px): "MEMBER COUNT — N" header, then members
  grouped by status (ONLINE → IDLE → DO NOT DISTURB → OFFLINE), each group
  with a count. Rebuilds when the active server changes.
- **Composer**: type a message, send via the **Send button** OR the **Enter
  key**. Shift/Ctrl/Alt/Meta+Enter fall through to the platform default.
- **State**: `active_server`, `active_channel`, `composer` (text), `sent`
  (per-session log of `(channel_id, author, body, ts_secs)` — user-sent
  messages get a real `now_secs()` stamp), `revision` (bumped on every send
  so the message list re-renders).

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

## Not yet done

- ~~Commit~~ — done (first commit `6670cb6d` added the app; `ef98615c` added
  categories/voice/timestamps). `discord-clone/` is now tracked in the parent.
- ~~Channel categories~~, ~~voice channels~~, ~~message timestamps~~ — done
  (see "Current state").
- Member-list interactions (mute / hide-activity / kick).
- User profile as a data-model entry (currently a hardcoded "you / Online"
  row); wire it into `data.rs` for future interactivity (e.g. a real
  `you` `Member` with editable status).
- Voice-channel interaction: clicking a voice row currently does nothing;
  Discord joins the channel + shows a "voice connection" bar in the user
  profile area. Could be modeled as a `joined_voice: Signal<Option<u32>>`.
