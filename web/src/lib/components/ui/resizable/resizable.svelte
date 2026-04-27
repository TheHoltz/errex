<script lang="ts">
  import type { Snippet } from 'svelte';
  import { cn } from '$lib/utils';

  // Minimal two-pane resizable. Left pane width is bound externally so
  // routes can persist it for the session. Drag the divider to resize.

  type Props = {
    leftPercent?: number;
    minLeft?: number;
    maxLeft?: number;
    class?: string;
    left?: Snippet;
    right?: Snippet;
  };

  let {
    leftPercent = $bindable(40),
    minLeft = 20,
    maxLeft = 75,
    class: className,
    left,
    right
  }: Props = $props();

  let container = $state<HTMLDivElement | undefined>(undefined);
  let dragging = $state(false);

  function startDrag(e: PointerEvent) {
    if (!container) return;
    dragging = true;
    e.preventDefault();
    const target = e.currentTarget as HTMLElement;
    target.setPointerCapture(e.pointerId);
  }

  function onMove(e: PointerEvent) {
    if (!dragging || !container) return;
    const rect = container.getBoundingClientRect();
    const pct = ((e.clientX - rect.left) / rect.width) * 100;
    leftPercent = Math.max(minLeft, Math.min(maxLeft, pct));
  }

  function endDrag(e: PointerEvent) {
    if (!dragging) return;
    dragging = false;
    (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
  }
</script>

<div bind:this={container} class={cn('flex h-full w-full overflow-hidden', className)}>
  <div class="overflow-hidden" style="width: {leftPercent}%;">
    {@render left?.()}
  </div>
  <div
    role="slider"
    aria-orientation="vertical"
    aria-label="Resize panes"
    aria-valuenow={Math.round(leftPercent)}
    aria-valuemin={minLeft}
    aria-valuemax={maxLeft}
    tabindex="0"
    class={cn(
      'relative w-px shrink-0 cursor-col-resize bg-border transition-colors',
      'hover:bg-primary/40 focus-visible:bg-primary/60 focus-visible:outline-none',
      dragging && 'bg-primary/60'
    )}
    onpointerdown={startDrag}
    onpointermove={onMove}
    onpointerup={endDrag}
    onpointercancel={endDrag}
  >
    <span class="absolute inset-y-0 -left-1 -right-1"></span>
  </div>
  <div class="flex-1 overflow-hidden">
    {@render right?.()}
  </div>
</div>
