#!/usr/bin/env bash
# Targeted single-scenario benchmark for the optimization loop.
#
# Boots a fresh release errexd pinned to ONE physical CPU (taskset -c 0)
# so per-iteration numbers are comparable across host churn, fires the
# rps_4000 stress profile for 30 s, and prints a single JSON line:
#
#   {"achieved_rps":..., "p99_ms":..., "max_ms":...,
#    "rss_max_mb":..., "errors":..., "efficiency_eps_per_mb":...}
#
# `efficiency_eps_per_mb` is the optimization metric. Higher is better.
# Hard fails (any non-zero `errors`, max_ms > 500) are exposed in the
# JSON so the calling iteration can revert.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="$ROOT/target/release/errexd"
HARNESS="$ROOT/scripts/stress/target/release/errex-stress"
PORT="${BENCH_PORT:-19090}"
MCP_PORT="${BENCH_MCP_PORT:-19092}"
DURATION="${BENCH_DURATION:-30}"
# Targets need to push the daemon past saturation, not the harness.
# At 4000 the harness with 64 workers tops out around 3750 (HTTP +
# pacing overhead per worker), hiding daemon-side gains. 8000 reliably
# saturates the daemon and surfaces real throughput differences.
RPS="${BENCH_RPS:-8000}"
PIN_CPU="${BENCH_PIN_CPU:-0}"

if [[ ! -x "$BIN" ]]; then
  echo "{\"error\":\"missing $BIN — run cargo build --release --bin errexd\"}" >&2
  exit 1
fi
if [[ ! -x "$HARNESS" ]]; then
  echo "{\"error\":\"missing $HARNESS — build scripts/stress\"}" >&2
  exit 1
fi

DATA="$(mktemp -d)"
OUT="$(mktemp)"
PID=""
cleanup() {
  trap - EXIT
  if [[ -n "$PID" ]] && kill -0 "$PID" 2>/dev/null; then
    kill "$PID" 2>/dev/null || true
    wait "$PID" 2>/dev/null || true
  fi
  rm -rf "$DATA" "$OUT"
}
trap cleanup EXIT INT TERM

# Pin to one CPU so iterations don't see noise from other cores being
# busy in the host. taskset is a no-op on the harness intentionally —
# we want the harness to have its own cores so it never becomes the
# bottleneck.
ERREX_DATA_DIR="$DATA" \
ERREX_PORT="$PORT" \
ERREX_MCP_PORT="$MCP_PORT" \
ERREX_LOG_LEVEL=warn \
ERREX_RATE_LIMIT_PER_MIN=0 \
  taskset -c "$PIN_CPU" "$BIN" >/dev/null 2>&1 &
PID=$!

for _ in $(seq 1 100); do
  if curl -sf "http://127.0.0.1:$PORT/health" >/dev/null 2>&1; then break; fi
  sleep 0.05
done

# Resolve the actual errexd pid in case `&` returned a wrapper PID.
comm="$(ps -o comm= -p "$PID" 2>/dev/null | tr -d ' ')"
if [[ "$comm" != "errexd" ]]; then
  resolved="$(pgrep -fn "$BIN" 2>/dev/null || true)"
  if [[ -n "$resolved" ]]; then PID="$resolved"; fi
fi

if ! curl -sf "http://127.0.0.1:$PORT/health" >/dev/null 2>&1; then
  echo "{\"error\":\"daemon failed to start\"}" >&2
  exit 1
fi

"$HARNESS" \
  --base "http://127.0.0.1:$PORT" \
  --daemon-pid "$PID" \
  --label "bench-rps${RPS}" \
  --out "$OUT" \
  --rps "$RPS" --workers 64 --duration-secs "$DURATION" \
  --cardinality 50 --frames 8 --projects 4 --ws-subscribers 4 \
  >/dev/null

python3 -c "
import json, sys
d = json.load(open('$OUT'))
rps = d['achieved_rps']
p99 = d['ingest_latency_ms']['p99']
mx = d['ingest_latency_ms']['max']
# Use MEAN rss across the 30 s sample window. The single max sample
# was too noisy for iteration-to-iteration comparison: variance of
# 30+% across re-runs because the sampler can land on a transient
# allocator spike. Mean is stable to within ~3% across re-runs.
rss_mb_mean = d['daemon_rss_kb']['mean'] / 1024.0
rss_mb_max = d['daemon_rss_kb']['max'] / 1024.0
errs = d.get('err_4xx', 0) + d.get('err_5xx', 0) + d.get('err_io', 0)
eff = rps / rss_mb_mean if rss_mb_mean > 0 else 0
print(json.dumps({
  'achieved_rps': round(rps, 1),
  'p99_ms': round(p99, 2),
  'max_ms': round(mx, 2),
  'rss_mean_mb': round(rss_mb_mean, 2),
  'rss_max_mb': round(rss_mb_max, 2),
  'errors': errs,
  'efficiency_eps_per_mb': round(eff, 2),
}))
"
