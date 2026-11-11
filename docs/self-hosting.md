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
`--public-url/--port/--with-livekit/--yes` for non-interactive use),
generates the session secret, writes the env file, and installs
`rwnd.service` as a `systemd --user` unit — no root needed. With
`--with-livekit` it also downloads the pinned LiveKit release
(sha256-verified), generates keys, writes `livekit.yaml`, installs
`rwnd-livekit.service`, and puts `CALL_PROVIDER`/`LIVEKIT_*` in the env
file. Read on for what everything means, and see
[deploy/README.md](../deploy/README.md) for the flag table.

## Requirements

- Rust (stable) with the WASM target: `rustup target add wasm32-unknown-unknown`
- The idealyst CLI (1.20 or newer) — see the
  [idealyst README](https://github.com/IdealystIO/idealyst-native#installing-the-cli)
- `wasm-bindgen-cli` 0.2.122, matching the pin in `discord-clone/Cargo.toml`:
  `cargo install wasm-bindgen-cli --version 0.2.122`
- For voice: the single-binary LiveKit server (see below). No other runtime
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

## Voice: LiveKit

Voice calls run on [LiveKit](https://github.com/livekit/livekit): one Go
binary, its own API key/secret pair.

```bash
# grab the binary (linux_amd64 shown)
tar xzf livekit_1.13.11_linux_amd64.tar.gz   # or download from LiveKit's releases
./livekit-server generate-keys               # prints "API Key: ..." and "API Secret: ..."
```

Minimal `livekit.yaml` — port `7880` is the HTTP/WebSocket service,
`7881` the ICE/TCP fallback port, `7882` a single ICE/UDP port instead of the
default `50000-60000` range (pinning one range makes forwarding far easier):

```yaml
port: 7880
bind_addresses:
  - 0.0.0.0
rtc:
  tcp_port: 7881
  udp_port: 7882
  use_external_ip: false
  node_ip: <your public or LAN IP>   # the IP LiveKit advertises to clients
keys:
  <generated-key>: <generated-secret>
```

Run with `livekit-server --config livekit.yaml`; `livekit-server ports`
prints what it actually bound.

Ports you must expose for voice to work:

| Port | Traffic |
|---|---|
| `7880` | WebSocket signalling (wss) — always needed |
| `7882` (or the `50000-60000` range) | ICE/UDP — the actual audio/video |
| `7881` | ICE/TCP fallback — only if UDP is blocked |

`use_external_ip: false` with `node_ip` set is what you want when a proxy or
port-forwarding sits in front; `true` advertises the server's own detected IP.

The RWND app's env for this provider (set in the app later, or via env now):

| Variable | Meaning |
|---|---|
| `CALL_PROVIDER=livekit` | Selects the LiveKit backend |
| `LIVEKIT_URL` | e.g. `wss://yourhost:7880` |
| `LIVEKIT_API_KEY` / `LIVEKIT_API_SECRET` | The pair from `generate-keys` |

Keep the secret out of the repo and out of URLs; it belongs in the process
environment or a gitignored file.

### When TURN is needed, and how

TURN (relay over HTTPS) is the fallback for clients whose UDP media traffic
can't reach the server — corporate networks, some proxies, tunnels. LiveKit
has a built-in TURN relay; turn it on and give it a domain and a TLS
certificate for that domain:

```yaml
turn:
  enabled: true
  domain: turn.yourhost.example   # DNS points at this server
  tls_port: 5349                  # TURN over TLS
  cert_file: /path/to/cert.pem
  key_file: /path/to/key.pem
```

(The cert and key can also come from `--turn-cert` / `--turn-key`, env
`LIVEKIT_TURN_CERT` / `LIVEKIT_TURN_KEY`.) Then open/forward TCP `5349` as
you would the others.

### No-ops option: LiveKit Cloud

[LiveKit Cloud](https://livekit.com) gives you a signalling URL and key/secret
with nothing installed or forwarded: set `LIVEKIT_URL` to your project's
`wss://<project>.livekit.cloud` URL plus your Cloud key/secret (or run
`deploy/install.sh --livekit-url ... --livekit-key ... --livekit-secret ...`
without `--with-livekit`) and you're done. UDP/TURN/TLS all become their
problem.

## HTTPS

Browsers gate the microphone and camera: `getUserMedia` only works on a
`https://` page or `localhost`. An https page also refuses to open a `ws://`
socket, so once the app is on https, the LiveKit signalling must be `wss://`
too — plain `ws://localhost:7880` stops working the moment the page is on a
domain. This is what you need when RWND sits on your own domain (voice over a
bare `http://` host is blocked by the browser regardless of LiveKit).

Caddy is the short path: it fetches certificates itself (ACME) when the
domains' DNS points at the machine, and upgrades WebSockets automatically.
[`deploy/Caddyfile.example`](../deploy/Caddyfile.example) is a validated
(`caddy validate --config deploy/Caddyfile.example --adapter caddyfile`) two-
vhost setup — the app on one domain, LiveKit signalling on another — with
comments on which ports (UDP 7882 media, TCP 7881 ICE/TCP, 5349 TURN/TLS)
still must be forwarded directly, since Caddy only proxies HTTP/WebSocket.
With it, set `LIVEKIT_URL=wss://livekit.example.com`.

nginx needs the WebSocket upgrade stanza explicitly on the LiveKit vhost:

```nginx
map $http_upgrade $connection_upgrade {
  default upgrade;
  ''      close;
}
server {
  server_name livekit.example.com;
  # ssl_cert/ssl_pem or certbot; Caddy gets certs itself, nginx doesn't.
  location / {
    proxy_pass http://127.0.0.1:7880;   # LiveKit's ws signalling endpoint
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

A Cloudflare tunnel also serves https+wss, so both vhosts above collapse into
it: point `PUBLIC_URL` at the tunnel URL and `LIVEKIT_URL` at a second tunnel
in front of 7880 (`wss://<random>.trycloudflare.com`). The tunnel carries the
app page and signalling; the ICE/UDP media ports don't go through it, so
tunnel voice needs TURN (5349) or LiveKit Cloud — see the sections above.

## Reverse proxies and tunnels

Whatever sits in front must upgrade WebSockets — the chat gateway is one
WebSocket connection to the RWND server, and LiveKit's signalling is another.

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

Cloudflare tunnels: a `trycloudflare.com` / `cloudflare tunnel` carries HTTP
and wss, so the app itself and LiveKit's signalling both work through it. It
does **not** forward the ICE/UDP media traffic, so voice over a tunnel needs
either the TURN fallback above or (simpler) LiveKit Cloud. Text chat through a
tunnel is fine.

## Picking the provider

- **Voice on now:** LiveKit, via the env above
  (`CALL_PROVIDER=livekit`, `LIVEKIT_URL`, `LIVEKIT_API_KEY`,
  `LIVEKIT_API_SECRET`).
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
