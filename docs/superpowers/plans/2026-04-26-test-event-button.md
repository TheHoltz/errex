# Test Event Button Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the "Copy a curl that sends a test event" link on `/projects/[name]` with a primary "Send test event" button that fires the test directly from the browser via `fetch`, while keeping the curl link as a small secondary affordance.

**Architecture:** Pure-logic helper in `web/src/lib/testEvent.ts` owns the wire call and returns a tagged result; `ProjectDetail.svelte` owns the state machine (`idle | sending | sent`), toasts, and timer cleanup. Helper is unit-tested with mocked `fetch`; component is exercised manually + via the existing `bun run check` type-check pass. No server-side changes — the daemon's CORS layer (dev) and same-origin SPA (prod) already permit the browser to POST the project's DSN.

**Tech Stack:** Svelte 5 runes, TypeScript strict, Vitest + jsdom, shadcn `Button`, `lucide-svelte` icons (`Send`, `Check`, `Loader2`), existing `toast` helper.

**Spec:** `docs/superpowers/specs/2026-04-26-test-event-button-design.md`

---

## File Structure

| File | Role |
|---|---|
| `web/src/lib/testEvent.ts` (new) | Pure async helper `sendTestEvent(dsn)`. Returns a tagged union — never throws, never touches DOM, never imports toast/Svelte. |
| `web/src/lib/testEvent.test.ts` (new) | Vitest covering: request shape, 2xx OK, non-2xx tag with truncated body, network throw tag. |
| `web/src/lib/components/ProjectDetail.svelte` (modify) | Replace the curl text-button with primary "Send test event" button + secondary "or copy as curl" link. Add `testStatus`, `revertHandle`, `sendTestEvent` handler. Reset both in the existing per-project effect at lines 50–58. |

---

## Task 1: Add the failing helper test

**Files:**
- Create: `web/src/lib/testEvent.test.ts`

- [ ] **Step 1: Write the failing test**

Create `web/src/lib/testEvent.test.ts`:

```ts
// Tests the wire-only helper used by the "Send test event" button on the
// project detail page. The component owns the state machine and toasts;
// this helper just talks to the ingest endpoint and reports what happened.

import { afterEach, describe, expect, it, vi } from 'vitest';
import { sendTestEvent } from './testEvent';

afterEach(() => {
  vi.restoreAllMocks();
});

const DSN = 'http://localhost:9090/api/demo/envelope/?sentry_key=abc123';

describe('sendTestEvent', () => {
  it('POSTs the documented JSON body to the DSN', async () => {
    const fetch = vi.spyOn(globalThis, 'fetch').mockResolvedValue({
      ok: true,
      status: 200,
      text: async () => '',
    } as Response);

    await sendTestEvent(DSN);

    expect(fetch).toHaveBeenCalledOnce();
    const [url, init] = fetch.mock.calls[0]!;
    expect(url).toBe(DSN);
    expect(init?.method).toBe('POST');
    expect((init?.headers as Record<string, string>)['content-type']).toBe(
      'application/json'
    );
    expect(JSON.parse(init?.body as string)).toEqual({
      event_id: 'test',
      level: 'error',
      message: 'errex test event',
    });
  });

  it('returns { kind: "ok" } on a 2xx response', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue({
      ok: true,
      status: 200,
      text: async () => '',
    } as Response);

    const result = await sendTestEvent(DSN);
    expect(result).toEqual({ kind: 'ok' });
  });

  it('returns { kind: "http", status, body } on a non-2xx response, body truncated to 140 chars', async () => {
    const longBody = 'x'.repeat(500);
    vi.spyOn(globalThis, 'fetch').mockResolvedValue({
      ok: false,
      status: 401,
      text: async () => longBody,
    } as Response);

    const result = await sendTestEvent(DSN);
    expect(result).toEqual({
      kind: 'http',
      status: 401,
      body: 'x'.repeat(140),
    });
  });

  it('returns { kind: "network", error } when fetch throws', async () => {
    const boom = new TypeError('Failed to fetch');
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(boom);

    const result = await sendTestEvent(DSN);
    expect(result).toEqual({ kind: 'network', error: boom });
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd web && bun test testEvent`
Expected: FAIL — `Cannot find module './testEvent'` (or similar import error). This is the right kind of "red".

- [ ] **Step 3: Commit the failing test**

```bash
cd /Users/r3g3n3r4/Projects/errex
git add web/src/lib/testEvent.test.ts
git commit -m "test: add failing helper tests for sendTestEvent"
```

---

## Task 2: Implement the helper

**Files:**
- Create: `web/src/lib/testEvent.ts`

- [ ] **Step 1: Write the minimal implementation**

