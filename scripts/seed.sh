#!/usr/bin/env bash
# Seed errexd with a realistic mix of issues across multiple projects so the
# dashboard's UX shines: variety of severities, exception types, event
# counts, and a recent burst that trips the spike detector.
#
# Usage:
#   ./scripts/seed.sh                # default base http://127.0.0.1:9090
#   ERREX_BASE=http://host:port ./scripts/seed.sh

set -euo pipefail

BASE="${ERREX_BASE:-http://127.0.0.1:9090}"

uuid() {
  # uuidgen on macOS prints uppercase with dashes; the daemon doesn't care,
  # but lowercase no-dash matches Sentry's wire format.
  if command -v uuidgen >/dev/null 2>&1; then
    uuidgen | tr -d - | tr '[:upper:]' '[:lower:]'
  else
    cat /proc/sys/kernel/random/uuid | tr -d -
  fi
}

# Send a single Sentry envelope. Args:
#   $1 project   $2 level   $3 ex_type   $4 ex_value   $5 function   $6 filename   $7 lineno
send_event() {
  local project="$1" level="$2" ty="$3" val="$4" fn="$5" file="$6" line="$7"
  local now id
  now="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  id="$(uuid)"

  printf '%s\n%s\n%s\n' \
    "{\"event_id\":\"$id\",\"sent_at\":\"$now\"}" \
    '{"type":"event"}' \
    "{\"event_id\":\"$id\",\"timestamp\":\"$now\",\"platform\":\"javascript\",\"level\":\"$level\",\"exception\":{\"values\":[{\"type\":\"$ty\",\"value\":\"$val\",\"stacktrace\":{\"frames\":[{\"function\":\"$fn\",\"filename\":\"$file\",\"lineno\":$line,\"in_app\":true}]}}]}}" \
    | curl -sf -X POST --data-binary @- \
        "$BASE/api/${project}/envelope/" >/dev/null
}

# Send N copies of the same event (groups under one fingerprint).
burst() {
  local n="$1"; shift
  for _ in $(seq 1 "$n"); do
    send_event "$@"
  done
}

echo "[seed] base = $BASE"

# --- web-frontend: classic JS errors ---
echo "[seed] web-frontend"
burst 12 web-frontend error    "TypeError"        "Cannot read properties of undefined (reading 'name')" "renderProfile" "src/components/Profile.tsx" 47
burst 4  web-frontend error    "ReferenceError"   "session is not defined"                                "checkSession"  "src/lib/auth.ts"             18
burst 2  web-frontend warning  "FetchError"       "Failed to fetch user profile"                          "fetchProfile"  "src/api/user.ts"             92

# --- api-backend: server-side ---
echo "[seed] api-backend"
burst 87  api-backend error  "DatabaseError"  "connection timeout after 30s"        "executeQuery" "src/db/pool.py"     142
burst 156 api-backend error  "ValueError"     "invalid literal for int() with base 10: 'foo'" "parsePageParam" "src/handlers/list.py" 24
burst 8   api-backend fatal  "TimeoutError"   "Stripe request timed out"            "chargeCustomer" "src/billing/stripe.py" 211
burst 3   api-backend warning "KeyError"      "'session_id' not in request cookies" "requireSession" "src/middleware/auth.py" 56

# --- worker: background jobs ---
echo "[seed] worker"
burst 22 worker error   "TaskFailedError"     "max retries (5) exceeded for job 'sync-orders'" "runJob" "src/worker/runner.go" 88
burst 4  worker warning "ConnectionResetError" "peer reset connection during webhook delivery" "deliverWebhook" "src/worker/webhook.go" 34

# --- mobile: a quieter project ---
echo "[seed] mobile"
burst 6 mobile error "OutOfMemoryError" "Failed to allocate 16MB image bitmap" "decodeBitmap" "ImageLoader.kt" 71

# --- recent burst: trip the spike detector on web-frontend ---
# Wait a moment so these arrive *now* relative to the dashboard's clock,
# then send a tight cluster. The spike heuristic needs ≥5 recent events.
echo "[seed] firing spike on web-frontend ChunkLoadError"
sleep 1
for _ in $(seq 1 15); do
  send_event web-frontend error "ChunkLoadError" "Loading chunk 7 failed (timeout: /assets/main-7.js)" "loadChunk" "node_modules/svelte/runtime.js" 1024
  sleep 0.05
done

echo "[seed] done — 13 issues seeded across 4 projects"
