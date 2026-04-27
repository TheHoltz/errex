<script lang="ts">
  import { Ban, BellOff, Check, Circle, Search, ShieldCheck } from 'lucide-svelte';
  import { Input } from '$lib/components/ui/input';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { eventStream } from '$lib/eventStream.svelte';
  import { filter, issues, load, projects, selection, visibleIssues } from '$lib/stores.svelte';
  import type { IssueStatus } from '$lib/types';
  import { cn } from '$lib/utils';
  import IssueRow from './IssueRow.svelte';

  type Props = {
    onSelect?: (id: number) => void;
    filterRef?: { current: HTMLInputElement | null };
  };
  let { onSelect, filterRef }: Props = $props();

  let inputEl = $state<HTMLInputElement | null>(null);
  $effect(() => {
    if (filterRef && inputEl) filterRef.current = inputEl;
  });

  const visible = $derived(visibleIssues());

  type StatusChip = {
    key: IssueStatus;
    label: string;
    Icon: typeof Circle;
  };

  const chips: StatusChip[] = [
    { key: 'unresolved', label: 'Não resolvidas', Icon: Circle },
    { key: 'resolved', label: 'Resolvidas', Icon: Check },
    { key: 'muted', label: 'Silenciadas', Icon: BellOff },
    { key: 'ignored', label: 'Ignoradas', Icon: Ban }
  ];

  function isChecked(s: IssueStatus): boolean {
    return filter.statuses.has(s);
  }

  function statusCount(s: IssueStatus): number {
    return issues.list.filter((i) => i.project === projects.current && i.status === s).length;
  }

  const allClearLabel = $derived.by(() => {
    void eventStream.tick;
    if (eventStream.lastAt == null) return 'Aguardando primeiro evento.';
    const minutes = Math.floor((Date.now() - eventStream.lastAt) / 60_000);
    if (minutes <= 0) return 'Tudo calmo · último evento agora mesmo.';
    if (minutes === 1) return 'Tudo calmo · último evento há 1 min.';
    if (minutes < 60) return `Tudo calmo · último evento há ${minutes} min.`;
    return `Tudo calmo · último evento há ${Math.floor(minutes / 60)} h.`;
  });

  const hasActiveFilter = $derived(
    filter.query.trim().length > 0 ||
      filter.statuses.size !== 1 ||
      !filter.statuses.has('unresolved')
  );
</script>

<div class="flex h-full flex-col">
  <div class="flex items-center gap-3 border-b border-border px-5 py-3">
    <div class="relative flex-1">
      <Search class="text-muted-foreground absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2" />
      <Input
        bind:ref={inputEl}
        bind:value={filter.query}
        placeholder="filtrar  /"
        class="h-10 pl-9 text-[14px]"
      />
    </div>
    <div class="flex items-center gap-1.5">
      {#each chips as chip (chip.key)}
        {@const on = isChecked(chip.key)}
        <Tooltip.Root>
          <Tooltip.Trigger>
            {#snippet child({ props })}
              <button
                {...props}
                type="button"
                onclick={() => filter.toggleStatus(chip.key)}
                aria-pressed={on}
                aria-label={chip.label}
                class={cn(
                  'border-border inline-flex h-9 w-9 items-center justify-center rounded-md border transition-colors',
                  on ? 'bg-accent text-foreground' : 'text-muted-foreground/60 hover:text-foreground hover:bg-accent/50'
                )}
              >
                <chip.Icon class="h-4 w-4" />
              </button>
            {/snippet}
          </Tooltip.Trigger>
          <Tooltip.Content>{chip.label} ({statusCount(chip.key)})</Tooltip.Content>
        </Tooltip.Root>
      {/each}
    </div>
  </div>

  <div class="flex-1 overflow-y-auto">
    {#if load.initialLoad}
      <ul class="flex flex-col gap-0">
        {#each Array.from({ length: 6 }) as _, i (i)}
          <li class="border-b border-border/50 px-5 py-4">
            <div class="flex items-center gap-4">
              <Skeleton class="h-2.5 w-2.5 rounded-full" />
              <Skeleton class="h-5 w-11" />
              <div class="flex flex-1 flex-col gap-2">
                <Skeleton class="h-3.5 w-3/4" />
                <Skeleton class="h-3 w-1/2" />
              </div>
              <Skeleton class="h-4 w-12" />
            </div>
          </li>
        {/each}
      </ul>
    {:else if visible.length === 0 && hasActiveFilter}
      <div class="text-muted-foreground flex flex-col items-center gap-2 p-8 text-center text-[12px]">
        <p>Nenhuma issue para esse filtro.</p>
        <button
          type="button"
          onclick={() => {
            filter.query = '';
            filter.statuses = new Set<IssueStatus>(['unresolved']);
          }}
          class="text-primary hover:underline text-[12px]"
        >
          Limpar filtros
        </button>
      </div>
    {:else if visible.length === 0}
      <div
        class={cn(
          'flex flex-col items-center justify-center gap-3 px-6 py-12 text-center',
          'text-muted-foreground'
        )}
      >
        <ShieldCheck class="text-emerald-500/80 h-8 w-8" />
        <p class="text-foreground text-[13px] font-medium">{allClearLabel}</p>
        <p class="text-[12px]">Sem issues abertas no projeto.</p>
      </div>
    {:else}
      {#each visible as issue (issue.id)}
        <IssueRow {issue} selected={issue.id === selection.issueId} {onSelect} />
      {/each}
    {/if}
  </div>
</div>