Create `web/src/lib/testEvent.ts`:

```ts
// Wire-only helper for the "Send test event" button on the project detail
// page. Posts the same JSON shape the documented curl one-liner uses, so
// browser-button parity with the curl path is exact. Returns a tagged
// result; the component layers state machine, toasts, and timers on top.

export type TestEventResult =
  | { kind: 'ok' }
  | { kind: 'http'; status: number; body: string }
  | { kind: 'network'; error: unknown };

const BODY_PREVIEW_LIMIT = 140;

export async function sendTestEvent(dsn: string): Promise<TestEventResult> {
  let res: Response;
  try {
    res = await fetch(dsn, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        event_id: 'test',
        level: 'error',
        message: 'errex test event',
      }),
    });
  } catch (error) {
    return { kind: 'network', error };
  }

  if (res.ok) return { kind: 'ok' };

  const text = await res.text();
  return {
    kind: 'http',
    status: res.status,
    body: text.slice(0, BODY_PREVIEW_LIMIT),
  };
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cd web && bun test testEvent`
Expected: PASS — all 4 cases green.

- [ ] **Step 3: Run the full check gate**

Run: `cd web && bun run check && bun test`
Expected: type-check clean, full suite green.

- [ ] **Step 4: Commit the implementation**

```bash
cd /Users/r3g3n3r4/Projects/errex
git add web/src/lib/testEvent.ts
git commit -m "feat(web): add sendTestEvent helper"
```

---

## Task 3: Wire the button into ProjectDetail

**Files:**
- Modify: `web/src/lib/components/ProjectDetail.svelte`

This task has no automated test — `ProjectDetail.svelte` is not currently
covered by component tests, the spec explicitly does not introduce them
for this change, and the wire call is already covered by Task 1. Manual
verification happens in Task 4.

- [ ] **Step 1: Add the imports**

In `web/src/lib/components/ProjectDetail.svelte`, the existing icon import block at lines 2–11 already pulls `Check`, `Loader2`, `Send`. Add the helper import next to the other `$lib` imports (alphabetical block around line 30):

```ts
import { sendTestEvent } from '$lib/testEvent';
```

- [ ] **Step 2: Add the state declarations**

Just after the existing `testingWebhook` declaration (around line 44):

```ts
let testStatus = $state<'idle' | 'sending' | 'sent'>('idle');
let revertHandle = $state<ReturnType<typeof setTimeout> | null>(null);
```

- [ ] **Step 3: Reset on project switch**

Inside the existing `$effect` at lines 50–58 (which already resets per-project state), add the reset right after `statsError = null;`:

```ts
testStatus = 'idle';
if (revertHandle) {
  clearTimeout(revertHandle);
  revertHandle = null;
}
```

- [ ] **Step 4: Add the handler**

Just before the `// ----- rotate -----` comment (around line 188, after `testWebhook`), add:

```ts
async function sendTest() {
  testStatus = 'sending';
  if (revertHandle) {
    clearTimeout(revertHandle);
    revertHandle = null;
  }
  const result = await sendTestEvent(project.dsn);
  if (result.kind === 'ok') {
    testStatus = 'sent';
    revertHandle = setTimeout(() => {
      testStatus = 'idle';
      revertHandle = null;
    }, 2000);
    return;
  }
  testStatus = 'idle';
  if (result.kind === 'http') {
    toast.error(`Ingest returned ${result.status}`, {
      description: result.body || `HTTP ${result.status}`,
    });
  } else {
    toast.error('Network error', { description: String(result.error) });
  }
}
```

- [ ] **Step 5: Replace the markup in the Connection section**

Locate the existing block at lines 357–364 (the `<button onclick={copyCurl}>` with the "Copy a curl that sends a test event" label). Replace ONLY that `<button>` with this two-element block. Both interactive elements use the shadcn `Button` primitive — per project rules, no raw `<button>` in feature code:

```svelte
<div class="flex flex-col items-start gap-1.5">
  <Button
    variant="outline"
    size="sm"
    onclick={sendTest}
    disabled={testStatus !== 'idle'}
    aria-label="Send a test event to this project's ingest endpoint"
    class={cn(testStatus === 'sent' && 'text-emerald-500 hover:text-emerald-500')}
  >
    {#if testStatus === 'sending'}
      <Loader2 class="h-3.5 w-3.5 animate-spin" />
      Sending…
    {:else if testStatus === 'sent'}
      <Check class="h-3.5 w-3.5" />
      Sent
    {:else}
      <Send class="h-3.5 w-3.5" />
      Send test event
    {/if}
  </Button>
  <Button
    variant="link"
    size="sm"
    onclick={copyCurl}
    class="text-muted-foreground hover:text-foreground h-auto gap-1.5 px-0 text-[11px] hover:no-underline"
  >
    <ClipboardCopy class="h-3 w-3" />
    or copy as curl
  </Button>
</div>
```

