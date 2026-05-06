#!/usr/bin/env python3
# Heavy-seed errexd's SQLite DB with many issues spread across the past 30
# days, varied levels, exception families, and event counts. Bypasses the
# HTTP envelope path so we can backdate received_at — the daemon stamps
# ingest time at Utc::now() and won't honor a payload timestamp.
#
# Usage:
#   python3 scripts/seed-fat.py
#
# The script auto-detects the running container's DB volume, falls back to
# ./data/errex.db. Safe to run while the daemon is up — SQLite WAL handles
# the concurrent write.

from __future__ import annotations

import hashlib
import json
import os
import random
import sqlite3
import sys
import uuid
from datetime import datetime, timedelta, timezone

# ── Locate the DB ─────────────────────────────────────────────────────────
CANDIDATES = [
    "/home/r3g3n3r4/.local/share/containers/storage/volumes/docker_errex-data/_data/store/errex.db",
    os.path.join(os.path.dirname(__file__), "..", "data", "errex.db"),
]
DB_PATH = next((p for p in CANDIDATES if os.path.exists(p)), None)
if not DB_PATH:
    print("[seed-fat] no errex.db found — start the daemon first", file=sys.stderr)
    sys.exit(1)
print(f"[seed-fat] db = {DB_PATH}")

# ── Catalog of issue templates ────────────────────────────────────────────
# Each tuple: (project, level, ex_type, value, function, file, line)
TEMPLATES = [
    # web-frontend
    ("web-frontend", "error",   "TypeError",       "Cannot read properties of undefined (reading 'name')",        "renderProfile",   "src/components/Profile.tsx",    47),
    ("web-frontend", "error",   "TypeError",       "Cannot read properties of null (reading 'map')",              "renderList",      "src/components/List.tsx",       89),
    ("web-frontend", "error",   "ReferenceError",  "session is not defined",                                       "checkSession",    "src/lib/auth.ts",                18),
    ("web-frontend", "error",   "ChunkLoadError",  "Loading chunk 7 failed (timeout: /assets/main-7.js)",         "loadChunk",       "node_modules/svelte/runtime.js", 1024),
    ("web-frontend", "error",   "ChunkLoadError",  "Loading chunk 12 failed (timeout: /assets/vendor.js)",        "loadChunk",       "node_modules/svelte/runtime.js", 1024),
    ("web-frontend", "warning", "FetchError",      "Failed to fetch user profile",                                 "fetchProfile",    "src/api/user.ts",                92),
    ("web-frontend", "warning", "FetchError",      "Network request failed: /api/feed (5xx)",                      "fetchFeed",       "src/api/feed.ts",                34),
    ("web-frontend", "error",   "SyntaxError",     "Unexpected token '<' in JSON at position 0",                  "parseJson",       "src/lib/json.ts",                12),
    ("web-frontend", "warning", "DOMException",    "QuotaExceededError: Failed to set localStorage",              "saveDraft",       "src/lib/storage.ts",             45),
    ("web-frontend", "info",    "WarnDeprecated",  "useFloating: bumped from v0.8 to v1.0 — breaking",            "Tooltip",         "src/components/Tooltip.tsx",     22),

    # api-backend
    ("api-backend", "error",  "DatabaseError",  "connection timeout after 30s",                                "executeQuery",     "src/db/pool.py",               142),
    ("api-backend", "error",  "DatabaseError",  "deadlock detected on UPDATE users SET last_seen",             "updateLastSeen",   "src/db/pool.py",                89),
    ("api-backend", "error",  "ValueError",     "invalid literal for int() with base 10: 'foo'",               "parsePageParam",   "src/handlers/list.py",          24),
    ("api-backend", "error",  "ValueError",     "expected datetime, got str",                                  "deserializeOrder", "src/handlers/orders.py",        67),
    ("api-backend", "fatal",  "TimeoutError",   "Stripe request timed out",                                    "chargeCustomer",   "src/billing/stripe.py",        211),
    ("api-backend", "fatal",  "TimeoutError",   "Webhook delivery timed out after 60s",                        "deliverWebhook",   "src/billing/webhooks.py",      144),
    ("api-backend", "warning","KeyError",       "'session_id' not in request cookies",                         "requireSession",   "src/middleware/auth.py",        56),
    ("api-backend", "warning","KeyError",       "'org_id' not in jwt payload",                                  "requireOrg",       "src/middleware/auth.py",        78),
    ("api-backend", "error",  "PermissionError","User not allowed to access /admin",                            "checkAdmin",       "src/middleware/perms.py",       32),
    ("api-backend", "error",  "IntegrityError", "UNIQUE constraint failed: users.email",                        "createUser",       "src/handlers/signup.py",        58),
    ("api-backend", "fatal",  "OutOfMemoryError","Heap exhausted while building report (~3.4GB)",               "buildReport",      "src/jobs/report.py",           208),
    ("api-backend", "info",   "AuditEvent",     "Admin %s changed retention to 7d",                            "updateRetention",  "src/admin/retention.py",        91),

    # worker
    ("worker", "error",  "TaskFailedError",     "max retries (5) exceeded for job 'sync-orders'", "runJob",         "src/worker/runner.go", 88),
    ("worker", "error",  "TaskFailedError",     "max retries (3) exceeded for job 'index-build'", "runJob",         "src/worker/runner.go", 88),
    ("worker", "warning","ConnectionResetError","peer reset connection during webhook delivery", "deliverWebhook", "src/worker/webhook.go", 34),
    ("worker", "error",  "PanicError",          "runtime error: invalid memory address",          "decodePayload",  "src/worker/decode.go", 47),
    ("worker", "fatal",  "StarvationError",     "no workers available for queue 'high-prio'",     "dispatch",       "src/worker/queue.go",  19),

    # mobile
    ("mobile", "error",   "OutOfMemoryError","Failed to allocate 16MB image bitmap",            "decodeBitmap",   "ImageLoader.kt",      71),
    ("mobile", "warning", "ANRError",        "Input dispatching timed out",                     "onTouchEvent",   "MainActivity.kt",     112),
    ("mobile", "error",   "NullPointerException", "Attempt to invoke virtual method on null", "renderHeader",  "HomeFragment.kt",     44),
    ("mobile", "info",    "BatteryHigh",     "App used >5% battery in 1 hour",                  "trackUsage",     "BatteryWatcher.kt",   78),

    # ingest-pipeline (new)
    ("ingest-pipeline", "fatal",  "SchemaError",      "Avro schema mismatch: expected v3, got v2",   "consumeMsg",   "src/consumer.rs",      98),
    ("ingest-pipeline", "error",  "BackpressureError","kafka consumer lag > 10000 on topic events", "drainQueue",   "src/consumer.rs",      215),
    ("ingest-pipeline", "warning","RebalanceError",   "consumer group rebalanced 3 times in 5 min",  "rebalance",    "src/group.rs",         142),
    ("ingest-pipeline", "info",   "PartitionAssign",  "assigned 12 partitions of topic 'events'",   "assign",       "src/group.rs",          78),
]


