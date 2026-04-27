<script lang="ts">
  import { untrack } from 'svelte';
  import { Collapsible } from '$lib/components/ui/collapsible';
  import type { Frame } from '$lib/types';
  import { cn } from '$lib/utils';

  type Props = { frame: Frame; anchor?: boolean };
  let { frame, anchor = false }: Props = $props();

  const inApp = $derived(frame.in_app === true);
  const hasVars = $derived(frame.vars && Object.keys(frame.vars).length > 0);
  // Throw-site anchor pre-expands so the eye lands on the row that
  // raised the exception without an extra click; mirrors Breadcrumbs.
  // Frames are keyed by index, so anchor is stable per mount — read it
  // once via untrack to seed `open` without subscribing.
  let open = $state(untrack(() => anchor));
</script>

<Collapsible
  bind:open
  showChevron={hasVars}
  triggerClass={cn(
    'px-2 py-1 font-mono text-[12px] gap-2',
    anchor && 'border-l-2 border-l-amber-500 bg-amber-500/5 pl-1.5'
  )}
  contentClass={cn(
    'bg-muted/30 px-3 py-2 font-mono text-[11px] border-b border-border/50',
    anchor && 'border-l-2 border-l-amber-500 bg-amber-500/5 pl-2.5'
  )}
>
  {#snippet header()}
    <span class={cn('flex min-w-0 flex-1 items-center gap-2', !inApp && !anchor && 'text-muted-foreground')}>
      <span class={cn('shrink-0 truncate', (inApp || anchor) && 'font-medium text-foreground')}>
        {frame.function ?? '<anonymous>'}
      </span>
      <span class="truncate text-muted-foreground">
        {frame.filename ?? frame.module ?? '?'}{frame.lineno != null ? `:${frame.lineno}` : ''}
      </span>
      {#if anchor}
        <span
          class="ml-auto shrink-0 rounded bg-amber-500/15 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide text-amber-500"
          aria-label="throw site"
        >
          throw
        </span>
      {/if}
    </span>
  {/snippet}

  {#if frame.vars}
    <table class="w-full border-collapse">
      <tbody>
        {#each Object.entries(frame.vars) as [k, v] (k)}
          <tr class="border-b border-border/30 last:border-0">
            <td class="text-muted-foreground py-0.5 pr-3 align-top whitespace-nowrap">{k}</td>
            <td class="py-0.5 align-top">{JSON.stringify(v)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</Collapsible>
