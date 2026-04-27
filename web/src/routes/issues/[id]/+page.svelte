<script lang="ts">
  import { getContext, onMount, untrack } from 'svelte';
  import IssueDetail from '$lib/components/IssueDetail.svelte';
  import IssueList from '$lib/components/IssueList.svelte';
  import { Resizable } from '$lib/components/ui/resizable';
  import { selectIssue } from '$lib/selection';
  import { issues, selection } from '$lib/stores.svelte';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();

  let leftPercent = $state(40);
  const filterRef = getContext<{ current: HTMLInputElement | null }>('filterRef');

  // Sync the URL parameter into our selection store. `untrack` is critical:
  // selectIssue reads selection.issueId internally, which would otherwise
  // make this effect rerun on every selection change and bounce the URL
  // value back over an in-list click (handleSelect → selectIssue mutates
  // issueId → effect re-fires with stale data.id → re-selects old row).
  onMount(() => {
    if (data.id != null) selectIssue(data.id);
  });

  $effect(() => {
    const id = data.id;
    if (id != null) untrack(() => selectIssue(id));
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
