#!/bin/sh
# Install RWND as per-user systemd --user services (no root needed).
# Idempotent: re-running reuses the session secret and the call-relay
# settings already in the env file, and rewrites + restarts the units.
set -eu

usage() {
    cat <<EOF
usage: install.sh [--prefix DIR] [--public-url URL] [--port N]
                  [--server-bin PATH | --repo DIR] [--web-dist DIR]
                  [--call-relay-url URL --call-relay-secret FILE
                   --call-relay-cert-hash H]
                  [--yes]

Install RWND (and, with the --call-relay-* flags, an idea-chat call relay
pointed at) under a per-user prefix, as ~/.config/systemd/user/rwnd.service.
--server-bin uses an already-built binary; --repo builds one with cargo.
Without --yes/--port etc. missing values are asked on a tty.
EOF
    exit 1
}

PREFIX="" PUBLIC_URL="" PORT="" SERVER_BIN="" WEB_DIST="" REPO=""
CALL_RELAY_URL="" CALL_RELAY_SECRET_FILE="" CALL_RELAY_CERT_HASH="" YES=""

while [ $# -gt 0 ]; do
    case "$1" in
        --prefix)         PREFIX=$2; shift 2 ;;
        --public-url)     PUBLIC_URL=$2; shift 2 ;;
        --port)           PORT=$2; shift 2 ;;
        --server-bin)     SERVER_BIN=$2; shift 2 ;;
        --web-dist)       WEB_DIST=$2; shift 2 ;;
        --repo)           REPO=$2; shift 2 ;;
        --call-relay-url)    CALL_RELAY_URL=$2; shift 2 ;;
        --call-relay-secret) CALL_RELAY_SECRET_FILE=$2; shift 2 ;;
        --call-relay-cert-hash) CALL_RELAY_CERT_HASH=$2; shift 2 ;;
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
OLD_RELAY_SECRET_FILE=$(getenv IDEA_CHAT_RELAY_SECRET)
OLD_RELAY_CERT_HASH=$(getenv IDEA_CHAT_RELAY_CERT_HASH)

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

CALL_NOTE="voice is off — set CALL_PROVIDER and IDEA_CHAT_RELAY_* in $PREFIX/env later (or pass the --call-relay-* flags)"
: "${CALL_RELAY_URL:=$OLD_RELAY_URL}" "${CALL_RELAY_SECRET_FILE:=$OLD_RELAY_SECRET_FILE}" "${CALL_RELAY_CERT_HASH:=$OLD_RELAY_CERT_HASH}"
if [ -n "$CALL_RELAY_URL" ] && [ -n "$CALL_RELAY_SECRET_FILE" ] && [ -n "$CALL_RELAY_CERT_HASH" ]; then
    # The relay runs on its own (dial it directly: QUIC/WebTransport is never
    # proxied); the app only records how to reach it.
    [ -f "$CALL_RELAY_SECRET_FILE" ] || { echo "install.sh: --call-relay-secret $CALL_RELAY_SECRET_FILE not found." >&2; exit 1; }
    cat >> "$PREFIX/env" <<EOF
CALL_PROVIDER=idea-chat
IDEA_CHAT_RELAY_URL=$CALL_RELAY_URL
IDEA_CHAT_RELAY_SECRET=$CALL_RELAY_SECRET_FILE
IDEA_CHAT_RELAY_CERT_HASH=$CALL_RELAY_CERT_HASH
EOF
    systemctl --user restart rwnd.service
    CALL_NOTE="voice: the idea-chat relay at $CALL_RELAY_URL (pin the cert hash you were given at its start)"
elif [ -n "$CALL_RELAY_URL$CALL_RELAY_SECRET_FILE$CALL_RELAY_CERT_HASH" ]; then
    echo "install.sh: --call-relay-url, --call-relay-secret and --call-relay-cert-hash go together; voice left off." >&2
fi
chmod 600 "$PREFIX/env"

echo
echo "Installed under $PREFIX. Next steps:"
echo "  1. The server listens on 127.0.0.1:$PORT. Put a reverse proxy (Caddy or nginx,"
echo "     see docs/self-hosting.md) or a tunnel in front of it for $PUBLIC_URL;"
echo "     to try it from another machine first: ssh -L $PORT:127.0.0.1:$PORT user@thishost"
echo "  2. The first-run setup code, if the wizard shows one, is in the log:"
echo "       journalctl --user -u rwnd.service -n 50"
echo "  3. $CALL_NOTE"
echo "     (docs/self-hosting.md explains how to run the call relay itself)."
echo "  4. So the service keeps running after you log out:"
echo "       loginctl enable-linger \$(id -un)    (may need sudo)"
echo "Logs: journalctl --user -u rwnd.service."
