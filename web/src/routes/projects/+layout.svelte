<script lang="ts">
  import { Loader2, Plus, Shield, Webhook } from 'lucide-svelte';
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import { admin } from '$lib/admin.svelte';
  import { auth } from '$lib/auth.svelte';
  import ProjectRail from '$lib/components/ProjectRail.svelte';
  import { Button } from '$lib/components/ui/button';
  import * as Card from '$lib/components/ui/card';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import { toast } from '$lib/toast.svelte';

  let { children } = $props();

  // We re-route the user to /projects/<first> on first load when they land
  // on /projects with at least one project — the rail-only view is fine
  // for "in between" but a cold landing should pick something useful.
  let didFirstLoadRedirect = $state(false);

  let firstProjectName = $state('');
  let creatingFirst = $state(false);

  const isViewerLockedOut = $derived(admin.error === 'forbidden');
  const isEmpty = $derived(
    !admin.loading && admin.error === null && admin.projects.length === 0
  );
  const activeName = $derived($page.params.name ?? null);

  onMount(() => {
    void admin.loadProjects();
  });

  // Auto-redirect /projects → /projects/<first> once we know the list. Only
  // fires once per session so the user can navigate back to /projects to
  // see the placeholder if they want.
  $effect(() => {
    if (
      !didFirstLoadRedirect &&
      $page.url.pathname === '/projects' &&
      admin.projects.length > 0
    ) {
      didFirstLoadRedirect = true;
      const first = admin.projects[0];
      if (first) void goto(`/projects/${encodeURIComponent(first.name)}`, { replaceState: true });
    }
  });

  async function createFirstProject(e: SubmitEvent) {
    e.preventDefault();
    const name = firstProjectName.trim();
    if (!name) return;
    creatingFirst = true;
    try {
      const created = await admin.createProject(name);
      firstProjectName = '';
      toast.success(`Project "${created.name}" created`);
      void goto(`/projects/${encodeURIComponent(created.name)}`, { replaceState: true });
    } catch (err) {
      toast.error('Failed to create project', { description: String(err) });
    } finally {
      creatingFirst = false;
    }
  }
</script>

<div class="flex h-full min-h-0">
  {#if isViewerLockedOut}
    <div class="mx-auto w-full max-w-2xl px-8 py-10">
      <Card.Root>
        <Card.Header class="flex-row items-start gap-3 space-y-0">
          <Shield class="text-amber-400 mt-0.5 h-5 w-5 shrink-0" />
          <div class="flex flex-col gap-1">
            <Card.Title class="text-[14px]">Admin only</Card.Title>
            <Card.Description class="text-[13px]">
              You're signed in as
              <span class="font-mono">{auth.user?.username ?? '?'}</span>
              ({auth.user?.role ?? '?'}). Project management requires the admin role —
              ask an administrator to elevate your account on the
              <a href="/team" class="text-primary underline">Team</a> page.
            </Card.Description>
          </div>
        </Card.Header>
      </Card.Root>
    </div>
  {:else if admin.loading && admin.projects.length === 0}
    <div class="text-muted-foreground flex flex-1 items-center justify-center gap-3 py-12 text-[13px]">
      <Loader2 class="h-4 w-4 animate-spin" />
      Loading projects…
    </div>
  {:else if isEmpty}
    <div class="flex flex-1 items-center justify-center px-6 py-10">
      <Card.Root class="w-full max-w-md border-dashed">
        <Card.Header class="items-center text-center">
          <div
            class="bg-accent/40 mb-2 inline-flex h-12 w-12 items-center justify-center rounded-full"
          >
            <Webhook class="text-muted-foreground h-5 w-5" />
          </div>
          <Card.Title class="text-[16px]">Create your first project</Card.Title>
          <Card.Description class="text-[13px]">
            A project gives you a DSN you can drop into an SDK or curl. Events you send to that
            DSN show up live in the issue list.
          </Card.Description>
        </Card.Header>
        <Card.Content>
          <form onsubmit={createFirstProject} class="mx-auto flex max-w-sm items-end gap-3">
            <div class="flex flex-1 flex-col gap-2">
              <Label for="firstp" class="text-[12px]">Project name</Label>
              <Input
                id="firstp"
                bind:value={firstProjectName}
                placeholder="my-app"
                autocomplete="off"
                class="h-10 text-[13px]"
              />
            </div>
            <Button
              type="submit"
              disabled={creatingFirst || firstProjectName.trim().length === 0}
              class="h-10"
            >
              {#if creatingFirst}
                <Loader2 class="h-4 w-4 animate-spin" />
              {:else}
                <Plus class="h-4 w-4" />
              {/if}
              Create
            </Button>
          </form>
        </Card.Content>
      </Card.Root>
    </div>
  {:else}
    <ProjectRail {activeName} />
    <main class="min-w-0 flex-1">
      {@render children?.()}
    </main>
  {/if}
</div>
