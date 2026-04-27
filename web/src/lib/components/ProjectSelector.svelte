<script lang="ts">
  import { ChevronsUpDown, FolderKanban } from 'lucide-svelte';
  import { Button } from '$lib/components/ui/button';
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
              <Button
                {...props}
                {...tooltipProps}
                variant="ghost"
                size="icon"
                class="text-muted-foreground hover:text-foreground h-10 w-10 data-[state=open]:bg-accent data-[state=open]:text-foreground"
                aria-label="Switch project"
              >
                <FolderKanban class="h-[18px] w-[18px]" />
              </Button>
            {/snippet}
          </Tooltip.Trigger>
          <Tooltip.Content side="right">Project: {projects.current}</Tooltip.Content>
        </Tooltip.Root>
      {:else}
        <Button
          {...props}
          variant="ghost"
          size="sm"
          class="text-muted-foreground hover:text-foreground hover:bg-transparent h-auto gap-1.5 p-0 text-[12px] font-normal tracking-tight"
        >
          {projects.current}
          <ChevronsUpDown class="h-3.5 w-3.5" />
        </Button>
      {/if}
    {/snippet}
  </Popover.Trigger>
  <Popover.Content side={variant === 'rail' ? 'right' : 'bottom'} align="start" class="w-56 p-1">
    <ul class="flex flex-col">
      {#each items as item (item.value)}
        <li>
          <Button
            variant="ghost"
            size="sm"
            onclick={() => pick(item.value)}
            class={cn(
              'h-auto w-full justify-between rounded-sm px-2 py-1.5 text-[12px] font-normal',
              item.value === projects.current && 'bg-accent/60 font-medium'
            )}
          >
            <span class="truncate">{item.label}</span>
            {#if item.count != null}
              <span class="text-muted-foreground tabular-nums text-[10px]">{item.count}</span>
            {/if}
          </Button>
        </li>
      {/each}
    </ul>
  </Popover.Content>
</Popover.Root>
