#!/usr/bin/env bash
# errex.sh — task runner for the errex workspace.
#
# All cargo work runs inside the official rust image so you don't need a
# local toolchain. A persistent `errex-target` docker volume keeps the
# build cache warm between calls so `check` / `fmt` / `tui` are fast.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPOSE=(docker compose -f "$ROOT/docker/docker-compose.yml")
RUST_IMAGE="rust:1-slim"

# Cargo-in-docker preset (non-interactive). Mounts source + reuses caches.
DOCKER_CARGO=(
  docker run --rm
  -v "$ROOT:/work"
  -v errex-cargo-registry:/usr/local/cargo/registry
  -v errex-target:/work/target
  -w /work
  -e CARGO_TERM_COLOR=always
  "$RUST_IMAGE"
)

cmd_up()      { "${COMPOSE[@]}" up --build "$@"; }
cmd_down()    { "${COMPOSE[@]}" down "$@"; }
cmd_restart() { "${COMPOSE[@]}" restart errexd; }
cmd_build()   { "${COMPOSE[@]}" build; }
cmd_ps()      { "${COMPOSE[@]}" ps; }
cmd_logs()    { "${COMPOSE[@]}" logs -f errexd; }

cmd_health() {
  curl -sf -w "\nHTTP %{http_code}\n" http://127.0.0.1:9090/health
}

# POST a sample Sentry envelope so you can watch the daemon log it. First
# arg is the project id (defaults to "demo").
cmd_test_event() {
  local project="${1:-demo}"
  local now
  now="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf '%s\n%s\n%s\n' \
    "{\"event_id\":\"$(uuidgen | tr -d - | tr A-Z a-z)\",\"sent_at\":\"$now\"}" \
    '{"type":"event"}' \
    "{\"timestamp\":\"$now\",\"platform\":\"javascript\",\"level\":\"error\",\"exception\":{\"values\":[{\"type\":\"TypeError\",\"value\":\"x is not a function\",\"stacktrace\":{\"frames\":[{\"function\":\"oops\",\"filename\":\"app.js\",\"lineno\":42}]}}]}}" \
    | curl -sf -w "\nHTTP %{http_code}\n" \
        -X POST --data-binary @- \
        "http://127.0.0.1:9090/api/${project}/envelope/"
}

cmd_check() {
  "${DOCKER_CARGO[@]}" bash -c '
    set -eo pipefail
    rustup component add rustfmt clippy >/dev/null 2>&1 || true
    echo "===== fmt --check ====="
    cargo fmt --all -- --check
    echo "===== clippy -D warnings ====="
    cargo clippy --workspace --all-targets -- -D warnings
    echo "===== test ====="
    cargo test --workspace
  '
  if [[ -d "$ROOT/web/node_modules" ]]; then
    echo "===== web: vitest ====="
    (cd "$ROOT/web" && bun run test)
  else
    echo "===== web: skipped (run 'bun install' in web/ to enable) ====="
  fi
}

cmd_fmt() {
  "${DOCKER_CARGO[@]}" bash -c '
    rustup component add rustfmt >/dev/null 2>&1 || true
    cargo fmt --all
  '
}

cmd_shell() {
  docker run --rm -it \
    -v "$ROOT:/work" \
    -v errex-cargo-registry:/usr/local/cargo/registry \
    -v errex-target:/work/target \
    -w /work \
    "$RUST_IMAGE" bash
}

# Build + run the TUI client in a throwaway container, pointed at the
# running daemon. host.docker.internal works on Docker Desktop (macOS/Win);
# override ERREX_DAEMON_URL on Linux if needed.
cmd_tui() {
  local daemon_url="${ERREX_DAEMON_URL:-ws://host.docker.internal:9091}"
  docker run --rm -it \
    -v "$ROOT:/work" \
    -v errex-cargo-registry:/usr/local/cargo/registry \
    -v errex-target:/work/target \
    -w /work \
    -e ERREX_DAEMON_URL="$daemon_url" \
    "$RUST_IMAGE" \
    bash -c 'cargo run --release --bin errex -- --daemon "$ERREX_DAEMON_URL"'
}

cmd_clean() {
  "${COMPOSE[@]}" down -v --remove-orphans 2>/dev/null || true
  docker volume rm errex-target errex-cargo-registry 2>/dev/null || true
  rm -rf "$ROOT/data"
  echo "cleaned: containers, build cache volumes, ./data"
}

cmd_help() {
  cat <<'EOF'
errex.sh — task runner for the errex workspace

Daemon lifecycle:
  up [-d]            build + start the daemon (add -d to detach)
  down               stop and remove containers
  restart            restart the daemon container
  build              (re)build the image without starting
  ps                 show service status
  logs               tail daemon logs

Probes:
  health             GET /health
  test-event [proj]  POST a sample Sentry envelope (default project: demo)

Dev (no local Rust needed — runs in rust:1-slim):
  check              fmt --check + clippy -D warnings + test (workspace)
  fmt                cargo fmt (auto-apply)
  tui                build and run the TUI client against the running daemon
  shell              interactive bash in rust:1-slim with the workspace mounted

Cleanup:
  clean              stop containers, drop build-cache volumes, delete ./data

Examples:
  ./errex.sh up -d && ./errex.sh test-event && ./errex.sh logs
  ./errex.sh check
  ./errex.sh tui

EOF
}

cmd="${1:-help}"
shift || true
case "$cmd" in
  -h|--help|help)            cmd_help ;;
  up|down|restart|build|ps|logs|health|fmt|check|shell|tui|clean) "cmd_$cmd" "$@" ;;
  test-event)                cmd_test_event "$@" ;;
  *)                         echo "unknown command: $cmd" >&2; cmd_help; exit 2 ;;
esac
