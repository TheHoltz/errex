<script lang="ts">
  import {
    Ban,
    BellOff,
    Check,
    Circle,
    Search,
    ShieldCheck,
    SlidersHorizontal,
    X
  } from 'lucide-svelte';
  import { onMount } from 'svelte';
  import { browser } from '$app/environment';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import { Checkbox } from '$lib/components/ui/checkbox';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import * as Popover from '$lib/components/ui/popover';
  import { Separator } from '$lib/components/ui/separator';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { eventStream } from '$lib/eventStream.svelte';
  import { parseFilterParams, serializeFilterParams } from '$lib/filterUrl';
  import { filter, issues, load, projects, selection, visibleIssues } from '$lib/stores.svelte';
  import type { IssueLevel, IssueStatus } from '$lib/types';
  import { cn } from '$lib/utils';
  import IssueRow from './IssueRow.svelte';

  type Props = {
    onSelect?: (id: number) => void;
    filterRef?: { current: HTMLInputElement | null };
  };
  let { onSelect, filterRef }: Props = $props();

  let inputEl = $state<HTMLInputElement | null>(null);
  $effect(() => {
    if (filterRef && inputEl) filterRef.current = inputEl;
  });

  // Hydrate filter state from URL once on mount. Subsequent changes flow
  // the other direction via the effect below; the two never both write in
  // the same tick because hydration is synchronous before the first
  // effect run.
  onMount(() => {
    if (!browser) return;
    const next = parseFilterParams(new URLSearchParams(location.search));
    filter.query = next.query;
    filter.statuses = next.statuses;
    filter.levels = next.levels;
    filter.sinceMs = next.sinceMs;
    filter.spikingOnly = next.spikingOnly;
  });

  // Push filter state into the URL so the active filter is reload-safe and
  // shareable. We use replaceState (not goto) to avoid polluting browser
  // history on every keystroke and to skip SvelteKit's data revalidation.
  $effect(() => {
    if (!browser) return;
    const params = serializeFilterParams({
      query: filter.query,
      statuses: filter.statuses,
      levels: filter.levels,
      sinceMs: filter.sinceMs,
      spikingOnly: filter.spikingOnly
    });
    const search = params.toString();
    const target = location.pathname + (search ? `?${search}` : '') + location.hash;
    if (target !== location.pathname + location.search + location.hash) {
      history.replaceState(history.state, '', target);
    }
  });

  // Re-evaluate when the 5 s tick advances so the since/spiking filters
  // stay current without each consumer wiring its own setInterval.
  const visible = $derived.by(() => {
    void eventStream.tick;
    return visibleIssues({
      isSpiking: (id: number) => eventStream.isSpiking(id)
    });
  });

  type StatusChip = {
    key: IssueStatus;
    label: string;
    Icon: typeof Circle;
    activeClass: string;
  };

  const statusChips: StatusChip[] = [
    { key: 'unresolved', label: 'Unresolved', Icon: Circle, activeClass: 'bg-foreground/10 text-foreground ring-1 ring-foreground/30' },
    { key: 'resolved',   label: 'Resolved',   Icon: Check,  activeClass: 'bg-emerald-500/15 text-emerald-500 ring-1 ring-emerald-500/40' },
    { key: 'muted',      label: 'Muted',      Icon: BellOff, activeClass: 'bg-amber-500/15 text-amber-500 ring-1 ring-amber-500/40' },
    { key: 'ignored',    label: 'Ignored',    Icon: Ban,    activeClass: 'bg-muted text-muted-foreground ring-1 ring-border' }
  ];

  const allLevels: IssueLevel[] = ['fatal', 'error', 'warning', 'info', 'debug'];

  function statusCount(s: IssueStatus): number {
    return issues.list.filter((i) => i.project === projects.current && i.status === s).length;
  }

  function levelCount(l: IssueLevel): number {
    return issues.list.filter(
      (i) => i.project === projects.current && (i.level?.toLowerCase() ?? '') === l
    ).length;
  }

  const projectTotal = $derived(
    issues.list.filter((i) => i.project === projects.current).length
  );

  const allClearLabel = $derived.by(() => {
    void eventStream.tick;
    if (eventStream.lastAt == null) return 'Waiting for first event.';
    const minutes = Math.floor((Date.now() - eventStream.lastAt) / 60_000);
    if (minutes <= 0) return 'All quiet · last event just now.';
    if (minutes === 1) return 'All quiet · last event 1 min ago.';
    if (minutes < 60) return `All quiet · last event ${minutes} min ago.`;
    return `All quiet · last event ${Math.floor(minutes / 60)} h ago.`;
  });

  const hasActiveFilter = $derived(
    filter.query.trim().length > 0 ||
      filter.statuses.size !== 1 ||
      !filter.statuses.has('unresolved') ||
      filter.levels.size > 0 ||
      filter.sinceMs != null ||
      filter.spikingOnly
  );

  function sinceLabel(ms: number): string {
    if (ms === 60 * 60 * 1000) return '1h';
    if (ms === 24 * 60 * 60 * 1000) return '24h';
    if (ms === 7 * 24 * 60 * 60 * 1000) return '7d';
    return `${Math.round(ms / 60000)}m`;
  }

  // Stable, human-readable order for the readout: matches the chip row.
  const STATUS_ORDER: IssueStatus[] = ['unresolved', 'resolved', 'muted', 'ignored'];
  const LEVEL_ORDER: IssueLevel[] = ['fatal', 'error', 'warning', 'info', 'debug'];

  const statusReadout = $derived(
    STATUS_ORDER.filter((s) => filter.statuses.has(s)).join(' + ')
  );
  const levelReadout = $derived(
    LEVEL_ORDER.filter((l) => filter.levels.has(l)).join(', ')
  );

  function clearFilters() {
    filter.query = '';
    filter.statuses = new Set<IssueStatus>(['unresolved']);
    filter.levels = new Set<IssueLevel>();
    filter.sinceMs = null;
    filter.spikingOnly = false;
  }
