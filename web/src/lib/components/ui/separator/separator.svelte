<script lang="ts">
  import { cn } from '$lib/utils';

  type Props = {
    orientation?: 'horizontal' | 'vertical';
    class?: string;
  };

  let { orientation = 'horizontal', class: className }: Props = $props();
</script>

<div
  role="separator"
  aria-orientation={orientation}
  class={cn(
    'shrink-0 bg-border',
    orientation === 'horizontal' ? 'h-px w-full' : 'h-full w-px',
    className
  )}
></div>