def fingerprint(project: str, ex_type: str, fn: str, file: str, lineno: int) -> str:
    """Match the daemon's fingerprint shape — 16-hex tag from a stable hash."""
    h = hashlib.sha256(f"{project}|{ex_type}|{fn}|{file}|{lineno}".encode()).hexdigest()
    return h[:16]


def event_payload(level: str, ex_type: str, value: str, fn: str, file: str, line: int, ts: datetime) -> str:
    return json.dumps({
        "event_id": str(uuid.uuid4()).replace("-", ""),
        "timestamp": ts.isoformat().replace("+00:00", "Z"),
        "platform": "javascript",
        "level": level,
        "environment": None,
        "release": None,
        "server_name": None,
        "message": None,
        "exception": {
            "values": [{
                "type": ex_type,
                "value": value,
                "module": None,
                "stacktrace": {
                    "frames": [{
                        "function": fn, "filename": file,
                        "lineno": line, "in_app": True
                    }]
                }
            }]
        }
    }, separators=(",", ":"))


def iso(dt: datetime) -> str:
    """SQLite-friendly ISO with Z, matches existing rows' format."""
    return dt.isoformat().replace("+00:00", "Z")


# ── Seed plan ─────────────────────────────────────────────────────────────
random.seed(42)
NOW = datetime.now(timezone.utc)

# How many issues do we want, total? We'll create one per template and
# repeat templates with different fingerprints (e.g. different line numbers)
# to reach the target.
TARGET_ISSUES = 60

con = sqlite3.connect(DB_PATH, isolation_level=None)  # autocommit per stmt
con.execute("PRAGMA journal_mode=WAL")
con.execute("PRAGMA busy_timeout=10000")

