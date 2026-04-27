<script lang="ts">
  import type { Snippet } from 'svelte';
  import { ChevronRight } from 'lucide-svelte';
  import { cn } from '$lib/utils';

  type Props = {
    open?: boolean;
    class?: string;
    triggerClass?: string;
    contentClass?: string;
    showChevron?: boolean;
    header?: Snippet;
    children?: Snippet;
  };

  let {
    open = $bindable(false),
    class: className,
    triggerClass,
    contentClass,
    showChevron = true,
    header,
    children
  }: Props = $props();

  function toggle() {
    open = !open;
  }
</script>

<div data-state={open ? 'open' : 'closed'} class={cn('flex flex-col', className)}>
  <button
    type="button"
    onclick={toggle}
    aria-expanded={open}
    class={cn(
      'flex w-full items-center gap-1 text-left',
      'hover:bg-accent/50 focus-visible:outline-none focus-visible:ring-1',
      'focus-visible:ring-ring rounded-sm',
      triggerClass
    )}
  >
    {#if showChevron}
      <ChevronRight
        class={cn('h-3 w-3 shrink-0 transition-transform', open && 'rotate-90')}
        strokeWidth={2.25}
      />
    {/if}
    {@render header?.()}
  </button>

  {#if open}
    <div class={cn(contentClass)}>
      {@render children?.()}
    </div>
  {/if}
</div>
