# Polish the "Create your first project" empty state

**Date:** 2026-04-26
**Scope:** purely visual — no new flow, no new components, no behavior change.

## Background

`/projects` renders three mutually exclusive states inside
`web/src/routes/projects/+layout.svelte`:

1. Admin lockout card (viewer role, lines 70–86).
2. Empty state — what this spec changes (lines 92–134).
3. Project rail + `ProjectDetail` (lines 135–140).

The empty state today is a small dashed-border `Card` containing a centered
icon, the title "Create your first project", a description, and a horizontal
form (input + Create button). It works but reads as an afterthought next to
the rest of the surface (`/setup`, `ProjectDetail`, etc.). This spec brings
it up to that bar without inflating scope.

## Goal

Same shape, more confident execution. The user should land on a single
centered card, see one obvious next action, and click Create. Behavior is
identical to today's. Diff is markup + tokens + one new helper string.

Anti-goals (explicitly out of scope):

- Multi-step wizards.
- Post-create teaching surfaces (DSN preview, language tabs, snippet,
  "listening for first event" indicator).
- Inline form validation beyond what `admin.createProject` already does on
  failure.
- Routing changes — the existing auto-redirect to `/projects/<first>` on
  load (lines 38–48) stays.

These were considered and rejected in the brainstorm in favor of a
polish-only diff that respects the lightweight-first constraint in
`CLAUDE.md`.

## Visual spec

### Card

- Width: `max-w-md` (existing) → kept. (~28rem.)
- Border: dashed → solid (`border-border`, no `border-dashed`).
- Padding: bumped from `Card.Header` defaults to a deliberate `p-8` on the
  outer card (matches the breathing room used by `/setup`).
- Top edge: 1px primary-tinted hairline as a subtle accent. Implementation:
  `::before` pseudo-element on the card root, full width, height `1px`,
  background `linear-gradient(90deg, transparent, hsl(var(--primary) / 0.45) 25%, hsl(var(--primary) / 0.45) 75%, transparent)`.
  Applied via an inline `style` attribute on `Card.Root`, not a Tailwind
  arbitrary-value class — matches the project's convention for HSL-with-alpha
  values (see `/setup/+page.svelte` for the same pattern).
- Drop shadow: stronger than the current default. Inline style:
  `box-shadow: 0 24px 60px hsl(0 0% 0% / 0.55);`.

### Icon

- Today: `Webhook` lucide icon, white-on-grey, `bg-accent/40` round container.
- Change to: same `Webhook` icon, primary-tinted square tile.
  - Container: `40×40` (`h-10 w-10`), `rounded-lg`, with an inline `style`
    setting `background: hsl(var(--primary) / 0.10); border: 1px solid hsl(var(--primary) / 0.22);`.
  - Stroke: `text-primary`.
  - HSL-with-alpha values go through inline `style`, per the project's
    `feedback_use_css_token_refs` convention. Pure-token colors
    (`text-primary`) stay on Tailwind utilities.

### Typography

- Title: `text-[16px]` → `text-[17px]`, `font-semibold`,
  `tracking-tight`. Copy unchanged: "Create your first project".
- Description (lede): `text-[13px]` → kept, `leading-relaxed` added,
  `max-w-[38ch]` to prevent over-long lines on wide viewports. Copy
  unchanged.

### Form

- Today: horizontal — input and Create button on the same row, `items-end gap-3`.
- Change to: vertical stack inside the form.
  - `Label` for "Project name", small `text-[11px]` muted.
  - `Input`, `h-10`, full width.
  - Helper line below input, `text-[11px] text-muted-foreground/80`. Copy:
    `shows up in URLs and the DSN · case-sensitive · max 64 chars`.
    - Phrased to mirror the actual server constraints
      (`validateNewProjectName` in `lib/projectsConsole.ts:70` enforces
      non-empty, ≤64, and project names are case-sensitive per the
      `isDeleteConfirmed` comment in the same file).
    - **Note for review:** this differs from the mockup's "lowercase,
      hyphens — appears in URLs and the DSN", which would have been a false
      claim — the validator does not enforce case or character class.
  - Create button: wrapped in a `<div class="flex justify-end">` below
    the input row so it sits flush right under the helper.
- Input gains `autofocus` — this is the only action on the page; cursor
  belongs there.
- Button label: "Create" → "Create project". Disambiguates inside a card
  whose title also reads "Create your first project".
