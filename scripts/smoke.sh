#!/usr/bin/env bash
# Smoke test: confirm a built daemon serves the SPA and the new API endpoints.
#
# Assumes:
#   - `bun run build` has already populated web/build/
#   - `cargo build --release --bin errexd` produced target/release/errexd
#
# Boots errexd on a temp port, hits / and /api/projects, and tears down.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$ROOT/target/release/errexd"
DATA="$(mktemp -d)"
PORT=19090
MCP_PORT=19092

if [[ ! -x "$BIN" ]]; then
  echo "smoke: $BIN not found; run cargo build --release --bin errexd first" >&2
  exit 1
fi

cleanup() {
  trap - EXIT
  if [[ -n "${PID:-}" ]] && kill -0 "$PID" 2>/dev/null; then
    kill "$PID" 2>/dev/null || true
    wait "$PID" 2>/dev/null || true
  fi
  rm -rf "$DATA"
}
trap cleanup EXIT INT TERM

ERREX_DATA_DIR="$DATA" \
ERREX_PORT="$PORT" \
ERREX_MCP_PORT="$MCP_PORT" \
ERREX_LOG_LEVEL=warn \
  "$BIN" &
PID=$!

# Wait up to 5s for the HTTP listener to come up.
for _ in $(seq 1 50); do
  if curl -sf "http://127.0.0.1:$PORT/health" >/dev/null; then break; fi
  sleep 0.1
done

echo "[smoke] /health"
curl -sf "http://127.0.0.1:$PORT/health" | grep -q '"status":"ok"'

echo "[smoke] /api/auth/setup-status (no auth needed)"
curl -sf "http://127.0.0.1:$PORT/api/auth/setup-status" | grep -q 'needs_setup'

echo "[smoke] /api/projects unauth → 401"
status="$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$PORT/api/projects")"
[[ "$status" == "401" ]] || { echo "smoke: expected 401, got $status" >&2; exit 1; }

echo "[smoke] /ws/anything unauth handshake rejected"
ws_status="$(curl -s -o /dev/null -w '%{http_code}' \
  -H "Connection: Upgrade" -H "Upgrade: websocket" \
  -H "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==" \
  -H "Sec-WebSocket-Version: 13" \
  "http://127.0.0.1:$PORT/ws/x")"
[[ "$ws_status" == "401" ]] || { echo "smoke: ws expected 401, got $ws_status" >&2; exit 1; }

echo "[smoke] / serves SPA shell"
body="$(curl -sf "http://127.0.0.1:$PORT/")"
echo "$body" | grep -qi '<html' || { echo "smoke: index.html not served" >&2; exit 1; }

echo "[smoke] response carries security headers"
hdrs="$(curl -sI "http://127.0.0.1:$PORT/health")"
echo "$hdrs" | grep -qi '^x-content-type-options: nosniff' \
  || { echo "smoke: missing X-Content-Type-Options" >&2; exit 1; }
echo "$hdrs" | grep -qi '^x-frame-options: DENY' \
  || { echo "smoke: missing X-Frame-Options" >&2; exit 1; }
echo "$hdrs" | grep -qi '^content-security-policy:' \
  || { echo "smoke: missing CSP" >&2; exit 1; }

echo "[smoke] OK"
