<script lang="ts">
  import '../app.css';

  import { onDestroy, onMount, setContext } from 'svelte';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import { AlertCircle, LogOut, Search, Settings, Users } from 'lucide-svelte';
  import { actions } from '$lib/actions.svelte';
  import { auth } from '$lib/auth.svelte';
  import { bootstrapSignedIn } from '$lib/bootstrap';
  import CommandPalette from '$lib/components/CommandPalette.svelte';
  import ConnectionStatus from '$lib/components/ConnectionStatus.svelte';
  import Freshness from '$lib/components/Freshness.svelte';
  import HeaderStats from '$lib/components/HeaderStats.svelte';
  import KeyboardShortcuts from '$lib/components/KeyboardShortcuts.svelte';
  import ProjectSelector from '$lib/components/ProjectSelector.svelte';
  import Toaster from '$lib/components/Toaster.svelte';
  import { Button } from '$lib/components/ui/button';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { toast } from '$lib/toast.svelte';
  import { cn } from '$lib/utils';
  import { disconnect } from '$lib/ws';

  let { children } = $props();

  let paletteOpen = $state(false);
  const filterRef: { current: HTMLInputElement | null } = $state({ current: null });
  setContext('filterRef', filterRef);
  setContext('openProjectSettings', () => goto('/projects'));

  // Routes that bypass the standard chrome (sidebar + topbar). Login and
  // setup are full-screen by design — the operator hasn't been
  // authenticated yet, so showing the regular UI shell would be
  // misleading.
  const path = $derived($page.url.pathname);
  const onAuthRoute = $derived(path === '/login' || path === '/setup');
  const onProjectsRoute = $derived(path.startsWith('/projects'));
  const onTeamRoute = $derived(path.startsWith('/team'));

  // Auth-gate effect: keep URL and `auth.status` in sync. Runs every time
  // either the route OR the auth state changes; the dual-direction redirect
  // converges fast (and the early-return guards prevent loops).
  $effect(() => {
    if (auth.status === 'unknown') return; // wait for hydrate

    if (auth.status === 'signed_out' && !onAuthRoute) {
      // First-run UX: if the daemon has zero users (and bootstrap is
      // enabled), send the visitor straight to /setup. Otherwise route to
      // /login with a `?next` so they come back to where they were after
      // signing in. Direct routing avoids the /login→/setup flash that
      // happens when /login has to discover needs_setup itself.
      if (auth.needsSetup) {
        void goto('/setup', { replaceState: true });
        return;
      }
      const next = path === '/' ? '' : `?next=${encodeURIComponent(path + $page.url.search)}`;
      void goto(`/login${next}`, { replaceState: true });
      return;
    }

    if (auth.status === 'signed_in' && onAuthRoute) {
      void goto('/', { replaceState: true });
      return;
    }

    if (auth.status === 'signed_in' && onTeamRoute && !auth.isAdmin()) {
      toast.error('admin only');
      void goto('/', { replaceState: true });
    }
  });

  onMount(async () => {
    actions.hydrate();
    await auth.hydrate();
    // The bootstrap fires from the $effect below — that path covers both
    // "valid cookie on first paint" and "user signs in within this tab".
  });

  // Run the signed-in bootstrap once per signed-in user. Keying on the
  // username lets a logout → re-login (potentially as a different user)
  // re-fetch projects and reconnect the WS, while a status flap with the
  // same user is a no-op.
  let bootstrappedFor = $state<string | null>(null);
  $effect(() => {
    const username = auth.status === 'signed_in' ? (auth.user?.username ?? null) : null;
    if (username && bootstrappedFor !== username) {
      bootstrappedFor = username;
      void bootstrapSignedIn();
    } else if (auth.status === 'signed_out') {
      bootstrappedFor = null;
    }
  });

  onDestroy(() => disconnect());

  async function signOut() {
    await auth.logout();
    void goto('/login', { replaceState: true });
  }
</script>

<!-- Tooltip.Provider must wrap every children-rendering branch: during
     a setup→/ or login→/ navigation, SvelteKit can swap `children` to the
     new route's +page.svelte before $page.url.pathname propagates, so for
     a frame we render signed-in content under the auth-route branch. If
     Tooltip.Provider lived only inside the signed-in branch, that frame
     would crash with "Context Tooltip.Provider not found" and leave the
     transition wedged. -->
