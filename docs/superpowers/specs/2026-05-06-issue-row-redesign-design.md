# IssueRow visual redesign — design spec

**Date:** 2026-05-06
**Files touched:** `web/src/lib/components/IssueRow.svelte`,
`web/src/lib/components/IssueList.svelte`,
`web/src/lib/components/Sparkline.svelte`,
`web/src/lib/utils.ts` (+ test).

## Why

The current row treats event count as metadata and severity as a 6×6px dot.
For a triage inbox, those signals are the protagonists — the user's first
question is "how bad?" and the second is "how often?". Both are buried
under uniform typography. Empty sparklines render as fragmented dots that
look like a loading bug. Column header and group header use identical
styling and confuse intent.

## What changes

### 1. Severity rail (replaces level dot)

A 2px wide vertical bar pinned to the row's left edge, full row height.
Color matches the issue level (warning amber / error red / fatal light-red).
Where consecutive rows share severity, the rail visually merges into a
band — free clustering signal. Removes the 6×6 dot.

### 2. Count tier — threshold colors

Event count is the primary signal. Render it 15px / weight 600 with color
chosen by tier:

- `count < 10` → default foreground (white)
- `10 ≤ count < 100` → amber
- `count ≥ 100` → light red

Tier resolution is a pure function `countTier(n) → 'low' | 'mid' | 'high'`
in `utils.ts`.

### 3. Row layout — CSS grid 2-row

Replace flex+sub-flex with `grid-template-columns: minmax(36px,auto) minmax(0,1fr) auto`
and `grid-template-rows: 1fr 1fr`. Count and title both sit in row 1;
users and subtitle both sit in row 2. `align-items: baseline`.

This guarantees baselines align by structure, not by line-height arithmetic.

Row height: 50px fixed. Padding: `0 16px` symmetric.

### 4. Meta cluster — sparkline + timestamp colocated

Sparkline (48px wide, 14px tall, 45% foreground alpha) and timestamp
("11h", `relativeTimeShort`) live in a single right-aligned flex cluster
with `gap: 10px`, spanning both grid rows centered. Stops the sparkline
from floating exiled to the right with a 500px void before it.

### 5. Sparkline empty-state

Currently `mode='bars'` renders empty buckets at `opacity: 0.25`, producing
the fragmented-dots look that reads as a bug. Change:

- Empty buckets render at opacity 0 (invisible).
- If `sum(values) === 0`, render a single horizontal 14×1px dash centered
  in the SVG.

### 6. Users — icon + count

Replace `"1 user" / "12 users"` text with `User` icon (lucide-svelte) +
tabular number. Width follows content. When `user_count` is null/0, the
slot is empty.

### 7. Subtitle hierarchy

Function in `text-foreground/70`, path in `text-muted-foreground`.
Separator `·` margin tightens to 4px (was 5px each side, total 10).

### 8. Column header — removed

The header band ("· EV ISSUE 24H LAST") is removed from `IssueList.svelte`.
Group header ("today (4)") stays, with its color elevated to `#b8b8b8` and
weight 500. `top:0` sticky offset for the group header drops from `top-6`
to `top-0`.

## Tests

- `countTier` — boundary tests for 0, 9, 10, 99, 100, 1000.
- Sparkline empty-state — when given all-zero values, the rendered SVG
  contains a `<line>` or `<rect>` matching the dash spec; when given
  non-zero values, it renders the bar layout.

Visual changes themselves are not tested (per CLAUDE.md "pure visual
styling" exemption).

## Out of scope

- Hover-revealed action buttons (resolve/ignore/assign) keep their
  current behavior; the meta cluster collapses to make room on hover via
  the existing `group-hover:invisible` pattern.
- Detail pane and rails (UserRail, ProjectRail) are unchanged.
- The sparkline color override per-issue (`fill={dotColor}`) is dropped —
  sparkline is now monochrome muted-foreground. Severity is communicated
  exclusively by the rail.