</script>

<div class="flex h-full flex-col">
  <div class="flex items-center gap-2 border-b border-border px-3 py-2">
    <div class="relative flex-1">
      <Search class="text-muted-foreground absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2" />
      <Input
        bind:ref={inputEl}
        bind:value={filter.query}
        placeholder="filter"
        class="h-9 pl-8 pr-16 text-[13px]"
      />
      {#if filter.query.length > 0}
        <Button
          variant="ghost"
          size="icon"
          class="absolute right-8 top-1/2 h-6 w-6 -translate-y-1/2 text-muted-foreground"
          aria-label="Clear filter query"
          onclick={() => (filter.query = '')}
        >
          <X class="h-3.5 w-3.5" />
        </Button>
      {:else}
        <kbd
          class="border-border text-muted-foreground absolute right-2 top-1/2 -translate-y-1/2 rounded border px-1.5 py-0.5 font-mono text-[10px]"
          aria-hidden="true"
        >
          /
        </kbd>
      {/if}
    </div>

    <div class="flex items-center gap-1">
      {#each statusChips as chip (chip.key)}
        {@const on = filter.statuses.has(chip.key)}
        {@const count = statusCount(chip.key)}
        {@const empty = count === 0 && !on}
        <Tooltip.Root>
          <Tooltip.Trigger>
            {#snippet child({ props })}
              <Button
                {...props}
                variant="ghost"
                size="sm"
                onclick={() => filter.toggleStatus(chip.key)}
                aria-pressed={on}
                aria-label={chip.label}
                class={cn(
                  'h-9 gap-1.5 px-2 tabular-nums',
                  on
                    ? chip.activeClass
                    : 'text-muted-foreground hover:text-foreground hover:bg-accent/50',
                  empty && 'opacity-40'
                )}
              >
                <chip.Icon class="h-4 w-4" />
                <span class="text-[12px]">{count}</span>
              </Button>
            {/snippet}
          </Tooltip.Trigger>
          <Tooltip.Content>{chip.label}</Tooltip.Content>
        </Tooltip.Root>
      {/each}

      <Separator orientation="vertical" class="mx-1 h-5" />

      <Popover.Root>
        <Popover.Trigger>
          {#snippet child({ props })}
            <Button
              {...props}
              variant="ghost"
              size="sm"
              aria-label="Filter by level"
              class={cn(
                'h-9 gap-1.5 px-2',
                filter.levels.size > 0
                  ? 'text-foreground bg-accent ring-1 ring-border'
                  : 'text-muted-foreground hover:text-foreground hover:bg-accent/50'
              )}
            >
              <SlidersHorizontal class="h-4 w-4" />
              {#if filter.levels.size > 0}
                <Badge variant="outline" class="h-4 px-1 text-[10px] tabular-nums">
                  {filter.levels.size}
                </Badge>
              {/if}
            </Button>
          {/snippet}
        </Popover.Trigger>
        <Popover.Content class="w-56 p-2">
          <div class="text-muted-foreground px-2 py-1 text-[11px] font-semibold uppercase tracking-wider">
            Level
          </div>
          <ul class="flex flex-col">
            {#each allLevels as lvl (lvl)}
              {@const checked = filter.levels.has(lvl)}
              {@const count = levelCount(lvl)}
              <li>
                <Label
                  class={cn(
                    'flex cursor-pointer items-center gap-2 rounded px-2 py-1.5 text-[12px] hover:bg-accent',
                    count === 0 && !checked && 'opacity-50'
                  )}
                >
                  <Checkbox
                    {checked}
                    onCheckedChange={() => filter.toggleLevel(lvl)}
                  />
                  <span class="flex-1 capitalize">{lvl}</span>
                  <span class="text-muted-foreground tabular-nums text-[11px]">{count}</span>
                </Label>
              </li>
            {/each}
          </ul>
          {#if filter.levels.size > 0}
            <div class="mt-1 border-t border-border pt-1">
              <Button
                variant="ghost"
                size="sm"
                class="h-7 w-full justify-start text-[12px] text-muted-foreground"
                onclick={() => (filter.levels = new Set())}
              >
                Clear levels
              </Button>
            </div>
          {/if}
        </Popover.Content>
      </Popover.Root>
    </div>
  </div>

  {#if hasActiveFilter && !load.initialLoad}
    <div
      class="flex flex-wrap items-center gap-x-2 gap-y-1 border-b border-border bg-muted/30 px-3 py-1.5 text-[11px] text-muted-foreground"
    >
      <span class="text-foreground tabular-nums font-medium">
        {visible.length}
      </span>
      <span>of</span>
      <span class="tabular-nums">{projectTotal}</span>
      {#if filter.query.trim().length > 0}
        <span class="text-border">·</span>
        <span>query:</span>
        <span class="text-foreground font-mono">"{filter.query}"</span>
      {/if}
      {#if statusReadout && (filter.statuses.size !== 1 || !filter.statuses.has('unresolved'))}
        <span class="text-border">·</span>
        <span>status:</span>
        <span class="text-foreground">{statusReadout}</span>
      {/if}
      {#if levelReadout}
        <span class="text-border">·</span>
        <span>level:</span>
        <span class="text-foreground">{levelReadout}</span>
      {/if}
      {#if filter.sinceMs != null}
        <span class="text-border">·</span>
        <span>since:</span>
        <span class="text-foreground">{sinceLabel(filter.sinceMs)}</span>
      {/if}
      {#if filter.spikingOnly}
        <span class="text-border">·</span>
        <span class="text-amber-500">spiking only</span>
      {/if}
      <Button
        variant="link"
        size="sm"
        class="ml-auto h-auto p-0 text-[11px]"
        onclick={clearFilters}
      >
        Clear all
      </Button>
    </div>
  {/if}

  <div class="flex-1 overflow-y-auto">
    {#if load.initialLoad}
      <ul class="flex flex-col gap-0">
        {#each Array.from({ length: 6 }) as _, i (i)}
          <li class="border-b border-border/50 px-5 py-4">
            <div class="flex items-center gap-4">
              <Skeleton class="h-2.5 w-2.5 rounded-full" />
              <Skeleton class="h-5 w-11" />
              <div class="flex flex-1 flex-col gap-2">
                <Skeleton class="h-3.5 w-3/4" />
                <Skeleton class="h-3 w-1/2" />
              </div>
              <Skeleton class="h-4 w-12" />
            </div>
          </li>
        {/each}
      </ul>
    {:else if visible.length === 0 && hasActiveFilter}
      <div class="text-muted-foreground flex flex-col items-center gap-2 p-8 text-center text-[12px]">
        <p>No issues match this filter.</p>
        <Button
          variant="link"
          size="sm"
          onclick={clearFilters}
          class="h-auto p-0 text-[12px]"
        >
          Clear all filters
        </Button>
      </div>
    {:else if visible.length === 0}
      <div
        class={cn(
          'flex flex-col items-center justify-center gap-3 px-6 py-12 text-center',
          'text-muted-foreground'
        )}
      >
        <ShieldCheck class="text-emerald-500/80 h-8 w-8" />
        <p class="text-foreground text-[13px] font-medium">{allClearLabel}</p>
        <p class="text-[12px]">No open issues in this project.</p>
      </div>
    {:else}
      {#each visible as issue (issue.id)}
        <IssueRow {issue} selected={issue.id === selection.issueId} {onSelect} />
      {/each}
    {/if}
  </div>
</div>