<Tooltip.Provider delayDuration={0}>
  {#if onAuthRoute}
    <!-- Full-screen routes (login/setup) draw their own chrome. -->
    {@render children?.()}
  {:else if auth.status === 'unknown' || auth.status === 'signed_out'}
    <!-- Brief flash while the auth-gate effect routes the user. Showing the
         sign-in card here for a frame is worse than a blank screen. -->
    <div class="bg-background h-screen"></div>
  {:else}
    <div class="flex h-screen">
      <aside
        class="border-border bg-background flex w-14 shrink-0 flex-col items-center gap-3 border-r py-4"
      >
        <a
          href="/"
          class="text-primary inline-flex h-10 w-10 items-center justify-center rounded-md"
          aria-label="errex"
          title="errex"
        >
          <AlertCircle class="h-[18px] w-[18px]" />
        </a>

        <Tooltip.Root>
          <Tooltip.Trigger>
            {#snippet child({ props })}
              <button
                {...props}
                type="button"
                onclick={() => (paletteOpen = true)}
                class="text-muted-foreground hover:text-foreground hover:bg-accent inline-flex h-10 w-10 items-center justify-center rounded-md transition-colors"
                aria-label="Search"
              >
                <Search class="h-[18px] w-[18px]" />
              </button>
            {/snippet}
          </Tooltip.Trigger>
          <Tooltip.Content side="right">
            Search <kbd class="text-muted-foreground ml-1 font-mono text-[10px]">⌘K</kbd>
          </Tooltip.Content>
        </Tooltip.Root>

        <Tooltip.Root>
          <Tooltip.Trigger>
            {#snippet child({ props })}
              <a
                {...props}
                href="/projects"
                class={cn(
                  'text-muted-foreground hover:text-foreground hover:bg-accent inline-flex h-10 w-10 items-center justify-center rounded-md transition-colors',
                  onProjectsRoute && 'bg-accent text-foreground'
                )}
                aria-label="Projects"
              >
                <Settings class="h-[18px] w-[18px]" />
              </a>
            {/snippet}
          </Tooltip.Trigger>
          <Tooltip.Content side="right">Projects · DSN · webhooks</Tooltip.Content>
        </Tooltip.Root>

        {#if auth.isAdmin()}
          <Tooltip.Root>
            <Tooltip.Trigger>
              {#snippet child({ props })}
                <a
                  {...props}
                  href="/team"
                  class={cn(
                    'text-muted-foreground hover:text-foreground hover:bg-accent inline-flex h-10 w-10 items-center justify-center rounded-md transition-colors',
                    onTeamRoute && 'bg-accent text-foreground'
                  )}
                  aria-label="Team"
                >
                  <Users class="h-[18px] w-[18px]" />
                </a>
              {/snippet}
            </Tooltip.Trigger>
            <Tooltip.Content side="right">Team · users · sessions</Tooltip.Content>
          </Tooltip.Root>
        {/if}

        <div class="mt-auto flex flex-col items-center gap-3">
          <Freshness />
          <ConnectionStatus />
        </div>
      </aside>

      <div class="flex min-w-0 flex-1 flex-col">
        <header
          class="border-border bg-background flex h-11 shrink-0 items-center gap-4 border-b px-5"
        >
          {#if onProjectsRoute}
            <span class="text-muted-foreground inline-flex items-center text-[12px] tracking-tight">
              <a href="/" class="hover:text-foreground transition-colors">errex</a>
              <span class="px-1.5 opacity-50">/</span>
              <a href="/projects" class="hover:text-foreground transition-colors">Projects</a>
              {#if $page.params.name}
                <span class="px-1.5 opacity-50">/</span>
                <span class="text-foreground font-mono">{$page.params.name}</span>
              {/if}
            </span>
          {:else if onTeamRoute}
            <span class="text-muted-foreground inline-flex items-center text-[12px] tracking-tight">
              <a href="/" class="hover:text-foreground transition-colors">errex</a>
              <span class="px-1.5 opacity-50">/</span>
              <a href="/team" class="hover:text-foreground transition-colors">Team</a>
              {#if $page.params.username}
                <span class="px-1.5 opacity-50">/</span>
                <span class="text-foreground font-mono">{$page.params.username}</span>
              {/if}
            </span>
          {:else}
            <span class="text-muted-foreground inline-flex items-center text-[12px] tracking-tight">
              <ProjectSelector variant="inline" />
              <span class="px-1.5 opacity-50">/</span>
              Issues
            </span>
            <div class="bg-border h-4 w-px"></div>
            <HeaderStats />
          {/if}

          {#if auth.user}
            <div class="ml-auto flex items-center gap-2">
              <span class="text-muted-foreground text-[12px]">
                <span class="text-foreground font-medium">{auth.user.username}</span>
                <span class="opacity-50"> · </span>
                <span>{auth.user.role}</span>
              </span>
              <Tooltip.Root>
                <Tooltip.Trigger>
                  {#snippet child({ props })}
                    <Button
                      {...props}
                      variant="ghost"
                      size="icon"
                      onclick={signOut}
                      class="h-8 w-8"
                      aria-label="Sign out"
                    >
                      <LogOut class="h-3.5 w-3.5" />
                    </Button>
                  {/snippet}
                </Tooltip.Trigger>
                <Tooltip.Content side="bottom">Sign out</Tooltip.Content>
              </Tooltip.Root>
            </div>
          {/if}
        </header>

        <main class="min-h-0 flex-1 overflow-y-auto">
          {@render children?.()}
        </main>
      </div>
    </div>
  {/if}
</Tooltip.Provider>

<Toaster />
<CommandPalette open={paletteOpen} onClose={() => (paletteOpen = false)} />
<KeyboardShortcuts
  onOpenPalette={() => (paletteOpen = true)}
  onFocusFilter={() => filterRef.current?.focus()}
/>
