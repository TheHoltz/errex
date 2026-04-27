<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { actions } from '$lib/actions.svelte';
  import { toggleMute, toggleResolve } from '$lib/issueOps';
  import { selectIssue } from '$lib/selection';
  import { issues, projects, selection, visibleIssues } from '$lib/stores.svelte';
  import { toast } from '$lib/toast.svelte';

  type Props = { onOpenPalette: () => void; onFocusFilter?: () => void };
  let { onOpenPalette, onFocusFilter }: Props = $props();

  function selectedIssue() {
    return selection.issueId == null ? null : (issues.get(selection.issueId) ?? null);
  }

  function moveSelection(delta: number) {
    const list = visibleIssues();
    if (list.length === 0) return;
    let idx = list.findIndex((i) => i.id === selection.issueId);
    if (idx === -1) idx = delta > 0 ? -1 : list.length;
    const next = Math.max(0, Math.min(list.length - 1, idx + delta));
    const target = list[next];
    if (!target) return;
    selectIssue(target.id);
    history.replaceState(history.state, '', `/issues/${target.id}`);
  }

  function isTypingTarget(t: EventTarget | null): boolean {
    if (!(t instanceof HTMLElement)) return false;
    const tag = t.tagName;
    return tag === 'INPUT' || tag === 'TEXTAREA' || t.isContentEditable;
  }

  function onKey(e: KeyboardEvent) {
    // Cmd/Ctrl-K opens the palette from anywhere, including inputs — that's
    // the global escape hatch and the convention every dev tool follows.
    const meta = e.metaKey || e.ctrlKey;
    if (meta && (e.key === 'k' || e.key === 'K')) {
      e.preventDefault();
      onOpenPalette();
      return;
    }

    // The rest of the shortcuts must not fire while typing.
    if (isTypingTarget(e.target)) return;
    if (e.altKey || meta || e.shiftKey) return;

    const issue = selectedIssue();

    switch (e.key) {
      case 'j':
        e.preventDefault();
        moveSelection(1);
        return;
      case 'k':
        e.preventDefault();
        moveSelection(-1);
        return;
      case '/':
        e.preventDefault();
        onFocusFilter?.();
        return;
      case 'Escape':
        if (selection.issueId != null) {
          selectIssue(null);
          if (location.pathname.startsWith('/issues/')) goto('/');
        }
        return;
    }

    if (!issue) return;

    switch (e.key) {
      case 'e':
        e.preventDefault();
        void toggleResolve(issue);
        return;
      case 'm':
        e.preventDefault();
        void toggleMute(issue);
        return;
      case 'a': {
        e.preventDefault();
        const prev = actions.assignToMe(issue);
        toast.success(`Atribuída a ${actions.me}`, {
          undo: () => actions.setAssignee(issue, prev)
        });
        return;
      }
    }

    // Reference projects so the linter doesn't drop the import; future
    // shortcuts (P to switch project) plug in here.
    void projects;
  }

  onMount(() => {
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });
</script>
