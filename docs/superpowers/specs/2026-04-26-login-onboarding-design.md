# Login + onboarding redesign — "Painted"

**Date:** 2026-04-26
**Scope:** `/login` and first-admin `/setup` (the only two pre-auth surfaces today)
**Status:** approved direction; ready for implementation plan

## Overview

Today's auth pages (`/login`, `/setup`) are functional but visually flat: a small centered Card on a plain dark background. This redesign keeps the same state machine and HTTP contract — cookie sessions, lockout countdown, setup-token bootstrap, `setup_disabled` empty-state, `?next=` deep-link redirect — and replaces the chrome with a single, deliberate visual treatment we call **Painted**: a soft orange + violet gradient backdrop, a translucent glass Card, and a small gradient brand mark.

The redesign also factors out the shared auth-page scaffolding so new auth surfaces (forgot-password, invited-user signup, etc.) can adopt it without duplicating CSS.

## In scope

- `/login` happy path
- `/login` HTTP 401 inline error ("wrong username or password")
- `/login` HTTP 429 lockout with monospace tabular countdown
- `/setup` `setup_disabled` empty-state (no `ERREX_ADMIN_TOKEN` configured)
- `/setup` step 1 — verify host access (paste setup token)
- `/setup` step 2 — create admin (username + password + confirm)
- New shared `AuthShell` component (backdrop + glass Card + brand mark)
- New `Stepper` component for the setup wizard

## Out of scope

- Invited-teammate signup flow (deferred — see `/team` work)
- First-run experience inside the app after first login (deferred)
- Light mode of the auth pages (the rest of the app is dark-first; no light variant ships with this work)
- Any change to the auth API contract or session/lockout behavior
- Displaying any instance-specific data on pre-auth surfaces (hostname, version, build SHA, uptime, user count, db size — explicitly forbidden)

## Visual direction — "Painted"

Page background:

- Base canvas: `--background` (`hsl(0 0% 7%)`)
- Warm orange wash, top-left quadrant, ~70% width × 110% height, blurred 56px:
  `radial-gradient(ellipse at 32% 50%, hsla(22, 94%, 53%, 0.55) 0%, hsla(22, 94%, 53%, 0.18) 32%, transparent 62%)`
- Cool violet wash, bottom-right quadrant, ~65% × 95%, blurred 64px:
  `radial-gradient(ellipse at 60% 40%, hsla(265, 70%, 55%, 0.42) 0%, hsla(285, 70%, 55%, 0.15) 35%, transparent 65%)`
- Faint SVG noise overlay (inline data URI, ~600 bytes) at 6% opacity, `mix-blend-mode: overlay` — keeps the gradient from looking "video game"

Card:

- Max width: 360px. On viewports < 380px wide, Card scales to `calc(100vw - 32px)` so 16px gutters survive on phones.
- Background: `hsla(0, 0%, 5.5%, 0.66)` over `backdrop-filter: blur(22px) saturate(140%)` (with `-webkit-backdrop-filter` for Safari)
- Border: 1px `hsla(0, 0%, 100%, 0.07)`
- Border radius: 12px
- Box shadow: `0 1px 0 hsla(0,0%,100%,0.05) inset, 0 30px 80px rgba(0,0,0,0.45)`
- Padding: 32px 30px 26px (login). 26px 24px 22px on step screens that have less vertical content.

