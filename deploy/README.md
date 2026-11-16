# deploy/ — one-script install for RWND

`install.sh` puts RWND on your machine as a per-user `systemd --user`
service: no root, no system packages, nothing outside a prefix directory you
choose plus `~/.config/systemd/user/`. It is idempotent — re-running with the
same answers reuses the generated session secret and the relay settings,
rewrites the unit, and restarts it.

With a repo (`--repo` or running the script from a checkout) and no
`--web-dist`, install.sh builds the web app itself (`idealyst build --web`,
needs the idealyst CLI) and installs it, so the quick install serves the app
at `/`. `--web-dist <dir>` skips the build with a bundle you built already.

```bash
sh deploy/install.sh                                # asks for everything
sh deploy/install.sh --yes --prefix ~/rwnd --public-url http://localhost:18080 --port 18080 --repo /path/to/repo
sh deploy/install.sh --yes --prefix ~/rwnd --port 18080 --relay-url https://relay.example.com:4433/ --relay-secret '…' --relay-cert-hash '…'
```

Flags:

| Flag | Meaning |
|---|---|
| `--prefix DIR` | Where to install (default `~/rwnd`): `bin/`, `data/`, `web/`, `env`. |
| `--public-url URL` | The address people use to reach the site (default `http://localhost:<port>`). |
| `--port N` | Port the RWND server binds (127.0.0.1 only: put a reverse proxy or tunnel in front; default 3000). |
| `--server-bin PATH` / `--repo DIR` | Use an already-built server binary, or build one from a repo checkout with cargo. |
| `--web-dist DIR` | A built web bundle (`idealyst build --web`'s dist dir); when given the server serves the app at `/`. When omitted with a repo, install.sh builds the bundle itself. |
| `--relay-url URL` | The idea-chat relay's address, for voice and video. Needs `--relay-secret`. |
| `--relay-secret S` | The relay's grant secret. Kept in the env file (mode 0600) and never printed. |
| `--relay-cert-hash H` | The relay's pinned certificate hash, when it uses one (leave out for a CA-issued certificate). |
| `--yes` | No questions; defaults for anything unset. |

What it writes: `$PREFIX/env` (mode 0600 — holds the session secret,
`PUBLIC_URL`, `PORT`, `DISCORD_DB`, `MEDIA_DIR`, and with voice on
`CALL_PROVIDER=idea-chat` and the `IDEA_CHAT_RELAY_*` settings), a generated
`DISCORD_SESSION_SECRET` (`openssl rand -hex 32`), and `rwnd.service` as a
user unit rendered from `rwnd.service.in` next to this script.

Voice is off until a relay is given. The relay itself is not installed by this
script: it is the idea-chat relay, run separately.

The script ends by printing what to do next: open the URL (and how to
port-forward it), read the first-run setup code from the log, whether voice is
on, and the systemd-linger hint so the service keeps running after you log out.

Full background — environment variables, reverse proxies, tunnels, the
idea-chat backend — is in [../docs/self-hosting.md](../docs/self-hosting.md).
