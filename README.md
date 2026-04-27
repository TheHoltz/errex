<div align="center">

# errex

**Self-hostable, Sentry-SDK-compatible error tracking — one binary, one SQLite file, fits on a $5 VPS.**

[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg?style=flat-square)](./LICENSE)
[![Status](https://img.shields.io/badge/status-alpha-orange?style=flat-square)](#status)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-dea584?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Stars](https://img.shields.io/github/stars/TheHoltz/errex?style=flat-square)](https://github.com/TheHoltz/errex/stargazers)

<br>

<img src="./docs/screenshots/dashboard.png" alt="errex dashboard" width="900" />

</div>

## About

errex is a tiny error tracker for people who want their own error inbox without standing up Sentry's Postgres + Redis + Kafka stack. Drop any Sentry SDK into your app, point it at errex, and you get grouped exceptions, stack traces, occurrence counts, regression detection, and Slack/Discord/Teams alerts.

The whole thing is **one Rust binary** with the SvelteKit dashboard embedded. Persistence is a single **SQLite** file. RAM is around **10 MB at idle**. If you're an indie dev or a homelabber, this is probably what you wanted Sentry to be.

> [!NOTE]
> errex is **alpha**. The hot path (ingest → group → store → broadcast) is wired and tested end-to-end. Source maps and multi-tenant orgs aren't shipped yet — see [Status](#status).

## Install

```bash
git clone https://github.com/TheHoltz/errex
cd errex
docker compose -f docker/docker-compose.yml up -d
```

Open <http://localhost:9090>, finish the first-run setup wizard, and you're in. The full env / CLI reference is in [docs/CONFIGURATION.md](./docs/CONFIGURATION.md).

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
