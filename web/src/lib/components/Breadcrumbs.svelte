<script lang="ts">
  import { Badge } from '$lib/components/ui/badge';
  import { Collapsible } from '$lib/components/ui/collapsible';
  import type { Breadcrumb } from '$lib/types';
  import { formatTimestamp } from '$lib/utils';

  type Props = { breadcrumbs: Breadcrumb[] };
  let { breadcrumbs }: Props = $props();

  let open = $state(true);

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
      <h2 class="text-[12px] font-semibold uppercase tracking-wider text-muted-foreground">
        Breadcrumbs
        <span class="ml-1 text-muted-foreground/70 normal-case">({breadcrumbs.length})</span>
      </h2>
    {/snippet}

    {#if breadcrumbs.length === 0}
      <p class="text-muted-foreground px-3 pb-3 text-[12px]">Sem breadcrumbs neste evento.</p>
    {:else}
      <ol class="flex flex-col">
        {#each breadcrumbs as bc, i (i)}
          <li
            class="flex items-start gap-2 border-t border-border/40 px-3 py-1.5 first:border-t-0"
          >
            <span class="text-muted-foreground shrink-0 font-mono text-[11px] tabular-nums">
              {bc.timestamp ? formatTimestamp(bc.timestamp) : '—'}
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
