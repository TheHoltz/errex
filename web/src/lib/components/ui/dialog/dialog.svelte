<script lang="ts">
  import type { Snippet } from 'svelte';
  import { onMount } from 'svelte';
  import { cn } from '$lib/utils';

  type Props = {
    open: boolean;
    onClose?: () => void;
    class?: string;
    children?: Snippet;
  };

  let { open, onClose, class: className, children }: Props = $props();

  let surface = $state<HTMLDivElement | undefined>(undefined);

  function onKey(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose?.();
    }
  }

  $effect(() => {
    // Move focus into the dialog when it opens so keyboard users land in the
    // search input automatically. The first focusable child is good enough
    // for our single-purpose palette.
    if (open && surface) {
      const focusable = surface.querySelector<HTMLElement>(
        'input, button, [tabindex]:not([tabindex="-1"])'
      );
      focusable?.focus();
    }
  });

  onMount(() => {
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });
</script>

{#if open}
  <div
    class="fixed inset-0 z-40 bg-background/70 backdrop-blur-sm"
    role="presentation"
    onclick={() => onClose?.()}
    onkeydown={() => {}}
  ></div>
  <div
    bind:this={surface}
    role="dialog"
    aria-modal="true"
    class={cn(
      'fixed left-1/2 top-[20%] z-50 w-[min(640px,calc(100vw-2rem))] -translate-x-1/2',
      'rounded-lg border border-border bg-popover text-popover-foreground shadow-2xl',
      className
    )}
  >
    {@render children?.()}
  </div>
{/if}
