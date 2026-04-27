<script lang="ts">
  import { Wifi, WifiOff } from 'lucide-svelte';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { connection } from '$lib/stores.svelte';
  import { cn } from '$lib/utils';

  const isConnected = $derived(connection.status === 'connected');
  const isPending = $derived(
    connection.status === 'reconnecting' || connection.status === 'connecting'
  );

  const iconClass = $derived(
    cn(
      'h-[18px] w-[18px]',
      isConnected
        ? 'text-emerald-500'
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
    class="text-muted-foreground hover:text-foreground hover:bg-accent inline-flex h-10 w-10 items-center justify-center rounded-md transition-colors"
    aria-label={label}
  >
    {#if isConnected || isPending}
      <Wifi class={iconClass} />
    {:else}
      <WifiOff class={iconClass} />
    {/if}
  </Tooltip.Trigger>
  <Tooltip.Content side="right">
    {label}
  </Tooltip.Content>
</Tooltip.Root>
