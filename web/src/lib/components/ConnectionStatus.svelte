<script lang="ts">
  import { Wifi, WifiOff } from 'lucide-svelte';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { connection } from '$lib/stores.svelte';
  import { cn } from '$lib/utils';

  const isConnected = $derived(connection.status === 'connected');
  const isPending = $derived(
    connection.status === 'reconnecting' || connection.status === 'connecting'
  );

  // Silence is good news: connected = muted (same weight as Freshness),
  // pending = amber pulse, disconnected = destructive. No saturated green
  // baseline — it competes with the active-nav rail and trains the eye to
  // ignore real status changes.
  const iconClass = $derived(
    cn(
      'h-4 w-4',
      isConnected
        ? 'text-muted-foreground/70'
        : isPending
          ? 'text-amber-500 animate-pulse'
          : 'text-destructive'
    )
  );

  const label = $derived(
    isConnected
      ? `connected${connection.serverVersion ? ` · v${connection.serverVersion}` : ''}`
      : connection.status
  );
</script>

<Tooltip.Root>
  <Tooltip.Trigger
    class="text-muted-foreground/70 inline-flex h-8 w-8 items-center justify-center rounded-md"
    aria-label={label}
  >
    {#if isConnected || isPending}
      <Wifi class={iconClass} strokeWidth={1.75} />
    {:else}
      <WifiOff class={iconClass} strokeWidth={1.75} />
    {/if}
  </Tooltip.Trigger>
  <Tooltip.Content side="right">
    {label}
  </Tooltip.Content>
</Tooltip.Root>
