# discord-clone: architecture notes

This is the client crate of [Cassette](../README.md). The top-level README
covers features, setup and configuration; this page covers how the pieces fit
together and how authentication works.

See [`RESUME.md`](RESUME.md) for a detailed engineering log of what was built
and the framework pitfalls hit along the way.

## Architecture notes

- `discord-server`'s `server` binary is both the API host (axum) **and** the
  static-file server for the client's WASM bundle — one process, one port.
- The `#[server]` macro in the shared `server` SDK crate keys off a Cargo
  feature: the client build (no `server` feature) gets typed RPC stub
  functions that POST to the backend; the server build (`--features server`)
  gets the real implementations plus axum route registration. Same source,
  two compiled shapes.
- Auth uses HMAC-signed session tokens. `login` returns a token formatted
  `v2.<user_id>.<token_version>.<expires_unix_secs>.<hex hmac-sha256>` (the
  MAC covers everything before it, keyed with the server's session secret),
  valid for 12 hours, and also sets it as an httpOnly `session` cookie
  (SameSite=Lax; `Secure` only when the request came over https). The
  browser replays that cookie on every later request, including the
  WebSocket upgrade and `/media` fetches; native clients send it as
  `Authorization: Bearer <token>`. Identity comes only from a token whose
  signature verifies, never from a client-supplied user id.
- Revocation: every request also checks the token's version against
  `users.token_version`. Changing the password or "Log out of all devices"
  bumps it, which ends every existing session; open gateway connections
  re-check within 30 seconds. `logout` clears this browser's cookie, and
  rotating the signing secret (`DISCORD_SESSION_SECRET` or the mode-0600
  `<db>.session-secret` file) invalidates every issued token at once.
- The legacy `x-user-id` header is REJECTED by default. Set
  `DISCORD_ALLOW_USER_ID_HEADER=1` to accept it for local development or
  test tooling; a request that presents a bearer token never falls back to it.
- Throttling: an account locks after 5 failed logins within 300 seconds
  (until the oldest failure ages out); per client IP, logins are limited to
  10 a minute and 60 an hour, sign-ups to 5 an hour, and invite redeems to
  30 per 10 minutes (10 per account). Loopback is exempt. Wrong-password
  errors are uniform ("invalid username or password") so the endpoint never
  reveals which usernames exist.
- Passwords are hashed with PBKDF2-HMAC-SHA256 (600,000 iterations); older,
  weaker hashes are upgraded on the next successful login.
- Data model: users, servers, channels, categories, messages, DM channels,
  server memberships, roles, invites, mentions, read-state — see
  `discord-server/migrations/` for the authoritative schema.

## Testing

`discord-server` has an in-crate test suite (`cargo test --features server`
from `discord-server/`) covering the data layer and a real end-to-end
WebSocket round-trip against a booted `server::router()` on an ephemeral
port (via `tokio-tungstenite`, since the framework's own `net::WebSocket`
client is URL-only and can't set the auth header the tests need).
