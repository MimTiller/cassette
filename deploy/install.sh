#!/bin/sh
# Install RWND as per-user systemd --user services (no root needed).
# Idempotent: re-running reuses the session secret and the LiveKit keys
# already in the env file, and rewrites + restarts the units.
set -eu

usage() {
    cat <<EOF
usage: install.sh [--prefix DIR] [--public-url URL] [--port N]
                  [--server-bin PATH | --repo DIR] [--web-dist DIR]
                  [--with-livekit] [--livekit-url WS_URL]
                  [--livekit-key K --livekit-secret S] [--livekit-bin PATH]
                  [--yes]

Install RWND (and optionally a LiveKit voice server) under a per-user prefix,
as ~/.config/systemd/user/rwnd.service (+ rwnd-livekit.service).
--server-bin uses an already-built binary; --repo builds one with cargo.
Without --yes/--port etc. missing values are asked on a tty.
EOF
    exit 1
}

PREFIX="" PUBLIC_URL="" PORT="" SERVER_BIN="" WEB_DIST="" REPO=""
WITH_LIVEKIT="" LIVEKIT_URL="" LIVEKIT_KEY="" LIVEKIT_SECRET="" LIVEKIT_BIN="" YES=""

while [ $# -gt 0 ]; do
    case "$1" in
        --prefix)         PREFIX=$2; shift 2 ;;
        --public-url)     PUBLIC_URL=$2; shift 2 ;;
        --port)           PORT=$2; shift 2 ;;
        --server-bin)     SERVER_BIN=$2; shift 2 ;;
        --web-dist)       WEB_DIST=$2; shift 2 ;;
        --repo)           REPO=$2; shift 2 ;;
        --with-livekit)   WITH_LIVEKIT=1; shift ;;
        --livekit-url)    LIVEKIT_URL=$2; shift 2 ;;
        --livekit-key)    LIVEKIT_KEY=$2; shift 2 ;;
        --livekit-secret) LIVEKIT_SECRET=$2; shift 2 ;;
        --livekit-bin)    LIVEKIT_BIN=$2; shift 2 ;;
        --yes)            YES=1; shift ;;
        -h|--help)        usage ;;
        *) echo "install.sh: unknown option $1" >&2; usage ;;
    esac
done

repo="$(dirname "$0")/.."   # this script ships in the repo's deploy/
if [ -d "$repo/discord-server" ]; then REPO=${REPO:-$repo}; fi

if [ -z "$YES" ] && { [ -z "$PREFIX" ] || [ -z "$PORT" ] || [ -z "$PUBLIC_URL" ] || [ -z "$WITH_LIVEKIT" ]; }; then
    [ -t 0 ] || { echo "install.sh: pass --yes plus the flags (or run on a tty)." >&2; usage; }
    if [ -z "$PREFIX" ]; then
        printf 'Install prefix (default %s/rwnd): ' "$HOME"; read -r line || line=""
        PREFIX=${line:-$HOME/rwnd}
    fi
    if [ -z "$PORT" ]; then
        printf 'Port to serve on (default 3000): '; read -r line || line=""
        PORT=${line:-3000}
    fi
    if [ -z "$PUBLIC_URL" ]; then
        printf 'Public URL people use to reach the site (default http://localhost:%s): ' "$PORT"
        read -r line || line=""
        PUBLIC_URL=${line:-http://localhost:$PORT}
    fi
    if [ -z "$WITH_LIVEKIT" ]; then
        printf 'Also install a LiveKit voice server (y/N)? '; read -r line || line=""
        case "$line" in [yY]*) WITH_LIVEKIT=1 ;; esac
    fi
fi
: "${PREFIX:=$HOME/rwnd}"
: "${PORT:=3000}"
: "${PUBLIC_URL:=http://localhost:$PORT}"

# Server binary: given, or built from the repo.
if [ -z "$SERVER_BIN" ]; then
    [ -n "$REPO" ] && [ -d "$REPO/discord-server" ] || { echo "install.sh: no --server-bin and no --repo with discord-server/. Build it first, or pass --repo." >&2; exit 1; }
    echo "Building the server binary (cargo --release, a few minutes)..."
    (cd "$REPO/discord-server" && cargo build --release --features server --bin server)
    SERVER_BIN="$REPO/discord-server/target/release/server"
fi
[ -x "$SERVER_BIN" ] || { echo "install.sh: server binary $SERVER_BIN not found or not executable." >&2; exit 1; }

mkdir -p "$PREFIX/bin" "$PREFIX/data"

