#!/bin/sh
# Install RWND as per-user systemd --user services (no root needed).
# Idempotent: re-running reuses the session secret and the relay settings
# already in the env file, and rewrites + restarts the unit.
set -eu

usage() {
    cat <<EOF
usage: install.sh [--prefix DIR] [--public-url URL] [--port N]
                  [--server-bin PATH | --repo DIR] [--web-dist DIR]
                  [--relay-url URL --relay-secret S [--relay-cert-hash H]]
                  [--yes]

Install RWND under a per-user prefix, as ~/.config/systemd/user/rwnd.service.
--server-bin uses an already-built binary; --repo builds one with cargo.
Voice and video go through the idea-chat relay: give --relay-url and
--relay-secret (and --relay-cert-hash when the relay uses a pinned certificate).
Without them voice stays off. Missing values are asked on a tty.
EOF
    exit 1
}

PREFIX="" PUBLIC_URL="" PORT="" SERVER_BIN="" WEB_DIST="" REPO=""
RELAY_URL="" RELAY_SECRET="" RELAY_CERT_HASH="" YES=""

while [ $# -gt 0 ]; do
    case "$1" in
        --prefix)         PREFIX=$2; shift 2 ;;
        --public-url)     PUBLIC_URL=$2; shift 2 ;;
        --port)           PORT=$2; shift 2 ;;
        --server-bin)     SERVER_BIN=$2; shift 2 ;;
        --web-dist)       WEB_DIST=$2; shift 2 ;;
        --repo)           REPO=$2; shift 2 ;;
        --relay-url)      RELAY_URL=$2; shift 2 ;;
        --relay-secret)   RELAY_SECRET=$2; shift 2 ;;
        --relay-cert-hash) RELAY_CERT_HASH=$2; shift 2 ;;
        --yes)            YES=1; shift ;;
        -h|--help)        usage ;;
        *) echo "install.sh: unknown option $1" >&2; usage ;;
    esac
done

repo="$(dirname "$0")/.."   # this script ships in the repo's deploy/
if [ -d "$repo/discord-server" ]; then REPO=${REPO:-$repo}; fi

if [ -z "$YES" ] && { [ -z "$PREFIX" ] || [ -z "$PORT" ] || [ -z "$PUBLIC_URL" ]; }; then
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
OLD_RELAY_URL=$(getenv IDEA_CHAT_RELAY_URL)
OLD_RELAY_HASH=$(getenv IDEA_CHAT_RELAY_CERT_HASH)
OLD_RELAY_SECRET=$(getenv IDEA_CHAT_RELAY_SECRET)

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

# Voice: the relay settings, from the flags or the env file the last install wrote.
: "${RELAY_URL:=$OLD_RELAY_URL}" "${RELAY_SECRET:=$OLD_RELAY_SECRET}" "${RELAY_CERT_HASH:=$OLD_RELAY_HASH}"
if [ -n "$RELAY_URL" ] && [ -n "$RELAY_SECRET" ]; then
    cat >> "$PREFIX/env" <<EOF
CALL_PROVIDER=idea-chat
IDEA_CHAT_RELAY_URL=$RELAY_URL
IDEA_CHAT_RELAY_CERT_HASH=$RELAY_CERT_HASH
IDEA_CHAT_RELAY_SECRET=$RELAY_SECRET
EOF
    VOICE_NOTE="voice: through the idea-chat relay at $RELAY_URL"
elif [ -n "$RELAY_URL$RELAY_SECRET" ]; then
    echo "install.sh: --relay-url and --relay-secret go together; voice left off." >&2
    VOICE_NOTE="voice is off: give --relay-url and --relay-secret"
else
    VOICE_NOTE="voice is off: give --relay-url and --relay-secret to turn it on"
fi
chmod 600 "$PREFIX/env"

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

echo
echo "Installed under $PREFIX. Next steps:"
echo "  1. The server listens on 127.0.0.1:$PORT. Put a reverse proxy (Caddy or nginx,"
echo "     see docs/self-hosting.md) or a tunnel in front of it for $PUBLIC_URL;"
echo "     to try it from another machine first: ssh -L $PORT:127.0.0.1:$PORT user@thishost"
echo "  2. The first-run setup code, if the wizard shows one, is in the log:"
echo "       journalctl --user -u rwnd.service -n 50"
echo "  3. $VOICE_NOTE"
echo "  4. So the service keeps running after you log out:"
echo "       loginctl enable-linger \$(id -un)    (may need sudo)"
echo "Logs: journalctl --user -u rwnd.service"
