# Self-hosting RWND

One server process, one SQLite file, one web bundle. Everything below is what
the repo actually does today; the in-app configuration screens (Instance
Settings and the first-run wizard) are landing in parallel.

## Quick install

```bash
sh deploy/install.sh   # with --repo or from a checkout, it builds the app too
```

With a repo and no `--web-dist`, install.sh runs `idealyst build --web
discord-clone` itself (needs the idealyst CLI, see Requirements) and installs
the bundle into `$PREFIX/web`, so the server it starts serves the app at
`/`. `--web-dist <dir>` takes a bundle you built already, e.g.
`discord-clone/dist/web`.

[`deploy/install.sh`](../deploy/README.md) sets all of this up in one run:
it asks where to install, for the public URL, and the port (or takes
`--public-url/--port/--yes` for non-interactive use),
generates the session secret, writes the env file, and installs
`rwnd.service` as a `systemd --user` unit — no root needed. Voice and video run
on idea-chat's call relay, a separate QUIC/WebTransport service you start
alongside the server; the installer does not set it up yet, so follow the
"Voice and video" section below for it. Read on for what everything means, and
see [deploy/README.md](../deploy/README.md) for the flag table.

## Requirements

- Rust (stable) with the WASM target: `rustup target add wasm32-unknown-unknown`
- The idealyst CLI (1.20 or newer) — see the
  [idealyst README](https://github.com/IdealystIO/idealyst-native#installing-the-cli)
- `wasm-bindgen-cli` 0.2.122, matching the pin in `discord-clone/Cargo.toml`:
  `cargo install wasm-bindgen-cli --version 0.2.122`
- For voice and video: the idea-chat call relay (see below). No other runtime
  dependencies — SQLite is compiled in (`rusqlite`, bundled).

## Building

Development, one command from the repo root (client + server + migrations,
hot reload at 127.0.0.1:8080):

```bash
idealyst dev --web --local discord-clone
```

Production:

```bash
# the web bundle (wasm + JS glue)
cd discord-clone && idealyst build --web

# the server binary (also serves the API, the WebSocket gateway,
# /media, and the bundle when WEB_DIST points at it)
cd discord-server && cargo build --release --features server --bin server
```

Run it as `target/release/server` with the env below; setting
`WEB_DIST` to the built bundle's dist directory makes the binary serve the
client at `/` and its `pkg/` at `/pkg`, so API and app share one origin.

## Environment

Every variable the server reads (verified against `discord-server/src`):

| Variable | Default | What it does |
|---|---|---|
| `HOST` | `127.0.0.1` | Address to bind. `0.0.0.0` exposes it on your LAN. |
| `PORT` | `3000` | Port to serve on. |
| `DISCORD_DB` | `./discord.db` | Path to the SQLite database (created and migrated on first start). |
| `MEDIA_DIR` | `media/` next to the database | Where uploaded images are stored. |
| `DISCORD_SESSION_SECRET` | — | Hex key for signing session tokens. If unset, the server generates one and persists it in `<db>.session-secret` (mode 0600) and reuses it on restart. |
| `PUBLIC_URL` | `http://localhost:<PORT>` | The address people use to reach the site, e.g. `https://chat.example.com`. Password-reset links start with it. |
| `TRUSTED_PROXIES` | unset | Comma-separated IPs of reverse proxies whose `X-Forwarded-For` you trust (e.g. `127.0.0.1,::1` for Caddy on the same host), so per-IP rate limits still apply. Leave unset when the server is reached directly. |
| `GIPHY_API_KEY` | — | Enables GIF search. Alternative: a `giphy.key` file next to the database. |
| `DISCORD_ALLOW_USER_ID_HEADER` | off | `=1` accepts the legacy `x-user-id` header. Local test tooling only — never in production. |
| `WEB_DIST` | unset | Directory of the built web bundle; when set, the binary serves the client at `/`. Set by `idealyst dev --web` / `idealyst run server`. |
| `DISCORD_MIGRATIONS` | crate's `migrations/` | Override for the migrations directory; for local tooling. |

The database, session secret, media folder and `giphy.key` are gitignored;
keep them that way.

`server reset-link <username>` (same env as the running server) prints a one-time
password-reset link valid for 24 hours.

## Voice and video: the idea-chat call relay

Calls run on idea-chat's call relay: a QUIC/WebTransport service from the
[idea-chat](https://github.com/IdealystIO/idea-chat) repo, separate from the
RWND server. Each client's media is end-to-end encrypted, so the relay only
forwards datagrams it cannot read. Build it from an idea-chat checkout:

```bash
cd idea-chat && cargo build --release -p call-relay   # target/release/call-relay
```

Its usage (from `crates/call-relay/src/main.rs`):

```text
call-relay [ADDR:PORT] [--cert FULLCHAIN.pem --key PRIVKEY.pem] [--host NAME]
           [--secret-file FILE] [--max-ttl SECS] [--grace SECS] [--throttle on|off]
call-relay mint    --secret-file FILE --room R --user U [--send avs] [--ttl SECS]
call-relay revoke  --secret-file FILE --relay URL [--hash HEX] --user U [--room R] [--send CODE]
```

Defaults: it listens on `127.0.0.1:4433`, grants mint with a 120 s ttl
unless `--ttl` says otherwise, the relay accepts grants up to `--max-ttl`
(default 600 s), and a lapsed grant stops a client's traffic after `--grace`
seconds (default 15).

**Identity and TLS.** With `--cert FULLCHAIN.pem --key PRIVKEY.pem` the relay
serves a CA-issued certificate and clients just dial
`https://<host>:<port>/` — no pinning. Without them it mints a self-signed
certificate and prints a cert hash clients must pin: a *new* hash on every
start, so restarting the relay means re-pinning (re-run and copy the printed
URL). Put a real CA cert in place for anything you run long-term.

**Membership.** With `--secret-file` (the file must hold at least 16 random
bytes; 32 or more is recommended — the relay refuses shorter secrets), the
relay checks `call-auth` grants: nobody joins without one, and nobody sends
more than their grant allows. The issuer mints grants with the same secret —
`call-relay mint` prints one; `call-relay revoke` delivers a revocation to a
running relay (a kick, or `--send` a limit such as listen-only). Grants are
short by design: a client renews its grant before it expires, and an issuer
that stops issuing has removed someone within one grant's lifetime.

**One port, no proxy.** The relay's connection is QUIC/WebTransport, not
HTTP/WebSocket: it cannot sit behind a reverse proxy or a tunnel at all.
Whatever fronts the app (the Caddy/nginx setup below) must leave the relay
reachable *directly* — the client dials its public `https://host:port/` URL.
There is no TURN fallback any more, and no Cloud option: the relay you run is
the relay you use.

The RWND app's env for this provider is still being implemented and is not
read by `discord-server` yet; expect `CALL_PROVIDER` and `IDEA_CHAT_RELAY_*`
(the relay's `https://` URL and its secret file) once the voice layer lands.
Until then, treat the names above as provisional.

## HTTPS

Two mechanisms sit behind https for calls, and both are worth knowing rather
than just configuring: browsers gate `getUserMedia` (the mic and camera) to
`https://` pages or `localhost`, and the relay is dialed as an `https://`
WebTransport URL regardless of what the page is on — its TLS identity is
what makes the connection trustworthy, not the page's. So an https app page
and a plain `127.0.0.1` LAN relay work on localhost pages, and over a real
domain both sides need real https.

Caddy is the short path: it fetches certificates itself (ACME) when the
domains' DNS points at the machine, and upgrades WebSockets automatically.
[`deploy/Caddyfile.example`](../deploy/Caddyfile.example) is a validated
(`caddy validate --config deploy/Caddyfile.example --adapter caddyfile`)
setup for the app; the relay is deliberately not in it — its QUIC
connection cannot be proxied, only dialed directly. With it, set
`PUBLIC_URL=https://your.domain`.

nginx needs the WebSocket upgrade stanza explicitly for the app:

```nginx
map $http_upgrade $connection_upgrade {
  default upgrade;
  ''      close;
}
server {
  # ssl_cert/ssl_pem or certbot; Caddy gets certs itself, nginx doesn't.
  location / {
    proxy_pass http://127.0.0.1:3000;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection $connection_upgrade;
  }
}
```

Two env settings complete the picture on any proxy: `TRUSTED_PROXIES` — the
proxy's IP(s), so the server trusts its `X-Forwarded-For` for per-IP rate
limits (`127.0.0.1,::1` when Caddy/nginx run on the same machine) — and
`PUBLIC_URL=https://your.domain`, the address people actually use.

## Reverse proxies and tunnels

Whatever sits in front must upgrade WebSockets — the chat gateway is one
WebSocket connection to the RWND server. (The call relay is not HTTP at all:
see the "no proxy" note above; it gets no vhost and no stanza.)

Caddy (it upgrades WebSockets automatically, and does TLS):

```
chat.example.com {
  reverse_proxy 127.0.0.1:3000
}
```

Caddy gets the certificate itself and sets `X-Forwarded-For`; set
`TRUSTED_PROXIES=127.0.0.1,::1` when it runs on the same host.

nginx:

```nginx
map $http_upgrade $connection_upgrade {
  default upgrade;
  ''      close;
}
server {
  # ... server_name, ssl_cert/ssl_pem (or use certbot) ...
  location / {
    proxy_pass http://127.0.0.1:3000;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection $connection_upgrade;
    proxy_set_header X-Forwarded-For $remote_addr;
  }
}
```

Then set `TRUSTED_PROXIES` to the proxy's IP(s) — the server only believes
`X-Forwarded-For` from addresses in that list — and set `PUBLIC_URL` to the
public `https://` address.

Cloudflare tunnels: a `trycloudflare.com` URL carries HTTP and wss, so the
app page and the chat gateway both work through it, and text chat over a
tunnel is fine. Calls do not: the relay's QUIC/WebTransport connection
cannot go through a tunnel. To use calls while the app is on a tunnel, dial
a relay that sits on its own directly reachable host:port (e.g. a CA-cert
relay on the machine itself, `https://<ip>:4433/`).

## Picking the provider

- **Voice and video:** the idea-chat call relay, started and pinned per the
  section above. It is not wired into this repo's server yet — env names are
  provisional, see the note there.
- **Later:** the same choice from inside the app — Voice & Video and Chat
  Backend pages plus a first-run setup wizard, coming in the Instance
  Settings area. Until those land, env is the only way to configure.

## Chat backend: idea-chat (experimental)

[idea-chat](https://github.com/IdealystIO/idea-chat) is a chat + calling
backend on the same idealyst framework: accounts, spaces, channels and
messages over one live connection, with end-to-end encrypted calls. RWND is
becoming its front end. The chat backend can point at an idea-chat server
instead of the bundled one; the wire protocol — routes, auth, frame shapes,
the schema gate — is worked out in [idea-chat-wire.md](idea-chat-wire.md).
This is experimental and still in progress; treat the bundled `discord-server`
as the stable choice today.
