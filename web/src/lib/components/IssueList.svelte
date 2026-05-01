<script lang="ts">
  import { ShieldCheck } from 'lucide-svelte';
  import { onMount, untrack } from 'svelte';
  import { browser } from '$app/environment';
  import { Button } from '$lib/components/ui/button';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import { eventStream } from '$lib/eventStream.svelte';
  import { parseFilterParams, serializeFilterParams } from '$lib/filterUrl';
  import { applyInputToFilter, filterToQueryString } from '$lib/queryBridge';
  import { filter, issues, load, projects, selection, visibleIssues } from '$lib/stores.svelte';
  import type { IssueLevel, IssueStatus } from '$lib/types';
  import IssueRow from './IssueRow.svelte';
  import UnifiedInput from './UnifiedInput.svelte';

  type Props = {
    onSelect?: (id: number) => void;
    filterRef?: { current: HTMLInputElement | null };
  };
  let { onSelect, filterRef }: Props = $props();

  // ─── URL hydration & sync ────────────────────────────────────────────
  // Hydrate filter state from URL once on mount. Subsequent changes flow
  // the other direction via the effect below; the two never both write
  // in the same tick because hydration is synchronous before the first
  // effect run.
  onMount(() => {
    if (!browser) return;
    const next = parseFilterParams(new URLSearchParams(location.search));
    filter.query = next.query;
    filter.statuses = next.statuses;
    filter.levels = next.levels;
    filter.sinceMs = next.sinceMs;
    filter.spikingOnly = next.spikingOnly;
    filter.sort = next.sort;
    filter.limit = next.limit ?? null;
  });

  // Push filter state into the URL so the active filter is reload-safe
  // and shareable. We use replaceState (not goto) to avoid polluting
  // browser history on every keystroke and to skip SvelteKit's data
  // revalidation.
  $effect(() => {
    if (!browser) return;
    const params = serializeFilterParams({
      query: filter.query,
      statuses: filter.statuses,
      levels: filter.levels,
      sinceMs: filter.sinceMs,
      spikingOnly: filter.spikingOnly,
      sort: filter.sort,
      limit: filter.limit
    });
    const search = params.toString();
    const target = location.pathname + (search ? `?${search}` : '') + location.hash;
    if (target !== location.pathname + location.search + location.hash) {
      history.replaceState(history.state, '', target);
    }
  });

  // ─── Unified-input ↔ filter store bridge ─────────────────────────────
  //
  // The input owns its own text state; on every change it parses + writes
  // to the discrete filter fields. A reverse effect regenerates the
  // input's text whenever an external mutator changes the filter
  // (e.g. HeaderStats's 1H chip), so the input never lies about state.
  // Loops are avoided by short-circuiting when the canonical form
  // already matches the current input value.
  let inputValue = $state(filterToQueryString(filter));

  $effect(() => {
    // Apply parser → filter store on user input.
    const v = inputValue;
    untrack(() => applyInputToFilter(v, filter));
  });

  $effect(() => {
    // Regenerate input when filter changes externally. Reading every
    // field tracks all dependencies; we then compare against the
    // current inputValue (untracked) to decide whether to update.
    void filter.statuses;
    void filter.levels;
    void filter.sinceMs;
    void filter.spikingOnly;
    void filter.sort;
    void filter.newOnly;
    void filter.staleOnly;
    void filter.limit;
    void filter.query;
    const fromFilter = filterToQueryString(filter);
    untrack(() => {
      if (fromFilter !== inputValue) inputValue = fromFilter;
    });
  });

  // expose the input element for keyboard shortcuts (`/` to focus).
  let unifiedRef: { focus: () => void } | undefined = $state();
  let inputEl: HTMLInputElement | null = $state(null);
  $effect(() => {
    if (filterRef && inputEl) filterRef.current = inputEl;
  });

  // ─── Visible list + readout ──────────────────────────────────────────
  const visible = $derived.by(() => {
    void eventStream.tick;
    return visibleIssues({
      isSpiking: (id: number) => eventStream.isSpiking(id)
    });
  });

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

  const hasActiveFilter = $derived(inputValue.trim().length > 0);

  // Sparkline of matching issues over the last 60 minutes — passed to
  // the input so the suggestion panel's match-preview can render the
  // density of results without computing it twice.
  const sparkline = $derived.by(() => {
    void eventStream.tick;
    const SLOTS = 60;
    const now = Date.now();
    const window = 60 * 60 * 1000;
    const start = now - window;
    const bucketMs = window / SLOTS;
    const out = new Array<number>(SLOTS).fill(0);
    for (const i of visible) {
      const t = Date.parse(i.first_seen);
      if (!Number.isFinite(t) || t < start) continue;
      const idx = Math.min(SLOTS - 1, Math.floor((t - start) / bucketMs));
      out[idx] = (out[idx] ?? 0) + 1;
    }
    return out;
  });

  function clearFilters() {
    inputValue = '';
    filter.query = '';
    filter.statuses = new Set<IssueStatus>(['unresolved']);
    filter.levels = new Set<IssueLevel>();
    filter.sinceMs = null;
    filter.spikingOnly = false;
    filter.sort = 'recent';
    filter.newOnly = false;
    filter.staleOnly = false;
    filter.limit = null;
  }
</script>

<div class="flex h-full flex-col">
  <div class="border-b border-border px-3 py-2">
    <UnifiedInput
      bind:value={inputValue}
      bind:this={unifiedRef}
      matchCount={visible.length}
      {sparkline}
      placeholder="filter — try `crashes overnight` or `top 10 errors today`"
    />
  </div>

  {#if hasActiveFilter && !load.initialLoad}
    <div
      class="flex flex-wrap items-center gap-x-2 gap-y-1 border-b border-border bg-muted/30 px-3 py-1.5 text-[11px] text-muted-foreground"
    >
      <span class="text-foreground tabular-nums font-medium">{visible.length}</span>
      <span>of</span>
      <span class="tabular-nums">{projectTotal}</span>
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
      <div
        class="text-muted-foreground flex flex-col items-center gap-2 p-8 text-center text-[12px]"
      >
        <p>No issues match this filter.</p>
        <Button
          variant="link"
          size="sm"
          onclick={clearFilters}
          class="text-muted-foreground h-auto p-0 text-[12px]"
        >
          Clear all filters
        </Button>
      </div>
    {:else if visible.length === 0}
      <div
        class="text-muted-foreground flex flex-col items-center justify-center gap-3 px-6 py-12 text-center"
      >
        <ShieldCheck class="h-8 w-8 text-emerald-500/80" />
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
