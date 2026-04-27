<script lang="ts">
  import { cn } from '$lib/utils';
  import type { HTMLInputAttributes } from 'svelte/elements';

  type Props = HTMLInputAttributes & {
    class?: string;
    value?: string;
    ref?: HTMLInputElement | null;
  };
  let {
    class: className,
    value = $bindable(''),
    ref = $bindable<HTMLInputElement | null>(null),
    ...rest
  }: Props = $props();
</script>

<input
  bind:this={ref}
  bind:value
  class={cn(
    'flex h-7 w-full rounded-md border border-input bg-background px-2 py-1 text-[12px]',
    'ring-offset-background file:border-0 file:bg-transparent file:text-sm file:font-medium',
    'placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2',
    'focus-visible:ring-ring focus-visible:ring-offset-1 disabled:cursor-not-allowed',
    'disabled:opacity-50',
    className
  )}
  {...rest}
/>
