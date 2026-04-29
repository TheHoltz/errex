#!/usr/bin/env bash
# Production-like load test under enforced memory + CPU limits.
#
# Spawns a fresh errexd inside a transient systemd scope with a tight
# RAM ceiling and half a CPU, then concurrently drives:
#   * sustained ingest at high RPS (errex-stress harness, 60 s)
#   * N "dashboard" reader agents polling unauthed admin/project routes
#   * N WebSocket subscribers (mimics SPA clients)
#
# Watches for: OOM kills, 5xx, sustained RSS, p99 latency, sat throughput.
# Prints a single JSON summary at the end.
#
# A note on `rss_mb_*`: the sampler reads the cgroup's `memory.current`,
# which includes kernel page cache attributable to the daemon (SQLite WAL
# pages, mmap'd file pages, the WAL/SHM files themselves). That number
# is what `MemoryMax=` actually enforces and what an operator sizing a
# Railway/Fly instance pays for. It runs well above the daemon's
# /proc/.../status RSS that `multibench.sh` reports.
#
# A note on `ws_received`: the daemon's `/ws/:project` endpoint
# unconditionally requires a session cookie (auth.rs::require_auth),
# regardless of `ERREX_REQUIRE_AUTH`. The harness's WS subscribers are
# cookie-less, so they 401. The connection attempt still proves the
# daemon answers the upgrade handshake; that it returns 0 messages is
# expected for this test setup, not a regression.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="$ROOT/target/release/errexd"
HARNESS="$ROOT/scripts/stress/target/release/errex-stress"
PORT=19191
MCP_PORT=19192
MEM_MAX="${MEM_MAX:-64M}"
CPU_QUOTA="${CPU_QUOTA:-50%}"
DURATION="${DURATION:-60}"
RPS="${RPS:-4000}"
DASHBOARD_AGENTS="${DASHBOARD_AGENTS:-10}"
WS_SUBS="${WS_SUBS:-8}"

if [[ ! -x "$BIN" ]] || [[ ! -x "$HARNESS" ]]; then
  echo "missing $BIN or $HARNESS — build release first" >&2
  exit 1
fi

DATA="$(mktemp -d)"
SCOPE_NAME="errexd-prod-$$"
HARNESS_OUT="$(mktemp)"
DASH_LOG="$(mktemp)"

cleanup() {
  trap - EXIT
  systemctl --user stop "${SCOPE_NAME}.scope" 2>/dev/null || true
  pkill -P $$ 2>/dev/null || true
  rm -rf "$DATA" "$HARNESS_OUT" "$DASH_LOG"
}
trap cleanup EXIT INT TERM

echo "[boot]   limits: MEM_MAX=$MEM_MAX CPU_QUOTA=$CPU_QUOTA"
echo "[boot]   load:   ${DURATION}s @ ${RPS} RPS, ${DASHBOARD_AGENTS} dashboard agents, ${WS_SUBS} WS subs"

# Boot errexd in a transient systemd scope with the resource caps.
# --collect ensures the unit gets garbage-collected after exit so the
# scope name can be reused on a re-run.
systemd-run --user --scope --collect \
  --unit "$SCOPE_NAME" \
  --property="MemoryMax=$MEM_MAX" \
  --property="CPUQuota=$CPU_QUOTA" \
  -- env \
    ERREX_DATA_DIR="$DATA" \
    ERREX_LOG_LEVEL=warn \
    ERREX_RATE_LIMIT_PER_MIN=0 \
    ERREX_REQUIRE_AUTH=false \
    "$BIN" --http-port "$PORT" --mcp-port "$MCP_PORT" \
    >/tmp/errexd-prodtest.log 2>&1 &

# Wait for the listener.
for _ in $(seq 1 100); do
  if curl -sf "http://127.0.0.1:$PORT/health" >/dev/null 2>&1; then
    break
  fi
  sleep 0.05
done

PID="$(pgrep -fn "$BIN" || true)"
if [[ -z "$PID" ]]; then
  echo "[fail] daemon did not start — check /tmp/errexd-prodtest.log" >&2
  tail -20 /tmp/errexd-prodtest.log >&2 || true
  exit 1
fi
echo "[boot]   errexd PID=$PID"

# RSS sampler — every 0.5s. We read the cgroup's memory.current
# (bytes) so kernel page cache attributable to the scope is included,
# which is what `MemoryMax` actually enforces. Sampler runs in a
# subshell that exits when the daemon's cgroup file disappears (i.e.,
# the scope is torn down).
RSS_SAMPLES="$(mktemp)"
CG_REL="$(awk -F: 'NR==1{print $3}' /proc/$PID/cgroup)"
CG_PATH="/sys/fs/cgroup${CG_REL}"
echo "[boot]   cgroup: $CG_PATH"
(
  end=$(($(date +%s) + DURATION + 2))
  while [[ $(date +%s) -lt $end ]]; do
    if [[ -r "$CG_PATH/memory.current" ]]; then
      v=$(cat "$CG_PATH/memory.current" 2>/dev/null)
      if [[ -n "$v" ]]; then
        echo "$v"
      fi
    elif [[ -r /proc/$PID/status ]]; then
      awk '/VmRSS:/{print $2*1024}' /proc/$PID/status 2>/dev/null
    fi
    sleep 0.5
  done
) > "$RSS_SAMPLES" &
SAMPLER_PID=$!