Brand mark (replaces today's `AlertCircle` + `bg-primary/10` square):

- 36×36, 9px radius
- `linear-gradient(140deg, hsl(22 94% 60%), hsl(36 96% 58%))`
- Inner shadow ring: `0 0 0 1px hsla(22, 94%, 50%, 0.35), 0 8px 24px hsla(22, 94%, 50%, 0.35)`
- Houses the existing `AlertCircle` from `lucide-svelte` in `hsl(22 96% 12%)` (deep amber, sits on the orange face)

Inputs (shadcn `Input`, with a glass-page-aware override class):

- Height: 36px (was h-10/40px today — slightly tighter to balance the Card padding)
- Background: `hsla(0, 0%, 5%, 0.7)` — opaque enough that text never reads through to the gradient
- Border: 1px `hsla(0, 0%, 100%, 0.10)`
- Focus ring: `border-color: var(--primary)` + `box-shadow: 0 0 0 3px hsla(22, 94%, 60%, 0.15)`

Button (shadcn `Button` default variant — primary already maps to burnt-orange):

- Height: 38px
- Inset highlight: `box-shadow: 0 1px 0 hsla(0,0%,100%,0.18) inset, 0 8px 22px hsla(22, 94%, 50%, 0.28)` — adds a subtle "pressed-from-above" gradient feel on the primary button only

Typography stays as-is — JetBrains Mono Variable, 12px base, lowercase voice.

## Screen specifications

### `/login` happy path

Layout: `AuthShell` → glass `Card` → vertical stack:

1. Brand mark (36×36 gradient block + AlertCircle)
2. Title `sign in to errex` (17px, weight 600, `-0.018em` tracking)
3. Sub `self-hosted error tracking` (11.5px, muted-foreground)
4. Username field (`Label` + `Input` with `autocomplete="username"`, autofocus)
5. Password field (`Label` + `Input type="password"` with `autocomplete="current-password"`)
6. Submit button — full width, primary, label `sign in`

State preserved from today: `?next=` deep-link redirect (only same-origin paths accepted), `auto-redirect to /setup if needs_setup`.

### `/login` HTTP 401 — wrong credentials

Identical to happy path plus a destructive-tinted single-line alert directly above the submit button:

- Border: 1px `hsla(0, 72%, 56%, 0.32)`
- Background: `hsla(0, 72%, 56%, 0.08)`
- Icon: lucide `AlertCircle` 14×14
- Copy: `wrong username or password` (today's exact string)

### `/login` HTTP 429 — lockout

Inline alert above the form (not above the button), same destructive treatment as 401, with a monospace tabular-nums countdown:

- Copy: `locked out — too many attempts. try again in 04:32.`
- Countdown updates from `auth.lockoutUntilEpoch` on a 1-second interval (existing logic)
- Form fields and submit button drop to `opacity: 0.5` and become disabled (existing logic)

### `/setup` `setup_disabled` empty-state

Same `AuthShell` + glass Card. Mark and title (`welcome to errex` / `set up your first operator account`), then a warm-amber alert (no form, no stepper):

- Border: 1px `hsla(38, 92%, 56%, 0.35)`
- Background: `hsla(38, 92%, 56%, 0.07)`
- Icon: lucide `AlertTriangle`
- Copy: `setup is disabled — daemon was started without ` then `ERREX_ADMIN_TOKEN` in an inline `<code>` tile, then `. set the env var and restart.`

### `/setup` step 1 — verify host access

Same shell. Mark, title `welcome to errex`, sub `set up your first operator account`. Then:

1. `Stepper`: pill 1 active (filled primary), label `verify host access`, separator line, pill 2 outline.
2. Explanation: `paste ` + `<code>ERREX_ADMIN_TOKEN</code>` + ` from the daemon environment. proves you have host access.`
3. `Label` + mono `Input type="password"` for the token (mono so the operator can spot typos at a glance).
4. Button `continue →`. Disabled until the token field is non-empty.

On submit, advances to step 2 in-place (no route change). On the dispatched submit returning 401 (server rejects token), bounce back to step 1 with an alert above the input: `invalid setup token — paste the value of ERREX_ADMIN_TOKEN`.

### `/setup` step 2 — create admin

Same shell. Mark, title `create your account`, sub `you'll use this to sign in from now on`. Then:

1. `Stepper`: pill 1 becomes a checkmark in primary tint with label `verified`, separator, pill 2 active.
2. `Label` + `Input` for username (`autocomplete="username"`, autofocus, max 64 chars).
3. `Label` + `Input type="password"` for password (`autocomplete="new-password"`, label includes `(min 12 chars)` since the daemon enforces that — surfacing it inline avoids a round-trip rejection).
4. `Label` + `Input type="password"` for confirm.
5. Button `create admin →`. Disabled until username is non-empty AND password ≥ 12 chars AND confirm matches.

On success, the daemon signs the operator in immediately (today's behavior — the `setup` handler mints a session cookie with the response) and the SPA routes to `/`.

## Components

### New: `AuthShell.svelte` (shared)

Wraps any pre-auth page. Provides:

- Full-viewport flex centering on `bg-background`
- Two absolutely-positioned gradient layers (orange wash, violet wash) and the noise overlay, behind a `pointer-events: none` mask so they never intercept clicks
- Glass `Card` slot (children render here)
- One required prop: `class?: string` for one-off overrides

Why a separate component (vs inline CSS on each page): the gradients + noise SVG together are ~30 lines of CSS and a 600-byte SVG data URI. Duplicating them across `login` and `setup` means changes have to land in two files. A shared shell also makes the future invited-user signup page free of redesign work.

### New: `Stepper.svelte`

Tiny pill-stepper for 2-step flows. Public API:

```ts
{
  current: 1 | 2;        // which step is active
  labels: [string, string];  // labels for step 1 and step 2 in their *active* state
  doneLabel?: string;    // label shown next to a completed pill (default: 'verified')
}
```

Pill states:

- **Inactive** (`current < n`): dark glass background, neutral border, numeric label inside
- **Active** (`current === n`): primary fill, primary-foreground text, primary-tinted shadow, numeric label
- **Done** (`current > n`): primary-tinted background + border, checkmark icon (lucide `Check`)

Rendering rule: render two pills side-by-side connected by a 1px separator line. Next to the *active* pill, render `labels[current - 1]` in the foreground color. Next to a *done* pill, render `doneLabel` in muted-foreground. Inactive pills get no adjacent label.

Reusable later by the eventual invited-user signup flow and any future multi-step admin task. If we ever need 3+ steps, generalize the API; today's only consumer is `/setup` so 2-step is hard-coded.

### Refactored: `web/src/routes/login/+page.svelte`

- Replaces today's `<div class="bg-background flex min-h-screen ...">` wrapper with `<AuthShell>`
- Replaces the existing brand-mark `<div class="bg-primary/10 ...">` with the new gradient mark element (could be inlined or factored out — see open question below)
- Inputs and Button keep their current shadcn imports; Input gets a new `auth-input` class for the glass-aware styling
- All state logic (`busy`, `error`, `nowTick`, `lockedSecs`, `next`, `submit`, lockout countdown) is preserved verbatim — the only diff is JSX/markup and styling

### Refactored: `web/src/routes/setup/+page.svelte`

- Same `AuthShell` swap
- `step` state machine (`'token' | 'user'`) preserved
- `pageState` machine preserved (`'loading' | 'ready' | 'disabled' | 'done'`)
- New `<Stepper current={step === 'token' ? 1 : 2} labels={['verify host access', 'create account']} />`

## Theme & accessibility

- Colors stay inside the existing `app.css` token set; no new tokens added. The two new gradient hues (orange wash, violet wash) are inline `hsla()` values inside `AuthShell` because they're decorative, not semantic.
- Focus rings on Inputs and Buttons retain shadcn's default ring variable (`--ring`); the focus state is visible against the glass.
- The lockout / 401 / setup-disabled alerts use destructive and warning hues with sufficient contrast (≥4.5:1 on text against the alert background).
- All form fields keep their existing `Label for=` associations and `autocomplete=` hints.
- `prefers-reduced-motion`: today's design has no motion. The new design adds none either — the gradients are static, the stepper transitions on click are CSS `transition: 120ms ease`, well under the 200ms threshold most reduced-motion guidelines flag. No marquee, no pulse, no parallax.

## Lightweight constraints

errex's hard non-functional rule is "lightweight first." Audit:

- No new images, no icon fonts, no third-party CSS — the noise SVG is ~600 bytes inline
- `backdrop-filter: blur()` is GPU-accelerated; a single auth page with two filtered layers is well under the budget
- No new JS dependencies; `Stepper` is ~40 LOC pure Svelte
- Bundle delta: estimate +1.5 KB gzipped (component + styles), no runtime cost on non-auth pages because routes are code-split

## Testing

Per CLAUDE.md, frontend TDD is mandatory.

Unit tests (Vitest, jsdom):

- `Stepper.test.ts` — renders correct pill states for `current = 1` and `current = 2`; checkmark appears on completed pills; labels render where supplied
- `AuthShell.test.ts` — renders children inside a glass-styled Card; gradients are rendered as decorative (`aria-hidden`) layers; no instance metadata escapes into the DOM

State-logic tests (existing, kept):

- Login submit happy/401/429 paths (already covered in `auth.test.ts` — verify still passing)
- Setup state machine transitions (`token → user`, `user → token` on 401)
- `next=` redirect rejects non-same-origin paths (already covered)

What does NOT get tests (per CLAUDE.md):

- Visual snapshots of the rendered Card / backdrop. Snapshots rot fast at this size.

## Open questions / decisions for the implementation plan

1. **Mark element — inline or factored?** The gradient mark is currently used by all three auth screens (login, setup-1, setup-2, setup-disabled). Two options: inline the markup in `AuthShell` (always visible, every auth page gets it free) or factor as `<AuthShell.Mark />`. Recommendation: inline in `AuthShell` since all current and foreseeable auth screens want it.
2. **Stepper position when there's an alert** — on the `setup_disabled` screen there's no stepper because there's no flow to advance through. On step 1 with an alert (invalid token), does the alert sit above or below the stepper? Recommendation: alert below stepper, above the explanation text — matches normal reading order.
3. **Backdrop on prefers-reduced-transparency** — Safari and macOS users can request reduced transparency. Should we drop the `backdrop-filter` and switch to a solid Card in that case? Recommendation: yes, tested via `@media (prefers-reduced-transparency: reduce)` falling back to `background: hsl(0 0% 9%)` (today's solid card color).

## Voice / copy

All UI strings stay lowercase, terse — matches today's voice:

- `sign in to errex` / `self-hosted error tracking`
- `welcome to errex` / `set up your first operator account`
- `create your account` / `you'll use this to sign in from now on`
- `wrong username or password` (existing 401 string, unchanged)
- `locked out — too many attempts. try again in 04:32.` (existing lockout copy, slightly tightened)
- `setup is disabled — daemon was started without ERREX_ADMIN_TOKEN. set the env var and restart.` (existing setup-disabled copy, slightly tightened)
