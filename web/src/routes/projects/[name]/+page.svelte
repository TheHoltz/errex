<script lang="ts">
  import { Loader2 } from 'lucide-svelte';
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import { admin } from '$lib/admin.svelte';
  import ProjectDetail from '$lib/components/ProjectDetail.svelte';
  import { projects } from '$lib/stores.svelte';
  import { toast } from '$lib/toast.svelte';
  import { connect } from '$lib/ws';

  const name = $derived($page.params.name ?? '');
  const project = $derived(admin.projects.find((p) => p.name === name) ?? null);

  // Switching the live WS to the project being viewed lets the activity
  // sparkline refresh in response to fresh events. Side effect: navigating
  // back to the issues page (`/`) shows issues for THIS project, not the
  // one previously selected — same model as clicking the project selector.
  // No-op when we're already pointed at it (e.g. user came from `/`).
  $effect(() => {
    if (name && projects.current !== name) {
      connect(name);
    }
  });

  // If we land here directly (deep link or refresh) and the project isn't in
  // the cache because admin.loadProjects hasn't run yet, the layout's onMount
  // will populate it. Once populated, if there's still no match, the project
  // genuinely doesn't exist — bounce to /projects with a toast.
  let bounced = $state(false);
  $effect(() => {
    if (!admin.loading && !project && !bounced && admin.projects.length > 0) {
      bounced = true;
      toast.error(`Project "${name}" not found`);
      void goto('/projects', { replaceState: true });
    }
  });

  onMount(() => {
    // Auth is handled by the root layout; if we got here we're signed in.
    // Just kick off a load if the cache is empty (e.g. deep link straight
    // to a project URL without first visiting /projects).
    if (admin.projects.length === 0 && !admin.loading) {
      void admin.loadProjects();
    }
  });
</script>

<svelte:head>
  <title>{name} · Projects · errex</title>
</svelte:head>

{#if project}
  <ProjectDetail {project} />
{:else}
  <div class="text-muted-foreground flex h-full items-center justify-center gap-2 text-[13px]">
    <Loader2 class="h-4 w-4 animate-spin" />
    Loading {name}…
  </div>
{/if}
