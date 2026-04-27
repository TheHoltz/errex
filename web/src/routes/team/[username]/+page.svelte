<script lang="ts">
  import { Loader2 } from 'lucide-svelte';
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import UserDetail from '$lib/components/UserDetail.svelte';
  import { team } from '$lib/team.svelte';
  import { toast } from '$lib/toast.svelte';

  const username = $derived($page.params.username ?? '');
  const user = $derived(team.users.find((u) => u.username === username) ?? null);

  let bounced = $state(false);
  $effect(() => {
    if (!team.loading && !user && !bounced && team.users.length > 0) {
      bounced = true;
      toast.error(`User "${username}" not found`);
      void goto('/team', { replaceState: true });
    }
  });

  onMount(() => {
    if (team.users.length === 0 && !team.loading) {
      void team.loadUsers();
    }
  });
</script>

<svelte:head>
  <title>{username} · Team · errex</title>
</svelte:head>

{#if user}
  <UserDetail {user} />
{:else}
  <div class="text-muted-foreground flex h-full items-center justify-center gap-2 text-[13px]">
    <Loader2 class="h-4 w-4 animate-spin" />
    Loading {username}…
  </div>
{/if}
