#!/usr/bin/env bash
# Run errexd + the SvelteKit dev server in parallel for local iteration.
#
#   - errexd binds :9090 (HTTP), :9091 (WS), :9092 (MCP stub) and runs with
#     ERREXD_DEV_MODE=true so its CORS policy permits direct fetches from
#     the Vite dev server on :5173.
#   - bun run dev binds :5173 with /api and /ws proxied to the daemon.
#
# Ctrl-C tears both processes down via a trap on EXIT.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

pids=()
cleanup() {
  trap - EXIT
  for pid in "${pids[@]:-}"; do
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
    fi
  done
  wait "${pids[@]:-}" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

echo "[dev] starting errexd on :9090/:9091 (dev mode)"
ERREXD_DEV_MODE=true \
ERREXD_LOG_LEVEL="${ERREXD_LOG_LEVEL:-info}" \
ERREXD_ADMIN_TOKEN="${ERREXD_ADMIN_TOKEN:-123}" \
  cargo run --quiet --bin errexd &
pids+=("$!")

# Give the daemon a moment to bind so the SPA's first /api/projects call
# doesn't race the listener coming up.
sleep 0.5

echo "[dev] starting Vite (SvelteKit) on :5173"
(
  cd web
  if [[ ! -d node_modules ]]; then
    echo "[dev] installing web/ deps with bun"
    bun install
  fi
  bun run dev
) &
pids+=("$!")

wait "${pids[@]}"
