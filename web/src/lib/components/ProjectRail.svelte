<script lang="ts">
  import { Loader2, Plus, Webhook } from 'lucide-svelte';
  import { goto } from '$app/navigation';
  import { admin } from '$lib/admin.svelte';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { projectActivityStatus } from '$lib/projectsConsole';
  import { toast } from '$lib/toast.svelte';
  import { cn, relativeTime } from '$lib/utils';

  type Props = { activeName: string | null };
  let { activeName }: Props = $props();

  let filter = $state('');
  let newName = $state('');
  let busy = $state(false);

  const filtered = $derived.by(() => {
    const needle = filter.trim().toLowerCase();
    const list = admin.projects.slice();
    // Stable sort: most-recently-active first, then alphabetic for the
    // long tail (NULLS LAST so "never used" projects don't elbow live ones
    // out of the top of the list).
    list.sort((a, b) => {
      const at = a.last_used_at ? Date.parse(a.last_used_at) : 0;
      const bt = b.last_used_at ? Date.parse(b.last_used_at) : 0;
      if (at !== bt) return bt - at;
      return a.name.localeCompare(b.name);
    });
    if (!needle) return list;
    return list.filter((p) => p.name.toLowerCase().includes(needle));
  });

  // Tick every 30s so "live → recent → idle" decays without a refetch.
  // The DB backing data hasn't changed; we just need the derived label
  // to re-evaluate against a newer `Date.now()`.
  let tick = $state(0);
  $effect(() => {
    const id = setInterval(() => (tick += 1), 30_000);
    return () => clearInterval(id);
  });

  async function createProject(e: SubmitEvent) {
    e.preventDefault();
    const name = newName.trim();
    if (!name) return;
    busy = true;
    try {
      const created = await admin.createProject(name);
      newName = '';
      toast.success(`Project "${created.name}" created`);
      void goto(`/projects/${encodeURIComponent(created.name)}`, { keepFocus: true });
    } catch (err) {
      toast.error('Failed to create project', { description: String(err) });
    } finally {
      busy = false;
    }
  }
</script>

<aside class="border-border bg-background flex w-[280px] shrink-0 flex-col border-r">
  <div class="border-border border-b p-3">
    <Input
      bind:value={filter}
      placeholder="filter projects…"
      autocomplete="off"
      class="h-9 text-[12.5px]"
      aria-label="Filter projects"
    />
  </div>

  <ul class="flex-1 overflow-y-auto" aria-label="Projects">
    {#each filtered as p (p.name)}
      {@const status = projectActivityStatus(p.last_used_at, Date.now() + tick * 0)}
      {@const isActive = p.name === activeName}
      <li>
        <a
          href={`/projects/${encodeURIComponent(p.name)}`}
          class={cn(
            'border-border hover:bg-accent/40 flex flex-col gap-1 border-b px-4 py-3 transition-colors',
            isActive && 'bg-accent/60 border-l-2 border-l-emerald-500 pl-[14px]'
          )}
          aria-current={isActive ? 'page' : undefined}
        >
          <span class="flex items-center gap-2">
            <span
              class={cn('h-2 w-2 shrink-0 rounded-full', status.tone)}
              title={status.label}
              aria-hidden="true"
            ></span>
            <span class="truncate font-mono text-[13px]">{p.name}</span>
            {#if p.webhook_url}
              <Webhook class="text-emerald-500 ml-auto h-3 w-3 shrink-0" aria-label="webhook configured" />
            {/if}
          </span>
          <span class="text-muted-foreground text-[11px]">
            {#if p.last_used_at}
              last event {relativeTime(p.last_used_at)}
            {:else}
              no events yet
            {/if}
          </span>
        </a>
      </li>
    {/each}

    {#if filtered.length === 0 && admin.projects.length > 0}
      <li class="text-muted-foreground px-4 py-6 text-center text-[12px]">
        No projects match "<span class="font-mono">{filter}</span>"
      </li>
    {/if}
  </ul>

  <form onsubmit={createProject} class="border-border bg-muted/30 flex items-center gap-2 border-t p-3">
    <Input
      bind:value={newName}
      placeholder="new project…"
      autocomplete="off"
      aria-label="New project name"
      class="h-9 flex-1 text-[12.5px]"
    />
    <Button
      type="submit"
      size="sm"
      disabled={busy || newName.trim().length === 0}
      class="h-9"
      aria-label="Create project"
    >
      {#if busy}
        <Loader2 class="h-3.5 w-3.5 animate-spin" />
      {:else}
        <Plus class="h-3.5 w-3.5" />
      {/if}
    </Button>
  </form>
</aside>
