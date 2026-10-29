<p align="center">
  <img src="discord-clone/icons/sunset.svg" width="112" alt="Cassette logo: a black cassette tape with an orange label">
</p>

# Cassette

Discord, but it lives on your own box.

Cassette is a group chat you host yourself. If you've used Discord you
already know your way around: servers down the left, channels next to them,
the conversation in the middle, and everyone who's online on the right. The
difference is where it runs. The whole thing is one server process and one
SQLite file, on a machine you own, and the only admin is you.

It's Rust from end to end. The browser client is compiled to WebAssembly with
the [idealyst](https://github.com/IdealystIO/idealyst-native) UI framework,
so there's no JavaScript framework underneath. Client and server share their
types and talk through plain Rust function calls, which means there's no
REST API for the two sides to drift apart on. Anything live, like new
messages, typing and who's online, comes in over a WebSocket.

## What's on the tape

**Talking**
- Messages land the moment they're sent, and you can see when someone's
  typing. Edit them, delete them, reply to them, react to them, pin the
  good ones
- Markdown, image uploads, an emoji picker, and a GIF picker (bring your own
  Giphy key)
- Your server's own custom emoji, used as `:name:` in messages and
  reactions. Rename one and every old message picks up the new name
- Paste a link and a preview card shows up with the page's title, blurb and
  image
- `@someone`, `@role`, `@everyone` and `@here`, with autocomplete. Messages
  that mention you light up
- Search a server's whole history and jump straight to the hit
- Come back to a busy channel and a "new messages since…" bar takes you to
  where you left off

**Running a server**
- Start one, share an invite link, and people are in
- Text and voice channels, sorted into categories you can fold away
- Roles with colours and 16 separate permissions, plus per-channel overrides
- Private channels, hidden from everyone you haven't let in
- Kick, ban and time out. You can only do that to people ranked below you,
  so moderators can't gang up on the owner

**You**
- A display name, avatar and "about me"
- Online, Idle, Do Not Disturb or Invisible
- Friends, DMs and group DMs, plus a block button for when you need it
- Browser notifications that you control per server and per channel: ping
  for everything, only @mentions, or nothing, or mute it for 15 minutes, a
  day, or until you say so
- Dark, light and purple themes, and a choice of accent colours
- Keyboard shortcuts. Press `?` to see them

Voice channels show who's hanging out in them, but there's no sound yet.
That's the last track on the tape.

## What's inside

| Path | What it is |
|---|---|
| [`discord-clone/`](discord-clone) | The client: every screen, style and bit of client state. Compiles to WASM. |
| [`discord-server/`](discord-server) | The backend: the SQLite data layer, the API functions, the WebSocket gateway and image storage. |
| [`discord-server/migrations/`](discord-server/migrations) | The database schema. Each file runs once, in order, when the server starts. |

The client pulls in `discord-server` for the types they share. Each
`#[server]` function is written once: in the client build it turns into a
typed network call, and in the server build it's the real handler. In
production a single binary serves the API and the client on one port.

[ARCHITECTURE.md](ARCHITECTURE.md) has the full tour. If you're about to
change something, read [AGENTS.md](AGENTS.md) first. It's the house rules,
for people and AI agents alike.

## Getting started

### Prerequisites

1. **Rust** (stable) via [rustup](https://rustup.rs/), with the WASM target:
   `rustup target add wasm32-unknown-unknown`
2. The **idealyst CLI** (1.20 or newer). See the
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

It's your server, so it should be hard to break into.

- Passwords are hashed with PBKDF2-HMAC-SHA256 at 600,000 iterations.
- You stay logged in with a signed token kept in an httpOnly cookie, good
  for 12 hours. Changing your password or hitting "Log out of all devices"
  kills every session at once.
- Logins, sign-ups and invite links are rate-limited, and an account locks
  for a bit after too many wrong passwords.
- The server checks every permission itself and never takes the client's
  word for it.
- Link previews won't fetch anything on your own network, so nobody can
  post a link that gets the server to go poking at your router.

The token format, revocation and the exact throttling limits are in
[`discord-clone/README.md`](discord-clone/README.md).

## Tests

```bash
cd discord-server && cargo test --features server   # data layer, permissions, gateway round-trip
cd discord-clone && cargo test                      # client logic
```

## License

MIT or Apache-2.0, whichever you prefer. Both are declared in each crate's
`Cargo.toml`.
