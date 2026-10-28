# Architecture

How Cassette fits together. Read this before changing anything that crosses
the client/server line. House rules for making changes are in
[AGENTS.md](AGENTS.md).

## The big picture

```
 browser (WASM)                                   server binary (native)
┌──────────────────────────┐   POST /_srv/<fn>   ┌───────────────────────────────┐
│ discord-clone            │ ──────────────────▶ │ #[server] fn                  │
│  app_shell + sections    │ ◀────────────────── │   └─ XService (permissions)   │
│  signals (ShellState)    │      JSON reply     │        └─ Db (SQLite)         │
│                          │                     │                               │
│  gateway_effects  ◀──────┼──── WebSocket ──────┼── realtime::hub().publish()   │
└──────────────────────────┘   WsServerMsg frames└───────────────────────────────┘
```

- **One process serves everything:** the server binary
  (`discord-server/src/bin/server.rs`) hosts the API at `/_srv/*`, the
  WebSocket gateway, uploaded media at `/media`, and the client bundle at `/`.
- **One source for the wire types:** `discord-clone` depends on
  `discord-server` without the `server` feature. In that build each
  `#[server]` function compiles to a typed RPC call; with
  `--features server` it is the real handler. Input and output structs are
  shared, so the client and server can't disagree about a request's shape.

## Server (`discord-server`)

### Layers

Each feature module (`auth`, `servers`, `server_settings`, `server_roles`,
`dms`, `friends`, `blocks`, `invites`, `profiles`, `media`, `gifs`) follows
the same three layers:

1. **`#[server]` functions.** These are thin. They get the database with
   `db_from_state()`, the caller with `crate::session::acting_user()`, call
   the service, and broadcast any change. They return
   `Result<T, ServerError>`; a refusal is `ServerError::failed("message")`,
   and the client shows that message to the user verbatim.
2. **A service** (`RoleService`, `InviteService`, …). This holds the rules:
   every permission check happens here, in plain Rust that takes `caller: i64`.
   That makes it unit-testable without HTTP.
3. **`db::Db`** (`src/db/mod.rs`). These are typed queries over one SQLite
   connection behind a mutex. Some newer modules (`server_roles`) keep their
   SQL in the module itself and take the connection with `db.lock()`.

Everything behind `#[cfg(feature = "server")]` is compiled only into the
server, so the WASM client never links SQLite.

### Permissions

`src/roles.rs` defines four levels (`Owner > Admin > Moderator > Member`,
stored in `server_memberships.role`) and an `Action` enum. Each action has a
minimum level (`Action::minimum_role`), and services check with
`role.can(Action::X)`. Named roles (`server_roles.rs`: colour, mentionable)
are identity and mention groups only; they grant no permissions.

### Database and migrations

- The schema lives entirely in `discord-server/migrations/NNNN_name.sql`.
  Files run once each, in filename order, at startup, and are recorded so
  they never re-run.
- Foreign keys are on. Membership-owned rows clean themselves up through
  `ON DELETE CASCADE` or triggers (for example `member_roles_on_leave`).
- Timestamps are UTC epoch milliseconds (`INTEGER`).
- Never edit a migration that has shipped; add a new one.

### Realtime

`src/realtime.rs` runs the gateway. Each connected browser is registered in
`hub()` under its user id.

- **Client to server:** `WsClientMsg` covers sending a message, typing,
  asking for history, and so on.
- **Server to client:** `WsServerMsg` covers new, edited and deleted
  messages, reactions, presence, `RolesChanged`, and so on. Both enums live
  in `server_settings.rs`.
- **Sending a frame:**
  `hub().publish(&audience, &WsServerMsg::X { .. })`, where the audience is
  usually `server_audience(db, server_id)` or a DM's two members.
- **When to broadcast:** a `#[server]` function that changes data other
  people can see should publish a frame, so open clients update without
  polling.

### Sessions and security

