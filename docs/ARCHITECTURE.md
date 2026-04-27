# Architecture

errex is two binaries' worth of work in one process: a Sentry-compatible
ingest daemon and an embedded SvelteKit dashboard. The whole thing is
built around a small set of constraints.

## Pipeline

```
                                          ┌────────────────────┐
SDK ──HTTP─▶  /api/<proj>/envelope/  ─▶  │  digest task       │
                                          │  (single writer)   │
                                          └─┬──────────┬───────┘
                                            │          │
                                            ▼          ▼
                                          SQLite   broadcast::Sender
                                            │          │
                              ┌─────────────┴──┐  ┌────┴───────┐
                              ▼                ▼  ▼            ▼
                     /api/issues, /event   webhook task    /ws/<proj> upgrade
                     (REST, embedded SPA)   (POST out)     (axum, same :9090)
```

Both REST and the WebSocket fan-out share the axum listener on `:9090`
— the SPA's WS URL is built from `location.host`, so unifying the ports
keeps the upgrade reachable in production without env-time configuration.

Three things move through this pipeline:

1. **Events** arrive as Sentry envelopes on `:9090`. The HTTP handler
   parses, validates, and pushes them onto a bounded mpsc channel.
2. **The digest task** drains that channel single-threaded, derives a
   stable fingerprint, upserts the corresponding `issue` row, inserts
   the event, and emits a broadcast notification.
3. **Subscribers** — the webhook task and the WebSocket fan-out — react
   to the broadcast and don't touch the database in the hot path.

## Hard constraints

These are decisions, not aspirations. Don't undo them without a measured
reason.

### Single-writer ingest

All DB mutations during the hot path go through the digest task. Readers
(REST API, WebSocket snapshot, admin CLI) are unbounded and concurrent
thanks to SQLite WAL mode, but writes are serialized. This:

- Keeps the sqlx pool small (≤4 connections).
- Eliminates write contention without explicit locking.
- Makes invariants like "fingerprint upsert and event insert happen
  together" trivially true without a transaction wrapper around every
  call site.

The CLI is the only other writer, and it's used outside the hot path
(project add/rotate/etc.).

### No in-memory issue cache

When the SPA opens a WebSocket, errex queries SQLite for the issue list
on every connect. That's sub-millisecond on the SQLite WAL and saves
N×Issue worth of RAM that would otherwise grow with the project. There's
no LRU, no Redis, no cache invalidation problem.

### Bounded buffers

Every channel has a fixed capacity:

- Ingest mpsc: 256
- Broadcast: 64
- Webhook mpsc: 64

If a buffer is full, the producer is rate-limited or events are dropped
with a tracing warning. Backpressure is intentional — unbounded queues
are how observability tools page their oncall at 3 AM.

### Embedded SPA

The SvelteKit dashboard is built statically and embedded in the binary
via `rust-embed`. There is no separate web server process, no static
hosting bucket, no service worker. Updating the dashboard means
recompiling the binary.

### Single SQLite file

No Postgres dep, no MySQL dep. Migrations are timestamp-prefixed `.sql`
files in `crates/errexd/migrations/`, applied at boot via sqlx. WAL mode
is on. Backups are `cp data/errex.db.*` while the daemon is running.

## What the wire types look like

The `errex-proto` crate holds everything that crosses a boundary:

- `Issue` — grouped exception (one row in `issues`)
- `Event` — single occurrence (one row in `events`)
- `Fingerprint` — 16-byte hash that groups events into issues
- `ServerMessage` / `ClientMessage` — WebSocket frames

These are pure data + serde. Any change to their JSON shape is a wire
break and must be matched by `web/src/lib/types.ts` and a new SQLx
migration if columns change.

## Why Rust, why SvelteKit

Rust because the binary is intended to live on someone else's hardware
and the whole pitch is "lightweight" — managed-language baselines
(Node, Python, JVM) are too heavy for the target deployment.

SvelteKit because the dashboard is small enough to ship with no
client-side router framework drama, and the static-adapter output drops
straight into `rust-embed`. Runes give us reactive state without the
import-store ceremony.

## What's deliberately missing

- **No source maps.** Symbolication is a real project on its own; errex
  doesn't pretend to do it yet.
- **No multi-tenancy.** errex is one tenant's error tracker. If you need
  isolation between teams or customers, run multiple instances.
- **No clustering.** SQLite is the storage layer; horizontal scaling is
  not in scope.
- **No retention beyond N days.** The retention task purges old events
  but doesn't archive them anywhere. Self-hosters who want long-term
  history should snapshot the SQLite file out of band.