if [ -z "$WEB_DIST" ] && [ -n "$REPO" ] && [ -d "$REPO/discord-clone" ]; then
    # With the repo and no prebuilt bundle, build the web app ourselves.
    if command -v idealyst >/dev/null 2>&1; then
        echo "Building the web app (idealyst build --web, a few minutes)..."
        (cd "$REPO" && idealyst build --web discord-clone) || true
        # The bundle lands in discord-clone/dist/web (index.html + pkg/).
        [ -f "$REPO/discord-clone/dist/web/index.html" ] && WEB_DIST="$REPO/discord-clone/dist/web"
    fi
    [ -n "$WEB_DIST" ] || {
        echo "install.sh: warning: no web app (no idealyst CLI or its build failed)," >&2
        echo "  so the server runs the API only and serves no page at /." >&2
        echo "  Install the idealyst CLI (https://github.com/IdealystIO/idealyst-native#installing-the-cli)" >&2
        echo "  or build the client with 'idealyst build --web discord-clone' from the repo" >&2
        echo "  and rerun with --web-dist discord-clone/dist/web." >&2
    }
fi
if [ -n "$WEB_DIST" ]; then
    mkdir -p "$PREFIX/web"
    cp -r "$WEB_DIST/." "$PREFIX/web/"
    cp -f "$SERVER_BIN" "$PREFIX/bin/server"
    WEBLINE="$PREFIX/web"
else
    cp -f "$SERVER_BIN" "$PREFIX/bin/server"
    WEBLINE=""
fi

# Secrets: reuse what the env file already has so tokens/keys stay valid.
# Read them all before the env file is rewritten below.
getenv() { grep "^$1=" "$PREFIX/env" 2>/dev/null | tail -1 | cut -d= -f2- || true; }
SECRET=$(getenv DISCORD_SESSION_SECRET)
[ -n "$SECRET" ] || SECRET=$(openssl rand -hex 32)
OLD_LK_URL=$(getenv LIVEKIT_URL)
OLD_LK_KEY=$(getenv LIVEKIT_API_KEY)
OLD_LK_SECRET=$(getenv LIVEKIT_API_SECRET)

umask 077
cat > "$PREFIX/env" <<EOF
HOST=127.0.0.1
PORT=$PORT
PUBLIC_URL=$PUBLIC_URL
DISCORD_DB=$PREFIX/data/discord.db
MEDIA_DIR=$PREFIX/data/media
DISCORD_SESSION_SECRET=$SECRET
${WEBLINE:+WEB_DIST=$WEBLINE}
EOF
umask 022

UNIT_DIR=$HOME/.config/systemd/user
mkdir -p "$UNIT_DIR"
render() { sed "s|__PREFIX__|$PREFIX|g" "$1" > "$2"; }
start_unit() { # write happened; make systemd see it, then (re)start it
    systemctl --user daemon-reload >/dev/null 2>&1 || true
    systemctl --user restart "$1" 2>/dev/null || systemctl --user start "$1"
}
render "$repo/deploy/rwnd.service.in" "$UNIT_DIR/rwnd.service"
start_unit rwnd.service
systemctl --user enable rwnd.service >/dev/null 2>&1 || true