`session.rs` verifies the signed `v2.` token, from the cookie or a bearer
header, and checks it against `users.token_version` for revocation.
`rate_limit.rs` throttles logins, sign-ups and invites. Details are in
[discord-clone/README.md](discord-clone/README.md).

### Media

`media.rs` stores uploads by content hash under `MEDIA_DIR` (default
`media/` next to the database) and serves them at `/media/<sha256>.<ext>`. The
database stores only the path.

## Client (`discord-clone`)

### Startup

`app()` (in `src/app.rs`):
1. Installs the theme.
2. Restores a saved session.
3. Shows `login_screen` until `authed` is true.
4. Then builds `app_shell`.

### `app_shell` and its sections

`app_shell` declares the app's shared state, which is signals plus a few
`Rc<RefCell<…>>` buffers. It gathers them into one `ShellState`
(`src/app/shell_state.rs`) and hands that to section builders in `src/app/`:

| Section | File | Owns |
|---|---|---|
| Gateway | `gateway_effects.rs` | Applying incoming `WsServerMsg` frames to state |
| Server rail | `rail.rs` | The left column of server icons |
| Channel sidebar | `sidebar.rs` | Channels, categories, the server menu |
| Messages | `messages.rs`, `message_view.rs` | Header, message list, one message row |
| Composer | `composer.rs` | The input, `@` autocomplete, attachments |
| Members | `member_list.rs`, `member_panels.rs` | The member list and moderation menus |
| Dialogs | `modals.rs` | Create/join server, channel dialogs, Server Settings |
| Full-screen overlays | `overlays.rs` | User Settings, DMs, friends |
| Popups | `popups.rs`, `user_popups.rs` | Menus, profile cards, Escape handling |

Each builder starts with `let ShellState { a, b, .. } = s.clone();` and
returns a `XParts` struct of elements, which `app_shell` places into the
layout.

**Adding shared state:**
1. Declare the signal in `app_shell`.
2. Add a field to `ShellState`.
3. Add the `x: x.clone()` line where `ShellState` is built.

### Feature modules

Logic that isn't a screen section lives in `src/*.rs`:
- `auth` (session storage)
- `servers`, `dms`, `friends`, `blocks`, `pins`, `reactions`, `invite` (wrappers around server calls)
- `notify` (browser notifications)
- `mentions` (the active server's members and roles, for mention rendering)
- `theme`
- `errors`

Reusable widgets live in `src/components/`.

### UI idioms (idealyst)

- **`ui! { … }`** builds elements. **`signal(x)`** makes reactive state.
  `Signal` is `Copy`, so `move` closures each capture their own copy.
- **`switch(key_fn, render_fn)`** rebuilds a subtree whenever the key changes.
  Anything the subtree depends on must be part of the key.
- **Dialogs and overlays are built once and hidden**, by collapsing to zero
  size, rather than created on open. Anything that must be fresh when they
  open goes in a `switch` keyed on the open flag.
- **Server calls from UI** run in `runtime_core::driver::spawn_async(async move { … })`.
- **Errors:** show failures with `crate::errors::friendly(&e)`.

### Theme

`src/theme.rs` defines 30 colour roles (`BG_PRIMARY`, `TEXT_MUTED`,
`ACCENT`, …) and three palettes (Dark, Light, Purple) over those roles.

- **Themed colours:** `col(theme::ROLE)`, which follows the active theme.
- **Icon colours:** `theme::live(theme::ROLE)`.
- **Data colours** (a role's colour, a swatch): `theme::fixed("#hex")`.

## Testing

- **Server:** `cargo test --features server` in `discord-server/`. This runs
  unit tests on services and pure functions, `db_tests` against a
  temporary database, and a real WebSocket round-trip against a booted
  router.
- **Client:** `cargo test` in `discord-clone/`, which covers pure logic only.
- **UI:** checked in a real browser against a dev server (see AGENTS.md).

## Running it

See the [README](README.md#getting-started). The live instance runs
`idealyst dev` under systemd. The section on deploying in AGENTS.md has
the restart routine.
