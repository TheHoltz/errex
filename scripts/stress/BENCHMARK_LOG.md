# errexd optimization log (Ralph loop)

**Goal:** maximize `efficiency_eps_per_mb = achieved_rps / rss_max_mb` for
the rps_4000 saturation profile, until plateau.

**Why this metric:** the project's load-bearing constraint is
"low hosting cost". Pure throughput rewards inflating RAM; pure RSS
minimization rewards starving the daemon. The ratio aligns the loop
with what the operator actually pays for.

## Hard constraints (any violation → revert iteration)

- `errors == 0` in the bench output
- `max_ms ≤ 500` (tail latency budget)
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`
  both pass
- `cargo test --workspace` all green
- `cd web && bun run check && bun run test` all green
- No changes to the Sentry envelope wire format (`/api/:project/envelope/`
  body parsing) or the WS `ServerMessage` shape
- No changes to the bench harness or its parameters that would inflate
  the metric without representing a real improvement

## Bench setup (deterministic across iterations)

- `scripts/stress/bench.sh` — boots a fresh release errexd pinned to
  `taskset -c 0` (one physical CPU), runs the harness for 30 s at
  rps=4000, 64 workers, cardinality 50, 8 frames, 4 projects, 4 WS subs.
- Fresh tempdir SQLite per run.
- Output is a single JSON line (see `bench.sh`).

## Termination

- 3 consecutive iterations with efficiency delta < +2% over the running
  best, OR
- 15 iterations total, OR
- `efficiency_eps_per_mb ≥ 200` (stretch goal — would mean we doubled
  efficiency from baseline)

When termination fires, emit `<promise>OPTIMIZATION_PLATEAU_REACHED</promise>`
followed by a one-paragraph summary.

## Hypothesis bank (priority order; pick highest-payoff not yet tried)

1. **Move `serde_json::to_string(event)` to the HTTP handler**, ship a
   pre-serialized payload string through the channel so the digest task
   does no JSON work. Today the serialize is inside
   `Store::upsert_batch_with_events`.
2. **Multi-row event INSERT** — replace the per-event INSERT loop in
   `upsert_batch_with_events` with one statement using N value-tuples.
3. **Explicit `PRAGMA wal_checkpoint(PASSIVE)`** between batches when the
   ingest channel becomes empty. Targets the tail-max stall (~1 s).
4. **Stop converting `event.event_id` to a String per insert**; use
   `as_simple()` which writes to a stack buffer.
5. **Stop binding `received_at` as RFC3339 string** in upsert; either
   bind a chrono `DateTime<Utc>` directly (sqlx supports it) or store
   as integer (seconds since epoch). Saves the format() per call.
6. **Pre-allocate `Vec<BatchUpsertInput>`** in `digest::run` once
   instead of inside `process_batch`. Reuse across batches. (Verify
   current state — may already be done.)
7. **`synchronous = OFF`** (NOT NORMAL). Trade-off: SQLite drops up to
   one transaction's writes on host power loss (not on daemon crash).
   For an error tracker that already accepts SDK retries, this is
   tolerable. Document in a code comment if applied.
8. **Profile first, then act** — `cargo flamegraph` or perf if
   available. If a single fn dominates, attack it directly.
9. **Reduce `BATCH_SIZE`** to 16 or **increase to 64** — current 32 was
   first guess, never measured. Bench both.
10. **Reduce `Event` struct size** — `serde(skip_serializing_if = "Option::is_none")` on
    optional fields lowers the JSON size and parse cost; field ordering
    can shrink the struct itself.

If the bank is exhausted without plateau, propose a new hypothesis
explicitly in the log entry before implementing.

---

## Iterations

### Iteration 0 — baseline

- **commit:** b598ecf (post-overhaul main)
- **hypothesis:** none — establish reproducible baseline.
- **changed:** none.
- **bench:** `{"achieved_rps":3749.2,"p99_ms":8.59,"max_ms":18.83,"rss_max_mb":23.06,"errors":0,"efficiency_eps_per_mb":162.59}`
- **decision:** BASELINE
- **notes:** Earlier sweep showed `max_ms ≈ 1037 ms` at rps_4000. After
  pinning to one CPU and running 30 s instead of 20 s, the tail
  collapses to 18.83 ms — the 1 s outlier was scheduler / WSL2 noise,
  not a WAL checkpoint stall. Tail-latency optimization (hypothesis 3
  in the bank) is now low priority unless it resurfaces.

### Iteration 3 — `synchronous = OFF`

- **hypothesis (bank #7):** SQLite's COMMIT fsync is the dominant
  per-batch cost on real disks (ext4 here). `synchronous=OFF` skips
  the fsync; trade-off is up to one txn lost on host power loss
  (NOT on daemon crash — WAL still recovers).
- **changed:** `crates/errexd/src/store.rs` (`SqliteSynchronous::Normal`
  → `Off`, comment block expanded with the trade-off rationale).
- **bench:** `{"achieved_rps":3748.6,"p99_ms":5.33,"max_ms":54.56,"rss_max_mb":20.75,"errors":0,"efficiency_eps_per_mb":180.66}`
- **delta vs running best (iter-1, 164.23):** efficiency +10.0%
  (164.23 → 180.66). RSS -9% (22.83 → 20.75 MB). Throughput flat.
  max regressed (11.36 → 54.56 ms) but still well under the 500 ms
  gate.
- **decision:** KEPT. Running best is now 180.66.
- **notes:** Throughput unchanged — saturation is somewhere else
  (HTTP parse / channel send / harness worker pacing at ~3750 RPS for
  4000 target with 64 workers). RSS dropped because async fsync needs
  less metadata buffering. Tail max grew because WAL checkpoint now
  runs without fsync and lands at random — still small in absolute
  terms. Trade-off for "ingest-on-cheap-host" is solidly worth it.

### Iteration 2 — multi-row event INSERT

- **hypothesis (bank #2):** replace per-event `INSERT INTO events ...`
  loop in `Store::upsert_batch_with_events` with one `INSERT ... VALUES
  (?,?,?), (?,?,?), ...` for the whole batch. Build via
  `sqlx::QueryBuilder::push_values`.
- **changed:** `crates/errexd/src/store.rs` (events INSERT lifted out
  of the per-input loop, batched after issue upsert phase).
- **bench:** `{"achieved_rps":3748.5,"p99_ms":5.24,"max_ms":54.21,"rss_max_mb":27.69,"errors":0,"efficiency_eps_per_mb":135.37}`
- **delta vs running best (iter-1, 164.23):** efficiency -17.6%
  (164.23 → 135.37). RSS +21% (22.83 → 27.69 MB). max regressed
  +377% (11.36 → 54.21 ms). Throughput unchanged.
- **decision:** REVERTED via `git restore .`.
- **notes:** Multi-row INSERT didn't move throughput — events INSERT
  was already cheap (sqlx prepared-statement cache covers the per-row
  parse). What it DID do: `QueryBuilder` builds a fresh SQL string
  + bind buffer per batch (variable N, can't be cached), pushing RSS
  up ~5 MB. Since efficiency metric penalizes RSS, this is a clear
  regression. Throughput bottleneck must be elsewhere — likely the
  issue UPSERT's BEGIN+SELECT+UPDATE round-trips, not the event
  insert.

### Iteration 1 — pre-serialize JSON + event_id in HTTP handler

- **hypothesis (bank #1 + #4):** move `serde_json::to_string(event)` and
  `Uuid::to_string` out of the digest's transaction. Ship pre-computed
  `event_id` and `payload` strings on `IngestEvent`; `BatchUpsertInput`
  borrows the strings, the writer just binds them.
- **changed:** `crates/errexd/src/digest.rs`,
  `crates/errexd/src/store.rs`, `crates/errexd/tests/store.rs`.
- **bench:** `{"achieved_rps":3749.6,"p99_ms":5.32,"max_ms":11.36,"rss_max_mb":22.83,"errors":0,"efficiency_eps_per_mb":164.23}`
- **delta vs baseline:** efficiency +1.0% (162.59 → 164.23). p99 -38%
  (8.59 → 5.32 ms). max -40% (18.83 → 11.36 ms). RSS -1%.
- **decision:** KEPT.
- **notes:** Throughput at saturation barely moved — the JSON serialize
  wasn't the throughput bottleneck. But it was clearly the
  per-event-latency bottleneck: doing the serialize on whichever HTTP
  worker thread is handling the request frees the single SQLite writer
  to spend its time on I/O only. Saturation likely now bottlenecked on
  raw SQLite write rate (next: hypothesis #2, multi-row INSERT).


