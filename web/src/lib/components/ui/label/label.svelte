<script lang="ts">
  import { cn } from '$lib/utils';
  import type { HTMLLabelAttributes } from 'svelte/elements';

  type Props = HTMLLabelAttributes & { class?: string };
  let { class: className, children, ...rest }: Props = $props();
</script>

<label
  class={cn(
    'text-[12px] font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70',
    className
  )}
  {...rest}
>
  {@render children?.()}
</label>
