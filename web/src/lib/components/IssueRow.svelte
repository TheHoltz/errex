<script lang="ts">
  import { Ban, BellOff, Check, Flame } from 'lucide-svelte';
  import { actions } from '$lib/actions.svelte';
  import { Badge } from '$lib/components/ui/badge';
  import * as Avatar from '$lib/components/ui/avatar';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { eventStream } from '$lib/eventStream.svelte';
  import type { Issue } from '$lib/types';
  import { cn, relativeTime } from '$lib/utils';
  import Sparkline from './Sparkline.svelte';

  type Props = {
    issue: Issue;
    selected?: boolean;
    onSelect?: (id: number) => void;
  };

  let { issue, selected = false, onSelect }: Props = $props();

  function levelDot(level: string | null | undefined): string {
    switch (level) {
      case 'fatal':
        return 'bg-red-500';
      case 'error':
        return 'bg-destructive';
      case 'warning':
        return 'bg-amber-500';
      case 'info':
        return 'bg-blue-400';
      case 'debug':
        return 'bg-muted-foreground';
      default:
        return 'bg-muted-foreground/60';
    }
  }

  const dotClass = $derived(levelDot(issue.level));
  const countVariant: 'destructive' | 'secondary' = $derived(
    issue.event_count >= 100 ? 'destructive' : 'secondary'
  );

  const sparkValues = $derived.by(() => {
    void eventStream.tick;
    return eventStream.buckets(issue.id, 30);
  });

  const spiking = $derived.by(() => {
    void eventStream.tick;
    return eventStream.isSpiking(issue.id);
  });

  const assignee = $derived(actions.assigneeFor(issue));
  const isMuted = $derived(issue.status === 'muted' || issue.status === 'ignored');

  const railClass = $derived(
    issue.level === 'fatal'
      ? 'before:bg-red-500'
      : issue.level === 'error'
        ? 'before:bg-destructive'
        : issue.level === 'warning'
          ? 'before:bg-amber-500'
          : 'before:bg-transparent'
  );

  const initial = $derived(assignee ? assignee[0]!.toUpperCase() : '');
</script>

<button
  type="button"
  onclick={() => onSelect?.(issue.id)}
  class={cn(
    'relative flex min-h-[60px] w-full items-center gap-4 px-5 py-3 text-left transition-colors',
    'hover:bg-accent border-b border-border/50',
    "before:absolute before:inset-y-0 before:left-0 before:w-0.5 before:content-['']",
    railClass,
    selected && 'bg-accent/70',
    isMuted && 'opacity-60'
  )}
>
  <span class={cn('h-2.5 w-2.5 shrink-0 rounded-full', dotClass)}></span>
  <Badge variant={countVariant} class="min-w-[2.75rem] justify-center px-2 py-0.5 text-[12px] tabular-nums">
    {issue.event_count}
  </Badge>
  <div class="flex min-w-0 flex-1 flex-col gap-1">
    <span class="truncate text-[14px] font-medium leading-snug text-foreground">{issue.title}</span>
    {#if issue.culprit}
      <span class="truncate font-mono text-[12px] leading-snug text-muted-foreground">{issue.culprit}</span>
    {/if}
  </div>

  {#if spiking}
    <Tooltip.Root>
      <Tooltip.Trigger>
        {#snippet child({ props })}
          <span {...props} aria-label="Subindo nos últimos 5 min" class="text-amber-400 shrink-0 inline-flex">
            <Flame class="h-4 w-4" />
          </span>
        {/snippet}
      </Tooltip.Trigger>
      <Tooltip.Content>Subindo nos últimos 5 min</Tooltip.Content>
    </Tooltip.Root>
  {/if}

  {#if issue.status === 'resolved'}
    <Tooltip.Root>
      <Tooltip.Trigger>
        {#snippet child({ props })}
          <span {...props} aria-label="Resolvida" class="text-emerald-500 shrink-0 inline-flex">
            <Check class="h-4 w-4" />
          </span>
        {/snippet}
      </Tooltip.Trigger>
      <Tooltip.Content>Resolvida</Tooltip.Content>
    </Tooltip.Root>
  {:else if issue.status === 'muted'}
    <Tooltip.Root>
      <Tooltip.Trigger>
        {#snippet child({ props })}
          <span {...props} aria-label="Silenciada" class="text-muted-foreground shrink-0 inline-flex">
            <BellOff class="h-4 w-4" />
          </span>
        {/snippet}
      </Tooltip.Trigger>
      <Tooltip.Content>Silenciada</Tooltip.Content>
    </Tooltip.Root>
  {:else if issue.status === 'ignored'}
    <Tooltip.Root>
      <Tooltip.Trigger>
        {#snippet child({ props })}
          <span {...props} aria-label="Ignorada" class="text-muted-foreground shrink-0 inline-flex">
            <Ban class="h-4 w-4" />
          </span>
        {/snippet}
      </Tooltip.Trigger>
      <Tooltip.Content>Ignorada</Tooltip.Content>
    </Tooltip.Root>
  {/if}

  {#if assignee}
    <Tooltip.Root>
      <Tooltip.Trigger>
        {#snippet child({ props })}
          <span {...props} aria-label={`Atribuída a ${assignee}`} class="shrink-0 inline-flex">
            <Avatar.Root class="h-6 w-6 text-[11px]">
              <Avatar.Fallback class="bg-accent text-foreground">{initial}</Avatar.Fallback>
            </Avatar.Root>
          </span>
        {/snippet}
      </Tooltip.Trigger>
      <Tooltip.Content>Atribuída a {assignee}</Tooltip.Content>
    </Tooltip.Root>
  {/if}

  <Sparkline values={sparkValues} accent={spiking} width={56} height={16} class="shrink-0" />
  <span class="shrink-0 text-[12px] text-muted-foreground tabular-nums">
    {relativeTime(issue.last_seen)}
  </span>
</button>
