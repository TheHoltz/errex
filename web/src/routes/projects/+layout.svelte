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
      <!-- Inline style on the root carries the primary-tinted top hairline
           and a confident drop shadow. HSL-with-alpha goes through `style`
           rather than Tailwind arbitrary-value classes; this matches the
           project convention used in /setup/+page.svelte. -->
      <Card.Root
        class="relative w-full max-w-md overflow-hidden p-8"
        style="box-shadow: 0 24px 60px hsl(0 0% 0% / 0.55);"
      >
        <span
          aria-hidden="true"
          class="pointer-events-none absolute inset-x-0 top-0 h-px"
          style="background: linear-gradient(90deg, transparent, hsl(var(--primary) / 0.45) 25%, hsl(var(--primary) / 0.45) 75%, transparent);"
        ></span>
        <div
          class="text-primary mb-4 inline-flex h-10 w-10 items-center justify-center rounded-lg"
          style="background: hsl(var(--primary) / 0.10); border: 1px solid hsl(var(--primary) / 0.22);"
        >
          <Webhook class="h-5 w-5" />
        </div>
        <h2 class="text-[17px] font-semibold tracking-tight">Create your first project</h2>
        <p class="text-muted-foreground mt-1.5 max-w-[38ch] text-[13px] leading-relaxed">
          A project gives you a DSN you can drop into an SDK or curl. Events you send to that
          DSN show up live in the issue list.
        </p>
        <form onsubmit={createFirstProject} class="mt-6 flex flex-col gap-2">
          <Label for="firstp" class="text-muted-foreground text-[11px]">Project name</Label>
          <Input
            id="firstp"
            bind:value={firstProjectName}
            placeholder="my-app"
            autocomplete="off"
            autofocus
            class="h-10 text-[13px]"
          />
          <span class="text-muted-foreground/80 text-[11px]">
            shows up in URLs and the DSN · case-sensitive · max 64 chars
          </span>
          <div class="mt-2 flex justify-end">
            <Button
              type="submit"
              disabled={creatingFirst || firstProjectName.trim().length === 0}
              class="h-10"
              style="box-shadow: 0 1px 0 hsla(0,0%,100%,0.18) inset, 0 8px 22px hsl(var(--primary) / 0.28);"
            >
              {#if creatingFirst}
                <Loader2 class="h-4 w-4 animate-spin" />
              {:else}
                <Plus class="h-4 w-4" />
              {/if}
              Create project
            </Button>
          </div>
        </form>
        <div
          class="border-border text-muted-foreground/70 mt-5 flex items-center gap-2 border-t pt-4 text-[11px]"
        >
          <span>next</span>
          <span class="bg-muted-foreground/50 h-1 w-1 rounded-full" aria-hidden="true"></span>
          <span>copy your DSN</span>
          <span class="bg-muted-foreground/50 h-1 w-1 rounded-full" aria-hidden="true"></span>
          <span>send a test event</span>
        </div>
      </Card.Root>
    </div>
  {:else}
    <ProjectRail {activeName} />
    <main class="min-w-0 flex-1">
      {@render children?.()}
    </main>
  {/if}
</div>
