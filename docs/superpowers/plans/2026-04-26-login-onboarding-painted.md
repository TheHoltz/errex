# Login + onboarding redesign — Painted — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reskin `/login` and `/setup` to the "Painted" direction — translucent glass Card over a soft orange + violet backdrop, with a small `Stepper` for the setup wizard. State machine, HTTP contract, and lockout/error behavior are preserved verbatim.

**Architecture:** Two new components live in `web/src/lib/components/`: `AuthShell.svelte` (painted backdrop + brand mark + title/subtitle + body slot) and `Stepper.svelte` (2-pill stepper, used only by the setup wizard for now). The existing `web/src/routes/login/+page.svelte` and `web/src/routes/setup/+page.svelte` are refactored to wrap their existing state logic in these components — the markup changes but every `$state`, `$derived`, `$effect`, and event handler stays exactly as it is today.

**Tech Stack:** SvelteKit 5 (runes), TypeScript strict, Tailwind CSS v4, shadcn-svelte primitives (`Button`, `Input`, `Label`), `lucide-svelte` icons (`AlertCircle`, `AlertTriangle`, `Check`, `Lock`, `Loader2`, `LogIn`, `KeyRound`, `UserPlus`, `ArrowRight`), Vitest (jsdom), `@testing-library/svelte` for component tests.

**Reference spec:** `docs/superpowers/specs/2026-04-26-login-onboarding-design.md` — read this first for the visual direction and copy.

**Note on git:** This repo isn't a git repo today (`git status` returns "fatal"). Each task ends with a "commit" step shown for the day this is initialized; if `git status` still errors, skip the commit steps and continue.

**Note on size primitives:** The existing app uses `Button` size `default` = `h-9` and `Input` height `h-7`. Today's `/login` overrides both to `h-10` for a more comfortable auth form. This plan keeps that convention — Inputs and Buttons in the new auth pages use `class="h-10"` with `text-[13px]` to match `/login`'s existing density. No new primitive sizes are introduced.

**Note on Card primitive:** The shadcn `Card` primitive auto-applies `gap-4 py-4 ring-1 ring-foreground/10 rounded-xl`. The Painted auth-card needs custom padding (32px 30px 26px), no inset gap, no ring, and a translucent glass background — overriding all of that fights the primitive. The plan uses a styled `<div>` for the auth-card container and reserves `Card` for in-app dashboard surfaces. `Input`, `Label`, and `Button` remain shadcn primitives.

---

## File Structure

**New files:**

- `web/src/lib/components/AuthShell.svelte` — full-viewport painted backdrop + glass Card + brand mark + title/subtitle. Body provided via `children` snippet.
- `web/src/lib/components/AuthShell.test.ts` — renders title/subtitle, marks gradient layers as `aria-hidden`.
- `web/src/lib/components/Stepper.svelte` — 2-pill stepper. Public API: `current: 1 | 2`, `labels: [string, string]`, `doneLabel?: string`.
- `web/src/lib/components/Stepper.test.ts` — pill state transitions (active / inactive / done).

**Modified files:**

- `web/src/routes/login/+page.svelte` — markup swap to `<AuthShell>`. State logic unchanged.
- `web/src/routes/setup/+page.svelte` — markup swap to `<AuthShell>` + `<Stepper>`. State logic unchanged.

**Untouched:**

- `web/src/lib/auth.svelte.ts`, `web/src/lib/api.ts`, `web/src/lib/auth.test.ts`, `crates/errexd/src/auth.rs`, daemon migrations. The contract is the same.
- `web/src/app.css` — no new global styles. All component styles live in their own `.svelte` file.

---

## Task 1: AuthShell — failing test for title/subtitle rendering

**Files:**

- Create: `web/src/lib/components/AuthShell.test.ts`

- [ ] **Step 1: Write the failing test**

Create `web/src/lib/components/AuthShell.test.ts`:

```ts
import { render, screen } from '@testing-library/svelte';
import { describe, it, expect } from 'vitest';
import AuthShell from './AuthShell.svelte';

describe('AuthShell', () => {
  it('renders the title and subtitle', () => {
    render(AuthShell, {
      props: { title: 'sign in to errex', subtitle: 'self-hosted error tracking' }
    });
    expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('sign in to errex');
    expect(screen.getByText('self-hosted error tracking')).toBeInTheDocument();
  });

  it('omits the subtitle paragraph when not provided', () => {
    render(AuthShell, { props: { title: 'sign in' } });
    expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('sign in');
    // No subtitle paragraph in the document.
    expect(screen.queryByTestId('auth-shell-subtitle')).toBeNull();
  });

  it('marks decorative gradient layers as aria-hidden', () => {
    const { container } = render(AuthShell, { props: { title: 't' } });
    const decorative = container.querySelectorAll('[aria-hidden="true"]');
    // Two gradient layers + one noise layer = 3.
    expect(decorative.length).toBeGreaterThanOrEqual(3);
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd web && bun test src/lib/components/AuthShell.test.ts`

