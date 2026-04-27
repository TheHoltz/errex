<script lang="ts">
  import { Activity } from 'lucide-svelte';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { eventStream } from '$lib/eventStream.svelte';
  import { connection } from '$lib/stores.svelte';
  import { cn } from '$lib/utils';

  const label = $derived.by(() => {
    void eventStream.tick;
    if (eventStream.lastAt == null) return 'no events yet';
    const seconds = Math.floor((Date.now() - eventStream.lastAt) / 1000);
    if (seconds < 5) return 'just now';
    if (seconds < 60) return `${seconds}s ago`;
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes}min ago`;
    const hours = Math.floor(minutes / 60);
    return `${hours}h ago`;
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

<!-- Status indicator, not a nav button. Smaller, no hover background — read,
     not click. The Activity icon was picked over RefreshCw because RefreshCw
     reads as "manual refresh action" and we already push events over WS. -->
<Tooltip.Root>
  <Tooltip.Trigger
    class="text-muted-foreground/70 inline-flex h-8 w-8 items-center justify-center rounded-md"
    aria-label={`Last event ${label}`}
  >
    <Activity
      class={cn(
        'h-4 w-4',
        stale && 'opacity-50',
        fresh && 'text-foreground animate-pulse'
      )}
      strokeWidth={1.75}
    />
  </Tooltip.Trigger>
  <Tooltip.Content side="right">
    Last event {label}
  </Tooltip.Content>
</Tooltip.Root>
