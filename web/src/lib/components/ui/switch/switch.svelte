<script lang="ts">
  import { cn } from '$lib/utils';

  type Props = {
    checked?: boolean;
    onCheckedChange?: (checked: boolean) => void;
    id?: string;
    class?: string;
    disabled?: boolean;
    'aria-labelledby'?: string;
  };

  let {
    checked = $bindable(false),
    onCheckedChange,
    id,
    class: className,
    disabled,
    'aria-labelledby': ariaLabelledby
  }: Props = $props();

  function toggle() {
    if (disabled) return;
    checked = !checked;
    onCheckedChange?.(checked);
  }
</script>

<button
  type="button"
  role="switch"
  aria-checked={checked}
  aria-labelledby={ariaLabelledby}
  {id}
  {disabled}
  onclick={toggle}
  data-state={checked ? 'checked' : 'unchecked'}
  class={cn(
    'peer relative inline-flex h-3.5 w-6.5 shrink-0 cursor-pointer items-center rounded-full',
    'border border-transparent transition-colors',
    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 ring-offset-background',
    'disabled:cursor-not-allowed disabled:opacity-50',
    'data-[state=checked]:bg-primary data-[state=unchecked]:bg-muted',
    className
  )}
>
  <span
    aria-hidden="true"
    class={cn(
      'pointer-events-none block h-2.5 w-2.5 rounded-full bg-background shadow ring-0 transition-transform',
      checked ? 'translate-x-3.5' : 'translate-x-0.5'
    )}
  ></span>
</button>
