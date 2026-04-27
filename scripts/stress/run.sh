#!/usr/bin/env bash
# Runs a sweep of stress scenarios against a freshly-booted release errexd.
# Per scenario: starts daemon on a temp port + temp data dir, fires the
# harness, captures the JSON report, kills daemon, repeats.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="$ROOT/target/release/errexd"
HARNESS="$ROOT/scripts/stress/target/release/errex-stress"
RESULTS_DIR="${RESULTS_DIR:-$ROOT/scripts/stress/results}"
PORT=19090
MCP_PORT=19092

mkdir -p "$RESULTS_DIR"

if [[ ! -x "$BIN" ]]; then
  echo "missing $BIN — run cargo build --release --bin errexd" >&2
  exit 1
fi
if [[ ! -x "$HARNESS" ]]; then
  echo "missing $HARNESS — run cargo build --release in scripts/stress" >&2
  exit 1
fi

PID=""
DATA=""
cleanup() {
  trap - EXIT
  if [[ -n "$PID" ]] && kill -0 "$PID" 2>/dev/null; then
    kill "$PID" 2>/dev/null || true
    wait "$PID" 2>/dev/null || true
  fi
  if [[ -n "$DATA" && -d "$DATA" ]]; then rm -rf "$DATA"; fi
}
trap cleanup EXIT INT TERM

start_daemon() {
  cleanup
  PID=""
  DATA="$(mktemp -d)"
  ERREXD_DATA_DIR="$DATA" \
  ERREXD_HTTP_PORT="$PORT" \
  ERREXD_MCP_PORT="$MCP_PORT" \
  ERREXD_LOG_LEVEL=warn \
  ERREXD_RATE_LIMIT_PER_MIN="${ERREXD_RATE_LIMIT_PER_MIN:-0}" \
    "$BIN" >/dev/null 2>&1 &
  PID=$!
  for _ in $(seq 1 100); do
    if curl -sf "http://127.0.0.1:$PORT/health" >/dev/null 2>&1; then break; fi
    sleep 0.05
  done
  # Resolve the actual errexd pid (some shell wrappers expose a wrapper pid
  # via $! instead of the daemon — fall through to pgrep if comm doesn't
  # match).
  local comm
  comm="$(ps -o comm= -p "$PID" 2>/dev/null | tr -d ' ')"
  if [[ "$comm" != "errexd" ]]; then
    local resolved
    resolved="$(pgrep -fn "$BIN" 2>/dev/null || true)"
    if [[ -n "$resolved" ]]; then PID="$resolved"; fi
  fi
  if ! curl -sf "http://127.0.0.1:$PORT/health" >/dev/null 2>&1; then
    echo "daemon failed to start" >&2
    exit 1
  fi
}

run_scenario() {
  local name="$1"; shift
  echo
  echo "=========================================="
  echo "[scenario] $name"
  echo "=========================================="
  start_daemon
  echo "[scenario] daemon pid=$PID, data=$DATA"
  "$HARNESS" \
    --base "http://127.0.0.1:$PORT" \
    --daemon-pid "$PID" \
    --label "$name" \
    --out "$RESULTS_DIR/$name.json" \
    "$@"
  # Sample DB size right after the run to estimate per-event storage cost.
  if [[ -f "$DATA/errex.db" ]]; then
    local sz
    sz=$(stat -c%s "$DATA/errex.db")
    echo "[scenario] db_size_bytes=$sz"
    # Annotate report with DB size.
    python3 -c "
import json,sys
p='$RESULTS_DIR/$name.json'
d=json.load(open(p))
d['db_size_bytes']=$sz
json.dump(d, open(p,'w'), indent=2)
" 2>/dev/null || true
  fi
}

# --- 1. Baseline: modest load, prove the path is healthy. ---
run_scenario baseline \
  --rps 200 --workers 16 --duration-secs 20 \
  --cardinality 50 --frames 8 --projects 4 --ws-subscribers 4

# --- 2. RPS ramp: find the knee. ---
for rps in 500 1000 2000 4000 8000; do
  run_scenario "rps_${rps}" \
    --rps "$rps" --workers 64 --duration-secs 20 \
    --cardinality 50 --frames 8 --projects 4 --ws-subscribers 4
done

# --- 3. Payload size: bigger stack frames. ---
run_scenario big_payload \
  --rps 500 --workers 32 --duration-secs 20 \
  --cardinality 50 --frames 64 --projects 4 --ws-subscribers 4

# --- 4. Low cardinality (heavy dedupe): few issues, many updates. ---
run_scenario low_cardinality \
  --rps 1000 --workers 32 --duration-secs 20 \
  --cardinality 5 --frames 8 --projects 4 --ws-subscribers 4

# --- 5. High cardinality (issue inserts dominate). ---
run_scenario high_cardinality \
  --rps 1000 --workers 32 --duration-secs 20 \
  --cardinality 5000 --frames 8 --projects 4 --ws-subscribers 4

# --- 6. WS fan-out: many subscribers vs broadcast capacity (64). ---
run_scenario ws_fanout \
  --rps 1000 --workers 32 --duration-secs 20 \
  --cardinality 50 --frames 8 --projects 4 --ws-subscribers 64

# --- 7. Gzip: typical SDK on-wire shape. ---
run_scenario gzip \
  --rps 500 --workers 32 --duration-secs 20 \
  --cardinality 50 --frames 8 --projects 4 --ws-subscribers 4 --gzip

# --- 8. Soak: 60s sustained at moderate load to surface drift / leaks. ---
run_scenario soak \
  --rps 500 --workers 32 --duration-secs 60 \
  --cardinality 50 --frames 8 --projects 4 --ws-subscribers 4

cleanup

echo
echo "=========================================="
echo "[stress] all scenarios complete. results: $RESULTS_DIR"
echo "=========================================="
ls -la "$RESULTS_DIR"
