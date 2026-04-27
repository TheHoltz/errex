<script lang="ts">
  import { Activity, AlertTriangle, Flame, PlugZap } from 'lucide-svelte';
  import { Badge } from '$lib/components/ui/badge';
  import { Separator } from '$lib/components/ui/separator';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { eventStream } from '$lib/eventStream.svelte';
  import { filter, issues, projects } from '$lib/stores.svelte';
  import { cn } from '$lib/utils';
  import Sparkline from './Sparkline.svelte';

  // The header reads `eventStream.tick` so the counters refresh on the same
  // 5 s heartbeat used by Freshness. A separate setInterval per counter
  // would also work but multiplies DOM updates needlessly.
  const ofProject = $derived(issues.list.filter((i) => i.project === projects.current));

  const SINCE_1H = 60 * 60 * 1000;
  // Anything older than this without a single event means the daemon is
  // probably not receiving anything — treat that case as "ingest stale" so
  // a `0 events/min` row does not look identical between "all good" and
  // "broken pipe".
  const INGEST_STALE_MS = 5 * 60 * 1000;

  const newLastHour = $derived.by(() => {
    void eventStream.tick;
    const cutoff = Date.now() - SINCE_1H;
    return ofProject.filter(
      (i) => +new Date(i.first_seen) >= cutoff && i.status === 'unresolved'
    ).length;
  });

  const spiking = $derived.by(() => {
    void eventStream.tick;
    return ofProject.filter((i) => eventStream.isSpiking(i.id)).length;
  });

  const ratePerMin = $derived.by(() => {
    void eventStream.tick;
    return Math.round(eventStream.ratePerMin());
  });

  // 60-min sparkline of the global event stream, bucketed in 60 slots.
  const buckets = $derived.by(() => {
    void eventStream.tick;
    const slots = 60;
    const window = SINCE_1H;
    const start = Date.now() - window;
    const bucketMs = window / slots;
    const out = new Array<number>(slots).fill(0);
    for (const t of eventStream.global) {
      if (t < start) continue;
      const idx = Math.min(slots - 1, Math.floor((t - start) / bucketMs));
      out[idx] = (out[idx] ?? 0) + 1;
    }
    return out;
  });

  // True when ingest has been silent for 5+ min OR has never spoken at
  // all. Distinct from "rate is 0 because the project is quiet" — that
  // case keeps the rate stat visible.
  const ingestStale = $derived.by(() => {
    void eventStream.tick;
    if (eventStream.lastAt == null) return true;
    return Date.now() - eventStream.lastAt > INGEST_STALE_MS;
  });

  function toggleSince1h() {
    filter.sinceMs = filter.sinceMs === SINCE_1H ? null : SINCE_1H;
  }

  function toggleSpiking() {
    filter.spikingOnly = !filter.spikingOnly;
  }

  const newActive = $derived(filter.sinceMs === SINCE_1H);
  const spikeActive = $derived(filter.spikingOnly);
</script>

<div class="flex items-center gap-3">
  <Tooltip.Root>
    <Tooltip.Trigger>
      {#snippet child({ props })}
        <button
          {...props}
          type="button"
          onclick={toggleSince1h}
          aria-pressed={newActive}
          aria-label="Filter to issues first seen in the last hour"
          class={cn(
            'flex items-baseline gap-1.5 rounded px-1.5 py-0.5 transition-colors',
            'hover:bg-accent/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
            newActive && 'bg-accent ring-1 ring-border'
          )}
        >
          <AlertTriangle
            class={cn(
              'h-3 w-3',
              newLastHour > 0 ? 'text-amber-400' : 'text-muted-foreground/60'
            )}
          />
          <span
            class={cn(
              'text-[13px] font-semibold tabular-nums',
              newLastHour > 0 ? 'text-foreground' : 'text-muted-foreground'
            )}
          >
            {newLastHour}
          </span>
          <span class="text-muted-foreground text-[10px] uppercase tracking-wider">new · 1h</span>
        </button>
      {/snippet}
    </Tooltip.Trigger>
    <Tooltip.Content>Issues first seen in the last hour. Click to filter.</Tooltip.Content>
  </Tooltip.Root>

  <Separator orientation="vertical" class="h-4" />

  <Tooltip.Root>
    <Tooltip.Trigger>
      {#snippet child({ props })}
        <button
          {...props}
          type="button"
          onclick={toggleSpiking}
          aria-pressed={spikeActive}
          aria-label="Filter to spiking issues"
          class={cn(
            'flex items-baseline gap-1.5 rounded px-1.5 py-0.5 transition-colors',
            'hover:bg-accent/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
            spikeActive && 'bg-accent ring-1 ring-border'
          )}
        >
          <Flame
            class={cn(
              'h-3 w-3',
              spiking > 0 ? 'text-amber-400' : 'text-muted-foreground/60'
            )}
          />
          <span
            class={cn(
              'text-[13px] font-semibold tabular-nums',
              spiking > 0 ? 'text-amber-400' : 'text-muted-foreground'
            )}
          >
            {spiking}
          </span>
          <span class="text-muted-foreground text-[10px] uppercase tracking-wider">spiking</span>
        </button>
      {/snippet}
    </Tooltip.Trigger>
    <Tooltip.Content>Issues whose 5-min rate is at least 3× the prior 5 min. Click to filter.</Tooltip.Content>
  </Tooltip.Root>

  <Separator orientation="vertical" class="h-4" />

  {#if ingestStale}
    <Tooltip.Root>
      <Tooltip.Trigger>
        {#snippet child({ props })}
          <Badge {...props} variant="outline" class="gap-1.5 border-amber-500/40 bg-amber-500/10 text-amber-500">
            <PlugZap class="h-3 w-3" />
            <span class="text-[10px] uppercase tracking-wider">no ingest</span>
          </Badge>
        {/snippet}
      </Tooltip.Trigger>
      <Tooltip.Content>
        {#if eventStream.lastAt == null}
          No events received yet. Daemon may be running but no SDK has connected.
        {:else}
          No events for {Math.round((Date.now() - eventStream.lastAt) / 60_000)} min — check the daemon and SDK config.
        {/if}
      </Tooltip.Content>
    </Tooltip.Root>
  {:else}
    <Tooltip.Root>
      <Tooltip.Trigger>
        {#snippet child({ props })}
          <div {...props} class="flex items-center gap-2">
            <div class="flex items-baseline gap-1.5">
              <Activity
                class={cn(
                  'h-3 w-3',
                  ratePerMin > 0 ? 'text-foreground' : 'text-muted-foreground/60'
                )}
              />
              <span
                class={cn(
                  'text-[13px] font-semibold tabular-nums',
                  ratePerMin > 0 ? 'text-foreground' : 'text-muted-foreground'
                )}
              >
                {ratePerMin}
              </span>
              <span class="text-muted-foreground text-[10px] uppercase tracking-wider">events/min</span>
            </div>
            <Sparkline values={buckets} width={96} height={14} accent={ratePerMin > 0} />
          </div>
        {/snippet}
      </Tooltip.Trigger>
      <Tooltip.Content>Events per minute over the last 5 min, project-wide. Sparkline covers 60 min.</Tooltip.Content>
    </Tooltip.Root>
  {/if}
</div>