- Button styling: keep the existing `Button` primary variant, add the
  primary-glow inline style already in use on `/setup`'s submit button:
  `box-shadow: 0 1px 0 hsla(0,0%,100%,0.18) inset, 0 8px 22px hsl(var(--primary) / 0.28)`.

### Footer line

- New: a single dim line under the form, separated by `border-t border-border` and `pt-4 mt-5`.
- Copy (three labels separated by small dot glyphs):
  `next  ·  copy your DSN  ·  send a test event`.
  - Separators rendered as small CSS dots — `<span>` with `h-1 w-1
    rounded-full bg-muted-foreground/50`. Matches the mockup; not text
    bullets.
- Container: `flex items-center gap-2`, `text-[11px] text-muted-foreground/70`.
- Purpose: orients the user to what comes after Create without prescribing
  or gating. Single line, no interactivity, no link affordances.

## Behavior

Unchanged. The submit handler stays:

```ts
async function createFirstProject(e: SubmitEvent) {
  e.preventDefault();
  const name = firstProjectName.trim();
  if (!name) return;
  creatingFirst = true;
  try {
    const created = await admin.createProject(name);
    firstProjectName = '';
    toast.success(`Project "${created.name}" created`);
    void goto(`/projects/${encodeURIComponent(created.name)}`, { replaceState: true });
  } catch (err) {
    toast.error('Failed to create project', { description: String(err) });
  } finally {
    creatingFirst = false;
  }
}
```

Loading state, error toasting, success toasting, and post-create redirect
all stay identical. Disabled state on the Create button when the trimmed
name is empty also stays.

## Where it lives

The empty state stays inline in `+layout.svelte`. No new component.

Rationale: the polish-only direction means roughly +20 lines of markup over
what's there now. Extracting a `FirstProjectFlow.svelte` for a single
empty-state card would be premature — the layout file already owns the
three mutually-exclusive states, and pulling one out would break the
"glance and see what renders when" property without shrinking the file
meaningfully.

If a future iteration adds a wizard or post-create teaching surface, *that*
is the moment to extract a component. Not this PR.

## Components used

All from `lib/components/ui/`:

- `Card`, `Card.Header`, `Card.Title`, `Card.Description`, `Card.Content`
  — the existing primitives.
- `Input`, `Label`, `Button` — same as today.

Lucide icons:

- `Webhook` (existing, kept).
- `Plus` (existing, on the Create button, kept).
- `Loader2` (existing, swapped in during the in-flight state, kept).

## Testing

Per `CLAUDE.md`, the TDD rule applies to *behavior*, not visual styling.
This change is markup and Tailwind classes; the only behavior unchanged.

What still gets a test:

- Existing `web/src/lib/admin.test.ts` already covers `admin.createProject`
  success and failure paths. Not touched.
- No new Vitest file.

What does NOT get a test:

- The visual changes themselves. Per CLAUDE.md: "Pure visual styling
  (Tailwind classes, layout decisions)" does not need tests, and "snapshot
  tests rot fast and don't catch real bugs at this size" — no snapshot
  test for the card.

## Edge cases

- **Admin lockout (viewer role):** unchanged — the existing
  `isViewerLockedOut` branch (lines 70–86) renders first, so the new card
  is only visible to admins.
- **Loading projects (`admin.loading`):** unchanged — the loading
  spinner branch (lines 87–91) renders before the empty branch.
- **Create fails (network / 4xx / name taken):** existing toast on failure;
  form keeps its content; Create button re-enabled. No new error UI.
- **`admin.projects.length` becomes ≥ 1 mid-render:** the existing
  `$effect` (lines 38–48) redirects to `/projects/<first>`. No interaction
  with the polished empty state.
- **User deletes their last project later:** they land back on `/projects`,
  see the polished empty state. Same code path.

## Files touched

- `web/src/routes/projects/+layout.svelte` — only file modified. Replace
  the block at lines 92–134 (the `isEmpty` branch). Keep imports for any
  new lucide icons (none required — `Plus`, `Webhook`, `Loader2` are
  already imported).

## Acceptance

- Visual diff matches the "A · confident card" mockup
  (`.superpowers/brainstorm/14419-1777249965/content/polish-three-ways.html`,
  left column).
- `./errex.sh check` and `bun test` pass — no new failures.
- Manual smoke: signed-in admin, no projects → see polished card → type
  "demo" → click "Create project" → land on `/projects/demo`.
- Manual smoke (regression): viewer role → still sees Admin-only card.
- Manual smoke (regression): existing project list → still sees rail +
  ProjectDetail.
