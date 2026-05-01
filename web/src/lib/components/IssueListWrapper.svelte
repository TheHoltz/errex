<!--
  Test-only wrapper. The app layout normally provides Tooltip.Provider; this
  shim provides it for component tests in jsdom so IssueList can be rendered
  in isolation. Not consumed by any production page.
-->
<script lang="ts">
  import * as Tooltip from '$lib/components/ui/tooltip';
  import IssueList from './IssueList.svelte';

  type Props = {
    onSelect?: (id: number) => void;
  };
  let { onSelect }: Props = $props();
</script>

<Tooltip.Provider delayDuration={0}>
  <IssueList {onSelect} />
</Tooltip.Provider>
