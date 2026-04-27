<script lang="ts">
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';

  // /settings is a thin landing point that bounces to the only sub-page
  // we have today. When more settings categories ship, replace this with
  // a real index that lists them.
  onMount(() => {
    void goto('/settings/retention', { replaceState: true });
  });
</script>

<svelte:head>
  <title>Settings · errex</title>
</svelte:head>