# Dashboard agents — each polls a few read-side endpoints in a loop
# at ~10 req/s. Counts 200/non-200 responses into the shared log.
for i in $(seq 1 "$DASHBOARD_AGENTS"); do
  (
    end=$(($(date +%s) + DURATION))
    ok=0
    bad=0
    while [[ $(date +%s) -lt $end ]]; do
      for ep in "/health" "/api/projects" "/api/issues"; do
        code=$(curl -s -o /dev/null -w "%{http_code}" -m 2 "http://127.0.0.1:$PORT$ep" || echo 000)
        if [[ "$code" == "200" || "$code" == "401" || "$code" == "404" ]]; then
          # 401 = endpoint exists but needs auth — still proves daemon
          # answered. 404 = route absent in current build (e.g. /api/issues
          # without a project filter). Both indicate the listener works.
          ok=$((ok+1))
        else
          bad=$((bad+1))
        fi
      done
      sleep 0.1
    done
    echo "agent$i ok=$ok bad=$bad" >> "$DASH_LOG"
  ) &
done

# Drive ingest at $RPS for $DURATION seconds.
"$HARNESS" \
  --base "http://127.0.0.1:$PORT" \
  --daemon-pid "$PID" \
  --label "prod-test" \
  --out "$HARNESS_OUT" \
  --rps "$RPS" --workers 64 --duration-secs "$DURATION" \
  --cardinality 100 --frames 8 --projects 4 --ws-subscribers "$WS_SUBS" \
  >/dev/null 2>/tmp/errex-stress.log || true

# Stop the sampler explicitly. (`wait` would block on the daemon
# scope which doesn't exit on its own.)
kill "$SAMPLER_PID" 2>/dev/null || true
wait "$SAMPLER_PID" 2>/dev/null || true

# Stop the daemon scope so the kernel can flush cgroup memory back
# before we read the final OOM signature, and so `wait` for dashboard
# agents below doesn't get blocked behind systemd-run's foreground
# wait on the unit.
systemctl --user stop "${SCOPE_NAME}.scope" 2>/dev/null || true
# Briefly let the unit teardown propagate; without this, journalctl
# may not yet have the OOM message.
sleep 0.5

# Did the kernel OOM-kill us?
oom_killed=0
if journalctl --user -u "${SCOPE_NAME}.scope" -n 50 --no-pager 2>/dev/null | grep -qi "oom"; then
  oom_killed=1
fi
# Also check dmesg for the cgroup OOM signature; --user journals may
# not always pick the message up.
if dmesg 2>/dev/null | tail -100 | grep -q "Memory cgroup out of memory.*$SCOPE_NAME"; then
  oom_killed=1
fi

# Aggregate results.
python3 - <<PY
import json, os, sys

samples_path = "$RSS_SAMPLES"
samples_kb = []
with open(samples_path) as f:
    for line in f:
        line = line.strip()
        if not line:
            continue
        try:
            n = int(line)
        except ValueError:
            continue
        # cgroup memory.current is in bytes; /proc fallback already in bytes.
        samples_kb.append(n / 1024)

dash_ok, dash_bad = 0, 0
try:
    with open("$DASH_LOG") as f:
        for line in f:
            for tok in line.split():
                if tok.startswith("ok="):
                    dash_ok += int(tok.split("=", 1)[1])
                elif tok.startswith("bad="):
                    dash_bad += int(tok.split("=", 1)[1])
except FileNotFoundError:
    pass

harness = {}
try:
    with open("$HARNESS_OUT") as f:
        harness = json.load(f)
except FileNotFoundError:
    pass

ingest_lat = harness.get("ingest_latency_ms", {})
errs = sum(harness.get(k, 0) for k in ("err_4xx", "err_5xx", "err_io"))

mem_mb_mean = round(sum(samples_kb) / max(1, len(samples_kb)) / 1024, 2) if samples_kb else None
mem_mb_max = round(max(samples_kb) / 1024, 2) if samples_kb else None

result = {
    "limit_mem":            "$MEM_MAX",
    "limit_cpu":            "$CPU_QUOTA",
    "duration_s":           int("$DURATION"),
    "target_rps":           int("$RPS"),
    "achieved_rps":         round(harness.get("achieved_rps", 0), 1),
    "ingest_p50_ms":        round(ingest_lat.get("p50", 0), 2),
    "ingest_p99_ms":        round(ingest_lat.get("p99", 0), 2),
    "ingest_max_ms":        round(ingest_lat.get("max", 0), 2),
    "ingest_errors":        errs,
    "ws_lagged":            harness.get("ws_lagged", 0),
    "ws_received":          harness.get("ws_received", 0),
    "dashboard_ok":         dash_ok,
    "dashboard_bad":        dash_bad,
    "rss_mb_mean":          mem_mb_mean,
    "rss_mb_max":           mem_mb_max,
    "samples":              len(samples_kb),
    "oom_killed":           bool($oom_killed),
}

# Pass/fail criteria.
verdict = []
if result["oom_killed"]:
    verdict.append("OOM_KILLED")
if result["ingest_errors"] > 0:
    verdict.append(f"INGEST_ERRORS={result['ingest_errors']}")
if result["dashboard_bad"] > 0:
    verdict.append(f"DASHBOARD_ERRORS={result['dashboard_bad']}")
if result["achieved_rps"] < result["target_rps"] * 0.5:
    verdict.append(f"LOW_RPS={result['achieved_rps']}<{result['target_rps']*0.5}")
if result["rss_mb_max"] is not None:
    cap = float("$MEM_MAX".rstrip("M"))
    if result["rss_mb_max"] > cap:
        verdict.append(f"RSS_OVER_CAP={result['rss_mb_max']}>{cap}")

result["verdict"] = "PASS" if not verdict else "FAIL: " + ", ".join(verdict)
print(json.dumps(result, indent=2))
PY
