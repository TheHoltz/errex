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

ERREXD_DATA_DIR="$DATA" \
ERREXD_HTTP_PORT="$PORT" \
ERREXD_MCP_PORT="$MCP_PORT" \
ERREXD_LOG_LEVEL=warn \
  "$BIN" &
PID=$!

# Wait up to 5s for the HTTP listener to come up.
for _ in $(seq 1 50); do
  if curl -sf "http://127.0.0.1:$PORT/health" >/dev/null; then break; fi
  sleep 0.1
done

echo "[smoke] /health"
curl -sf "http://127.0.0.1:$PORT/health" | grep -q '"status":"ok"'

echo "[smoke] /api/projects (empty)"
curl -sf "http://127.0.0.1:$PORT/api/projects" | grep -q '\['

echo "[smoke] / serves SPA shell"
body="$(curl -sf "http://127.0.0.1:$PORT/")"
echo "$body" | grep -qi '<html' || { echo "smoke: index.html not served" >&2; exit 1; }

echo "[smoke] OK"
