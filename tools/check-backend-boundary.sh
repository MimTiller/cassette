#!/bin/sh
# Enforce the backend boundary: UI code (anything in discord-clone/src
# outside discord-clone/src/backend/) must not reach a #[server] function of
# discord-server/src — not by its full path (`discord_server::servers::
# list_servers(`), and not through an imported module alias (`permissions::
# foo(`). Every backend call must go through the seam: `crate::backend::<verb>`
# (or the Gateway handle it hands back). The gateway socket itself may only be
# opened inside the seam.
#
# Usage: sh tools/check-backend-boundary.sh
# Exits 0 when the boundary holds, 1 with a file:line list when it doesn't.

set -e

SRV_SRC="discord-server/src"
UI_SRC="discord-clone/src"
SEAM="discord-clone/src/backend"

if [ ! -d "$SRV_SRC" ] || [ ! -d "$UI_SRC" ]; then
    echo "check-backend-boundary: run from the repo root" >&2
    exit 1
fi

# --- 1. every #[server] function name in the backend server ------------------
# A standalone #[server] attribute line; the fn declaration that follows
# names an RPC. Prose that merely mentions #[server] never matches because
# the attribute must sit alone on its line.
rpc_names=$(
    grep -rn -A1 '^[[:space:]]*#\[server\][[:space:]]*$' "$SRV_SRC" --include='*.rs' \
    | grep -E '^[^:]+[-:][0-9]+[:-][[:space:]]*(pub[[:space:]]+)?(async[[:space:]]+)?fn[[:space:]]+[a-z_]+' \
    | sed -E 's/.*fn[[:space:]]+([a-z_]+).*/\1/' \
    | sort -u
)
names=$(for n in $rpc_names; do printf '%s|' "$n"; done)
names=${names%|}
if [ -z "$names" ]; then
    echo "check-backend-boundary: no #[server] fns found — wrong directory?" >&2
    exit 1
fi

viol=$(mktemp)

# --- 2. RPC-name calls outside the seam --------------------------------------
# Flag any call whose module path ends in an RPC name — `X::…::rpc(` — unless
# the path goes through the seam (`backend::rpc(`). Uppercase-leading paths
# (types, impl blocks like `PasswordAuth::login`) aren't module paths and
# don't count. Comment lines don't count.
for f in $(find "$UI_SRC" -name '*.rs' ! -path "$SEAM/*" ! -path "$SEAM" | sort); do
    grep -nE '(^|[^A-Za-z0-9_:])[a-z_][A-Za-z0-9_]*(::[a-z_][A-Za-z0-9_]*)*::('"$names"')[[:space:]]*\(' "$f" \
    | grep -v 'backend::' \
    | grep -vE '^[0-9]+:[[:space:]]*(//|/\*)' \
    | sed "s|^|$f:|" >> "$viol" || true
done

# --- 3. the gateway socket may only be opened inside the seam ----------------
for f in $(find "$UI_SRC" -name '*.rs' ! -path "$SEAM/*" ! -path "$SEAM" | sort); do
    grep -nE 'message_gateway\s*\(|UseSocket<' "$f" \
    | grep -vE '^[0-9]+:[[:space:]]*(//|/\*)' \
    | sed "s|^|$f:|" >> "$viol" || true
done

if [ -s "$viol" ]; then
    sort -u "$viol" | while IFS= read -r hit; do
        f=$(echo "$hit" | cut -d: -f1)
        line=$(echo "$hit" | cut -d: -f2)
        seg=$(echo "$hit" | cut -d: -f3- | tr -d '\t' | sed -E 's/^ *//')
        echo "$f:$line: $seg"
    done
    rm -f "$viol"
    exit 1
fi
rm -f "$viol"
echo "backend boundary OK: UI files outside $SEAM reach the backend only through crate::backend"
exit 0