LK_NOTE="voice is off — set CALL_PROVIDER/LIVEKIT_* later or rerun with --with-livekit"
if [ -n "$WITH_LIVEKIT" ]; then
    if [ -z "$LIVEKIT_BIN" ]; then
        echo "Downloading pinned livekit-server 1.13.11 linux_amd64..."
        curl -fL -o "$PREFIX/bin/livekit.tar.gz" \
            https://github.com/livekit/livekit/releases/download/v1.13.11/livekit_1.13.11_linux_amd64.tar.gz
        EXPECTED=$(curl -fsL https://github.com/livekit/livekit/releases/download/v1.13.11/checksums.txt |
            grep 'linux_amd64\.tar\.gz$' | cut -d' ' -f1)
        ACTUAL=$(sha256sum "$PREFIX/bin/livekit.tar.gz" | cut -d' ' -f1)
        [ "$EXPECTED" = "$ACTUAL" ] || { echo "install.sh: sha256 mismatch, refusing." >&2; exit 1; }
        tar xzf "$PREFIX/bin/livekit.tar.gz" -C "$PREFIX/bin" livekit-server
        rm -f "$PREFIX/bin/livekit.tar.gz"
        LIVEKIT_BIN="$PREFIX/bin/livekit-server"
    else
        # A given binary (testing, or a binary you already run): put it at
        # the unit's path without disturbing where you keep it.
        if [ "$LIVEKIT_BIN" != "$PREFIX/bin/livekit-server" ]; then
            [ -x "$LIVEKIT_BIN" ] || { echo "install.sh: --livekit-bin $LIVEKIT_BIN not executable." >&2; exit 1; }
            cp -f "$LIVEKIT_BIN" "$PREFIX/bin/livekit-server"
            LIVEKIT_BIN="$PREFIX/bin/livekit-server"
        fi
    fi
    if [ -z "$LIVEKIT_KEY" ] || [ -z "$LIVEKIT_SECRET" ]; then
        LIVEKIT_KEY=$OLD_LK_KEY
        LIVEKIT_SECRET=$OLD_LK_SECRET
    fi
    if [ -z "$LIVEKIT_KEY" ] || [ -z "$LIVEKIT_SECRET" ]; then
        "$LIVEKIT_BIN" generate-keys > "$PREFIX/.livekit-keys"
        LIVEKIT_KEY=$(sed -n 's/^API Key: *//p' "$PREFIX/.livekit-keys")
        LIVEKIT_SECRET=$(sed -n 's/^API Secret: *//p' "$PREFIX/.livekit-keys")
        rm -f "$PREFIX/.livekit-keys"
    fi
    esc() { printf '%s' "$1" | sed 's/[\\&|]/\\&/g'; }
    # Default URL: same host as PUBLIC_URL, port 7880.
    : "${LIVEKIT_URL:=ws://$(printf '%s' "${PUBLIC_URL#*://}" | cut -d/ -f1 | sed 's/:.*//'):7880}"
    lk_hostpart=${LIVEKIT_URL#*://}
    lk_host=${lk_hostpart%%:*}
    lk_port=${lk_hostpart##*:}
    if [ "$lk_port" = "$lk_hostpart" ]; then lk_port=7880; fi
    case $lk_port in ''|*[!0-9]*) lk_port=7880 ;; esac
    LKHOST=$lk_host
    LKPORT=$lk_port
    NODE_IP=$LKHOST
    case $LKHOST in
        *[!0-9.]*) NODE_IP=$(hostname -i 2>/dev/null | cut -d' ' -f1 || true) ;;
    esac
    [ -n "$NODE_IP" ] || NODE_IP=$LKHOST

    cat >> "$PREFIX/env" <<EOF
CALL_PROVIDER=livekit
LIVEKIT_URL=$LIVEKIT_URL
LIVEKIT_API_KEY=$LIVEKIT_KEY
LIVEKIT_API_SECRET=$LIVEKIT_SECRET
EOF
    sed -e "s|__PORT__|$LKPORT|" -e "s|__TCP_PORT__|$((LKPORT + 1))|" \
        -e "s|__UDP_PORT__|$((LKPORT + 2))|" -e "s|__NODE_IP__|$(esc "$NODE_IP")|" \
        -e "s|__API_KEY__|$(esc "$LIVEKIT_KEY")|" -e "s|__API_SECRET__|$(esc "$LIVEKIT_SECRET")|" \
        "$repo/deploy/livekit.yaml.in" > "$PREFIX/livekit.yaml"
    render "$repo/deploy/rwnd-livekit.service.in" "$UNIT_DIR/rwnd-livekit.service"
    start_unit rwnd-livekit.service
    systemctl --user enable rwnd-livekit.service >/dev/null 2>&1 || true
    LK_NOTE="voice: $LKPORT (wss signalling), $((LKPORT + 1)) (ICE/TCP), $((LKPORT + 2)) (ICE/UDP)"
else
    # An existing LiveKit (yours, or LiveKit Cloud): point the app at it.
    : "${LIVEKIT_URL:=$OLD_LK_URL}" "${LIVEKIT_KEY:=$OLD_LK_KEY}" "${LIVEKIT_SECRET:=$OLD_LK_SECRET}"
    if [ -n "$LIVEKIT_URL" ] && [ -n "$LIVEKIT_KEY" ] && [ -n "$LIVEKIT_SECRET" ]; then
        cat >> "$PREFIX/env" <<EOF
CALL_PROVIDER=livekit
LIVEKIT_URL=$LIVEKIT_URL
LIVEKIT_API_KEY=$LIVEKIT_KEY
LIVEKIT_API_SECRET=$LIVEKIT_SECRET
EOF
        systemctl --user restart rwnd.service
        LK_NOTE="voice: using the LiveKit at $LIVEKIT_URL"
    elif [ -n "$LIVEKIT_URL$LIVEKIT_KEY$LIVEKIT_SECRET" ]; then
        echo "install.sh: --livekit-url, --livekit-key and --livekit-secret go together; voice left off." >&2
    fi
fi
chmod 600 "$PREFIX/env" "$PREFIX/livekit.yaml" 2>/dev/null || chmod 600 "$PREFIX/env"

echo
echo "Installed under $PREFIX. Next steps:"
echo "  1. The server listens on 127.0.0.1:$PORT. Put a reverse proxy (Caddy or nginx,"
echo "     see docs/self-hosting.md) or a tunnel in front of it for $PUBLIC_URL;"
echo "     to try it from another machine first: ssh -L $PORT:127.0.0.1:$PORT user@thishost"
echo "  2. The first-run setup code, if the wizard shows one, is in the log:"
echo "       journalctl --user -u rwnd.service -n 50"
echo "  3. $LK_NOTE"
echo "     (docs/self-hosting.md explains what each voice port carries)."
echo "  4. So the services keep running after you log out:"
echo "       loginctl enable-linger \$(id -un)    (may need sudo)"
echo "Logs: journalctl --user -u rwnd.service (add -u rwnd-livekit.service)."
