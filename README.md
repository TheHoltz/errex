<div align="center">

# errex

**Self-hosted error tracking in 7 MB of RAM. One 5 MB binary. Zero deps. Sentry-SDK compatible.**

[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg?style=flat-square)](./LICENSE)
[![Status](https://img.shields.io/badge/status-alpha-orange?style=flat-square)](#status)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-dea584?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Stars](https://img.shields.io/github/stars/TheHoltz/errex?style=flat-square)](https://github.com/TheHoltz/errex/stargazers)

<br>

<img src="./docs/screenshots/dashboard.png" alt="errex dashboard" width="900" />

</div>

## About

errex is a tiny, **self-hostable error tracker** for people who want their own error inbox without standing up Sentry's Postgres + Redis + Kafka stack. Drop any **Sentry SDK** into your app, point it at errex, and you get grouped exceptions, stack traces, occurrence counts, regression detection, and Slack / Discord / Teams alerts.

The whole thing is **one 5 MB Rust binary** with a fast **SvelteKit dashboard** embedded. Persistence is a single **SQLite** file (no Postgres, no Redis). The ingest pipeline is single-writer with bounded buffers — backpressure, not unbounded queues.

errex is also **MCP-ready**: an AI agent can plug straight into the daemon to triage issues, summarize stack traces, and resolve duplicates without touching the dashboard. (Stub today; the protocol surface is wired.)

If you're an indie dev, a homelabber, or running a small product, this is probably what you wanted Sentry to be — **error monitoring that fits on the same $5 VPS as your app, with room to spare**.

## Numbers (measured, not estimated)

Single CPU, 30-second sustained workload, daemon `taskset -c 0`-pinned. **Idle is post-warmup** — every SPA asset (HTML + JS bundles + favicon) has been served at least once, so the dashboard is "loaded into memory" the way it would be after one operator opens it.

| operating point                       | achieved RPS | p99 ingest | RSS mean   | RSS max |
|---------------------------------------|-------------:|-----------:|-----------:|--------:|
| **idle** (daemon + SPA warm)          |          —   |        —   | **6.9 MB** |  7.0 MB |
| **typical** (100 RPS)                 |          100 |     <2 ms  | **9.8 MB** | 10.0 MB |
| **saturation** (8000 target)          |         7491 |    4.9 ms  | **10.5 MB**| 10.8 MB |
| **96 MB cgroup cap**, 4k RPS, 60 s    |         2712 |   32.8 ms  | 77 MB †    | 96 MB † |

† Cgroup memory includes kernel page cache for SQLite WAL/SHM/db files — that's what `MemoryMax=` actually enforces and what an operator pays for. **0 OOM kills, 0 ingest errors, 0 dashboard errors** under that cap.

Stripped binary (daemon + embedded SPA + assets): **5.01 MB**. Zero ingest errors at every operating point. WebSocket fan-out is lossless to 64 subscribers under sustained load. Reproduce any of these with `scripts/stress/multibench.sh` and `scripts/stress/prod_test.sh`.

### What this means for hosting cost

- **Smallest sane tier: 128 MB.** Daemon + WAL page cache fit comfortably with crash margin.
- The smallest tier on Railway / Fly / Render / etc. is **256 MB**. errex uses **2.7%** of that at idle.
- 100 RPS sustained leaves **96% of a 256 MB tier free** for your other workloads.
- Spike to 8000 events/sec: still sub-11 MB daemon RSS. No tier upgrade needed.
- **No memory cliff under cgroup pressure.** Survived sustained 4k RPS at a 96 MB cap with 0 OOM kills, 0 errors. Kernel reclaims page cache on demand.
- **Live dashboard updates over WebSocket.** Browser-validated end-to-end: events appear in the SPA in under 100 ms after ingest. No polling, no stale counters.
- Frontend is included — no second container, no nginx in front of static files, no extra service to provision.
- Compare:

| | min RAM | binary | external deps | services | dashboard | install |
|---|---:|---:|---|---:|---|---|
| **errex** | **~7 MB** | **5 MB** | none | **1** | embedded | one binary |
| GlitchTip | ~512 MB | n/a | Postgres + Redis | 3 | separate | docker-compose |
| Sentry self-host | ~4 GB | n/a | Postgres + Redis + Kafka + Snuba + Clickhouse | ~10 | separate | full stack |

> [!NOTE]
> errex is **alpha**. The hot path (ingest → group → store → broadcast) is wired and tested end-to-end. Source maps and multi-tenant orgs aren't shipped yet — see [Status](#status). Numbers above are single-host bench results; multi-day soak, 100+ simultaneous dashboard users, and login spikes (argon2 transient memory) have not been stress-validated. Plan headroom accordingly.

## Validated

- **432 tests** across daemon + SPA, all green
- `scripts/stress/multibench.sh` — measures idle, low, and saturation in one run
- `scripts/stress/prod_test.sh` — drives ingest + dashboard polling + WS subs under enforced `MemoryMax=` and `CPUQuota=` cgroups; the row above came from this
- `tests/concurrency.rs` — pins the read-vs-write contract: 16 reader tasks + 1 writer, p99 read < 100 ms
- `tests/spa.rs` — pins SPA mime coverage: CI fails if a future SvelteKit asset type lands without an explicit content-type mapping
- Real-browser smoke via Chrome DevTools MCP: SPA loads, login flow works end-to-end, live counters update over WebSocket from a curl-driven ingest stream, 0 console errors, 0 4xx/5xx

## Install

The fastest path is the prebuilt container — multi-arch (amd64 + arm64), 5 MB stripped binary inside, no dependencies:

```bash
docker run -d --name errex \
  -p 9090:9090 \
  -v errex-data:/data \
  -e ERREX_ADMIN_TOKEN="$(openssl rand -hex 16)" \
  ghcr.io/theholtz/errex:latest
```

Open <http://localhost:9090/setup>, paste the admin token from the env, finish the wizard. Done.

Or build from source:

```bash
git clone https://github.com/TheHoltz/errex
cd errex
docker compose -f docker/docker-compose.yml up -d
```

The full env / CLI reference is in [docs/CONFIGURATION.md](./docs/CONFIGURATION.md).


## How it works

```
SDK ──envelope──▶ /api/<project>/envelope/ ──▶ digest ──▶ SQLite
                                                  │
                                                  ├──▶ broadcast ──▶ WebSocket ──▶ dashboard
                                                  │
                                                  └──▶ webhook ──▶ Slack / Discord / Teams
```

Single-writer ingest pipeline; readers hit SQLite directly via WAL. No in-memory caches. Bounded channels with intentional backpressure. The dependency footprint is deliberately tiny — see [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) for the design rationale.

## Status

| | |
|---|---|
| ✅ | Sentry envelope ingest (gzip + plaintext) |
| ✅ | SQLite persistence with WAL |
| ✅ | Fingerprint-based grouping |
| ✅ | Live WebSocket updates |
| ✅ | Resolve / mute / ignore / regression detection |
| ✅ | DSN auth, retention, per-project rate limits |
| ✅ | Slack / Discord / Teams webhooks |
| 🟡 | MCP server (stub — for AI triage agents) |
| ❌ | Source maps / symbolication |
| ❌ | Multi-tenant orgs |

## Contributing

PRs welcome. Read [CONTRIBUTING.md](./CONTRIBUTING.md) and [CLAUDE.md](./CLAUDE.md) first — Rust changes require failing-test-first TDD, and `./errex.sh check` must be green.

## License

[AGPL-3.0](./LICENSE). Run errex however you want, for whatever reason. If you fork it and run the modified version as a network service, you must publish your changes. That's the whole deal.
