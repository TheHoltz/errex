<script lang="ts">
  import { Check } from 'lucide-svelte';

  type Props = {
    current: 1 | 2;
    labels: [string, string];
    doneLabel?: string;
  };

  let { current, labels, doneLabel = 'verified' }: Props = $props();

  // Per-pill state: 'done' (current > n), 'active' (current === n), 'inactive' (current < n).
  function pillState(n: 1 | 2): 'done' | 'active' | 'inactive' {
    if (current > n) return 'done';
    if (current === n) return 'active';
    return 'inactive';
  }

  const pill1 = $derived(pillState(1));
  const pill2 = $derived(pillState(2));
</script>

<div class="flex items-center gap-2 text-[10px]">
  <div
    data-stepper-pill={pill1}
    class="flex h-[22px] w-[22px] items-center justify-center rounded-md border text-[11px] font-semibold transition-colors duration-150"
    class:bg-primary={pill1 === 'active'}
    class:text-primary-foreground={pill1 === 'active'}
    class:border-transparent={pill1 === 'active'}
    style:box-shadow={pill1 === 'active'
      ? '0 6px 18px hsl(var(--primary) / 0.32)'
      : 'none'}
  >
    {#if pill1 === 'done'}
      <Check class="h-3 w-3" style="color: hsl(var(--primary));" />
    {:else}
      1
    {/if}
  </div>

  {#if pill1 === 'active'}
    <span class="text-foreground">{labels[0]}</span>
  {:else if pill1 === 'done'}
    <span class="text-muted-foreground">{doneLabel}</span>
  {/if}

  <div class="bg-border h-px flex-1"></div>

  <div
    data-stepper-pill={pill2}
    class="flex h-[22px] w-[22px] items-center justify-center rounded-md border text-[11px] font-semibold transition-colors duration-150"
    class:bg-primary={pill2 === 'active'}
    class:text-primary-foreground={pill2 === 'active'}
    class:border-transparent={pill2 === 'active'}
    style:box-shadow={pill2 === 'active'
      ? '0 6px 18px hsl(var(--primary) / 0.32)'
      : 'none'}
  >
    {#if pill2 === 'done'}
      <Check class="h-3 w-3" style="color: hsl(var(--primary));" />
    {:else}
      2
    {/if}
  </div>

  {#if pill2 === 'active'}
    <span class="text-foreground">{labels[1]}</span>
  {/if}
</div>

<style>
  /* Done-pill subtle tint — lighter than the active pill's box-shadow but
     visually distinguishes "completed" from "not yet started". */
  div[data-stepper-pill='done'] {
    background-color: hsl(var(--primary) / 0.10);
    border-color: hsl(var(--primary) / 0.45);
  }
  div[data-stepper-pill='inactive'] {
    background-color: hsla(0, 0%, 5%, 0.6);
    border-color: hsla(0, 0%, 100%, 0.10);
    color: hsl(0 0% 63%);
  }
</style>
