# deploy/ — one-script install for RWND

`install.sh` puts RWND on your machine as a per-user `systemd --user`
service: no root, no system packages, nothing outside a prefix directory you
choose plus `~/.config/systemd/user/`. It is idempotent — re-running with the
same answers reuses the generated session secret and the call-relay settings,
rewrites the units, and restarts them.

With a repo (`--repo` or running the script from a checkout) and no
`--web-dist`, install.sh builds the web app itself (`idealyst build --web`,
needs the idealyst CLI) and installs it, so the quick install serves the app
at `/`. `--web-dist <dir>` skips the build with a bundle you built already.

```bash
sh deploy/install.sh                                # asks for everything
sh deploy/install.sh --yes --prefix ~/rwnd --public-url http://localhost:18080 --port 18080 --repo /path/to/repo
sh deploy/install.sh --yes --prefix ~/rwnd --port 18080 --call-relay-url https://relay.example.com:8443/ --call-relay-secret ~/relay-secret.txt --call-relay-cert-hash <hash>
```

Flags:

| Flag | Meaning |
|---|---|
| `--prefix DIR` | Where to install (default `~/rwnd`): `bin/`, `data/`, `env`. |
| `--public-url URL` | The address people use to reach the site (default `http://localhost:<port>`). |
| `--port N` | Port the RWND server binds (127.0.0.1 only: put a reverse proxy or tunnel in front; default 3000). |
| `--server-bin PATH` / `--repo DIR` | Use an already-built server binary, or build one from a repo checkout with cargo. |
| `--web-dist DIR` | A built web bundle (`idealyst build --web`'s dist dir); when given the server serves the app at `/`. When omitted with a repo, install.sh builds the bundle itself. |
| `--call-relay-url URL` / `--call-relay-secret FILE` / `--call-relay-cert-hash H` | Wire voice to an idea-chat call relay you already run: its public `https://host:port/` URL, the file holding its secret, and the certificate hash printed at the relay's start (clients pin it). Re-running reuses these from the env file. |
| `--yes` | No questions; defaults for anything unset. |

What it writes: `$PREFIX/env` (mode 0600 — holds the session secret,
`PUBLIC_URL`, `PORT`, `DISCORD_DB`, `MEDIA_DIR`, and with a relay
`CALL_PROVIDER`/`IDEA_CHAT_RELAY_*`), a generated `DISCORD_SESSION_SECRET` (`openssl
rand -hex 32`), and the `rwnd.service` user unit rendered from the `.in`
template next to this script.

The script ends by printing what to do next: open the URL (and how to
port-forward it), read the first-run setup code from the log, which relay the
app is pointed at, and the systemd-linger hint so the service keeps running
after you log out.

Full background — environment variables, reverse proxies, tunnels, and the
idea-chat call relay — is in
[../docs/self-hosting.md](../docs/self-hosting.md).