# Don't blow away existing data — additive.
existing = {(p, fp) for p, fp in con.execute("SELECT project, fingerprint FROM issues")}
print(f"[seed-fat] existing issues: {len(existing)}")

inserted_issues = 0
inserted_events = 0

con.execute("BEGIN")
try:
    for i in range(TARGET_ISSUES):
        tpl = TEMPLATES[i % len(TEMPLATES)]
        project, level, ex_type, value, fn, file, line = tpl

        # Vary line number on repeats so the fingerprint is distinct
        if i >= len(TEMPLATES):
            line = line + (i // len(TEMPLATES)) * 7

        fp = fingerprint(project, ex_type, fn, file, line)
        if (project, fp) in existing:
            continue
        existing.add((project, fp))

        # Spread first_seen across past 30 days; last_seen drifts forward.
        # Bias the distribution so ~30% are "today/yesterday" (recent),
        # ~50% within the past week, the rest within 30 days.
        bucket = random.random()
        if bucket < 0.3:
            first_seen = NOW - timedelta(hours=random.uniform(0, 24))
            last_seen = NOW - timedelta(minutes=random.uniform(0, 60 * 23))
        elif bucket < 0.6:
            days_ago = random.uniform(1, 3)
            first_seen = NOW - timedelta(days=days_ago)
            last_seen = NOW - timedelta(hours=random.uniform(0, 24))
        elif bucket < 0.85:
            days_ago = random.uniform(3, 7)
            first_seen = NOW - timedelta(days=days_ago)
            last_seen = first_seen + timedelta(hours=random.uniform(1, 48))
        else:
            days_ago = random.uniform(7, 30)
            first_seen = NOW - timedelta(days=days_ago)
            last_seen = first_seen + timedelta(hours=random.uniform(1, 72))

        # Event count: heavy-tailed. Most issues 1-20, some 50-200, a few 500+.
        r = random.random()
        if r < 0.5:
            event_count = random.randint(1, 20)
        elif r < 0.85:
            event_count = random.randint(20, 200)
        elif r < 0.97:
            event_count = random.randint(200, 800)
        else:
            event_count = random.randint(800, 2500)

        # Status: mostly unresolved, some resolved/muted.
        s = random.random()
        status = "unresolved" if s < 0.78 else ("resolved" if s < 0.93 else "muted")

        culprit = f"{fn} in {file}"

        cur = con.execute(
            "INSERT INTO issues (project, fingerprint, title, culprit, level, event_count, first_seen, last_seen, status) "
            "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            (project, fp, f"{ex_type}: {value}", culprit, level, event_count, iso(first_seen), iso(last_seen), status)
        )
        issue_id = cur.lastrowid
        inserted_issues += 1

        # Insert SOME real events for this issue — enough for the sparkline to
        # have signal but capped so we don't bloat the DB. Spread received_at
        # between first_seen and last_seen.
        n_events = min(event_count, 24)
        for _ in range(n_events):
            t = first_seen + (last_seen - first_seen) * random.random()
            payload = event_payload(level, ex_type, value, fn, file, line, t)
            con.execute(
                "INSERT INTO events (issue_id, event_id, payload, received_at) VALUES (?, ?, ?, ?)",
                (issue_id, str(uuid.uuid4()).replace("-", ""), payload, iso(t))
            )
            inserted_events += 1

    # Make sure projects rows exist for any new ones we referenced.
    project_set = {t[0] for t in TEMPLATES}
    existing_projects = {r[0] for r in con.execute("SELECT name FROM projects")}
    for p in project_set - existing_projects:
        # projects table may have additional cols (dsn, created_at). Insert
        # with INSERT OR IGNORE and hope the schema has defaults. If not,
        # fall back to a minimal set.
        try:
            con.execute("INSERT OR IGNORE INTO projects (name) VALUES (?)", (p,))
        except Exception as e:
            print(f"  ! could not insert project {p}: {e}")

    con.execute("COMMIT")
except Exception:
    con.execute("ROLLBACK")
    raise

print(f"[seed-fat] inserted {inserted_issues} issues, {inserted_events} events")
print(f"[seed-fat] db now has {con.execute('SELECT COUNT(*) FROM issues').fetchone()[0]} issues, "
      f"{con.execute('SELECT COUNT(*) FROM events').fetchone()[0]} events, "
      f"{con.execute('SELECT COUNT(*) FROM projects').fetchone()[0]} projects")
con.close()
