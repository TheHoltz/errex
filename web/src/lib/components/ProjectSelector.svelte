<script lang="ts">
  import { ChevronsUpDown, FolderKanban } from 'lucide-svelte';
  import * as Popover from '$lib/components/ui/popover';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { cn } from '$lib/utils';
  import { projects } from '$lib/stores.svelte';
  import { connect } from '$lib/ws';

  type Props = { variant?: 'rail' | 'inline' };
  let { variant = 'rail' }: Props = $props();

  let open = $state(false);

  const items = $derived(
    projects.available.length > 0
      ? projects.available.map((p) => ({ value: p.project, label: p.project, count: p.issue_count }))
      : [{ value: projects.current, label: projects.current, count: null as number | null }]
  );

  function pick(next: string) {
    open = false;
    if (next === projects.current) return;
    connect(next);
  }
</script>

<Popover.Root bind:open>
  <Popover.Trigger>
    {#snippet child({ props })}
      {#if variant === 'rail'}
        <Tooltip.Root>
          <Tooltip.Trigger>
            {#snippet child({ props: tooltipProps })}
              <button
                {...props}
                {...tooltipProps}
                type="button"
                class="text-muted-foreground hover:text-foreground hover:bg-accent inline-flex h-10 w-10 items-center justify-center rounded-md transition-colors data-[state=open]:bg-accent data-[state=open]:text-foreground"
                aria-label="Switch project"
              >
                <FolderKanban class="h-[18px] w-[18px]" />
              </button>
            {/snippet}
          </Tooltip.Trigger>
          <Tooltip.Content side="right">Project: {projects.current}</Tooltip.Content>
        </Tooltip.Root>
      {:else}
        <button
          {...props}
          type="button"
          class="text-muted-foreground hover:text-foreground inline-flex items-center gap-1.5 text-[12px] tracking-tight transition-colors"
        >
          {projects.current}
          <ChevronsUpDown class="h-3.5 w-3.5" />
        </button>
      {/if}
    {/snippet}
  </Popover.Trigger>
  <Popover.Content side={variant === 'rail' ? 'right' : 'bottom'} align="start" class="w-56 p-1">
    <ul class="flex flex-col">
      {#each items as item (item.value)}
        <li>
          <button
            type="button"
            onclick={() => pick(item.value)}
            class={cn(
              'hover:bg-accent flex w-full items-center justify-between rounded-sm px-2 py-1.5 text-[12px]',
              item.value === projects.current && 'bg-accent/60 font-medium'
            )}
          >
            <span class="truncate">{item.label}</span>
            {#if item.count != null}
              <span class="text-muted-foreground tabular-nums text-[10px]">{item.count}</span>
            {/if}
          </button>
        </li>
      {/each}
    </ul>
  </Popover.Content>
</Popover.Root>
