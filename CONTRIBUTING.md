# Contributing to errex

errex is small and aims to stay that way. Read this and `CLAUDE.md`
before opening a PR.

## Hard rules

1. **TDD on Rust changes.** Failing test first; then implementation.
   Tests live in `crates/<crate>/tests/*.rs` (integration) or
   `#[cfg(test)] mod tests` blocks (unit, especially for private fns).
2. **`./errex.sh check` must pass** — fmt + clippy `-D warnings` +
   `cargo test --workspace`. CI runs the same command.
3. **Lightweight first.** Default to DB queries over in-memory caches;
   bound every channel; keep the sqlx pool small. New deps need a
   one-line justification in the PR description.
4. **The frontend is exempt from TDD.** SvelteKit components iterate
   visually; the test infra cost is high relative to value at this stage.

## Layout

```
crates/
├── errex-proto/   wire types (Issue, Event, Fingerprint, ServerMessage)
└── errexd/        daemon binary
    ├── src/
    │   ├── main.rs       wiring + CLI
    │   ├── config.rs     CLI/env config
    │   ├── digest.rs     single-writer ingest pipeline
    │   ├── ingest.rs     HTTP routes (Sentry envelope + browser API)
    │   ├── ws.rs         WebSocket fan-out
    │   ├── store.rs      SQLite read/write methods
    │   ├── fingerprint.rs grouping algorithm
    │   ├── rate_limit.rs token-bucket per project
    │   ├── retention.rs  background event purge
    │   ├── webhook.rs    outbound Slack/Discord/Teams alerts
    │   ├── spa.rs        embedded SvelteKit serving
    │   └── mcp.rs        MCP listener (stub)
    ├── migrations/       SQLx migrations (timestamp-prefixed)
    └── tests/            integration tests
web/                       SvelteKit 5 SPA, embedded into the daemon at compile time
docker/                    Multi-stage Dockerfile + compose
scripts/                   dev.sh, smoke.sh, seed.sh
```

## Local development

The fast path uses Docker for the daemon and `bun run dev` for the SPA:

```bash
./errex.sh up -d                              # daemon on :9090 (HTTP+WS) and :9092 (MCP)
(cd web && bun install && bun run dev)        # SPA on :5173 with HMR
open http://localhost:5173
```

Vite proxies `/api` and `/ws` to `:9090` automatically. Daemon is in
`ERREXD_DEV_MODE=true` (set by compose) so CORS lets the dev server in.

For Rust changes, `./errex.sh check` runs the full bar in a `rust:1-slim`
container so you don't need a local toolchain.

## Adding a feature

1. **Open an issue** describing the behavior change. Include why a smaller
   alternative isn't enough.
2. **Write the failing test first.** For new Rust behavior:
   - public Store method → `crates/errexd/tests/store.rs`
   - HTTP route → `crates/errexd/tests/api.rs`
   - private function → `#[cfg(test)] mod tests` in the same file
   - wire-format change → `crates/errex-proto/tests/wire_format.rs`
3. **Make it pass with the smallest change.** Resist refactoring during
   green; do that as a separate step with the test as a safety net.
4. **Run `./errex.sh check`.** No warnings, no skipped tests.
5. **Commit with a body** explaining the *why*. The PR title should be
   short; the description should justify the change against the
   "lightweight first" rule and link the issue.

## Adding a new wire field

A wire change touches `errex-proto`, the SQLite schema, and the SPA
types. Order matters:

1. Add the field + serde defaults in `errex-proto`. Existing
   `wire_format.rs` round-trip tests must still pass.
2. Add a SQLx migration. Existing rows must accept the new column with a
   safe default.
3. Update `Store` SELECT statements + the `IssueRow → Issue` mapping.
4. Add a test that exercises the new column end-to-end.
5. Update `web/src/lib/types.ts` to mirror the new field.
6. Wire the SPA component(s) that read it.

## Adding a new HTTP route

1. Test in `crates/errexd/tests/api.rs` against `ingest::build_router`.
2. Handler in `crates/errexd/src/ingest.rs`.
3. Route registration in `build_router`.
4. Add CORS method to the `dev_mode` block if it's a non-GET/POST verb.

## Filing a security report

Open a private GitHub Security Advisory rather than a public issue.
errex is small enough that there's no triage queue — direct contact is
the path.

## License

By contributing you agree your work is licensed under
[AGPL-3.0](./LICENSE), matching the workspace.
