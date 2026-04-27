<script lang="ts" generics="T extends string">
  import { ChevronDown } from 'lucide-svelte';
  import { cn } from '$lib/utils';

  // Native <select> wrapped to match shadcn density. We intentionally do not
  // pull in bits-ui's listbox here — the project picker is the only consumer
  // and the native control is fine for the dev-tool aesthetic.

  type Option = { value: T; label: string };

  type Props = {
    value?: T;
    options: Option[];
    onChange?: (value: T) => void;
    class?: string;
    disabled?: boolean;
    placeholder?: string;
  };

  let {
    value = $bindable<T | undefined>(undefined),
    options,
    onChange,
    class: className,
    disabled,
    placeholder
  }: Props = $props();

  function handle(e: Event) {
    const next = (e.currentTarget as HTMLSelectElement).value as T;
    value = next;
    onChange?.(next);
  }
</script>

<div class={cn('relative inline-flex items-center', className)}>
  <select
    {disabled}
    onchange={handle}
    class={cn(
      'h-7 appearance-none rounded-md border border-input bg-background pl-2 pr-7 text-[12px]',
      'ring-offset-background focus-visible:outline-none focus-visible:ring-2',
      'focus-visible:ring-ring focus-visible:ring-offset-1 disabled:opacity-50'
    )}
  >
    {#if placeholder && !value}
      <option value="" disabled selected>{placeholder}</option>
    {/if}
    {#each options as opt (opt.value)}
      <option value={opt.value} selected={opt.value === value}>{opt.label}</option>
    {/each}
  </select>
  <ChevronDown
    class="text-muted-foreground pointer-events-none absolute right-2 h-3 w-3"
    strokeWidth={2}
  />
</div>
