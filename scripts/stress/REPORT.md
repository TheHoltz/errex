# errexd stress-test report

Autonomous load test, 12 scenarios, single Linux host (WSL2). Daemon =
release `target/release/errexd`, fresh tempdir SQLite per scenario. Harness
= `scripts/stress/` (Tokio, reqwest, tokio-tungstenite). Reproducible with
`scripts/stress/run.sh` and analyzed via `scripts/stress/analyze.py`.

## Verdict

errexd is **comfortably within its self-host budget on the small-machine
profile**: zero errors across all scenarios, RSS bounded 9–19 MB, no
leak/drift over a 60s soak, WS fan-out lossless to 64 subscribers at 1k
RPS, gzip ingest works, and large-payload (64-frame stack) traffic at 500
RPS is a non-event.

The **single ceiling** observed is **~1700 events/sec sustained ingest**
(8-frame events, default config). At target rates ≥ 2000 RPS the daemon
plateaus at this rate regardless of incoming pressure, with p99 latency
rising from 5 ms → 50 ms but never erroring. Daemon CPU pegs to one core
at the plateau; harness CPU stays low. The bottleneck is the
single-writer digest task, not network or pool.

## Scenario summary

| scenario | target_rps | achieved | sent | 2xx | 5xx | io_err | ingest p99 | ingest max | ws p99 | rss max | db |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| baseline (200 RPS, 50 fp, 8 frames, 4 WS) | 200 | 182 | 4000 | 4000 | 0 | 0 | 9.0 ms | 19.4 ms | 4.0 ms | 10.8 MB | 5.6 MB |
| rps_500 | 500 | 455 | 10048 | 10048 | 0 | 0 | 5.5 ms | 54.0 ms | 5.0 ms | 16.8 MB | 14.4 MB |
| rps_1000 | 1000 | 909 | 20032 | 20032 | 0 | 0 | 4.9 ms | 55.3 ms | 5.0 ms | 18.2 MB | 28.7 MB |
| rps_2000 | 2000 | **1704** | 37690 | 37690 | 0 | 0 | 44.8 ms | 228.9 ms | 4.0 ms | 19.2 MB | 53.7 MB |
| rps_4000 | 4000 | **1690** | 37322 | 37322 | 0 | 0 | 48.9 ms | 209.0 ms | 3.0 ms | 16.2 MB | 53.1 MB |
| rps_8000 | 8000 | **1685** | 37183 | 37183 | 0 | 0 | 50.8 ms | 172.9 ms | 3.0 ms | 18.0 MB | 52.9 MB |
| big_payload (64 frames @ 500 RPS) | 500 | 455 | 10016 | 10016 | 0 | 0 | 8.7 ms | 34.1 ms | 6.0 ms | 17.9 MB | 80.9 MB |
| low_cardinality (5 fp @ 1k RPS) | 1000 | 908 | 20000 | 20000 | 0 | 0 | 10.0 ms | 55.2 ms | 4.0 ms | 13.0 MB | 28.5 MB |
| high_cardinality (5000 fp @ 1k RPS) | 1000 | 908 | 20000 | 20000 | 0 | 0 | 9.8 ms | 53.6 ms | 4.0 ms | 14.5 MB | 28.6 MB |
| ws_fanout (64 subs @ 1k RPS) | 1000 | 908 | 20000 | 20000 | 0 | 0 | 10.6 ms | 53.7 ms | 4.0 ms | 15.0 MB | 28.5 MB |
| gzip (500 RPS, gzip envelopes) | 500 | 455 | 10016 | 10016 | 0 | 0 | 7.5 ms | 35.3 ms | 5.0 ms | 16.5 MB | 14.4 MB |
| soak (60 s @ 500 RPS) | 500 | 484 | 30016 | 30016 | 0 | 0 | 7.6 ms | 54.3 ms | 4.0 ms | 15.5 MB | 43.0 MB |

The two REVIEW rows (`rps_4000`, `rps_8000`) are **not failures** — they
are the same plateau as `rps_2000`, just with bigger target gaps. Daemon
delivered 100% of the events it accepted.

## Bottleneck map

Each finding is keyed to the source location it lives in.

### F1. Throughput plateau ≈ 1700 RPS — single-writer digest task is CPU bound

