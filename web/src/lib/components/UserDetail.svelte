<script lang="ts">
  import {
    Check,
    Loader2,
    LogOut,
    Power,
    PowerOff,
    Shield,
    Trash2
  } from 'lucide-svelte';
  import { goto } from '$app/navigation';
  import { auth } from '$lib/auth.svelte';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import { Dialog } from '$lib/components/ui/dialog';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import { Select } from '$lib/components/ui/select';
  import { Separator } from '$lib/components/ui/separator';
  import { team } from '$lib/team.svelte';
  import { toast } from '$lib/toast.svelte';
  import { cn, relativeTime } from '$lib/utils';
  import type { Role, User, UserSession } from '$lib/api';

  type Props = { user: User };
  let { user }: Props = $props();

  // ----- sessions -----
  let sessions = $state<UserSession[]>([]);
  let sessionsError = $state<string | null>(null);

  let lastSeenName = $state<string | null>(null);
  $effect(() => {
    if (user.username !== lastSeenName) {
      lastSeenName = user.username;
      sessions = [];
      sessionsError = null;
      void refreshSessions();
    }
  });

  async function refreshSessions() {
    try {
      sessions = await team.listUserSessions(user.username);
      sessionsError = null;
    } catch (err) {
      sessionsError = String(err);
    }
  }

  // ----- role change -----
  // Initialise empty and let the $effect populate from the prop on mount
  // AND on every prop change. Initialising directly from props would
  // capture only the first value (svelte/state_referenced_locally).
  let role = $state<Role>('viewer');
  $effect(() => {
    role = user.role; // re-sync when switching users
  });
  let savingRole = $state(false);
  async function saveRole() {
    if (role === user.role) return;
    savingRole = true;
    try {
      await team.updateUser(user.username, { role });
      toast.success(`${user.username} is now ${role}`);
    } catch (err) {
      role = user.role; // rollback dropdown
      toast.error('Failed to change role', { description: String(err) });
    } finally {
      savingRole = false;
    }
  }

  // ----- password change -----
  let pwOpen = $state(false);
  let newPw = $state('');
  let confirmPw = $state('');
  let pwBusy = $state(false);
  let pwError = $state<string | null>(null);
  function openPw() {
    newPw = '';
    confirmPw = '';
    pwError = null;
    pwOpen = true;
  }
  async function savePassword() {
    if (newPw.length < 12) {
      pwError = 'password must be at least 12 characters';
      return;
    }
    if (newPw !== confirmPw) {
      pwError = 'passwords do not match';
      return;
    }
    pwBusy = true;
    try {
      await team.updateUser(user.username, { password: newPw });
      toast.success('Password updated');
      pwOpen = false;
    } catch (err) {
      pwError = String(err);
    } finally {
      pwBusy = false;
    }
  }

  // ----- deactivate / activate -----
  let togglingActive = $state(false);
  async function toggleActive() {
    togglingActive = true;
    const next = !user.deactivated_at;
    try {
      await team.updateUser(user.username, { deactivated: next });
      toast.success(next ? 'User deactivated' : 'User reactivated');
      await refreshSessions();
    } catch (err) {
      toast.error('Failed to update status', { description: String(err) });
    } finally {
      togglingActive = false;
    }
  }

  // ----- revoke sessions -----
  let revokingAll = $state(false);
  async function revokeAll() {
    revokingAll = true;
    try {
      const n = await team.revokeUserSessions(user.username);
      toast.success(`Revoked ${n} session${n === 1 ? '' : 's'}`);
      await refreshSessions();
    } catch (err) {
      toast.error('Failed to revoke sessions', { description: String(err) });
    } finally {
      revokingAll = false;
    }
  }

  // ----- delete -----
  let delOpen = $state(false);
  let delTyped = $state('');
  let delBusy = $state(false);
  function openDelete() {
    delTyped = '';
    delOpen = true;
  }
  async function performDelete() {
    if (delTyped.trim() !== user.username) return;
    delBusy = true;
    try {
      await team.deleteUser(user.username);
      toast.success(`Deleted user "${user.username}"`);
      delOpen = false;
      void goto('/team', { replaceState: true });
    } catch (err) {
      toast.error('Failed to delete user', { description: String(err) });
    } finally {
      delBusy = false;
    }
  }

  // The "you can't shoot yourself in the foot" guards are also enforced
  // server-side; we surface them in the UI so the buttons aren't a
  // misleading dead end.
  const isSelf = $derived(auth.user?.username === user.username);
</script>

