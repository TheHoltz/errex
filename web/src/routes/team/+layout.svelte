<script lang="ts">
  import { Loader2 } from 'lucide-svelte';
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import UserRail from '$lib/components/UserRail.svelte';
  import { team } from '$lib/team.svelte';

  let { children } = $props();

  let didFirstLoadRedirect = $state(false);

  const activeUsername = $derived(($page.params as { username?: string }).username ?? null);

  onMount(() => {
    void team.loadUsers();
  });

  // Land on the first user when the operator hits /team with N>0 users.
  // Same pattern as /projects: feels less empty, deep links still work.
  $effect(() => {
    if (
      !didFirstLoadRedirect &&
      $page.url.pathname === '/team' &&
      team.users.length > 0
    ) {
      didFirstLoadRedirect = true;
      const first = team.users[0];
      if (first) void goto(`/team/${encodeURIComponent(first.username)}`, { replaceState: true });
    }
  });
</script>

<div class="flex h-full min-h-0">
  {#if team.loading && team.users.length === 0}
    <div class="text-muted-foreground flex flex-1 items-center justify-center gap-3 py-12 text-[13px]">
      <Loader2 class="h-4 w-4 animate-spin" /> Loading users…
    </div>
  {:else}
    <UserRail {activeUsername} />
    <main class="min-w-0 flex-1">
      {@render children?.()}
    </main>
  {/if}
</div>
