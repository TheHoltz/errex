# Configuration

Everything you need to operate an errex deployment: env vars, the `errexd`
CLI, and how to wire up SDKs and webhooks.

## Environment variables

All configuration is via env vars. Defaults are sensible for a single-host
self-host.

| Variable | Default | What it does |
|---|---|---|
| `ERREXD_DATA_DIR` | `./data` | SQLite file location |
| `ERREXD_HTTP_PORT` | `PORT` env, else `9090` | HTTP + SPA + WebSocket fan-out (single axum listener). Reads `PORT` automatically so Railway / Fly / Render / Heroku one-click deploys "just work". |
| `ERREXD_MCP_PORT` | `9092` | MCP listener (stub) |
| `ERREXD_LOG_LEVEL` | `info` | tracing filter |
| `ERREXD_DEV_MODE` | `false` | Enable CORS for the Vite dev server |
| `ERREXD_REQUIRE_AUTH` | `false` | Validate `sentry_key` on ingest |
| `ERREXD_RETENTION_DAYS` | `30` | Purge events older than N days; `0` disables |
| `ERREXD_RATE_LIMIT_PER_MIN` | `6000` | Per-project ingest cap; `0` = unlimited |
| `ERREXD_RATE_LIMIT_BURST` | `200` | Token-bucket burst capacity |
| `ERREXD_PUBLIC_URL` | `http://localhost:9090` | **Set this for any non-localhost deployment.** Embedded in DSNs, webhook payloads, and the dashboard's curl example. The daemon warns at boot if you bound to a public interface but left this at the default. |
| `ERREXD_ADMIN_TOKEN` | _(unset)_ | Setup-wizard token; required to bootstrap the first admin user |

## Custom domain — one env var

To put errex behind `https://errex.example.com`, set exactly one variable:

```bash
ERREXD_PUBLIC_URL=https://errex.example.com
```

That's it. The DSN handed to SDKs becomes
`https://<token>@errex.example.com/<project>` (the canonical Sentry shape
every SDK parses). Webhook payloads link back to
`https://errex.example.com/issues/<id>`. The dashboard's curl example
points at `https://errex.example.com/api/<project>/envelope/`.

Per-platform recipes:

### Railway / Fly / Render / Heroku

1. Add a custom domain in the platform UI.
2. Set the service env vars:
   - `ERREXD_PUBLIC_URL=https://errex.example.com`
   - `ERREXD_ADMIN_TOKEN=$(openssl rand -hex 16)` (first run only)
3. Deploy. The platform sets `PORT`; errexd reads it automatically.
   No `ERREXD_HTTP_PORT` override needed.

### Caddy

```Caddyfile
errex.example.com {
    reverse_proxy localhost:9090
}
```

```bash
ERREXD_PUBLIC_URL=https://errex.example.com docker compose -f docker/docker-compose.yml up -d
```

Caddy handles TLS automatically.

### Nginx / Traefik

Forward `Host`, `X-Forwarded-Proto`, and the `Upgrade`/`Connection`
headers required for the WebSocket. The SPA builds its WS URL from
`window.location`, so as long as the proxy upgrade path is wired,
realtime updates work without extra config.

```nginx
server {
    listen 443 ssl;
    server_name errex.example.com;

    location / {
        proxy_pass http://127.0.0.1:9090;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection $connection_upgrade;
    }
}
```

### Ports

errex listens on two ports by default:

- `9090` — HTTP (Sentry ingest, REST API, embedded SPA, WebSocket upgrade)
- `9092` — MCP listener (stub for AI agents)

If you put errex behind a reverse proxy, forward everything to `9090` —
the SPA, REST API, and WebSocket fan-out all share that listener. The SPA
builds its WS URL from `location.host`, so anything that already proxies
HTTP correctly will handle `/ws/<project>` upgrades without extra config
(make sure the proxy passes through `Upgrade`/`Connection` headers).

## First-run setup

The setup wizard at `/setup` requires a one-shot **setup token** so an
attacker can't claim the admin slot before you do. Set
`ERREXD_ADMIN_TOKEN` to a value you control before starting the daemon
the first time:

```yaml
# docker/docker-compose.yml
environment:
  ERREXD_ADMIN_TOKEN: ${ERREXD_ADMIN_TOKEN:-changeme}
```

```bash
ERREXD_ADMIN_TOKEN=$(openssl rand -hex 16) docker compose -f docker/docker-compose.yml up -d
```

Open <http://localhost:9090/setup>, paste the token, choose a username
and password. After the first admin exists, the setup endpoint refuses
all further calls regardless of token.

## Project management

All project ops are CLI subcommands on the `errexd` binary. They open the
SQLite file directly (WAL mode), so you can run them while the daemon is
up.

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

`project add` prints the DSN you give to your SDK:

```
project: my-app
token:   3f4a9b8e2c1d4f5e8a7b6c5d4e3f2a1b
dsn:     https://errex.example.com/api/my-app/envelope/?sentry_key=3f4a9b8e2c1d4f5e8a7b6c5d4e3f2a1b
```

To require this token on ingest, set `ERREXD_REQUIRE_AUTH=true`. Off by
default — fine when the daemon is on a private network.

## Wiring up an SDK

errex speaks the [Sentry envelope](https://develop.sentry.dev/sdk/envelopes/)
wire format. Any official `@sentry/*` SDK works unchanged.

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

```python
sentry_sdk.init(dsn="https://3f4a...@errex.example.com/my-app")
```

errex accepts the token via either `?sentry_key=` or the standard
`X-Sentry-Auth` header.

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

## Dashboard keyboard shortcuts

- `Cmd+K` — command palette
- `j` / `k` — navigate the issue list
- `e` — resolve
- `m` — mute
- `a` — assign to me
- `/` — focus the filter input
