<script lang="ts">
  import { Collapsible } from '$lib/components/ui/collapsible';
  import type { Frame } from '$lib/types';
  import { cn } from '$lib/utils';

  type Props = { frame: Frame };
  let { frame }: Props = $props();

  const inApp = $derived(frame.in_app === true);
  const hasVars = $derived(frame.vars && Object.keys(frame.vars).length > 0);
  let open = $state(false);
</script>

<Collapsible
  bind:open
  showChevron={hasVars}
  triggerClass="px-2 py-1 font-mono text-[12px] gap-2"
  contentClass="bg-muted/30 px-3 py-2 font-mono text-[11px] border-b border-border/50"
>
  {#snippet header()}
    <span class={cn('flex min-w-0 flex-1 items-center gap-2', !inApp && 'text-muted-foreground')}>
      <span class={cn('shrink-0 truncate', inApp && 'font-medium text-foreground')}>
        {frame.function ?? '<anonymous>'}
      </span>
      <span class="truncate text-muted-foreground">
        {frame.filename ?? frame.module ?? '?'}{frame.lineno != null ? `:${frame.lineno}` : ''}
      </span>
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
