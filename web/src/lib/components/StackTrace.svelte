<script lang="ts">
  import { Layers } from 'lucide-svelte';
  import { partitionFrames, throwSiteIndex } from '$lib/eventDetail';
  import StackFrame from './StackFrame.svelte';
  import type { Stack } from '$lib/types';

  type Props = { exception: Stack | null; loading?: boolean };
  let { exception, loading = false }: Props = $props();

  const frames = $derived(exception?.frames ?? []);
  const partition = $derived(partitionFrames(frames));
  const throwSite = $derived(throwSiteIndex(frames));
</script>

<section class="flex flex-col">
  <header class="border-b border-border px-3 py-2">
    <h2
      class="flex items-center gap-1.5 text-[12px] font-semibold uppercase tracking-wider text-muted-foreground"
    >
      <Layers class="h-3.5 w-3.5" />
      Stack trace
      {#if partition.inApp + partition.lib > 0}
        <span class="ml-1 text-muted-foreground/70 normal-case tracking-normal">
          {partition.inApp} in your code{partition.lib > 0 ? ` · ${partition.lib} lib` : ''}
        </span>
      {/if}
    </h2>
  </header>

  {#if loading}
    <p class="text-muted-foreground px-3 py-4 text-[12px]">Loading stack…</p>
  {:else if !exception || frames.length === 0}
    <p class="text-muted-foreground px-3 py-4 text-[12px]">
      No stack trace for the latest event.
    </p>
  {:else}
    <ol class="flex flex-col">
      {#each frames as frame, i (i)}
        <li>
          <StackFrame {frame} anchor={i === throwSite} />
        </li>
      {/each}
    </ol>
  {/if}
</section>
