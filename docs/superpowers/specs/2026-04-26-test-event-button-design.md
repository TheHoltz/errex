# Test event button on `/projects/[name]`

**Date:** 2026-04-26
**Surface:** `web/src/lib/components/ProjectDetail.svelte` (Connection · DSN section)

## Problem

Today, after creating a project the only "did it work?" affordance is a text
link that copies a curl command to the clipboard:

> *Copy a curl that sends a test event*

The user then has to switch to a terminal, paste, run, and switch back to see
the issue appear. That round-trip is the largest friction point in
"new project → first event acknowledged" — it should be one click from the
browser they are already in.

## Goal

Replace the curl link with a primary "Send test event" button that fires the
test directly from the browser via `fetch`. Keep a small secondary "or copy
as curl" link for the cross-machine / inside-a-container case where the user
genuinely needs the shell command.

Non-goal: simulating a real Sentry SDK envelope. The current curl posts
`{"event_id":"test","level":"error","message":"errex test event"}`; the
browser button posts the same payload to the same URL. Paridade total.

## UX shape

The Connection section (currently `ProjectDetail.svelte:352-365`) becomes:

```
Connection · DSN
┌────────────────────────────────────────────┐
│ <DsnSnippet />                             │
└────────────────────────────────────────────┘
[ ⏵ Send test event ]
or copy as curl
```

- **Primary button** — `<Button variant="outline" size="sm">` with the
  `Send` icon, label *"Send test event"*. Composes the existing shadcn
  `Button` primitive at `web/src/lib/components/ui/button/`. No bespoke
  styling.
- **Secondary link** — same plain `<button>` that exists today, with the
  label shortened to *"or copy as curl"* (was *"Copy a curl that sends a
  test event"*). Same icon, same muted styling, same `copyCurl()` handler.
  Sits directly under the primary button.

### Button states

Modeled on the existing `rotatedCopied` flow in this same component
(`ProjectDetail.svelte:216-225, 491-510`). One piece of `$state` —
`testStatus: 'idle' | 'sending' | 'sent'` — drives the visuals:

| State | Icon | Label | Disabled | Notes |
|---|---|---|---|---|
| `idle` | `Send` | "Send test event" | no | default |
| `sending` | `Loader2` (spin) | "Sending…" | yes | during fetch |
| `sent` | `Check` | "Sent" | yes | green via `text-emerald-500` token; reverts to `idle` after 2000 ms |

Errors do **not** introduce a separate visual state on the button. The
button reverts to `idle` and a `toast.error(...)` describes the failure —
matching how `testWebhook` already handles failures
(`ProjectDetail.svelte:174-185`).

## Behavior

```ts
async function sendTestEvent() {
  testStatus = 'sending';
  try {
    const res = await fetch(project.dsn, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        event_id: 'test',
        level: 'error',
        message: 'errex test event',
      }),
    });
    if (!res.ok) {
      toast.error(`Ingest returned ${res.status}`, {
        description: (await res.text()).slice(0, 140) || res.statusText,
      });
      testStatus = 'idle';
      return;
    }
    testStatus = 'sent';
    setTimeout(() => { testStatus = 'idle'; }, 2000);
  } catch (err) {
    toast.error('Network error', { description: String(err) });
    testStatus = 'idle';
  }
}
```

Notes:

- `project.dsn` is already the full ingest URL with `?sentry_key=…` baked
  in (`crates/errexd/src/ingest.rs:261-268`). No URL construction needed
  on the client.
- CORS works in dev (the daemon allows `http://localhost:5173` when
  `dev_mode` is on — `ingest.rs:133-147`) and in production the SPA is
  same-origin. No new server-side change required.
- The 2 s revert timer is owned by a dedicated `$effect` whose cleanup
  function calls `clearTimeout`. Concretely: `sendTestEvent` sets
  `testStatus = 'sent'` and stores the handle in
  `let revertHandle = $state<ReturnType<typeof setTimeout> | null>(null)`;
  an effect that depends on `revertHandle` returns a cleanup that clears
  it. This guarantees the timer is killed when the component unmounts,
  when the user switches projects (the existing project-switch effect at
  `ProjectDetail.svelte:50-58` already resets per-project state — we
  also reset `testStatus` to `idle` and `revertHandle` to `null` there),
  or when a new send overwrites the handle.
- No rate limiting. The existing webhook test button has none either, and
  the ingest endpoint already deduplicates by fingerprint, so spam clicks
  produce one issue with N occurrences — fine.

## Side effects (intentional)

A successful click creates (or bumps) an `errex test event` issue in the
project. The activity sparkline on the same page ticks within ~1 s via the
WS subscription that already drives this view. We do **not** add code to
"watch for the tick" — the UI updates because the data updated. The
inline `Sent ✓` confirmation is the explicit success signal; the sparkline
tick is the ambient one.

## Tests (TDD, frontend)

Per the project's TDD rule, the new logic ships with `*.test.ts` tests.
The fetch handler is the only piece worth covering — visuals are not in
scope for snapshot tests.

To make `sendTestEvent` testable in isolation it gets extracted to a pure
helper alongside the component, at `web/src/lib/testEvent.ts`:

```ts
export async function sendTestEvent(dsn: string): Promise<
  | { kind: 'ok' }
  | { kind: 'http'; status: number; body: string }
  | { kind: 'network'; error: unknown }
>;
```

The component owns the state machine + toasts; the helper owns the wire
call. Tests live at `web/src/lib/testEvent.test.ts` and cover, with
`vi.spyOn(globalThis, 'fetch')`:

1. POSTs to the given DSN with the documented JSON body and
   `content-type: application/json`.
2. Returns `{ kind: 'ok' }` on a 2xx response.
3. Returns `{ kind: 'http', status, body }` on non-2xx, truncating body
   to ≤140 chars to match the toast description.
4. Returns `{ kind: 'network', error }` when `fetch` throws.

No Rust changes. No new component tests beyond the helper — the existing
`ProjectDetail.svelte` is not currently covered by component tests and
this change does not warrant introducing them (visual states, no logic
that isn't already in the helper).

## What does not change

- `copyCurl()` and the curl payload are untouched.
- `testWebhook()` (the Slack/Discord webhook test) is untouched — it
  already does the right thing.
- No server-side, schema, or WS changes.
- No new shadcn primitives. The button is `Button` from
  `lib/components/ui/button`; the icons are existing `lucide-svelte`
  imports already in this file (`Send`, `Check`, `Loader2`).

## Out of scope

- Rendering a richer success panel ("Event received — view in issues
  list"). The sparkline tick + the issue appearing in the list is enough
  signal at this size; adding a panel would mean reactively binding to
  the WS event stream just for this confirmation, and that's the
  overengineered (C) option we already discarded.
- Sending an actual Sentry envelope from the browser. The endpoint
  accepts both shapes and the simple JSON is fine for "did it arrive".
- A toast on success. The inline `Sent ✓` is the success affordance;
  doubling it with a toast would be noise.