Notes on the second Button:
- `variant="link"` is the link-shaped shadcn variant (no background, no border, inline-flex). Class overrides shrink it (`h-auto px-0`) and re-tint it to muted (the variant's default `text-primary` would read as a primary action — wrong here; this is a secondary affordance). `hover:no-underline` keeps it visually closer to the original muted text-link, since the variant's default `hover:underline` would be a louder hover than intended.
- This intentionally also upgrades the existing curl-copy element from a raw `<button>` to the primitive, complying with the project's "shadcn primitives only" rule.

- [ ] **Step 6: Type-check and run the suite**

Run: `cd web && bun run check && bun test`
Expected: type-check clean, full suite green (including the helper tests from Task 1).

- [ ] **Step 7: Commit the wiring**

```bash
cd /Users/r3g3n3r4/Projects/errex
git add web/src/lib/components/ProjectDetail.svelte
git commit -m "feat(web): replace curl link with click-to-test button"
```

---

## Task 4: Manual verification

This is a UI change. Per CLAUDE.md, UI features must be exercised in a real browser before being declared done.

- [ ] **Step 1: Boot the dev stack**

Run (from repo root): `./scripts/dev.sh`
Expected: daemon on `:9090`, Vite on `:5173`, both ready.

- [ ] **Step 2: Visit a project detail page**

Open `http://localhost:5173/projects/<some-project-name>` in a browser.
Pick any project the seed script created, or create a fresh one via the projects list.

- [ ] **Step 3: Verify idle state**

Expected: Below the DSN code block you see the outline button "Send test event" with the send icon, and below it the small muted "or copy as curl" link. The previous "Copy a curl that sends a test event" text-link no longer appears.

- [ ] **Step 4: Click "Send test event" — happy path**

Expected sequence:
1. Button shows spinner + "Sending…" briefly.
2. Button morphs to green `Check` + "Sent" for ~2 s, then returns to idle.
3. The activity sparkline above ticks within ~1 s (an `errex test event` issue gets created or its count bumps).
4. No toast — success is the inline state and the sparkline tick.

- [ ] **Step 5: Click "or copy as curl"**

Expected: green "Test command copied" toast (existing behavior, unchanged). Button below it stays in `idle`. Pasting into a terminal still produces the same curl as before.

- [ ] **Step 6: Force the error path**

In DevTools → Network, set throttling to "Offline", then click "Send test event".
Expected: button reverts to `idle`, red "Network error" toast appears with the `TypeError` message in the description.

Restore network. In a separate tab, rotate the project's ingest token via the danger-zone Rotate flow but do NOT update the page (the in-memory `project.dsn` is now stale). Click "Send test event".
Expected: button reverts to `idle`, red "Ingest returned 401" toast (or whatever status the daemon returns for an invalid token) with the truncated body in the description.

If the rotate flow auto-refreshes `project.dsn` so the second case is hard to reproduce, edit `project.dsn` in the Svelte devtools or temporarily change the token portion to something invalid in the source — the goal is to land a non-2xx response.

- [ ] **Step 7: Switch projects mid-flight**

Click "Send test event" on project A, then immediately click project B in the rail (before the 2 s "Sent" timer expires).
Expected: project B page loads in `idle` state. No stray "Sent" or "Sending…" labels carry over. (This validates the cleanup added in Task 3 Step 3.)

- [ ] **Step 8: Mark the task complete**

If all six checks above pass, the feature is verified. If any fails, return to Task 3 and fix; do not declare done.

---

## Self-Review

**Spec coverage:** Every section of the spec has a corresponding task —
UX shape (Task 3 Step 5), state machine (Task 3 Steps 2–4), behavior /
fetch contract (Tasks 1 + 2), side-effect note about sparkline (Task 4
Step 4), TDD helper (Tasks 1 + 2), out-of-scope items (none implemented,
correctly absent). Timer cleanup explicitly handled in Task 3 Step 3 and
Task 4 Step 7.

**Placeholder scan:** No TBD/TODO. All file paths concrete. All commands
runnable. All code blocks complete.

**Type consistency:** `TestEventResult` shape matches between Task 1
test assertions and Task 2 implementation. `testStatus` literal union
matches between Task 3 Step 2 declaration, Step 4 handler assignments,
and Step 5 markup conditionals (`'idle' | 'sending' | 'sent'`).
`revertHandle` typed `ReturnType<typeof setTimeout> | null` consistently
in Steps 2, 3, and 4.

No issues found.
