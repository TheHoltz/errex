<script lang="ts">
  import { Loader2, Plus, Shield, UserPlus } from 'lucide-svelte';
  import { goto } from '$app/navigation';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import { Select } from '$lib/components/ui/select';
  import { team } from '$lib/team.svelte';
  import { toast } from '$lib/toast.svelte';
  import { cn, relativeTime } from '$lib/utils';
  import type { Role } from '$lib/api';

  type Props = { activeUsername: string | null };
  let { activeUsername }: Props = $props();

  let filter = $state('');

  const filtered = $derived.by(() => {
    const needle = filter.trim().toLowerCase();
    const list = team.users.slice().sort((a, b) => a.username.localeCompare(b.username));
    if (!needle) return list;
    return list.filter((u) => u.username.toLowerCase().includes(needle));
  });

  // ----- new user form -----
  let newUsername = $state('');
  let newPassword = $state('');
  let newRole = $state<Role>('viewer');
  let creating = $state(false);
  let createOpen = $state(false);

  async function createUser(e: SubmitEvent) {
    e.preventDefault();
    const u = newUsername.trim();
    if (u.length === 0 || newPassword.length < 12) return;
    creating = true;
    try {
      await team.createUser(u, newPassword, newRole);
      toast.success(`Created user "${u}"`);
      newUsername = '';
      newPassword = '';
      newRole = 'viewer';
      createOpen = false;
      void goto(`/team/${encodeURIComponent(u)}`);
    } catch (err) {
      toast.error('Failed to create user', { description: String(err) });
    } finally {
      creating = false;
    }
  }
</script>

<aside class="border-border bg-background flex w-[280px] shrink-0 flex-col border-r">
  <div class="border-border border-b p-3">
    <Input
      bind:value={filter}
      placeholder="filter users…"
      autocomplete="off"
      class="h-9 text-[12.5px]"
      aria-label="Filter users"
    />
  </div>

  <ul class="flex-1 overflow-y-auto" aria-label="Users">
    {#each filtered as u (u.username)}
      {@const isActive = u.username === activeUsername}
      <li>
        <a
          href={`/team/${encodeURIComponent(u.username)}`}
          class={cn(
            'border-border hover:bg-accent/40 flex flex-col gap-1 border-b px-4 py-3 transition-colors',
            isActive && 'bg-accent/60 border-l-2 border-l-emerald-500 pl-[14px]'
          )}
          aria-current={isActive ? 'page' : undefined}
        >
          <span class="flex items-center gap-2">
            <span
              class={cn(
                'h-2 w-2 shrink-0 rounded-full',
                u.deactivated_at ? 'bg-muted-foreground/40' : 'bg-emerald-500'
              )}
              title={u.deactivated_at ? 'deactivated' : 'active'}
              aria-hidden="true"
            ></span>
            <span class="truncate text-[13px] font-medium">{u.username}</span>
            {#if u.role === 'admin'}
              <Shield
                class="text-amber-400 ml-auto h-3.5 w-3.5 shrink-0"
                aria-label="admin"
              />
            {/if}
          </span>
          <span class="text-muted-foreground text-[11px]">
            {#if u.last_login_at}
              last seen {relativeTime(u.last_login_at)}
            {:else}
              never signed in
            {/if}
          </span>
        </a>
      </li>
    {/each}

    {#if filtered.length === 0 && team.users.length > 0}
      <li class="text-muted-foreground px-4 py-6 text-center text-[12px]">
        No users match "<span class="font-mono">{filter}</span>"
      </li>
    {/if}
  </ul>

  <div class="border-border bg-muted/30 border-t p-3">
    {#if createOpen}
      <form onsubmit={createUser} class="flex flex-col gap-2">
        <div class="flex flex-col gap-1">
          <Label for="nu" class="text-[11px]">Username</Label>
          <Input id="nu" bind:value={newUsername} autocomplete="off" class="h-8 text-[12px]" />
        </div>
        <div class="flex flex-col gap-1">
          <Label for="np" class="text-[11px]">Password (≥12 chars)</Label>
          <Input
            id="np"
            type="password"
            bind:value={newPassword}
            autocomplete="new-password"
            class="h-8 text-[12px]"
          />
        </div>
        <div class="flex flex-col gap-1">
          <Label for="nr" class="text-[11px]">Role</Label>
          <Select
            bind:value={newRole}
            options={[
              { value: 'viewer', label: 'viewer' },
              { value: 'admin', label: 'admin' }
            ]}
            class="h-8 text-[12px]"
          />
        </div>
        <div class="flex items-center justify-end gap-2 pt-1">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onclick={() => (createOpen = false)}
            disabled={creating}
            class="h-8"
          >
            Cancel
          </Button>
          <Button
            type="submit"
            size="sm"
            disabled={creating || newUsername.trim().length === 0 || newPassword.length < 12}
            class="h-8"
          >
            {#if creating}
              <Loader2 class="h-3 w-3 animate-spin" />
            {:else}
              <Plus class="h-3 w-3" />
            {/if}
            Create
          </Button>
        </div>
      </form>
    {:else}
      <Button
        variant="outline"
        size="sm"
        onclick={() => (createOpen = true)}
        class="h-9 w-full"
      >
        <UserPlus class="h-3.5 w-3.5" /> New user
      </Button>
    {/if}
  </div>
</aside>
