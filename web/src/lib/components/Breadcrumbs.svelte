<script lang="ts">
  import { Footprints } from 'lucide-svelte';
  import { Badge } from '$lib/components/ui/badge';
  import { Collapsible } from '$lib/components/ui/collapsible';
  import { breadcrumbRelativeTime } from '$lib/eventDetail';
  import type { Breadcrumb } from '$lib/types';
  import { cn } from '$lib/utils';

  type Props = { breadcrumbs: Breadcrumb[]; crashTimestamp?: string | null };
  let { breadcrumbs, crashTimestamp = null }: Props = $props();

  let open = $state(true);

  // Anchor index: the breadcrumb closest to (and ≤) the crash timestamp.
  // That row is the "last thing the user did before it broke", which we
  // subtly highlight so the eye lands on it without hunting.
  const anchorIndex = $derived.by(() => {
    if (!crashTimestamp || breadcrumbs.length === 0) return -1;
    const crash = Date.parse(crashTimestamp);
    if (!Number.isFinite(crash)) return -1;
    let best = -1;
    let bestDelta = Number.POSITIVE_INFINITY;
    breadcrumbs.forEach((bc, i) => {
      if (!bc.timestamp) return;
      const t = Date.parse(bc.timestamp);
      if (!Number.isFinite(t) || t > crash) return;
      const d = crash - t;
      if (d < bestDelta) {
        bestDelta = d;
        best = i;
      }
    });
    return best;
  });

  function levelVariant(level: string | null): 'info' | 'warning' | 'destructive' | 'outline' {
    switch (level) {
      case 'error':
      case 'fatal':
        return 'destructive';
      case 'warning':
        return 'warning';
      case 'info':
        return 'info';
      default:
        return 'outline';
    }
  }
</script>

<section>
  <Collapsible bind:open triggerClass="px-3 py-2 sticky top-0 bg-background">
    {#snippet header()}
      <h2
        class="flex items-center gap-1.5 text-[12px] font-semibold uppercase tracking-wider text-muted-foreground"
      >
        <Footprints class="h-3.5 w-3.5" />
        Breadcrumbs
        <span class="ml-1 text-muted-foreground/70 normal-case">({breadcrumbs.length})</span>
      </h2>
    {/snippet}

    {#if breadcrumbs.length === 0}
      <p class="text-muted-foreground px-3 pb-3 text-[12px]">No breadcrumbs for this event.</p>
    {:else}
      <ol class="flex flex-col">
        {#each breadcrumbs as bc, i (i)}
          {@const isAnchor = i === anchorIndex}
          <li
            class={cn(
              'flex items-start gap-2 border-t border-border/40 px-3 py-1.5 first:border-t-0',
              isAnchor && 'border-l-2 border-l-amber-500 bg-amber-500/5 pl-2.5'
            )}
          >
            <span
              class={cn(
                'shrink-0 font-mono text-[11px] tabular-nums',
                isAnchor ? 'text-amber-500 font-semibold' : 'text-muted-foreground'
              )}
              aria-label={isAnchor ? 'last breadcrumb before the crash' : undefined}
            >
              {crashTimestamp ? breadcrumbRelativeTime(crashTimestamp, bc.timestamp) : (bc.timestamp ?? '—')}
            </span>
            <Badge variant={levelVariant(bc.level ?? null)} class="shrink-0">
              {bc.category ?? 'event'}
            </Badge>
            <span class="min-w-0 flex-1 break-words text-[12px]">{bc.message ?? ''}</span>
          </li>
        {/each}
      </ol>
    {/if}
  </Collapsible>
</section>
