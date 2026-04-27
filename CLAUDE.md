# errex — engineering rules

This file is binding for any agent or contributor working in this repo. It
captures non-obvious project constraints. Read it before touching code.

## Project shape

- `crates/errex-proto` — wire types shared across the workspace (Issue, Event,
  Fingerprint, ServerMessage, ClientMessage). Pure data + serde.
- `crates/errexd` — error-tracking daemon. HTTP ingest (Sentry envelope
  compatible), WebSocket fan-out, SQLite persistence, embedded SvelteKit SPA
  via `rust-embed`. Single binary.
- `web/` — SvelteKit 5 SPA (TypeScript strict, Tailwind v4, Svelte runes).
  Static-adapter build is bundled into the daemon at compile time.
- `docker/` — multi-stage Dockerfile (bun → cargo → distroless), compose file.
- `scripts/dev.sh` — boots daemon + Vite dev server in parallel.
- `scripts/seed.sh` — seeds the daemon with a realistic mix of issues.
- `scripts/smoke.sh` — boots a release binary and probes the API.
- `errex.sh` — task runner; everything that touches Rust runs in
  `rust:1-slim` so a local toolchain is not required.

## Hard non-functional constraints

1. **Lightweight first.** errex targets self-host on small machines. RAM is a
   first-class constraint, not an afterthought.
   - Prefer DB queries to in-memory caches when SQLite is fast enough.
   - Bound every channel and buffer; never let a queue grow unbounded.
   - Keep the sqlx pool small (≤4 connections); the digest task is single-writer.
   - Don't preload payloads. Stream where possible.
   - If you must cache, justify it with a measured number, not "for speed".

2. **Self-host friendly.** Single binary, SQLite (no Postgres dep), zero
   required external services. SPA is embedded — no separate web server.

3. **Sentry-SDK compatible ingest.** `/api/:project/envelope/` accepts the
   real envelope format. SDKs should "just work" by pointing at errexd.

## Testing — TDD is mandatory (Rust AND frontend)

**Rule:** every code change ships with tests. New behavior gets a failing
test FIRST, then the implementation that makes it pass. Bug fixes get a test
that reproduces the bug, then the fix.

This applies to **Rust** (`crates/`) and the **frontend** (`web/`). The
specific test surfaces and runners differ per stack — see below.

### Rust (`crates/`)

What MUST be tested:

- Every public method on `Store` (`upsert_issue`, `insert_event`,
  `latest_event`, `load_issues`, `list_issues_by_project`,
  `project_summaries`, `set_status`, project/webhook helpers, etc.).
- The fingerprint algorithm (`crate::fingerprint::derive`) — stability
  across normalization edge cases. Fingerprint regression is a silent UX
  catastrophe (issues fragment), so this gets generous coverage.
- The Sentry envelope parser (`crate::ingest::parse_envelope`) — header,
  length-prefixed items, gzip detection, malformed inputs.
- Wire types in `errex-proto`: serialize ↔ deserialize round-trips of
  `Event`, `Issue`, `ServerMessage`, `ClientMessage`, `IssueStatus` so any
  breaking change to JSON shape fails CI loudly.
- HTTP routes via `axum::Router` `oneshot` requests against an in-memory
  `AppState` (status codes, body shapes, auth boundaries).

How to write tests:

- Integration tests live in `crates/<name>/tests/*.rs`. Each file is its
  own binary; `mod` the source files via `#[path]`.
- Pure unit tests live in `#[cfg(test)] mod tests` blocks at the bottom of
  the source file.
- Tempdir + fresh SQLite for any test that touches `Store`. Never reuse
  `./data/errex.db`.
- No mocks of `Store`; use the real type with a tempdir.
- Tokio tests use `#[tokio::test]`.

### Frontend (`web/`)

What MUST be tested:

- Every module under `web/src/lib/` that contains pure logic
  (`api.ts`, `eventDetail.ts`, `eventStream.svelte.ts`, `actions.svelte.ts`,
  `selection.ts`, `stores.svelte.ts`, `utils.ts`, `toast.svelte.ts`,
  `ws.ts`).
- New behavior in components when the behavior is NOT purely visual —
  filter logic, command palette dispatch, keyboard shortcut handling,
  state transitions in modals.

What does NOT need tests:

- Pure visual styling (Tailwind classes, layout decisions).
- Component rendering output verified against a snapshot (snapshot tests
  rot fast and don't catch real bugs at this size).

How to write tests:

- Vitest, jsdom env. Files are `*.test.ts` colocated with the module
  they cover, e.g. `api.test.ts` next to `api.ts`. The runner picks them
  up from anywhere under `src/`.
- For Svelte 5 stores (`*.svelte.ts`), test by importing the singleton and
  invoking its methods — runes work in test context.
- For HTTP code, mock `fetch` via `vi.spyOn(globalThis, 'fetch')`.
- Component tests use `@testing-library/svelte` only when needed (event
  dispatch, conditional rendering tied to logic). Visual regressions are
  not in scope.

### How to run

- Rust: `./errex.sh check` runs `fmt --check`, `clippy -D warnings`,
  `cargo test --workspace` inside the rust container.
- Frontend: `bun test` (or `bun run check && bun test`) inside `web/`.
- Combined gate: a green PR has both green. CI runs both.

### TDD workflow (required)

1. Write the test that describes the desired behavior. Run it; confirm it
   fails for the right reason (compile error, missing import, assertion
   mismatch — all valid "red").
2. Implement the smallest change that makes the test pass.
3. Refactor with the test as your safety net.
4. Repeat.

Do **not** open a PR or mark a task done if step 1 was skipped.

## Iteration speed

- **For SPA changes (CSS, components, copy):** use `bun run dev` on
  `:5173` against the running daemon on `:9090`. Vite HMR is sub-second.
  Do NOT rebuild the docker image for visual iteration.
- **For Rust changes:** rebuild via `./errex.sh check && ./errex.sh build`
  or run the binary directly from the `errex-target` docker volume to
  avoid full image rebuilds during inner-loop work. The SPA bundled inside
  the binary is a snapshot of `web/build/` at compile time.
- Docker rebuild is required only when shipping or when the runtime image
  shape changes (deps, base image, file layout).

## Style — Rust

- `rustfmt` (config in `rustfmt.toml`) is enforced. Run before commit.
- `clippy --workspace --all-targets -- -D warnings` is enforced. No
  `#[allow]` without a one-line `// reason:` comment.
- Errors propagate via `anyhow::Result` at boundaries (main, task entries)
  and `thiserror`-derived enums inside crates (`DaemonError`, `ProtoError`).
- Don't add error handling for cases that can't happen. Trust framework
  invariants. Validate at system boundaries (HTTP body, JSON parse).
- Comments: explain the **why** when non-obvious. Skip "what" comments —
  identifiers should carry that.
- No backwards-compat shims for removed code. Delete cleanly.

## Style — frontend

- Svelte 5 runes only: `$state`, `$derived`, `$props`, `$bindable`,
  `$effect`. No `export let`, no stores from `svelte/store`.
- TypeScript strict; no `any`. Extend wire types in `lib/types.ts` to
  match `errex-proto` field-for-field.
- **shadcn primitives are the only UI vocabulary.** Feature components
  in `lib/components/` and routes in `src/routes/` MUST compose the
  primitives in `lib/components/ui/` (currently: avatar, badge, button,
  card, checkbox, collapsible, dialog, input, label, popover, resizable,
  select, separator, skeleton, tooltip). Specifically forbidden in
  feature/route code:
  - Raw `<button>` — use `Button` (variant=`ghost size=icon` for icon
    buttons, `link` for inline text actions).
  - Raw `<input>`, `<select>`, `<dialog>`, `<label>` — use the
    corresponding primitive.
  - DIY chrome that recreates a primitive's look (e.g.,
    `rounded-full px-2 py-0.5 text-xs` for a badge, `rounded-md border
    bg-card` for a card, `animate-pulse bg-muted` for a skeleton,
    custom hover popovers).
  - **Escape hatch:** if no primitive fits (kbd shortcut, code block,
    flame graph, etc.), add a new primitive to `lib/components/ui/`
    FIRST in the same PR, then consume it. Do not improvise inline.
  - Pure layout elements (`div`, `section`, `nav`, `ul`, `table`, etc.)
    and SVG/icon usage are not primitives and remain raw HTML.
- Never reach for an in-memory cache in the frontend either — the WS
  socket plus REST is enough; bounded ring buffers if you need windowed
  state (see `eventStream.svelte.ts`).
- Tests colocate as `*.test.ts` next to the module they cover.

## What NOT to do

- Don't touch `Cargo.toml` at the workspace root casually — workspace
  members and shared deps are deliberate.
- Don't add features that grow the binary or RSS without measuring the
  cost.
- Don't push direct DB writes from anywhere except the digest task or
  admin CLI. The single-writer-during-hot-path invariant is load-bearing.
- Don't add new endpoints to `ingest.rs` or new pure-logic frontend
  modules without also adding tests for them (per TDD rule above).
- Don't add visual snapshot tests for components — they rot fast and
  catch nothing real at this size.
- Don't bypass the shadcn primitive layer in feature components or
  routes. If a primitive is missing, extend `lib/components/ui/`
  instead of inlining the markup (see "Style — frontend").

## Quick references

- Architecture sketch: see `crates/errexd/src/main.rs` for the wiring.
- Schema: `crates/errexd/migrations/`.
- Wire format: `crates/errex-proto/src/`.