Expected: FAIL — `AuthShell.svelte` does not exist yet (`Cannot find module './AuthShell.svelte'`).

---

## Task 2: AuthShell — minimal skeleton to make the test pass

**Files:**

- Create: `web/src/lib/components/AuthShell.svelte`

- [ ] **Step 1: Write the minimum AuthShell that satisfies the failing test**

Create `web/src/lib/components/AuthShell.svelte`:

```svelte
<script lang="ts">
  import { AlertCircle } from 'lucide-svelte';
  import type { Snippet } from 'svelte';

  type Props = {
    title: string;
    subtitle?: string;
    children?: Snippet;
  };

  let { title, subtitle, children }: Props = $props();
</script>

<div class="bg-background relative flex min-h-screen items-center justify-center overflow-hidden px-4 py-10">
  <!--
    Decorative painted backdrop. Two soft radial gradients (warm orange wash
    + cool violet wash) plus a faint inline-SVG noise overlay so the gradient
    doesn't look "video game". All three layers are aria-hidden because they
    carry no information; pointer-events: none so they never intercept clicks.
  -->
  <div
    aria-hidden="true"
    class="pointer-events-none absolute"
    style="left:-8%;top:-18%;width:70%;height:110%;
           background:radial-gradient(ellipse at 32% 50%, hsla(22,94%,53%,0.55) 0%, hsla(22,94%,53%,0.18) 32%, transparent 62%);
           filter:blur(56px);"
  ></div>
  <div
    aria-hidden="true"
    class="pointer-events-none absolute"
    style="right:-10%;bottom:-28%;width:65%;height:95%;
           background:radial-gradient(ellipse at 60% 40%, hsla(265,70%,55%,0.42) 0%, hsla(285,70%,55%,0.15) 35%, transparent 65%);
           filter:blur(64px);"
  ></div>
  <div
    aria-hidden="true"
    class="pointer-events-none absolute inset-0 mix-blend-overlay opacity-[0.06]"
    style="background-image:url(&quot;data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='180' height='180'><filter id='n'><feTurbulence type='fractalNoise' baseFrequency='0.85' numOctaves='2'/></filter><rect width='100%' height='100%' filter='url(%23n)'/></svg>&quot;);"
  ></div>

  <!--
    Glass auth-card. Not the shadcn Card primitive: the primitive's auto-
    applied gap/padding/ring fights us here, and this is a one-off pre-auth
    surface — the Card primitive stays for in-app dashboard cards.
  -->
  <div
    class="relative z-10 flex w-full max-w-[360px] flex-col gap-[18px] rounded-xl border p-[32px_30px_26px]"
    style="background:hsla(0,0%,5.5%,0.66);
           backdrop-filter:blur(22px) saturate(140%);
           -webkit-backdrop-filter:blur(22px) saturate(140%);
           border-color:hsla(0,0%,100%,0.07);
           box-shadow:0 1px 0 hsla(0,0%,100%,0.05) inset, 0 30px 80px rgba(0,0,0,0.45);"
  >
    <div class="flex flex-col gap-[3px]">
      <div
        class="mb-3 flex h-9 w-9 items-center justify-center rounded-[9px]"
        style="background:linear-gradient(140deg, hsl(22 94% 60%), hsl(36 96% 58%));
               box-shadow:0 0 0 1px hsla(22,94%,50%,0.35), 0 8px 24px hsla(22,94%,50%,0.35);"
      >
        <AlertCircle class="h-[18px] w-[18px]" style="color:hsl(22 96% 12%);" />
      </div>
      <h1 class="text-[17px] font-semibold tracking-[-0.018em]">{title}</h1>
      {#if subtitle}
        <p data-testid="auth-shell-subtitle" class="text-muted-foreground text-[11.5px]">
          {subtitle}
        </p>
      {/if}
    </div>

    {@render children?.()}
  </div>
</div>

<style>
  /* Drop the backdrop blur if the OS asks for reduced transparency.
     Falls back to the same dark Card color used elsewhere in the app. */
  @media (prefers-reduced-transparency: reduce) {
    div[style*='backdrop-filter'] {
      background: hsl(0 0% 9%) !important;
      backdrop-filter: none !important;
      -webkit-backdrop-filter: none !important;
    }
  }
</style>
```