- **Where**: [crates/errexd/src/digest.rs:27-142](../../crates/errexd/src/digest.rs#L27-L142) — digest receives one event at a time, runs sync fingerprint + a multi-statement upsert transaction + insert + broadcast.
- **Evidence**: `rps_2000`, `rps_4000`, `rps_8000` all converge to 1685–1704 sustained RPS. Daemon process pegs one CPU at saturation; ingest p99 climbs from 5 ms → 50 ms while throughput stays flat.
- **Per-event cost**: ~580 µs of digest work at the ceiling.
- **Why it caps here**: each event triggers (1) JSON parse, (2) fingerprint hash, (3) BEGIN tx, (4) SELECT existing issue, (5) INSERT-or-UPDATE issue, (6) COMMIT, (7) INSERT event row, (8) broadcast send. Steps 3–6 are serialized through one writer ([store.rs:184-273](../../crates/errexd/src/store.rs#L184-L273)).

**Improvement candidates** (in order of effort/payoff):

1. **Move JSON parse + fingerprint off the digest task**: parse and fingerprint in the HTTP handler ([ingest.rs:778-784](../../crates/errexd/src/ingest.rs#L778-L784)), then send a pre-fingerprinted event through the channel. This frees the single writer to do only DB I/O. Expected gain: 30–50% RPS.
2. **Batch upserts**: digest already drains one event per loop iteration. Coalesce up to N events from the channel under one transaction (pseudo-`recv_many`) and amortize BEGIN/COMMIT. Risk: increases tail latency for the *first* event in a batch. Bound the batch size (e.g., 32) and add a deadline (e.g., 5 ms) to keep p99 from blowing up.
3. **Two-tier writer**: keep `upsert_issue` single-threaded but issue `insert_event` from a small pool. Issue rows are referenced by id once it exists, so post-upsert the event insert is independent. Slightly more complex; bigger payoff at high cardinality.

### F2. No backpressure visible to operators or the SDK

- **Where**: [crates/errexd/src/ingest.rs:169-171](../../crates/errexd/src/ingest.rs#L169-L171) — `/health` returns a static `{"status":"ok"}`.
- **Evidence**: under saturation we have no way to distinguish "daemon is fine" from "ingest channel is full and HTTP handlers are awaiting capacity". An operator running a small VM can only observe the symptom (slow responses) not the cause.
- **Recommendation**: extend `/health` (or add `/metrics`) to expose, at minimum:
  - `ingest_channel.depth` and `.capacity` ([main.rs:119](../../crates/errexd/src/main.rs#L119), `mpsc::channel(256)`)
  - `webhook_channel.depth` and `.capacity` ([main.rs:124](../../crates/errexd/src/main.rs#L124), `mpsc::channel(64)`)
  - `broadcast.subscriber_count` and `.lagged_total` (today the lag is logged at debug level only at [ws.rs:83-84](../../crates/errexd/src/ws.rs#L83-L84))
  - `events_total`, `issues_total`, `digest_busy_us` (digest loop work time)
  - `rss_kb` (read from `/proc/self/status` once per request)
  - All cheap counters, no meaningful RAM cost; satisfies the "lightweight first" rule.

### F3. Default rate-limit is OFF — self-host can be unboundedly hammered

- **Where**: [crates/errexd/src/config.rs:55-56](../../crates/errexd/src/config.rs#L55-L56) — `ERREXD_RATE_LIMIT_PER_MIN` default = `0` ⇒ disabled.
- **Evidence**: 0× 429s across all scenarios, including 8000 RPS targets.
- **Recommendation**: ship a non-zero default for self-host (e.g., 6000/min ≈ 100 RPS per project, burst 200) and document that it is per-project. The current zero default invites a misbehaving SDK to consume the whole digest budget on a small VM. The token bucket itself is solid ([rate_limit.rs:16-44](../../crates/errexd/src/rate_limit.rs#L16-L44)).

### F4. Storage growth is linear in frame count and event rate

- **Evidence (DB size after 20s):**
  - 8 frames @ 1000 RPS: 28.7 MB ⇒ ~1.4 KB/event
  - 64 frames @ 500 RPS: 80.9 MB ⇒ ~8.1 KB/event
- **Implication**: at the 1700 RPS plateau with 8-frame events, storage grows ≈ **9 GB/day**. Self-host ops will need a tested retention policy.
- **Where**: [crates/errexd/src/retention.rs](../../crates/errexd/src/retention.rs) is currently 64 lines — verify that:
  1. Retention runs continuously, not just on boot.
  2. It bounds **events** per issue and **issues** per project, not just by age (an attacker who keeps a single fingerprint hot can otherwise grow storage forever within the time budget).
  3. The retention worker uses the same SQLite pool budget (≤ 4 conns) and does not contend with the digest task during the hot path.

### F5. WS broadcast is healthier than its capacity suggests

- **Where**: [crates/errexd/src/main.rs:120](../../crates/errexd/src/main.rs#L120) — `broadcast::channel(64)`.
- **Evidence**: `ws_fanout` (64 subscribers × 1 k RPS = ~64 k msg/sec on the broadcast bus) delivered **1,280,000 / 1,280,000 messages**, p99 ws-lag 4 ms, zero `Lagged` errors.
- **Why**: tokio's `broadcast::Sender::send` is per-receiver, so capacity is the **per-subscriber buffer**, not aggregate; with healthy receivers it never fills.
- **No change needed.** If/when richer WS payloads or slower clients land, revisit. If a `Lagged` ever fires it will be silent today (only a debug log at [ws.rs:83-84](../../crates/errexd/src/ws.rs#L83-L84)) — see F2: surface it as a counter.

### F6. Tail-latency spike at saturation (max ≫ p99) — likely SQLite checkpoint stutter

- **Evidence**: at the plateau, p99 is 50 ms but max hits 209 ms (`rps_4000`) and 229 ms (`rps_2000`). Below the knee the gap is small (p99 5 ms, max 55 ms), so it is not just background OS scheduling.
- **Suspected cause**: SQLite WAL checkpoint or `synchronous=NORMAL` group-commit fsync stalling the single writer mid-burst.
- **Where to investigate**: [crates/errexd/src/store.rs:160-162](../../crates/errexd/src/store.rs#L160-L162) and the migrations. Worth setting `PRAGMA wal_autocheckpoint`, `PRAGMA busy_timeout`, and possibly `PRAGMA mmap_size`. If the checkpoint hypothesis holds, scheduling checkpoints from the digest task between bursts (rather than relying on auto-checkpoint) flattens tail latency.
- **Priority**: low — at the rates errex actually targets (well below the plateau) the spikes do not surface.

### F7. Insert-heavy and update-heavy paths are equivalent

- **Evidence**: `low_cardinality` (5 fp, 1 k RPS) and `high_cardinality` (5000 fp, 1 k RPS) finish with identical achieved RPS (908) and identical latency profile (p99 ≈ 10 ms, max ≈ 54 ms).
- **Read**: the upsert + INSERT cost dominates whatever index/dedupe pressure either workload adds at this scale. Good design — no nasty surprise on either extreme.

## Harness limitations (read before acting on numbers)

1. **Achieved RPS caps at ~91% of target on the healthy path.** This is a harness pacing artifact: workers use `next += interval; if next < now { next = now }` (no catch-up), so any startup lag is permanent loss. Real SDKs have many clients each at low RPS — closer to `--workers` ≫ 32. The plateau number (1700 RPS) is the daemon's, not the harness'.
2. **`ws_max` shows occasional ~3 s spikes** despite p99 ≤ 5 ms. Investigated: the WS subscriber processes a single tokio task that locks the latency histogram per message; when many messages arrive in a tight burst the very first one's `last_seen` ends up "stale" relative to harness wall clock. **Server-side WS broadcast is fine** (counts match exactly, zero `Lagged`).
3. **No internal counters were sampled.** All daemon-side metrics (channel depth, digest busy time, broadcast lag) are inferred from the outside. Closing this gap is itself one of the report's recommendations (F2).

## Files

- `scripts/stress/Cargo.toml`, `scripts/stress/src/main.rs` — the harness.
- `scripts/stress/run.sh` — orchestrates the 12 scenarios with a fresh
  daemon per scenario.
- `scripts/stress/analyze.py` — re-aggregates the per-scenario JSONs into
  a single Markdown table; rerun any time.
- `scripts/stress/results/*.json` — raw per-scenario reports
  (configuration, latency histograms, RSS samples, DB size).

## Suggested next steps (ranked)

1. **Expose minimal metrics on `/health` or `/metrics`** (F2). Cheapest; unlocks every other improvement by making them measurable.
2. **Move JSON parse + fingerprint off the digest task** (F1.1). Biggest single throughput unlock.
3. **Batch upserts in the digest loop** (F1.2). Stacks with #2.
4. **Pick a non-zero default `rate_limit_per_min`** (F3). Operationally safer.
5. **Add a per-project retention smoke test** (F4). Storage is linear; verify the brake works under a stress profile.
6. (Lower priority) **Investigate WAL-checkpoint stutter** (F6) once the throughput unlocks above are merged — checkpoint behavior changes when the writer's duty cycle changes.
