<script lang="ts">
  import { RefreshCw } from 'lucide-svelte';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { eventStream } from '$lib/eventStream.svelte';
  import { connection } from '$lib/stores.svelte';
  import { cn } from '$lib/utils';

  const label = $derived.by(() => {
    void eventStream.tick;
    if (eventStream.lastAt == null) return 'sem eventos ainda';
    const seconds = Math.floor((Date.now() - eventStream.lastAt) / 1000);
    if (seconds < 5) return 'agora mesmo';
    if (seconds < 60) return `há ${seconds}s`;
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `há ${minutes}min`;
    const hours = Math.floor(minutes / 60);
    return `há ${hours}h`;
  });

  const stale = $derived.by(() => {
    void eventStream.tick;
    if (connection.status !== 'connected') return false;
    if (eventStream.lastAt == null) return false;
    return Date.now() - eventStream.lastAt > 120_000;
  });

  const fresh = $derived.by(() => {
    void eventStream.tick;
    if (eventStream.lastAt == null) return false;
    return Date.now() - eventStream.lastAt < 5_000;
  });
</script>

<Tooltip.Root>
  <Tooltip.Trigger
    class="text-muted-foreground hover:text-foreground hover:bg-accent inline-flex h-10 w-10 items-center justify-center rounded-md transition-colors"
    aria-label={`Último evento ${label}`}
  >
    <RefreshCw
      class={cn(
        'h-[18px] w-[18px]',
        stale && 'opacity-50',
        fresh && 'animate-pulse text-foreground'
      )}
    />
  </Tooltip.Trigger>
  <Tooltip.Content side="right">
    Último evento {label}
  </Tooltip.Content>
</Tooltip.Root>
