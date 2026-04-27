# errex

A Sentry-SDK-compatible error tracking daemon plus a SvelteKit dashboard,
shipped as a single ~6 MB binary with an embedded SPA. Aimed at self-host
on a personal VPS / homelab where you want your own error inbox without
standing up Sentry's full Postgres + Redis + Kafka stack.

- **One binary.** Daemon, dashboard, and SQLite all in one process.
- **Sentry-SDK compatible.** Point any `@sentry/*` SDK at errex and it just works.
- **Lightweight.** Single SQLite file, ~50 MB image, ~10 MB RSS at idle.
- **AGPL-3.0.** No telemetry. Self-host freely; if you fork errex and run the modified version as a network service, you must publish your changes.

```bash
docker compose -f docker/docker-compose.yml up -d
docker compose -f docker/docker-compose.yml exec errexd errexd project add my-app
# → prints DSN → drop into your SDK
open http://localhost:9090
```

> Status: **alpha**. Persistence, status sharing, DSN tokens, retention,
> rate limiting, and webhook alerts are wired and tested. Source maps,
> multi-tenant orgs, and a real MCP server are still TODO.

## Quickstart

### 1. Run the daemon

```bash
git clone https://github.com/TheHoltz/errex
cd errex
docker compose -f docker/docker-compose.yml up -d
```

The daemon listens on:

- `:9090` — HTTP (Sentry ingest + the SPA)
- `:9091` — WebSocket (live updates for the SPA)
- `:9092` — MCP (stub)

A SQLite file lands in `./data/errex.db`.

### 2. Create a project + DSN

```bash
docker compose -f docker/docker-compose.yml exec errexd \
  errexd project add my-app --public-url https://errex.example.com
```

Output:

```
project: my-app
token:   3f4a9b8e2c1d4f5e8a7b6c5d4e3f2a1b
dsn:     https://errex.example.com/api/my-app/envelope/?sentry_key=3f4a9b8e2c1d4f5e8a7b6c5d4e3f2a1b
```

To require this token on ingest, set `ERREXD_REQUIRE_AUTH=true`. Off by
default — fine when the daemon is on a private network.

### 3. Point an SDK at it

JavaScript / TypeScript:

```js
import * as Sentry from '@sentry/browser';
Sentry.init({
  dsn: 'https://errex.example.com/api/my-app/envelope/?sentry_key=3f4a...'
});
```

Python:

```python
import sentry_sdk
sentry_sdk.init(dsn="https://errex.example.com/api/my-app/envelope/?sentry_key=3f4a...")
```

The SDK's normal DSN parsing also works:

```
sentry_sdk.init(dsn="https://3f4a...@errex.example.com/my-app")
```

errex accepts the token via either `?sentry_key=` or the standard
`X-Sentry-Auth` header.

### 4. Open the dashboard

```
http://localhost:9090
```

Cmd-K opens the command palette. `j`/`k` navigate the issue list, `e`
resolves, `m` mutes, `a` assigns to you.

## Configuration (env)

| Variable | Default | What it does |
|---|---|---|
| `ERREXD_DATA_DIR` | `./data` | SQLite file location |
| `ERREXD_HTTP_PORT` | `9090` | HTTP + SPA port |
| `ERREXD_WS_PORT` | `9091` | WebSocket fan-out port |
| `ERREXD_MCP_PORT` | `9092` | MCP listener (stub) |
| `ERREXD_LOG_LEVEL` | `info` | tracing filter |
| `ERREXD_DEV_MODE` | `false` | Enable CORS for the Vite dev server |
| `ERREXD_REQUIRE_AUTH` | `false` | Validate `sentry_key` on ingest |
| `ERREXD_RETENTION_DAYS` | `30` | Purge events older than N days; `0` disables |
| `ERREXD_RATE_LIMIT_PER_MIN` | `0` | Per-project ingest cap; `0` = unlimited |
| `ERREXD_RATE_LIMIT_BURST` | `200` | Token-bucket burst capacity |
| `ERREXD_PUBLIC_URL` | `http://localhost:9090` | Embedded in webhook payloads |

## Project management

All project ops are CLI subcommands. They open the SQLite file directly
(WAL mode), so you can run them while the daemon is up.

```bash
errexd project add <name> [--public-url URL]    # create + emit DSN
errexd project list                              # show tokens + last-used
errexd project rotate <name>                     # invalidate previous DSN
errexd project set-webhook <name> <url>          # Slack/Discord/Teams URL
errexd project unset-webhook <name>
```

Inside Docker:

```bash
docker compose -f docker/docker-compose.yml exec errexd errexd project list
```

## Webhook alerts

Set a webhook URL on a project and errex will POST a Slack-compatible
payload on:

- **First occurrence** of a new fingerprint (color: `danger`)
- **Regression** — a resolved issue saw a new event (color: `warning`)

Slack / Discord (with the `/slack` suffix) / Teams "Incoming Webhook"
all accept the same shape.

```bash
errexd project set-webhook my-app https://hooks.slack.com/services/T0/B0/XXXX
```

Muted and ignored issues never fire webhooks.

## Architecture

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
                     /api/issues, /event   webhook task    WebSocket fan-out
                     (REST, embedded SPA)   (POST out)     :9091 → SPA
```

- **Single writer.** All DB mutations go through the digest task. Readers
  are unbounded and concurrent thanks to SQLite WAL.
- **No in-memory issue cache.** WS snapshot queries the DB on every connect
  — sub-millisecond, saves N×Issue worth of RAM.
- **Bounded buffers.** Ingest mpsc (256), broadcast (64), webhook mpsc (64).
  Backpressure is intentional.

## Development

```bash
./errex.sh up -d               # start daemon (Docker)
cd web && bun install && bun run dev   # SPA on :5173 with HMR proxy
```

Rust changes:

```bash
./errex.sh check               # fmt + clippy + cargo test --workspace
./errex.sh build               # rebuild docker image
./errex.sh logs                # tail daemon logs
./errex.sh test-event          # send a sample envelope
./scripts/seed.sh              # seed dashboard with realistic issues
```

### TDD is mandatory for Rust changes

Read [`CLAUDE.md`](./CLAUDE.md) before contributing. Short version:

- Every Rust change ships with tests; failing test FIRST.
- `./errex.sh check` must pass — fmt + clippy `-D warnings` + workspace tests.
- The frontend (`web/`) is exempt from TDD; it iterates visually.

## Status

| Feature | Status |
|---|---|
| Sentry envelope ingest | ✅ |
| SQLite persistence | ✅ |
| Issue grouping (fingerprint) | ✅ scaffolded; needs better algorithm before scale |
| WebSocket live updates | ✅ |
| Status sharing (resolve/mute/ignore) | ✅ |
| Regression detection | ✅ |
| DSN tokens / ingest auth | ✅ optional |
| Retention | ✅ |
| Rate limiting | ✅ |
| Slack/Discord/Teams webhooks | ✅ |
| SvelteKit dashboard | ✅ |
| Source maps / symbolication | ❌ |
| Multi-tenant orgs | ❌ |
| MCP server | 🟡 stub |
| AI triage | 🟡 stub |

## License

[AGPL-3.0](./LICENSE). errex is free software — you can run it, modify
it, and redistribute it on any terms compatible with the AGPL. The
"network use" clause means: if you run a modified version of errex as a
service that other people interact with over the network, you must make
the source of your modified version available to those users. Running
unmodified errex is unrestricted.
