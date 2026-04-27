<script lang="ts">
  import { getContext } from 'svelte';
  import { goto } from '$app/navigation';
  import IssueDetail from '$lib/components/IssueDetail.svelte';
  import IssueList from '$lib/components/IssueList.svelte';
  import { Resizable } from '$lib/components/ui/resizable';
  import { selectIssue } from '$lib/selection';
  import { issues, load, projects, selection } from '$lib/stores.svelte';

  let leftPercent = $state(40);
  const filterRef = getContext<{ current: HTMLInputElement | null }>('filterRef');

  const selectedIssue = $derived(
    selection.issueId == null ? null : (issues.get(selection.issueId) ?? null)
  );

  // First-run nudge: when the daemon has no projects yet, send the operator
  // straight to /projects which has the "create your first project" hero.
  // We wait for `load.initialLoad` to settle so a slow /api/projects call
  // doesn't trigger the redirect before the data lands.
  $effect(() => {
    if (!load.initialLoad && projects.available.length === 0) {
      void goto('/projects', { replaceState: true });
    }
  });

  function handleSelect(id: number) {
    selectIssue(id);
    goto(`/issues/${id}`, { replaceState: true, keepFocus: true, noScroll: true });
  }
</script>

<Resizable bind:leftPercent class="h-full" minLeft={20} maxLeft={75}>
  {#snippet left()}
    <IssueList onSelect={handleSelect} {filterRef} />
  {/snippet}
  {#snippet right()}
    <IssueDetail issue={selectedIssue} />
  {/snippet}
</Resizable>
