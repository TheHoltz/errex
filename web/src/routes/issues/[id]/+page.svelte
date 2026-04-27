<script lang="ts">
  import { getContext, onMount } from 'svelte';
  import IssueDetail from '$lib/components/IssueDetail.svelte';
  import IssueList from '$lib/components/IssueList.svelte';
  import { Resizable } from '$lib/components/ui/resizable';
  import { selectIssue } from '$lib/selection';
  import { issues, selection } from '$lib/stores.svelte';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();

  let leftPercent = $state(40);
  const filterRef = getContext<{ current: HTMLInputElement | null }>('filterRef');

  // Sync the URL parameter into our selection store. The `selectIssue`
  // helper is idempotent — calling it with the current id is a no-op — so
  // running it both onMount and on $effect (for client-side navigations)
  // keeps the deep-link case correct without spamming requests.
  onMount(() => {
    if (data.id != null) selectIssue(data.id);
  });

  $effect(() => {
    if (data.id != null) selectIssue(data.id);
  });

  const selectedIssue = $derived(
    selection.issueId == null ? null : (issues.get(selection.issueId) ?? null)
  );

  function handleSelect(id: number) {
    selectIssue(id);
    history.replaceState(history.state, '', `/issues/${id}`);
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
