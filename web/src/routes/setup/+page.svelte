<script lang="ts">
  import {
    AlertCircle,
    AlertTriangle,
    ArrowRight,
    KeyRound,
    Loader2,
    UserPlus
  } from 'lucide-svelte';
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { api, HttpError } from '$lib/api';
  import { auth } from '$lib/auth.svelte';
  import AuthShell from '$lib/components/AuthShell.svelte';
  import Stepper from '$lib/components/Stepper.svelte';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';

  type Step = 'token' | 'user';

  let step = $state<Step>('token');
  let token = $state('');
  let username = $state('');
  let password = $state('');
  let passwordConfirm = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);
  /** True iff the daemon is in a state where setup makes sense. Other
   *  states: setup_disabled (no env token; show docs), or already-set-up
   *  (redirect to /login). */
  let pageState = $state<'loading' | 'ready' | 'disabled' | 'done'>('loading');

  onMount(async () => {
    try {
      const status = await api.auth.setupStatus();
      if (status.setup_disabled) {
        pageState = 'disabled';
      } else if (!status.needs_setup) {
        pageState = 'done';
        void goto('/login', { replaceState: true });
      } else {
        pageState = 'ready';
      }
    } catch {
      pageState = 'ready';
    }
  });

  function advanceFromToken(e: SubmitEvent) {
    e.preventDefault();
    if (token.trim().length === 0) return;
    error = null;
    step = 'user';
  }

  async function completeSetup(e: SubmitEvent) {
    e.preventDefault();
    if (password !== passwordConfirm) {
      error = 'passwords do not match';
      return;
    }
    busy = true;
    error = null;
    try {
      await auth.setup(token.trim(), username.trim(), password);
      void goto('/', { replaceState: true });
    } catch (err) {
      if (err instanceof HttpError) {
        if (err.status === 401) {
          // Wrong token: kick the operator back to step 1 so they can paste
          // the right one without losing the username they typed.
          step = 'token';
          error = 'invalid setup token — paste the value of ERREXD_ADMIN_TOKEN';
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

  // Title/subtitle vary per step. Step 1 keeps the "welcome to errex" framing
  // because the operator hasn't proven host access yet. Step 2 switches to
  // "create your account" because the wizard now talks about the user, not
  // the daemon.
  const title = $derived(step === 'user' ? 'create your account' : 'welcome to errex');
  const subtitle = $derived(
    step === 'user'
      ? "you'll use this to sign in from now on"
      : 'set up your first operator account'
  );
</script>

<svelte:head>
  <title>Set up · errex</title>
</svelte:head>

<AuthShell {title} {subtitle}>
  {#if pageState === 'loading'}
    <div class="text-muted-foreground flex items-center justify-center gap-2 py-6 text-[12px]">
      <Loader2 class="h-4 w-4 animate-spin" /> checking daemon…
    </div>
  {:else if pageState === 'disabled'}
    <div
      role="alert"
      class="flex items-start gap-2 rounded-md border p-2.5 text-[11px] leading-relaxed"
      style="border-color: hsla(38,92%,56%,0.35); background: hsla(38,92%,56%,0.07);"
    >
      <AlertTriangle class="mt-0.5 h-3.5 w-3.5 shrink-0" style="color: hsl(38 92% 70%);" />
      <div>
        setup is disabled — daemon was started without
        <code class="bg-muted/40 mx-1 rounded px-1.5 py-0.5 font-mono text-[10.5px]">
          ERREXD_ADMIN_TOKEN
        </code>
        . set the env var and restart.
      </div>
    </div>
  {:else if pageState === 'done'}
    <div class="text-muted-foreground flex items-center justify-center gap-2 py-6 text-[12px]">
      <Loader2 class="h-4 w-4 animate-spin" /> setup already complete — sending you to /login…
    </div>
  {:else if step === 'token'}
    <!-- Step 1: setup token -->
    <Stepper current={1} labels={['verify host access', 'create account']} />
    <p class="text-muted-foreground text-[11.5px]">
      paste
      <code class="bg-muted/40 mx-1 rounded px-1.5 py-0.5 font-mono text-[10.5px]">
        ERREXD_ADMIN_TOKEN
      </code>
      from the daemon environment. proves you have host access.
    </p>
    <form onsubmit={advanceFromToken} class="flex flex-col gap-[14px]">
      <div class="flex flex-col gap-[5px]">
        <Label for="token" class="text-[11px]">setup token</Label>
        <Input
          id="token"
          type="password"
          bind:value={token}
          autocomplete="off"
          autofocus
          class="h-10 font-mono text-[12px]"
        />
      </div>
      {#if error}
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
        disabled={token.trim().length === 0}
        class="h-10 w-full"
        style="box-shadow: 0 1px 0 hsla(0,0%,100%,0.18) inset, 0 8px 22px hsl(var(--primary) / 0.28);"
      >
        <KeyRound class="h-4 w-4" />
        continue
        <ArrowRight class="h-3.5 w-3.5" />
      </Button>
    </form>
  {:else}
    <!-- Step 2: user creation -->
    <Stepper current={2} labels={['verify host access', 'create account']} />
    <form onsubmit={completeSetup} class="flex flex-col gap-[14px]">
      <div class="flex flex-col gap-[5px]">
        <Label for="u" class="text-[11px]">username</Label>
        <Input
          id="u"
          bind:value={username}
          autocomplete="username"
          autofocus
          class="h-10 text-[13px]"
          disabled={busy}
        />
      </div>
      <div class="flex flex-col gap-[5px]">
        <Label for="p" class="text-[11px]">password (min 12 chars)</Label>
        <Input
          id="p"
          type="password"
          bind:value={password}
          autocomplete="new-password"
          class="h-10 text-[13px]"
          disabled={busy}
        />
      </div>
      <div class="flex flex-col gap-[5px]">
        <Label for="pc" class="text-[11px]">confirm password</Label>
        <Input
          id="pc"
          type="password"
          bind:value={passwordConfirm}
          autocomplete="new-password"
          class="h-10 text-[13px]"
          disabled={busy}
        />
      </div>
      {#if error}
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
        disabled={busy || username.trim().length === 0 || password.length < 12}
        class="h-10 w-full"
        style="box-shadow: 0 1px 0 hsla(0,0%,100%,0.18) inset, 0 8px 22px hsl(var(--primary) / 0.28);"
      >
        {#if busy}
          <Loader2 class="h-4 w-4 animate-spin" />
        {:else}
          <UserPlus class="h-4 w-4" />
        {/if}
        create admin
        <ArrowRight class="h-3.5 w-3.5" />
      </Button>
    </form>
  {/if}
</AuthShell>
