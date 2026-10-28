# Discord Clone

A full-stack Discord-style chat app — real auth, servers/channels, live
WebSocket messaging, and DMs — built entirely on the
[idealyst-native](../README.md) Rust UI framework as its flagship example of
a full-stack idealyst project. One Rust codebase compiles to a WASM client
*and* a native API server; no React, no JS framework, no hand-written HTTP.

## What's here

Two standalone crates (each its own detached Cargo workspace, not members of
the parent repo's workspace):

| Crate | Role |
|---|---|
| [`discord-clone/`](.) | The client: UI tree, styles, local state. Compiles to WASM for the browser. |
| [`discord-server/`](../discord-server) | The backend: SQLite-backed API (`#[server]` fns) + WebSocket message gateway, built as a single binary that also serves the client's static WASM bundle. |

They share wire types (`discord-server` is a dependency of `discord-clone`,
built with default/client features so its `#[server]` functions compile down
to typed RPC stub calls instead of real server bodies).

## Features

**Live / persistent** (backed by real SQLite data, not mock):
- Email/password signup + login (PBKDF2-HMAC-SHA256, salted)
- Servers, channels, categories, server membership
- Real-time channel messaging and DMs over a WebSocket gateway, with
  message history persisted across reconnects/restarts
- Server invites, roles, channel read-state
- Moderation actions, message mentions

**UI-complete, backed by mock data** (not yet wired to the live backend):
- Voice channel join/leave UI (presence-only, no real audio)
- Some member-list interactions (mute / hide-activity / kick are scaffolded,
  low priority)

See [`discord-clone/RESUME.md`](RESUME.md) for a detailed, commit-by-commit
engineering log of what's implemented and the framework pitfalls hit along
the way — useful if you're picking up development on this project.

## Prerequisites

1. **Rust** stable (1.78+), via [rustup](https://rustup.rs/).
2. The **`idealyst` CLI**, built from source:

   ```bash
   export CARGO_REGISTRIES_IDEALYST_INDEX=sparse+https://crates.idealyst.io/index/
   cargo install --git https://github.com/IdealystIO/idealyst-native idealyst-cli
   ```

   See the [idealyst-native README](../README.md#installing-the-cli) for
   details (this env var is required, not optional — cargo needs it to
   resolve the framework's own crates).
3. `wasm-bindgen-cli` matching the pinned version in both crates' `Cargo.toml`
   (currently `0.2.122`) — `idealyst build --web` checks this and fails loudly
   on a mismatch.

No database setup needed: `discord-server` uses `rusqlite` with the
`bundled` feature, so SQLite is compiled in — no system SQLite, no external
DB service.

## Running it

**Full-stack dev mode** (hot-reload client + live API server, one command):

```bash
cd discord-clone
idealyst dev --web --local
```

This builds and runs `discord-server`'s `server` binary alongside the client
bundle and serves both the UI and the `/_srv/*` API from one origin at
`http://127.0.0.1:8080/`. SQL migrations under `discord-server/migrations/`
run automatically against the server's SQLite file on startup.

**Production build:**

```bash
cd discord-clone
idealyst build --web
```

Serves the static bundle + API from `http://127.0.0.1:8010/`.

## Configuration

Server-side environment variables and files (all read by `discord-server`):

- `DISCORD_DB` — path to the SQLite database file (default `./discord.db`).
- `DISCORD_SESSION_SECRET` — hex signing secret for session tokens. If
  unset, the server creates `<db>.session-secret` (e.g. `discord.db.session-secret`)
  beside the database with a fresh random secret (mode 0600 on unix);
  tokens survive restarts as long as that file does. Gitignored.
- `DISCORD_ALLOW_USER_ID_HEADER=1` — opt back in to the legacy `x-user-id`
  header session (local dev/test tooling only; rejected by default).
- `GIPHY_API_KEY` — the Giphy API key for GIF search, kept server-side
  (the client never sees it). Alternatively, the key can live in a
  gitignored `giphy.key` file next to the database file (`DISCORD_DB`);
  the env var wins if both are present. Without a key, GIF search returns
  "GIF search isn't set up yet (no Giphy key)".

## Architecture notes

- `discord-server`'s `server` binary is both the API host (axum) **and** the
  static-file server for the client's WASM bundle — one process, one port.
- The `#[server]` macro in the shared `server` SDK crate keys off a Cargo
  feature: the client build (no `server` feature) gets typed RPC stub
  functions that POST to the backend; the server build (`--features server`)
  gets the real implementations plus axum route registration. Same source,
  two compiled shapes.
- Auth uses HMAC-signed session tokens. `login` returns a token formatted
  `v1.<user_id>.<expires_unix_secs>.<hex hmac-sha256>` (the MAC covers the
  `v1.<user_id>.<expires>` prefix, keyed with the server's session secret)
  and valid for 12 hours, and also sets it as an httpOnly `session` cookie
  (SameSite=Lax; `Secure` only when the request came over https). The
  browser replays that cookie on every later request — and on the
  WebSocket upgrade handshake, where JS cannot read it — so the web client
  never holds the token itself; native clients instead send it back as
  `Authorization: Bearer <token>`. Identity comes only from a token whose
  signature verifies against the server secret, never from a client-supplied
  user id.
- The legacy `x-user-id` header (the old stand-in session) is REJECTED by
  default. Set `DISCORD_ALLOW_USER_ID_HEADER=1` to accept it for local
  development or test tooling; a request that presents a bearer token never
  falls back to the legacy header — a bad token is simply unauthenticated.
- Login throttling (server-side, per account): after 5 failed logins within
  a trailing 300-second window the account is locked and even the correct
  password is rejected until the window slides past the oldest failure; a
  successful login clears the failure log. Wrong-password errors are
  uniform ("invalid username or password") so the endpoint never leaks which
  usernames are registered.
- Sessions are revocable: `logout` clears the session cookie (deleted with
  the same flags), tokens expire on their 12-hour TTL, and the signing
  secret — from `DISCORD_SESSION_SECRET` or a mode-0600 `<db>.session-secret`
  file beside the database — can be rotated to invalidate every issued
  token at once.
- Data model: users, servers, channels, categories, messages, DM channels,
  server memberships, roles, invites, mentions, read-state — see
  `discord-server/migrations/` for the authoritative schema.

## Testing

`discord-server` has an in-crate test suite (`cargo test --features server`
from `discord-server/`) covering the data layer and a real end-to-end
WebSocket round-trip against a booted `server::router()` on an ephemeral
port (via `tokio-tungstenite`, since the framework's own `net::WebSocket`
client is URL-only and can't set the auth header the tests need).
