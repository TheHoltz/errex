<script lang="ts">
  import { ArrowRight, Ban, Check, User } from 'lucide-svelte';
  import { actions } from '$lib/actions.svelte';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { eventStream } from '$lib/eventStream.svelte';
  import { toggleIgnore, toggleResolve } from '$lib/issueOps';
  import { toast } from '$lib/toast.svelte';
  import type { Issue } from '$lib/types';
  import { cn, countTier, midTruncatePath, relativeTimeShort } from '$lib/utils';
  import Sparkline from './Sparkline.svelte';

  type Props = {
    issue: Issue;
    selected?: boolean;
    onSelect?: (id: number) => void;
  };

  let { issue, selected = false, onSelect }: Props = $props();

  // Severity rail color — drives the 2px left edge. Null for non-severity
  // levels (info/debug/null) and transactions, which render railless so a
  // wall of medium-importance noise doesn't steal attention from real
  // errors. Fatal gets a softened light-red so it reads as "above error"
  // without being shouty.
  function railColor(level: string | null | undefined): string | null {
    switch (level) {
      case 'fatal':
        return 'hsl(0 90% 75%)';
      case 'error':
        return 'hsl(var(--destructive))';
      case 'warning':
        return 'hsl(38 92% 50%)';
      default:
        return null;
    }
  }

  // Threshold-tier visual treatment for the count pill. Composes onto the
  // Badge "outline" variant so we keep using the primitive — only the
  // bg/border/text get tier-specific overrides. Saturation stays ≤15% per
  // vibe.md so nothing reads as alarmist on the calm rows.
  function tierPillClass(n: number): string {
    switch (countTier(n)) {
      case 'high':
        return 'border-destructive/30 bg-destructive/15 text-red-300';
      case 'mid':
        return 'border-amber-500/25 bg-amber-500/15 text-amber-400';
      default:
        return 'border-border/60 bg-foreground/[0.04] text-foreground';
    }
  }

  const isTxn = $derived(issue.kind === 'txn');
  const rail = $derived(isTxn ? null : railColor(issue.level));
  const pillCls = $derived(tierPillClass(issue.event_count));

  const sparkValues = $derived.by(() => {
    void eventStream.tick;
    return eventStream.buckets(issue.id, 24);
  });

  const spiking = $derived.by(() => {
    void eventStream.tick;
    return eventStream.isSpiking(issue.id);
  });

  const isMuted = $derived(issue.status === 'muted' || issue.status === 'ignored');

  // Split daemon-formatted "<func> in <file>" into its two parts. When
  // the daemon couldn't determine a frame, fall back to whatever culprit
  // is — the row still renders, just without the structured layout.
  const subtitleParts = $derived.by(() => {
    if (!issue.culprit) return null;
    const idx = issue.culprit.indexOf(' in ');
    if (idx === -1) return { fn: null, file: issue.culprit };
    return {
      fn: issue.culprit.slice(0, idx),
      file: issue.culprit.slice(idx + 4)
    };
  });
  const truncatedFile = $derived(subtitleParts?.file ? midTruncatePath(subtitleParts.file, 40) : '');

  function activate() {
    onSelect?.(issue.id);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      activate();
    }
  }

  function onResolve(e: MouseEvent) {
    e.stopPropagation();
    void toggleResolve(issue);
  }
  function onIgnore(e: MouseEvent) {
    e.stopPropagation();
    void toggleIgnore(issue);
  }
  function onAssign(e: MouseEvent) {
    e.stopPropagation();
    const prev = actions.assignToMe(issue);
    toast.success(`Assigned to ${actions.me}`, {
      undo: () => actions.setAssignee(issue, prev)
    });
  }
</script>

<!--
  Row layout: 2-row CSS grid. Col 1 holds the count pill (row 1) and the
  affected-users count (row 2, icon + number). Col 2 holds the issue
  identity — title (row 1), subtitle (row 2). Col 3 spans both rows for
  the meta cluster (sparkline + timestamp). Severity is signaled by a
  2px rail pinned to the left edge.

  Row is a div, not a button, so the hover-revealed action Buttons inside
  remain valid HTML (no nested buttons). Keyboard activation lives in
  onKey for Enter/Space parity.
-->
<div
  role="button"
  tabindex={selected ? 0 : -1}
  aria-pressed={selected}
  onclick={activate}
  onkeydown={onKey}
  class={cn(
    'group relative grid h-16 cursor-pointer items-center gap-x-4 border-b border-border/40 px-5 outline-none',
    'grid-cols-[auto_minmax(0,1fr)_auto] grid-rows-[1fr_1fr]',
    'hover:bg-foreground/3 focus-visible:bg-foreground/4',
    selected && 'bg-foreground/2.5',
    isMuted && 'opacity-55'
  )}