- [ ] **Step 2: Run the test to verify it passes**

Run: `cd web && bun test src/lib/components/AuthShell.test.ts`

Expected: PASS — all three test cases green.

- [ ] **Step 3: Run the typecheck to make sure the component is valid**

Run: `cd web && bun run check`

Expected: zero errors. (Warnings about unrelated files are fine; this task's diff must not introduce any.)

- [ ] **Step 4: Commit**

```bash
git add web/src/lib/components/AuthShell.svelte web/src/lib/components/AuthShell.test.ts
git commit -m "feat(web): add AuthShell painted-backdrop wrapper for /login and /setup"
```

(If the repo is not git-initialized, skip this step.)

---

## Task 3: Stepper — failing test for active/inactive/done pill states

**Files:**

- Create: `web/src/lib/components/Stepper.test.ts`

- [ ] **Step 1: Write the failing test**

Create `web/src/lib/components/Stepper.test.ts`:

```ts
import { render, screen } from '@testing-library/svelte';
import { describe, it, expect } from 'vitest';
import Stepper from './Stepper.svelte';

describe('Stepper', () => {
  it('renders pill 1 active and pill 2 inactive when current is 1', () => {
    const { container } = render(Stepper, {
      props: { current: 1, labels: ['verify host access', 'create account'] }
    });
    const pills = container.querySelectorAll('[data-stepper-pill]');
    expect(pills).toHaveLength(2);
    expect(pills[0].getAttribute('data-stepper-pill')).toBe('active');
    expect(pills[1].getAttribute('data-stepper-pill')).toBe('inactive');
    expect(pills[0]).toHaveTextContent('1');
    expect(pills[1]).toHaveTextContent('2');
    // Active label visible, inactive label hidden.
    expect(screen.getByText('verify host access')).toBeInTheDocument();
  });

  it('shows pill 1 done with checkmark and pill 2 active when current is 2', () => {
    const { container } = render(Stepper, {
      props: { current: 2, labels: ['verify host access', 'create account'] }
    });
    const pills = container.querySelectorAll('[data-stepper-pill]');
    expect(pills[0].getAttribute('data-stepper-pill')).toBe('done');
    expect(pills[1].getAttribute('data-stepper-pill')).toBe('active');
    // Done pill renders a check icon (lucide Check), no numeric "1" label.
    expect(pills[0]).not.toHaveTextContent('1');
    expect(pills[0].querySelector('svg')).not.toBeNull();
    // Active label for step 2 is visible; default doneLabel "verified" is shown for step 1.
    expect(screen.getByText('create account')).toBeInTheDocument();
    expect(screen.getByText('verified')).toBeInTheDocument();
  });

  it('uses the doneLabel prop when provided', () => {
    render(Stepper, {
      props: { current: 2, labels: ['a', 'b'], doneLabel: 'all good' }
    });
    expect(screen.getByText('all good')).toBeInTheDocument();
    expect(screen.queryByText('verified')).toBeNull();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd web && bun test src/lib/components/Stepper.test.ts`

Expected: FAIL — `Stepper.svelte` does not exist yet.

---

## Task 4: Stepper — implementation

**Files:**

- Create: `web/src/lib/components/Stepper.svelte`

- [ ] **Step 1: Implement the component**

Create `web/src/lib/components/Stepper.svelte`:

```svelte
<script lang="ts">
  import { Check } from 'lucide-svelte';

  type Props = {
    current: 1 | 2;
    labels: [string, string];
    doneLabel?: string;
  };

  let { current, labels, doneLabel = 'verified' }: Props = $props();

  // Per-pill state: 'done' (current > n), 'active' (current === n), 'inactive' (current < n).
  function pillState(n: 1 | 2): 'done' | 'active' | 'inactive' {
    if (current > n) return 'done';
    if (current === n) return 'active';
    return 'inactive';
  }

  const pill1 = $derived(pillState(1));
  const pill2 = $derived(pillState(2));
</script>

<div class="flex items-center gap-2 text-[10px]">
  <div
    data-stepper-pill={pill1}
    class="flex h-[22px] w-[22px] items-center justify-center rounded-md border text-[11px] font-semibold transition-colors duration-150"
    class:bg-primary={pill1 === 'active'}
    class:text-primary-foreground={pill1 === 'active'}
    class:border-transparent={pill1 === 'active'}
    style:box-shadow={pill1 === 'active'
      ? '0 6px 18px hsla(22, 94%, 50%, 0.32)'
      : 'none'}
  >
    {#if pill1 === 'done'}
      <Check class="h-3 w-3" style="color: hsl(22 94% 63%);" />
    {:else}
      1
    {/if}
  </div>

  {#if pill1 === 'active'}
    <span class="text-foreground">{labels[0]}</span>
  {:else if pill1 === 'done'}
    <span class="text-muted-foreground">{doneLabel}</span>
  {/if}

  <div class="bg-border h-px flex-1"></div>

  <div
    data-stepper-pill={pill2}
    class="flex h-[22px] w-[22px] items-center justify-center rounded-md border text-[11px] font-semibold transition-colors duration-150"
    class:bg-primary={pill2 === 'active'}
    class:text-primary-foreground={pill2 === 'active'}
    class:border-transparent={pill2 === 'active'}
    style:box-shadow={pill2 === 'active'
      ? '0 6px 18px hsla(22, 94%, 50%, 0.32)'
      : 'none'}
  >
    {#if pill2 === 'done'}
      <Check class="h-3 w-3" style="color: hsl(22 94% 63%);" />
    {:else}
      2
    {/if}
  </div>

  {#if pill2 === 'active'}
    <span class="text-foreground">{labels[1]}</span>
  {/if}
</div>

<style>
  /* Done-pill subtle tint — lighter than the active pill's box-shadow but
     visually distinguishes "completed" from "not yet started". */
  div[data-stepper-pill='done'] {
    background-color: hsla(22, 94%, 60%, 0.10);
    border-color: hsla(22, 94%, 60%, 0.45);
  }
  div[data-stepper-pill='inactive'] {
    background-color: hsla(0, 0%, 5%, 0.6);
    border-color: hsla(0, 0%, 100%, 0.10);
    color: hsl(0 0% 63%);
  }
</style>
```

- [ ] **Step 2: Run the test to verify it passes**

Run: `cd web && bun test src/lib/components/Stepper.test.ts`

Expected: PASS — all three test cases green.

- [ ] **Step 3: Run typecheck**

Run: `cd web && bun run check`

Expected: zero errors.

- [ ] **Step 4: Commit**

```bash
git add web/src/lib/components/Stepper.svelte web/src/lib/components/Stepper.test.ts
git commit -m "feat(web): add Stepper for the /setup wizard"
```

---

## Task 5: Refactor `/login` to use `AuthShell`

**Files:**

- Modify: `web/src/routes/login/+page.svelte` (full rewrite of the markup; script block stays nearly identical)

This task is a purely visual change — the script block (state, derived values, effect, event handlers) is unchanged from today. No new tests; the existing `web/src/lib/auth.test.ts` already covers the login state machine via the `auth` store.

- [ ] **Step 1: Read the existing file to confirm the script block we're keeping**

Run: `cat web/src/routes/login/+page.svelte | head -90`

Confirm the existing script block defines: `username`, `password`, `busy`, `error`, `nowTick`, `lockedSecs`, `isLocked`, `next`, `submit`, `pad`, `lockedText`, `onMount` setup-status check, `$effect` interval. We keep all of these verbatim.

- [ ] **Step 2: Replace the file with the AuthShell-wrapped version**

Replace `web/src/routes/login/+page.svelte` with:

```svelte
<script lang="ts">
  import { AlertCircle, Loader2, Lock, LogIn } from 'lucide-svelte';
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import { api, HttpError } from '$lib/api';
  import { auth } from '$lib/auth.svelte';
  import AuthShell from '$lib/components/AuthShell.svelte';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';

  let username = $state('');
  let password = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);

  // Lockout countdown — populated from `auth.lockoutUntilEpoch` if a 429
  // came back. The interval reads `Date.now()` each tick so it counts
  // down monotonically without us needing to store the deadline twice.
  let nowTick = $state(Date.now());
  $effect(() => {
    const id = setInterval(() => (nowTick = Date.now()), 1000);
    return () => clearInterval(id);
  });
  const lockedSecs = $derived(
    Math.max(0, Math.ceil((auth.lockoutUntilEpoch - nowTick) / 1000))
  );
  const isLocked = $derived(lockedSecs > 0);

  // If the daemon needs setup (no users + token configured), bounce
  // operators here straight to the wizard. Same UI element either way for
  // anyone who lands at /login by default.
  onMount(async () => {
    try {
      const status = await api.auth.setupStatus();
      if (status.needs_setup) {
        void goto('/setup', { replaceState: true });
      }
    } catch {
      // Daemon unreachable — leave the form up so they can retry.
    }
  });

  // Where to send them after a successful login. Honors ?next= so a
  // protected route can deep-link back after auth, but only if the target
  // is a same-origin path (rejects http://attacker.com).
  const next = $derived.by(() => {
    const raw = $page.url.searchParams.get('next');
    if (!raw || !raw.startsWith('/') || raw.startsWith('//')) return '/';
    return raw;
  });

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (busy || isLocked) return;
    error = null;
    busy = true;
    try {
      await auth.login(username.trim(), password);
      void goto(next, { replaceState: true });
    } catch (err) {
      if (err instanceof HttpError) {
        if (err.status === 429) {
          error = 'too many attempts — wait for the timer to reset';
        } else if (err.status === 401) {
          error = 'wrong username or password';
        } else {
          error = err.message;
        }
      } else {
        error = String(err);
      }
    } finally {
      busy = false;
    }
  }

  function pad(n: number): string {
    return n.toString().padStart(2, '0');
  }
  const lockedText = $derived(
    isLocked ? `${pad(Math.floor(lockedSecs / 60))}:${pad(lockedSecs % 60)}` : ''
  );
</script>

<svelte:head>
  <title>Sign in · errex</title>
</svelte:head>

<AuthShell title="sign in to errex" subtitle="self-hosted error tracking">
  <form onsubmit={submit} class="flex flex-col gap-[14px]">
    {#if isLocked}
      <div
        class="flex items-start gap-2 rounded-md border p-2.5 text-[11px] leading-relaxed"
        style="border-color:hsla(0,72%,56%,0.32); background:hsla(0,72%,56%,0.08);"
      >
        <Lock class="mt-0.5 h-3.5 w-3.5 shrink-0" style="color:hsl(0 72% 70%);" />
        <div>
          locked out — too many attempts. try again in
          <span class="font-mono tabular-nums font-semibold">{lockedText}</span>.
        </div>
      </div>
    {/if}

    <div class="flex flex-col gap-[5px]">
      <Label for="u" class="text-[11px]">username</Label>
      <Input
        id="u"
        bind:value={username}
        autocomplete="username"
        autofocus
        class="h-10 text-[13px]"
        disabled={busy || isLocked}
      />
    </div>
    <div class="flex flex-col gap-[5px]">
      <Label for="p" class="text-[11px]">password</Label>
      <Input
        id="p"
        type="password"
        bind:value={password}
        autocomplete="current-password"
        class="h-10 text-[13px]"
        disabled={busy || isLocked}
      />
    </div>

    {#if error && !isLocked}
      <div
        class="flex items-start gap-2 rounded-md border p-2 text-[11px] leading-relaxed"
        style="border-color:hsla(0,72%,56%,0.32); background:hsla(0,72%,56%,0.08);"
      >
        <AlertCircle class="mt-0.5 h-3.5 w-3.5 shrink-0" style="color:hsl(0 72% 70%);" />
        <div>{error}</div>
      </div>
    {/if}

    <Button
      type="submit"
      disabled={busy || isLocked || username.trim().length === 0 || password.length === 0}
      class="h-10 w-full"
      style="box-shadow: 0 1px 0 hsla(0,0%,100%,0.18) inset, 0 8px 22px hsla(22, 94%, 50%, 0.28);"
    >
      {#if busy}
        <Loader2 class="h-4 w-4 animate-spin" />
      {:else}
        <LogIn class="h-4 w-4" />
      {/if}
      sign in
    </Button>
  </form>
</AuthShell>
```

- [ ] **Step 3: Run the existing auth tests to confirm the state machine still works**

Run: `cd web && bun test src/lib/auth.test.ts`

Expected: PASS — these test the `auth` store, which we haven't touched. They should remain green.

- [ ] **Step 4: Run the full test suite**

Run: `cd web && bun test`

Expected: PASS — every test still green, including the two new component tests.

- [ ] **Step 5: Run typecheck**

Run: `cd web && bun run check`

Expected: zero errors.

- [ ] **Step 6: Smoke-test in the browser**

Per CLAUDE.md "iteration speed" rule: visual changes use `bun run dev` against the running daemon, not a docker rebuild.

Open two terminals:

```bash
# Terminal 1: start the daemon (if not already running)
ERREX_DEV_MODE=true ERREX_ADMIN_TOKEN=devtoken cargo run -p errexd

# Terminal 2: start the SPA
cd web && bun run dev
```

Visit `http://localhost:5173/login` and verify by eye:

- The painted backdrop renders (orange wash top-left, violet wash bottom-right, faint noise overlay).
- The glass Card is centered with title `sign in to errex`, subtitle `self-hosted error tracking`, and the gradient mark above.
- Submitting wrong credentials shows a destructive-tinted alert above the button: `wrong username or password`.
- Submitting many wrong credentials triggers the daemon's lockout — the alert above the form switches to `locked out — too many attempts. try again in MM:SS` with a counting-down monospace timer. Inputs and button dim and disable.

If any of the above looks broken, fix in this task before committing.

- [ ] **Step 7: Commit**

```bash
git add web/src/routes/login/+page.svelte
git commit -m "feat(web): reskin /login to the Painted direction"
```

---

## Task 6: Refactor `/setup` to use `AuthShell` + `Stepper`

**Files:**

- Modify: `web/src/routes/setup/+page.svelte` (full rewrite of markup; script block stays nearly identical, with one cosmetic addition — see Step 1)

This task is also a purely visual change. The state machine (`step: 'token' | 'user'`, `pageState: 'loading' | 'ready' | 'disabled' | 'done'`) is preserved.

- [ ] **Step 1: Read the existing file to confirm the script block we're keeping**

Run: `cat web/src/routes/setup/+page.svelte | head -90`

Confirm the existing script defines: `step`, `token`, `username`, `password`, `passwordConfirm`, `busy`, `error`, `pageState`, `onMount` status probe, `advanceFromToken`, `completeSetup`. We keep all of these verbatim. The only logic change in this task is the title/subtitle strings used in the markup — these match the spec's per-step copy (`welcome to errex` for step 1, `create your account` for step 2).

- [ ] **Step 2: Replace the file with the AuthShell-wrapped version**

Replace `web/src/routes/setup/+page.svelte` with:

```svelte
<script lang="ts">
  import {
    AlertCircle,
    AlertTriangle,
    ArrowRight,
    KeyRound,
    Loader2,
    UserPlus
  } from 'lucide-svelte';
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { api, HttpError } from '$lib/api';
  import { auth } from '$lib/auth.svelte';
  import AuthShell from '$lib/components/AuthShell.svelte';
  import Stepper from '$lib/components/Stepper.svelte';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';

  type Step = 'token' | 'user';

  let step = $state<Step>('token');
  let token = $state('');
  let username = $state('');
  let password = $state('');
  let passwordConfirm = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);
  /** True iff the daemon is in a state where setup makes sense. Other
   *  states: setup_disabled (no env token; show docs), or already-set-up
   *  (redirect to /login). */
  let pageState = $state<'loading' | 'ready' | 'disabled' | 'done'>('loading');

  onMount(async () => {
    try {
      const status = await api.auth.setupStatus();
      if (status.setup_disabled) {
        pageState = 'disabled';
      } else if (!status.needs_setup) {
        pageState = 'done';
        void goto('/login', { replaceState: true });
      } else {
        pageState = 'ready';
      }
    } catch {
      pageState = 'ready';
    }
  });

  async function advanceFromToken(e: SubmitEvent) {
    e.preventDefault();
    if (token.trim().length === 0) return;
    error = null;
    step = 'user';
  }

  async function completeSetup(e: SubmitEvent) {
    e.preventDefault();
    if (password !== passwordConfirm) {
      error = 'passwords do not match';
      return;
    }
    busy = true;
    error = null;
    try {
      await auth.setup(token.trim(), username.trim(), password);
      void goto('/', { replaceState: true });
    } catch (err) {
      if (err instanceof HttpError) {
        if (err.status === 401) {
          // Wrong token: kick the operator back to step 1 so they can paste
          // the right one without losing the username they typed.
          step = 'token';
          error = 'invalid setup token — paste the value of ERREX_ADMIN_TOKEN';
        } else {
          error = err.message;
        }
      } else {
        error = String(err);
      }
    } finally {
      busy = false;
    }
  }

  // Title/subtitle vary per step. Step 1 keeps the "welcome to errex" framing
  // because the operator hasn't proven host access yet. Step 2 switches to
  // "create your account" because the wizard now talks about the user, not
  // the daemon.
  const title = $derived(step === 'user' ? 'create your account' : 'welcome to errex');
  const subtitle = $derived(
    step === 'user'
      ? "you'll use this to sign in from now on"
      : 'set up your first operator account'
  );
</script>

<svelte:head>
  <title>Set up · errex</title>
</svelte:head>

<AuthShell {title} {subtitle}>
  {#if pageState === 'loading'}
    <div class="text-muted-foreground flex items-center justify-center gap-2 py-6 text-[12px]">
      <Loader2 class="h-4 w-4 animate-spin" /> checking daemon…
    </div>
  {:else if pageState === 'disabled'}
    <div
      class="flex items-start gap-2 rounded-md border p-2.5 text-[11px] leading-relaxed"
      style="border-color:hsla(38,92%,56%,0.35); background:hsla(38,92%,56%,0.07);"
    >
      <AlertTriangle class="mt-0.5 h-3.5 w-3.5 shrink-0" style="color:hsl(38 92% 70%);" />
      <div>
        setup is disabled — daemon was started without
        <code class="rounded px-1.5 py-0.5 font-mono text-[10.5px]" style="background:hsla(0,0%,0%,0.4);">
          ERREX_ADMIN_TOKEN
        </code>
        . set the env var and restart.
      </div>
    </div>
  {:else if pageState === 'done'}
    <div class="text-muted-foreground flex items-center justify-center gap-2 py-6 text-[12px]">
      <Loader2 class="h-4 w-4 animate-spin" /> setup already complete — sending you to /login…
    </div>
  {:else if step === 'token'}
    <!-- Step 1: setup token -->
    <Stepper current={1} labels={['verify host access', 'create account']} />
    <p class="text-muted-foreground text-[11.5px]">
      paste
      <code class="rounded px-1.5 py-0.5 font-mono text-[10.5px]" style="background:hsla(0,0%,0%,0.4);">
        ERREX_ADMIN_TOKEN
      </code>
      from the daemon environment. proves you have host access.
    </p>
    <form onsubmit={advanceFromToken} class="flex flex-col gap-[14px]">
      <div class="flex flex-col gap-[5px]">
        <Label for="token" class="text-[11px]">setup token</Label>
        <Input
          id="token"
          type="password"
          bind:value={token}
          autocomplete="off"
          autofocus
          class="h-10 font-mono text-[12px]"
        />
      </div>
      {#if error}
        <div
          class="flex items-start gap-2 rounded-md border p-2 text-[11px] leading-relaxed"
          style="border-color:hsla(0,72%,56%,0.32); background:hsla(0,72%,56%,0.08);"
        >
          <AlertCircle class="mt-0.5 h-3.5 w-3.5 shrink-0" style="color:hsl(0 72% 70%);" />
          <div>{error}</div>
        </div>
      {/if}
      <Button
        type="submit"
        disabled={token.trim().length === 0}
        class="h-10 w-full"
        style="box-shadow: 0 1px 0 hsla(0,0%,100%,0.18) inset, 0 8px 22px hsla(22, 94%, 50%, 0.28);"
      >
        <KeyRound class="h-4 w-4" />
        continue
        <ArrowRight class="h-3.5 w-3.5" />
      </Button>
    </form>
  {:else}
    <!-- Step 2: user creation -->
    <Stepper current={2} labels={['verify host access', 'create account']} />
    <form onsubmit={completeSetup} class="flex flex-col gap-[14px]">
      <div class="flex flex-col gap-[5px]">
        <Label for="u" class="text-[11px]">username</Label>
        <Input
          id="u"
          bind:value={username}
          autocomplete="username"
          autofocus
          class="h-10 text-[13px]"
          disabled={busy}
        />
      </div>
      <div class="flex flex-col gap-[5px]">
        <Label for="p" class="text-[11px]">password (min 12 chars)</Label>
        <Input
          id="p"
          type="password"
          bind:value={password}
          autocomplete="new-password"
          class="h-10 text-[13px]"
          disabled={busy}
        />
      </div>
      <div class="flex flex-col gap-[5px]">
        <Label for="pc" class="text-[11px]">confirm password</Label>
        <Input
          id="pc"
          type="password"
          bind:value={passwordConfirm}
          autocomplete="new-password"
          class="h-10 text-[13px]"
          disabled={busy}
        />
      </div>
      {#if error}
        <div
          class="flex items-start gap-2 rounded-md border p-2 text-[11px] leading-relaxed"
          style="border-color:hsla(0,72%,56%,0.32); background:hsla(0,72%,56%,0.08);"
        >
          <AlertCircle class="mt-0.5 h-3.5 w-3.5 shrink-0" style="color:hsl(0 72% 70%);" />
          <div>{error}</div>
        </div>
      {/if}
      <Button
        type="submit"
        disabled={busy || username.trim().length === 0 || password.length < 12}
        class="h-10 w-full"
        style="box-shadow: 0 1px 0 hsla(0,0%,100%,0.18) inset, 0 8px 22px hsla(22, 94%, 50%, 0.28);"
      >
        {#if busy}
          <Loader2 class="h-4 w-4 animate-spin" />
        {:else}
          <UserPlus class="h-4 w-4" />
        {/if}
        create admin
        <ArrowRight class="h-3.5 w-3.5" />
      </Button>
    </form>
  {/if}
</AuthShell>
```

- [ ] **Step 3: Run the full test suite**

Run: `cd web && bun test`

Expected: PASS — all tests including the two new component tests.

- [ ] **Step 4: Run typecheck**

Run: `cd web && bun run check`

Expected: zero errors.

- [ ] **Step 5: Smoke-test the four `/setup` states**

With the daemon and SPA both running (per Task 5 step 6), wipe the existing user(s) so setup mode is reachable:

```bash
# Stop the daemon, wipe its data dir, restart
rm -rf data/errex.db data/errex.db-wal data/errex.db-shm
ERREX_DEV_MODE=true ERREX_ADMIN_TOKEN=devtoken cargo run -p errexd
```

Visit `http://localhost:5173/setup` and verify:

- **Loading flicker** — the "checking daemon…" spinner appears briefly on first load.
- **Step 1** — Stepper shows pill 1 active with label `verify host access`, pill 2 outlined. Pasting the token and clicking `continue` advances to step 2 in-place.
- **Step 2** — Stepper now shows pill 1 with a checkmark and label `verified`, pill 2 active with label `create account`. Username + password + confirm all render with `h-10` height. The submit button enables only when both fields are non-empty and password ≥ 12 chars.
- **Wrong token round-trip** — Type a wrong token at step 1, advance to step 2, fill in a username/password, submit. Server returns 401, the page bounces back to step 1 with the destructive alert above the form: `invalid setup token — paste the value of ERREX_ADMIN_TOKEN`. Username field is preserved (intentional — see comment in `completeSetup`).
- **Setup-disabled** — Restart the daemon WITHOUT `ERREX_ADMIN_TOKEN` set:
  ```bash
  ERREX_DEV_MODE=true cargo run -p errexd
  ```
  Visit `/setup`. The amber-tinted alert renders inside the Card explaining the missing env var. No form, no Stepper.
- **Already-set-up redirect** — Complete setup once, then visit `/setup` again. The page should briefly show the "setup already complete — sending you to /login…" message, then redirect.

- [ ] **Step 6: Commit**

```bash
git add web/src/routes/setup/+page.svelte
git commit -m "feat(web): reskin /setup to the Painted direction with Stepper"
```

---

## Task 7: Final verification

- [ ] **Step 1: Run the combined gate**

Per CLAUDE.md the green-PR gate is `./errex.sh check && bun test`.

Run:

```bash
./errex.sh check
cd web && bun test
```

Expected: both green. The Rust check is unchanged behavior — included only because CLAUDE.md treats it as the combined gate.

- [ ] **Step 2: Manual smoke summary**

Verify the full auth flow end-to-end one last time:

1. Wipe data, start daemon with `ERREX_ADMIN_TOKEN=devtoken`.
2. `/setup` step 1 → enter `devtoken` → step 2 → create admin → land on `/`.
3. Sign out from the app header.
4. `/login` → sign in with the admin you just created → land on `/`.
5. Sign out, then attempt 6 wrong logins quickly → confirm lockout banner counts down.

- [ ] **Step 3: Final commit (if anything was tweaked during smoke testing)**

```bash
git status
# If anything changed during smoke testing:
git add -A && git commit -m "fix(web): tweaks from auth-flow smoke test"
```

If nothing changed, this step is a no-op.

---

## Self-review notes

**Spec coverage:**

- `/login` happy path → Task 5
- `/login` 401 inline error → Task 5 (renders below inputs, above button, when `error && !isLocked`)
- `/login` 429 lockout → Task 5 (renders above inputs when `isLocked`, with countdown)
- `/setup` setup-disabled → Task 6 (the `pageState === 'disabled'` branch)
- `/setup` step 1 → Task 6 (the `step === 'token'` branch with `<Stepper current={1} ...>`)
- `/setup` step 2 → Task 6 (the `step === 'user'` branch with `<Stepper current={2} ...>`)
- New `AuthShell` component → Tasks 1–2
- New `Stepper` component → Tasks 3–4
- Theme/a11y notes (aria-hidden gradients, prefers-reduced-transparency fallback) → Task 2 (the inline `<style>` block in `AuthShell.svelte`)
- TDD per CLAUDE.md → component tests added in Tasks 1, 3 before any implementation; existing `auth.test.ts` covers the state machine already
- Lightweight constraint → no new bundles, no new global CSS, ~600-byte inline noise SVG

**Open questions resolved per the spec:**

1. *Mark element — inline or factored?* — Inlined inside `AuthShell` (Task 2).
2. *Stepper position when there's an alert* — Stepper renders before the form, alert renders below the input(s) inside the form (Task 6).
3. *`prefers-reduced-transparency` fallback* — Implemented as a `<style>` rule inside `AuthShell.svelte` (Task 2).
