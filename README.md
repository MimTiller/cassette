# Cassette

A self-hosted, Discord-style chat app written entirely in Rust. Servers,
channels, real-time messaging, DMs, friends, roles and moderation, all in one
codebase that compiles to a WebAssembly client and a native API server.

Cassette is built on the [idealyst](https://github.com/IdealystIO/idealyst-native)
UI framework: there is no JavaScript framework and no hand-written HTTP layer.
The client calls the server through typed `#[server]` functions, and live
updates arrive over a WebSocket gateway.

## Features

**Accounts**
- Sign up and log in with a username and password
- Change your username or password, log out of every device at once
- Avatars, profile cards and online / idle / do-not-disturb status

**Servers and channels**
- Create a server, or join one with an invite link
- Text and voice channels, grouped into collapsible categories
- Server name and icon, ownership transfer
- Unread counts and mention badges per channel and server

**Messaging**
- Real-time messages with history, edit and delete
- Replies, emoji reactions, pinned messages
- Markdown, image attachments, a GIF picker (Giphy) and an emoji picker
- Typing indicators and date dividers
- `@user`, `@role`, `@everyone` and `@here` mentions with autocomplete;
  messages that mention you are highlighted

**People**
- Direct messages (with GIFs)
- Friends and friend requests
- Blocking, which hides a user's messages

**Roles and moderation**
- Permission levels: owner, admin, moderator, member
- Named roles with colours that tint member names and can be @mentioned
- Kick, ban and timeout

**Settings**
- Full-screen User Settings and Server Settings
- Dark, light and purple themes, plus an accent colour
- Browser notifications (all messages, mentions only, or nothing)
- Keyboard shortcuts (`?` lists them)

Voice channels show who has joined, but there is no audio yet.

## Repository layout

| Path | What it is |
|---|---|
| [`discord-clone/`](discord-clone) | The client: UI, styles and client state. Compiles to WASM. |
| [`discord-server/`](discord-server) | The backend: SQLite data layer, `#[server]` API functions, the WebSocket gateway and media storage. |
| [`discord-server/migrations/`](discord-server/migrations) | The database schema, applied in filename order on startup. |

The client depends on `discord-server` for the shared wire types. Built
without the `server` feature, its `#[server]` functions compile to typed RPC
calls; built with it, they are the real handlers. In production a single
server binary hosts both the API and the client bundle on one port.

For how the pieces fit together, see [ARCHITECTURE.md](ARCHITECTURE.md).
Before making changes, read [AGENTS.md](AGENTS.md), the house rules for
contributors (human or AI).

## Getting started

### Prerequisites

1. **Rust** (stable) via [rustup](https://rustup.rs/), with the WASM target:
   `rustup target add wasm32-unknown-unknown`
2. The **idealyst CLI** (3.x). See the
   [idealyst README](https://github.com/IdealystIO/idealyst-native#installing-the-cli).
3. **`wasm-bindgen-cli` 0.2.122**, to match the version pinned in
   `discord-clone/Cargo.toml`:
   `cargo install wasm-bindgen-cli --version 0.2.122`

The framework crates come from the idealyst registry, which is already
configured in [`.cargo/config.toml`](.cargo/config.toml). SQLite is compiled
in (`rusqlite` with `bundled`), so there is no database to install.

### Run it

From the repository root:

```bash
idealyst dev --web --local discord-clone
```

This builds the client and the server, applies the migrations, and serves
everything at <http://127.0.0.1:8080/> with hot reload. Sign up to create
the first account, then create a server.

Useful options: `--port <n>` changes the port, and `HOST=0.0.0.0` makes it
reachable from other machines on your network.

### Production build

```bash
cd discord-clone
idealyst build --web
```

## Configuration

All settings are read by the server:

| Setting | Purpose |
|---|---|
| `DISCORD_DB` | Path to the SQLite database. Default: `./discord.db`. |
| `MEDIA_DIR` | Where uploaded images are stored. Default: `media/` next to the database. |
| `DISCORD_SESSION_SECRET` | Hex key for signing session tokens. If unset, a random one is created in `<db>.session-secret` (mode 0600) and reused. |
| `GIPHY_API_KEY` | Enables GIF search. Alternatively, put the key in a `giphy.key` file next to the database. |
| `DISCORD_ALLOW_USER_ID_HEADER=1` | Accepts the legacy `x-user-id` header. Only for local test tooling; never enable it in production. |
| `PUBLIC_URL` | The address people use to reach the site, e.g. `https://chat.example.com`. Password reset links start with it. Default: `http://localhost:<PORT>`. |

The database, session secret, media folder and `giphy.key` are all
gitignored.

## Resetting a forgotten password

There is no email, so whoever runs the server creates a reset link and
passes it on. With the same settings as the running server:

```bash
server reset-link <username>
```

It prints a link that works once, for 24 hours. Opening it shows a form for
choosing a new password; using it logs the account out everywhere and lifts
any failed-login lockout. Creating a new link cancels the previous one.

## Security

- Passwords are hashed with PBKDF2-HMAC-SHA256 (600,000 iterations).
- Sessions use HMAC-signed tokens in an httpOnly cookie, valid for 12 hours.
  Changing your password or logging out everywhere revokes every session.
- Logins, sign-ups and invite redemptions are rate-limited, and accounts
  lock briefly after repeated failed logins.
- Every permission check happens on the server.

[`discord-clone/README.md`](discord-clone/README.md) covers the token format,
revocation and throttling limits in detail.

## Tests

```bash
cd discord-server && cargo test --features server   # data layer, permissions, gateway round-trip
cd discord-clone && cargo test                      # client logic
```

## License

The crates are declared `MIT OR Apache-2.0` in their `Cargo.toml` files.
