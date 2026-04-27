<script lang="ts">
  import { Layers } from 'lucide-svelte';
  import StackFrame from './StackFrame.svelte';
  import type { Stack } from '$lib/types';

  type Props = { exception: Stack | null; loading?: boolean };
  let { exception, loading = false }: Props = $props();
</script>

<section class="flex flex-col">
  <header class="border-b border-border px-3 py-2">
    <h2
      class="flex items-center gap-1.5 text-[12px] font-semibold uppercase tracking-wider text-muted-foreground"
    >
      <Layers class="h-3.5 w-3.5" />
      Stack trace
    </h2>
    {#if exception?.type || exception?.value}
      <p class="mt-1 font-mono text-[12px]">
        <span class="font-medium">{exception.type ?? ''}</span>
        {#if exception.value}<span class="text-muted-foreground">: {exception.value}</span>{/if}
      </p>
    {/if}
  </header>

  {#if loading}
    <p class="text-muted-foreground px-3 py-4 text-[12px]">Loading stack…</p>
  {:else if !exception || exception.frames.length === 0}
    <p class="text-muted-foreground px-3 py-4 text-[12px]">
      No stack trace for the latest event.
    </p>
  {:else}
    <ol class="flex flex-col">
      {#each exception.frames as frame, i (i)}
        <li>
          <StackFrame {frame} />
        </li>
      {/each}
    </ol>
  {/if}
</section>
