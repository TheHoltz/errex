#!/usr/bin/env bash
# Multi-load-point benchmark for the hosting-cost optimization phase.
#
# Boots one fresh release errexd pinned to one CPU and measures RSS at
# three operating points the same daemon goes through in real life:
#
#   1. IDLE      — boot + 30 s of no traffic. The floor an operator must
#                  provision for. This is the dominant Railway cost.
#   2. LOW LOAD  — 30 s @ 100 RPS sustained. Realistic self-host load
#                  for ~10 SDKs each emitting at typical rates.
#   3. SATURATION— 30 s @ 8000 RPS target. Headroom check; we want this
#                  to not collapse below 5000 RPS or 50 ms p99.
#
# Prints one JSON line with all three readings + a derived cost score.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="$ROOT/target/release/errexd"
HARNESS="$ROOT/scripts/stress/target/release/errex-stress"
PORT=19090
MCP_PORT=19092
PIN_CPU=0

if [[ ! -x "$BIN" ]] || [[ ! -x "$HARNESS" ]]; then
  echo "{\"error\":\"missing binaries\"}" >&2
  exit 1
fi

DATA="$(mktemp -d)"
PID=""
cleanup() {
  trap - EXIT
  if [[ -n "$PID" ]] && kill -0 "$PID" 2>/dev/null; then
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
ERREX_RATE_LIMIT_PER_MIN=0 \
  taskset -c "$PIN_CPU" "$BIN" >/dev/null 2>&1 &
PID=$!

# Wait for ready.
for _ in $(seq 1 100); do
  if curl -sf "http://127.0.0.1:$PORT/health" >/dev/null 2>&1; then break; fi
  sleep 0.05
done

# Resolve the actual errexd pid.
comm="$(ps -o comm= -p "$PID" 2>/dev/null | tr -d ' ')"
if [[ "$comm" != "errexd" ]]; then
  resolved="$(pgrep -fn "$BIN" 2>/dev/null || true)"
  if [[ -n "$resolved" ]]; then PID="$resolved"; fi
fi

# Sample RSS for $1 seconds, print mean+max in kB.
sample_rss() {
  local duration="$1"
  local samples=()
  local end=$(($(date +%s) + duration))
  while [[ $(date +%s) -lt $end ]]; do
    local rss
    rss=$(awk '/VmRSS:/ {print $2}' /proc/"$PID"/status 2>/dev/null || echo 0)
    samples+=("$rss")
    sleep 0.5
  done
  printf '%s\n' "${samples[@]}" | awk '
    NR == 1 { min=$1; max=$1 }
    { sum+=$1; if ($1<min) min=$1; if ($1>max) max=$1; n++ }
    END { printf "%.0f %.0f %.0f", sum/n, min, max }
  '
}

# Run a load profile via the harness, output the JSON it writes.
run_load() {
  local rps="$1" dur="$2" out
  out="$(mktemp)"
  "$HARNESS" \
    --base "http://127.0.0.1:$PORT" \
    --daemon-pid "$PID" \
    --label "load-rps$rps" \
    --out "$out" \
    --rps "$rps" --workers 64 --duration-secs "$dur" \
    --cardinality 50 --frames 8 --projects 4 --ws-subscribers 4 \
    >/dev/null
  cat "$out"
  rm -f "$out"
}

# 0. WARM-UP: fault in SPA pages by simulating one full browser pageview.
#    rust-embed stores the SPA inside the binary, but pages aren't
#    resident until something requests them. A real operator opens the
#    dashboard once on boot, so the realistic "idle floor" is post-SPA-
#    warmup, not literal first-launch.
WEB_BUILD="$ROOT/web/build"
if [[ -d "$WEB_BUILD" ]]; then
  while read -r path; do
    rel="${path#$WEB_BUILD}"
    curl -sf -o /dev/null "http://127.0.0.1:$PORT$rel" || true
  done < <(find "$WEB_BUILD" -type f)
fi

# 1. IDLE: 30 s of no traffic, post-SPA-warmup.
read -r idle_mean idle_min idle_max <<< "$(sample_rss 30)"

# 2. LOW LOAD: 100 RPS for 30 s, RSS sampled by the harness itself.
low_json="$(run_load 100 30)"

# 3. SATURATION: 8000 RPS target for 30 s.
sat_json="$(run_load 8000 30)"

# Combine.
python3 -c "
import json, sys
low = json.loads('''$low_json''')
sat = json.loads('''$sat_json''')
idle_mean_mb = $idle_mean / 1024.0
idle_min_mb = $idle_min / 1024.0
idle_max_mb = $idle_max / 1024.0
low_rss_mean_mb = low['daemon_rss_kb']['mean'] / 1024.0
low_rss_max_mb  = low['daemon_rss_kb']['max']  / 1024.0
sat_rss_mean_mb = sat['daemon_rss_kb']['mean'] / 1024.0
sat_rss_max_mb  = sat['daemon_rss_kb']['max']  / 1024.0
sat_rps  = sat['achieved_rps']
sat_p99  = sat['ingest_latency_ms']['p99']
sat_max  = sat['ingest_latency_ms']['max']
errs     = sum(low.get(k,0) for k in ('err_4xx','err_5xx','err_io')) + \
           sum(sat.get(k,0) for k in ('err_4xx','err_5xx','err_io'))
# Cost score: idle RSS dominates Railway cost. Lower better.
# Saturation is only a headroom gate (validated separately).
print(json.dumps({
  'idle_rss_mean_mb':  round(idle_mean_mb, 2),
  'idle_rss_min_mb':   round(idle_min_mb, 2),
  'idle_rss_max_mb':   round(idle_max_mb, 2),
  'low_rss_mean_mb':   round(low_rss_mean_mb, 2),
  'low_rss_max_mb':    round(low_rss_max_mb, 2),
  'sat_achieved_rps':  round(sat_rps, 0),
  'sat_p99_ms':        round(sat_p99, 2),
  'sat_max_ms':        round(sat_max, 2),
  'sat_rss_mean_mb':   round(sat_rss_mean_mb, 2),
  'sat_rss_max_mb':    round(sat_rss_max_mb, 2),
  'errors':            errs,
  'cost_score_mb':     round(idle_mean_mb, 2),
  'headroom_ok':       sat_rps >= 5000 and sat_p99 <= 50 and errs == 0,
}))
"
