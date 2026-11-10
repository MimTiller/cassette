# deploy/ — one-script install for RWND

`install.sh` puts RWND on your machine as a per-user `systemd --user`
service: no root, no system packages, nothing outside a prefix directory you
choose plus `~/.config/systemd/user/`. It is idempotent — re-running with the
same answers reuses the generated session secret and LiveKit keys, rewrites
the units, and restarts them.

```bash
sh deploy/install.sh                                # asks for everything
sh deploy/install.sh --yes --prefix ~/rwnd --public-url http://localhost:18080 --port 18080 --repo /path/to/repo
sh deploy/install.sh --yes --prefix ~/rwnd --port 18080 --with-livekit --livekit-url ws://localhost:17880
```

Flags:

| Flag | Meaning |
|---|---|
| `--prefix DIR` | Where to install (default `~/rwnd`): `bin/`, `data/`, `env`, `livekit.yaml`. |
| `--public-url URL` | The address people use to reach the site (default `http://localhost:<port>`). |
| `--port N` | Port the RWND server binds (127.0.0.1; default 3000). |
| `--server-bin PATH` / `--repo DIR` | Use an already-built server binary, or build one from a repo checkout with cargo. |
| `--web-dist DIR` | A built web bundle (`idealyst build --web`'s dist dir); when given the server serves the app at `/`. |
| `--with-livekit` | Also install the LiveKit voice server: pinned 1.13.11 release binary (sha256-checked against the release's checksums file), generated keys, `livekit.yaml`, and a `rwnd-livekit` unit. |
| `--livekit-url/--livekit-key/--livekit-secret` | Point the app at an existing LiveKit instead of installing one. |
| `--livekit-bin PATH` | Use this LiveKit binary instead of downloading (testing). |
| `--yes` | No questions; defaults for anything unset. |

What it writes: `$PREFIX/env` (mode 0600 — holds the session secret,
`PUBLIC_URL`, `PORT`, `DISCORD_DB`, `MEDIA_DIR`, and with LiveKit
`CALL_PROVIDER`/`LIVEKIT_*`), a generated `DISCORD_SESSION_SECRET` (`openssl
rand -hex 32`), `rwnd.service` and (with `--with-livekit`) `rwnd-livekit.service`
user units rendered from the `.in` templates next to this script.

The script ends by printing what to do next: open the URL (and how to
port-forward it), read the first-run setup code from the log, which ports to
forward for voice, and the systemd-linger hint so the services keep running
after you log out.

Full background — environment variables, reverse proxies, tunnels, TURN,
LiveKit Cloud, the idea-chat backend — is in
[../docs/self-hosting.md](../docs/self-hosting.md).
