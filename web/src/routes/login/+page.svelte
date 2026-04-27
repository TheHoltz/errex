<script lang="ts">
  import { AlertCircle, Loader2, Lock, LogIn } from 'lucide-svelte';
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import { api, HttpError } from '$lib/api';
  import { auth } from '$lib/auth.svelte';
  import AuthShell from '$lib/components/AuthShell.svelte';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';

  let username = $state('');
  let password = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);

  // Lockout countdown — populated from `auth.lockoutUntilEpoch` if a 429
  // came back. The interval reads `Date.now()` each tick so it counts
  // down monotonically without us needing to store the deadline twice.
  let nowTick = $state(Date.now());
  $effect(() => {
    const id = setInterval(() => (nowTick = Date.now()), 1000);
    return () => clearInterval(id);
  });
  const lockedSecs = $derived(
    Math.max(0, Math.ceil((auth.lockoutUntilEpoch - nowTick) / 1000))
  );
  const isLocked = $derived(lockedSecs > 0);

  // If the daemon needs setup (no users + token configured), bounce
  // operators here straight to the wizard. Same UI element either way for
  // anyone who lands at /login by default.
  onMount(async () => {
    try {
      const status = await api.auth.setupStatus();
      if (status.needs_setup) {
        void goto('/setup', { replaceState: true });
      }
    } catch {
      // Daemon unreachable — leave the form up so they can retry.
    }
  });

  // Where to send them after a successful login. Honors ?next= so a
  // protected route can deep-link back after auth, but only if the target
  // is a same-origin path (rejects http://attacker.com).
  const next = $derived.by(() => {
    const raw = $page.url.searchParams.get('next');
    if (!raw || !raw.startsWith('/') || raw.startsWith('//')) return '/';
    return raw;
  });

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (busy || isLocked) return;
    error = null;
    busy = true;
    try {
      await auth.login(username.trim(), password);
      void goto(next, { replaceState: true });
    } catch (err) {
      if (err instanceof HttpError) {
        if (err.status === 429) {
          error = 'too many attempts — wait for the timer to reset';
        } else if (err.status === 401) {
          error = 'wrong username or password';
        } else {
          error = err.message;
        }
      } else {
        error = String(err);
      }
    } finally {
      busy = false;
    }
  }

  function pad(n: number): string {
    return n.toString().padStart(2, '0');
  }
  const lockedText = $derived(
    isLocked ? `${pad(Math.floor(lockedSecs / 60))}:${pad(lockedSecs % 60)}` : ''
  );
</script>

<svelte:head>
  <title>Sign in · errex</title>
</svelte:head>

<AuthShell title="sign in to errex" subtitle="self-hosted error tracking">
  <form onsubmit={submit} class="flex flex-col gap-[14px]">
    {#if isLocked}
      <div
        role="alert"
        class="flex items-start gap-2 rounded-md border p-2.5 text-[11px] leading-relaxed"
        style="border-color: hsl(var(--destructive) / 0.32); background: hsl(var(--destructive) / 0.08);"
      >
        <Lock class="mt-0.5 h-3.5 w-3.5 shrink-0" style="color: hsl(var(--destructive));" />
        <div>
          locked out — too many attempts. try again in
          <span class="font-mono tabular-nums font-semibold">{lockedText}</span>.
        </div>
      </div>
    {/if}

    <div class="flex flex-col gap-[5px]">
      <Label for="u" class="text-[11px]">username</Label>
      <Input
        id="u"
        bind:value={username}
        autocomplete="username"
        autofocus
        class="h-10 text-[13px]"
        disabled={busy || isLocked}
      />
    </div>
    <div class="flex flex-col gap-[5px]">
      <Label for="p" class="text-[11px]">password</Label>
      <Input
        id="p"
        type="password"
        bind:value={password}
        autocomplete="current-password"
        class="h-10 text-[13px]"
        disabled={busy || isLocked}
      />
    </div>

    {#if error && !isLocked}
      <div
        role="alert"
        class="flex items-start gap-2 rounded-md border p-2 text-[11px] leading-relaxed"
        style="border-color: hsl(var(--destructive) / 0.32); background: hsl(var(--destructive) / 0.08);"
      >
        <AlertCircle class="mt-0.5 h-3.5 w-3.5 shrink-0" style="color: hsl(var(--destructive));" />
        <div>{error}</div>
      </div>
    {/if}

    <Button
      type="submit"
      disabled={busy || isLocked || username.trim().length === 0 || password.length === 0}
      class="h-10 w-full"
      style="box-shadow: 0 1px 0 hsla(0,0%,100%,0.18) inset, 0 8px 22px hsl(var(--primary) / 0.28);"
    >
      {#if busy}
        <Loader2 class="h-4 w-4 animate-spin" />
      {:else}
        <LogIn class="h-4 w-4" />
      {/if}
      sign in
    </Button>
  </form>
</AuthShell>