>
  {#if rail}
    <span
      aria-hidden="true"
      class="absolute inset-y-0 left-0 w-0.5"
      style="background-color: {rail}"
    ></span>
  {/if}
  {#if selected}
    <span aria-hidden="true" class="absolute inset-y-0 left-0 w-0.5 bg-foreground"></span>
  {/if}

  <!-- Col 1 (spans both rows) — count pill + affected users stacked.
       Wrapping the pair in a single flex-col + self-center keeps the
       pill vertically centered in the row when users is absent, and
       keeps the pill+users pair centered as a unit when users is set. -->
  <div class="col-start-1 row-span-2 flex flex-col items-end gap-1 self-center">
    {#if isTxn}
      <span
        class="inline-flex h-6 min-w-9 items-center justify-center rounded-md text-[13px] leading-none text-muted-foreground/70"
        aria-hidden="true"
      >
        ›
      </span>
    {:else}
      <Badge
        variant="outline"
        class={cn(
          'inline-flex h-6 min-w-9 items-center justify-center rounded-md px-2.5 text-[13px] font-semibold tabular-nums leading-none',
          pillCls
        )}
      >
        {issue.event_count}
      </Badge>
    {/if}
    {#if issue.user_count != null && issue.user_count > 0}
      <span
        class="inline-flex items-center gap-0.5 text-[10px] tabular-nums leading-none text-muted-foreground/60"
      >
        <User class="h-2.5 w-2.5 stroke-[2.5]" aria-hidden="true" />
        {issue.user_count}
      </span>
    {/if}
  </div>

  <!-- Col 2 / Row 1 — title with inline regressed/spiking signals -->
  <div
    class="col-start-2 row-start-1 flex min-w-0 items-center gap-1.5 self-end pb-0.5 text-[13px] leading-none"
  >
    {#if issue.regressed}
      <Badge
        variant="warning"
        class="rounded-sm px-1 py-0 text-[9px] font-semibold uppercase leading-none"
      >
        regressed
      </Badge>
    {/if}
    {#if spiking}
      <Tooltip.Root>
        <Tooltip.Trigger>
          {#snippet child({ props })}
            <span
              {...props}
              aria-label="Spiking in last 5 min"
              class="shrink-0 leading-none text-amber-400"
            >
              ↑
            </span>
          {/snippet}
        </Tooltip.Trigger>
        <Tooltip.Content>Spiking in last 5 min</Tooltip.Content>
      </Tooltip.Root>
    {/if}
    <span class="truncate text-foreground">{issue.title}</span>
  </div>

  <!-- Col 2 / Row 2 — subtitle: function in fg/70, path in muted -->
  {#if subtitleParts}
    <div
      class="col-start-2 row-start-2 flex min-w-0 items-baseline self-start pt-1 text-[11px] leading-none"
    >
      {#if subtitleParts.fn}
        <span class="max-w-[40%] shrink-0 truncate text-foreground/70">{subtitleParts.fn}</span>
        <span aria-hidden="true" class="mx-1 shrink-0 text-muted-foreground/40">·</span>
      {/if}
      <span class="min-w-0 flex-1 truncate text-muted-foreground">{truncatedFile}</span>
    </div>
  {/if}

  <!-- Col 3 (spans both rows) — meta cluster: sparkline + timestamp.
       Hover-revealed action buttons sit absolutely on top, so the cluster
       gets group-hover:invisible to clear the slot. -->
  <div class="relative col-start-3 row-span-2 flex items-center gap-2.5 self-center justify-self-end">
    <Sparkline
      values={sparkValues}
      mode="bars"
      width={48}
      height={14}
      fill="currentColor"
      class="text-muted-foreground/40 group-hover:invisible"
    />
    <span
      class="block whitespace-nowrap text-right text-[11px] tabular-nums leading-none text-muted-foreground/70 group-hover:invisible"
    >
      {relativeTimeShort(issue.last_seen)}
    </span>
    <div
      class="pointer-events-none absolute inset-0 flex items-center justify-end gap-1 opacity-0 transition-opacity group-hover:pointer-events-auto group-hover:opacity-100"
    >
      <Tooltip.Root>
        <Tooltip.Trigger>
          {#snippet child({ props })}
            <Button
              {...props}
              variant="secondary"
              size="icon"
              class="h-6 w-6 rounded-sm bg-secondary"
              aria-label="Resolve (e)"
              onclick={onResolve}
            >
              <Check class="h-3.5 w-3.5" />
            </Button>
          {/snippet}
        </Tooltip.Trigger>
        <Tooltip.Content>Resolve · e</Tooltip.Content>
      </Tooltip.Root>
      <Tooltip.Root>
        <Tooltip.Trigger>
          {#snippet child({ props })}
            <Button
              {...props}
              variant="secondary"
              size="icon"
              class="h-6 w-6 rounded-sm bg-secondary"
              aria-label="Ignore (i)"
              onclick={onIgnore}
            >
              <Ban class="h-3.5 w-3.5" />
            </Button>
          {/snippet}
        </Tooltip.Trigger>
        <Tooltip.Content>Ignore · i</Tooltip.Content>
      </Tooltip.Root>
      <Tooltip.Root>
        <Tooltip.Trigger>
          {#snippet child({ props })}
            <Button
              {...props}
              variant="secondary"
              size="icon"
              class="h-6 w-6 rounded-sm bg-secondary"
              aria-label="Assign to me (a)"
              onclick={onAssign}
            >
              <ArrowRight class="h-3.5 w-3.5" />
            </Button>
          {/snippet}
        </Tooltip.Trigger>
        <Tooltip.Content>Assign · a</Tooltip.Content>
      </Tooltip.Root>
    </div>
  </div>
</div>