<div class="flex h-full flex-col overflow-y-auto">
  <div class="mx-auto flex w-full max-w-3xl flex-col gap-7 px-7 py-7">
    <!-- ----- Header ----- -->
    <header class="flex flex-col gap-1.5">
      <div class="flex items-center gap-2">
        <h1 class="text-[18px] font-semibold tracking-tight">{user.username}</h1>
        {#if user.role === 'admin'}
          <Badge variant="warning" class="gap-1 rounded-full px-2">
            <Shield class="h-3 w-3" /> admin
          </Badge>
        {:else}
          <Badge variant="secondary" class="rounded-full px-2">viewer</Badge>
        {/if}
        {#if user.deactivated_at}
          <Badge
            variant="destructive"
            class="bg-destructive/10 text-destructive rounded-full px-2"
          >
            deactivated
          </Badge>
        {/if}
        {#if isSelf}
          <span class="text-muted-foreground ml-1 text-[11px]">(you)</span>
        {/if}
      </div>
      <p class="text-muted-foreground text-[12px]">
        joined {relativeTime(user.created_at)}
        {#if user.last_login_at}
          · last sign-in {relativeTime(user.last_login_at)}
          {#if user.last_login_ip}
            from <span class="font-mono">{user.last_login_ip}</span>
          {/if}
        {:else}
          · never signed in
        {/if}
      </p>
    </header>

    <!-- ----- Role ----- -->
    <section class="flex flex-col gap-2">
      <Label class="text-muted-foreground text-[10px] uppercase tracking-wider">Role</Label>
      <div class="flex items-center gap-2">
        <Select
          bind:value={role}
          options={[
            { value: 'viewer', label: 'viewer — can browse, can\'t change' },
            { value: 'admin', label: 'admin — full access' }
          ]}
          class="h-10 flex-1"
          disabled={savingRole}
        />
        <Button onclick={saveRole} disabled={savingRole || role === user.role} class="h-10">
          {#if savingRole}<Loader2 class="h-4 w-4 animate-spin" />{:else}<Check class="h-4 w-4" />{/if}
          Save role
        </Button>
      </div>
    </section>

    <Separator />

    <!-- ----- Password ----- -->
    <section class="flex flex-col gap-2">
      <Label class="text-muted-foreground text-[10px] uppercase tracking-wider">Password</Label>
      <div
        class="border-border bg-card flex items-center justify-between gap-3 rounded-md border p-3"
      >
        <div class="flex flex-col gap-0.5">
          <p class="text-[13px] font-medium">Set a new password</p>
          <p class="text-muted-foreground text-[11px]">
            Existing sessions stay valid; the user's next sign-in uses the new value.
          </p>
        </div>
        <Button variant="outline" onclick={openPw} class="shrink-0">Change password</Button>
      </div>
    </section>

    <Separator />

    <!-- ----- Sessions ----- -->
    <section class="flex flex-col gap-2">
      <div class="flex items-center justify-between">
        <Label class="text-muted-foreground text-[10px] uppercase tracking-wider">
          Active sessions
        </Label>
        <Button
          variant="outline"
          size="sm"
          onclick={revokeAll}
          disabled={revokingAll || sessions.length === 0}
          class="h-8 text-[12px]"
        >
          {#if revokingAll}<Loader2 class="h-3 w-3 animate-spin" />{:else}<LogOut class="h-3 w-3" />{/if}
          Revoke all
        </Button>
      </div>
      {#if sessionsError}
        <p class="text-destructive text-[11px]">{sessionsError}</p>
      {:else if sessions.length === 0}
        <p class="text-muted-foreground text-[12px]">No active sessions.</p>
      {:else}
        <ul class="border-border bg-card divide-y divide-border rounded-md border">
          {#each sessions as s (s.id)}
            <li class="flex items-center gap-3 px-3 py-2 text-[12px]">
              <span class="text-muted-foreground font-mono text-[11px]">
                {s.ip ?? 'unknown'}
              </span>
              <span class="text-muted-foreground truncate">
                {s.user_agent ?? '—'}
              </span>
              <span class="text-muted-foreground ml-auto whitespace-nowrap text-[11px]">
                seen {relativeTime(s.last_seen_at)}
              </span>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <Separator />

    <!-- ----- Danger zone ----- -->
    <section class="flex flex-col gap-3">
      <Label class="text-destructive text-[10px] uppercase tracking-wider">Danger zone</Label>

      <div
        class="border-destructive/30 bg-destructive/5 flex items-center justify-between gap-3 rounded-md border p-3"
      >
        <div class="flex flex-col gap-0.5">
          <p class="text-[13px] font-medium">
            {user.deactivated_at ? 'Reactivate user' : 'Deactivate user'}
          </p>
          <p class="text-muted-foreground text-[11px]">
            {user.deactivated_at
              ? "Allows the user to sign in again. Their existing sessions are still revoked."
              : 'Revokes all sessions and prevents new sign-ins. Reversible.'}
          </p>
        </div>
        <Button
          variant="outline"
          onclick={toggleActive}
          disabled={togglingActive}
          class={cn(
            'shrink-0',
            !user.deactivated_at &&
              'border-destructive/40 text-destructive hover:bg-destructive/10 hover:text-destructive'
          )}
        >
          {#if togglingActive}
            <Loader2 class="h-3.5 w-3.5 animate-spin" />
          {:else if user.deactivated_at}
            <Power class="h-3.5 w-3.5" />
          {:else}
            <PowerOff class="h-3.5 w-3.5" />
          {/if}
          {user.deactivated_at ? 'Reactivate' : 'Deactivate'}
        </Button>
      </div>

      <div
        class="border-destructive/30 bg-destructive/5 flex items-center justify-between gap-3 rounded-md border p-3"
      >
        <div class="flex flex-col gap-0.5">
          <p class="text-[13px] font-medium">Delete user</p>
          <p class="text-muted-foreground text-[11px]">
            Permanently removes the account and revokes every session. Cannot be undone.
          </p>
        </div>
        <Button
          variant="outline"
          onclick={openDelete}
          class="border-destructive/40 text-destructive hover:bg-destructive/10 hover:text-destructive shrink-0"
        >
          <Trash2 class="h-3.5 w-3.5" />
          Delete
        </Button>
      </div>
    </section>
  </div>
</div>

<!-- Password change modal -->
<Dialog open={pwOpen} onClose={() => (pwOpen = false)} class="w-[min(420px,calc(100vw-2rem))]">
  <div class="flex flex-col gap-5 p-6">
    <div class="flex flex-col gap-1.5">
      <h2 class="text-[15px] font-semibold tracking-tight">
        Change password for <span class="font-mono">{user.username}</span>
      </h2>
      <p class="text-muted-foreground text-[12px]">
        Minimum 12 characters. Their existing sessions stay valid.
      </p>
    </div>
    <div class="flex flex-col gap-2">
      <Label for="np2" class="text-[12px]">New password</Label>
      <Input
        id="np2"
        type="password"
        bind:value={newPw}
        autocomplete="new-password"
        class="h-10 text-[13px]"
      />
    </div>
    <div class="flex flex-col gap-2">
      <Label for="cp2" class="text-[12px]">Confirm</Label>
      <Input
        id="cp2"
        type="password"
        bind:value={confirmPw}
        autocomplete="new-password"
        class="h-10 text-[13px]"
      />
    </div>
    {#if pwError}
      <p class="text-destructive text-[12px]">{pwError}</p>
    {/if}
    <div class="flex justify-end gap-2">
      <Button variant="ghost" onclick={() => (pwOpen = false)} disabled={pwBusy}>Cancel</Button>
      <Button onclick={savePassword} disabled={pwBusy || newPw.length < 12}>
        {#if pwBusy}<Loader2 class="h-4 w-4 animate-spin" />{:else}<Check class="h-4 w-4" />{/if}
        Update password
      </Button>
    </div>
  </div>
</Dialog>

<!-- Delete user modal (type-to-confirm) -->
<Dialog open={delOpen} onClose={() => (delOpen = false)} class="w-[min(480px,calc(100vw-2rem))]">
  <div class="flex flex-col gap-5 p-6">
    <div class="flex flex-col gap-1.5">
      <h2 class="text-[15px] font-semibold tracking-tight">
        Delete user <span class="font-mono">{user.username}</span>
      </h2>
      <p class="text-muted-foreground text-[12px]">
        Removes the account and every session. The action cannot be undone.
      </p>
    </div>
    <div class="flex flex-col gap-2">
      <Label for="dt" class="text-[12px]">
        Type <span class="font-mono">{user.username}</span> to confirm
      </Label>
      <Input
        id="dt"
        bind:value={delTyped}
        autocomplete="off"
        placeholder={user.username}
        class="h-10 font-mono text-[13px]"
      />
    </div>
    <div class="flex justify-end gap-2">
      <Button variant="ghost" onclick={() => (delOpen = false)} disabled={delBusy}>Cancel</Button>
      <Button
        onclick={performDelete}
        disabled={delBusy || delTyped.trim() !== user.username}
        class="bg-destructive text-destructive-foreground hover:bg-destructive/90"
      >
        {#if delBusy}<Loader2 class="h-4 w-4 animate-spin" />{:else}<Trash2 class="h-4 w-4" />{/if}
        Delete forever
      </Button>
    </div>
  </div>
</Dialog>
