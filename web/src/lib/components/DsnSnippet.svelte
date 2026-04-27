<script lang="ts">
  import { Check, Copy } from 'lucide-svelte';
  import { Button } from '$lib/components/ui/button';
  import { toast } from '$lib/toast.svelte';
  import { cn } from '$lib/utils';

  type Props = { dsn: string; label?: string; class?: string };
  let { dsn, label = 'DSN', class: className }: Props = $props();

  let copied = $state(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(dsn);
      copied = true;
      toast.success('Copied to clipboard');
      setTimeout(() => (copied = false), 1500);
    } catch (err) {
      toast.error('Could not copy', { description: String(err) });
    }
  }
</script>

<div class={cn('flex items-center gap-2', className)}>
  <code
    class="border-border bg-muted/40 text-muted-foreground flex-1 truncate rounded-md border px-3 py-2 font-mono text-[12px]"
    title={dsn}
    aria-label={label}>{dsn}</code
  >
  <Button variant="outline" size="sm" onclick={copy} aria-label={`Copy ${label}`}>
    {#if copied}
      <Check class="h-3.5 w-3.5" />
    {:else}
      <Copy class="h-3.5 w-3.5" />
    {/if}
  </Button>
</div>
