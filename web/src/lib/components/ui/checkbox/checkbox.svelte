<script lang="ts">
  import { Check } from 'lucide-svelte';
  import { cn } from '$lib/utils';

  type Props = {
    checked?: boolean;
    onCheckedChange?: (checked: boolean) => void;
    id?: string;
    class?: string;
    disabled?: boolean;
  };

  let {
    checked = $bindable(false),
    onCheckedChange,
    id,
    class: className,
    disabled
  }: Props = $props();

  function toggle() {
    if (disabled) return;
    checked = !checked;
    onCheckedChange?.(checked);
  }
</script>

<button
  type="button"
  role="checkbox"
  aria-checked={checked}
  {id}
  {disabled}
  onclick={toggle}
  class={cn(
    'peer h-4 w-4 shrink-0 rounded-sm border border-primary',
    'ring-offset-background focus-visible:outline-none focus-visible:ring-2',
    'focus-visible:ring-ring focus-visible:ring-offset-1',
    'disabled:cursor-not-allowed disabled:opacity-50',
    'data-[state=checked]:bg-primary data-[state=checked]:text-primary-foreground',
    'flex items-center justify-center',
    className
  )}
  data-state={checked ? 'checked' : 'unchecked'}
>
  {#if checked}
    <Check class="h-3 w-3" strokeWidth={3} />
  {/if}
</button>
