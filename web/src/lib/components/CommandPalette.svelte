<script lang="ts">
  import { Search } from 'lucide-svelte';
  import { goto } from '$app/navigation';
  import { Button } from '$lib/components/ui/button';
  import { Dialog } from '$lib/components/ui/dialog';
  import { actions } from '$lib/actions.svelte';
  import { toggleMute, toggleResolve } from '$lib/issueOps';
  import { issues, projects, selection } from '$lib/stores.svelte';
  import { toast } from '$lib/toast.svelte';
  import { cn } from '$lib/utils';
  import { connect } from '$lib/ws';

  type Props = { open: boolean; onClose: () => void };
  let { open, onClose }: Props = $props();

  type CommandKind = 'action' | 'issue' | 'project';

  interface Command {
    id: string;
    kind: CommandKind;
    label: string;
    hint?: string;
    keywords: string;
    run: () => void;
  }

  let query = $state('');
  let cursor = $state(0);

  // Reset when the palette is reopened so the user always starts fresh.
  $effect(() => {
    if (open) {
      query = '';
      cursor = 0;
    }
  });

  const selected = $derived(
    selection.issueId == null ? null : (issues.get(selection.issueId) ?? null)
  );

  const commands = $derived.by<Command[]>(() => {
    const list: Command[] = [];

    // Issue actions — only when there's a selection. Status changes go
    // through `toggleResolve`/`toggleMute` which speak to the API; assignee
    // remains local until the proto adds an assignee field.
    if (selected) {
      const issue = selected;
      list.push({
        id: 'resolve',
        kind: 'action',
        label: issue.status === 'resolved' ? 'Reopen issue' : 'Resolve issue',
        hint: 'E',
        keywords: 'resolve fix reopen',
        run: () => {
          void toggleResolve(issue);
          onClose();
        }
      });
      list.push({
        id: 'mute',
        kind: 'action',
        label: issue.status === 'muted' ? 'Reactivate issue' : 'Mute issue',
        hint: 'M',
        keywords: 'mute silence reactivate',
        run: () => {
          void toggleMute(issue);
          onClose();
        }
      });
      list.push({
        id: 'assign',
        kind: 'action',
        label: 'Assign to me',
        hint: 'A',
        keywords: 'assign',
        run: () => {
          const prev = actions.assignToMe(issue);
          toast.success(`Assigned to ${actions.me}`, {
            undo: () => actions.setAssignee(issue, prev)
          });
          onClose();
        }
      });
      list.push({
        id: 'copy',
        kind: 'action',
        label: 'Copy issue link',
        keywords: 'copy link',
        run: () => {
          const url = `${location.origin}/issues/${issue.id}`;
          navigator.clipboard?.writeText(url).then(
            () => toast.success('Link copied'),
            () => toast.error('Could not copy')
          );
          onClose();
        }
      });
    }

    // Project switcher.
    for (const p of projects.available) {
      if (p.project === projects.current) continue;
      list.push({
        id: `project:${p.project}`,
        kind: 'project',
        label: `Switch project · ${p.project}`,
        hint: `${p.issue_count}`,
        keywords: `project switch ${p.project}`,
        run: () => {
          connect(p.project);
          onClose();
        }
      });
    }

    // Issue search (only for the active project, top 50 by recency).
    // IssuesStore.list is unordered since the sort feature moved ordering
    // into visibleIssues. Sort here so "top 50" stays "by recency."
    const q = query.trim().toLowerCase();
    if (q.length >= 1) {
      const recent = [...issues.list].sort(
        (a, b) => Date.parse(b.last_seen) - Date.parse(a.last_seen)
      );
      let count = 0;
      for (const issue of recent) {
        if (issue.project !== projects.current) continue;
        if (
          !issue.title.toLowerCase().includes(q) &&
          !(issue.culprit?.toLowerCase().includes(q) ?? false) &&
          !issue.fingerprint.toLowerCase().includes(q)
        ) {
          continue;
        }
        list.push({
          id: `issue:${issue.id}`,
          kind: 'issue',
          label: issue.title,
          hint: issue.culprit ?? `#${issue.id}`,
          keywords: `${issue.title} ${issue.culprit ?? ''} ${issue.fingerprint}`,
          run: () => {
            goto(`/issues/${issue.id}`);
            onClose();
          }
        });
        if (++count >= 50) break;
      }
    }

    if (q.length === 0) return list;
    return list.filter((c) => c.label.toLowerCase().includes(q) || c.keywords.includes(q));
  });

  function onKey(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      cursor = Math.min(cursor + 1, Math.max(commands.length - 1, 0));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      cursor = Math.max(cursor - 1, 0);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      commands[cursor]?.run();
    }
  }

  function kindLabel(k: CommandKind): string {
    switch (k) {
      case 'action':
        return 'Action';
      case 'issue':
        return 'Issue';
      case 'project':
        return 'Project';
    }
  }
</script>

<Dialog {open} {onClose}>
  <div class="border-b border-border flex items-center gap-3 px-4 py-3">
    <Search class="text-muted-foreground h-4 w-4 shrink-0" />
    <input
      type="text"
      bind:value={query}
      onkeydown={onKey}
      placeholder="Search issue, switch project, run action…"
      class="flex-1 bg-transparent text-[14px] focus:outline-none placeholder:text-muted-foreground"
      autocomplete="off"
      spellcheck="false"
    />
    <kbd
      class="text-muted-foreground border-border rounded border px-2 py-0.5 font-mono text-[11px]"
    >
      ESC
    </kbd>
  </div>
  <ul class="max-h-[55vh] overflow-y-auto py-2">
    {#if commands.length === 0}
      <li class="text-muted-foreground px-4 py-10 text-center text-[13px]">
        Nothing found for "{query}".
      </li>
    {:else}
      {#each commands as cmd, i (cmd.id)}
        <li>
          <Button
            variant="ghost"
            size="sm"
            onmouseenter={() => (cursor = i)}
            onclick={() => cmd.run()}
            class={cn(
              'h-auto w-full justify-start gap-3 rounded-none px-4 py-2.5 text-left text-[13px] font-normal',
              i === cursor ? 'bg-accent text-accent-foreground' : 'text-foreground'
            )}
          >
            <span
              class="text-muted-foreground w-16 shrink-0 text-[11px] uppercase tracking-wider"
            >
              {kindLabel(cmd.kind)}
            </span>
            <span class="min-w-0 flex-1 truncate">{cmd.label}</span>
            {#if cmd.hint}
              <span class="text-muted-foreground shrink-0 font-mono text-[11px]">{cmd.hint}</span>
            {/if}
          </Button>
        </li>
      {/each}
    {/if}
  </ul>
  <div
    class="text-muted-foreground border-t border-border flex items-center gap-4 px-4 py-2 text-[11px]"
  >
    <span><kbd class="font-mono">↑↓</kbd> navigate</span>
    <span><kbd class="font-mono">↵</kbd> execute</span>
    <span class="ml-auto">errex command palette</span>
  </div>
</Dialog>
