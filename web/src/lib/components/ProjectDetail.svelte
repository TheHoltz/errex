<script lang="ts">
  import {
    Check,
    ClipboardCopy,
    Loader2,
    Pencil,
    RotateCcw,
    Send,
    Trash2,
    X
  } from 'lucide-svelte';
  import { goto } from '$app/navigation';
  import { admin } from '$lib/admin.svelte';
  import DeleteProjectModal from '$lib/components/DeleteProjectModal.svelte';
  import DsnSnippet from '$lib/components/DsnSnippet.svelte';
  import Sparkline from '$lib/components/Sparkline.svelte';
  import { eventStream } from '$lib/eventStream.svelte';
  import { projects } from '$lib/stores.svelte';
  import { Button } from '$lib/components/ui/button';
  import { Dialog } from '$lib/components/ui/dialog';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import { Separator } from '$lib/components/ui/separator';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import {
    buildTestEventCurl,
    formatWebhookHealth,
    projectActivityStatus,
    validateNewProjectName
  } from '$lib/projectsConsole';
  import { sendTestEvent } from '$lib/testEvent';
  import { toast } from '$lib/toast.svelte';
  import { cn, relativeTime } from '$lib/utils';
  import type { ActivityStats, AdminProject } from '$lib/api';

  // How long the "Sent" affordance lingers before reverting to "Send test event".
  const SENT_REVERT_MS = 2000;

  type Props = { project: AdminProject };
  let { project }: Props = $props();

  let stats = $state<ActivityStats | null>(null);
  let statsError = $state<string | null>(null);
  // Empty initial value — the $effect below populates from `project.webhook_url`
  // on mount AND on project switch. Initialising from props directly would
  // capture only the first value (svelte/state_referenced_locally).
  let webhookDraft = $state('');
  let savingWebhook = $state(false);
  let testingWebhook = $state(false);
  let testStatus = $state<'idle' | 'sending' | 'sent'>('idle');
  let revertHandle = $state<ReturnType<typeof setTimeout> | null>(null);

  // Reset draft + reload activity whenever the active project changes.
  // Without this, switching from "alpha" to "beta" in the rail would keep
  // showing alpha's webhook draft and stats until the user did something.
  let lastSeenName = $state<string | null>(null);
  $effect(() => {
    if (project.name !== lastSeenName) {
      lastSeenName = project.name;
      webhookDraft = project.webhook_url ?? '';
      stats = null;
      statsError = null;
      testStatus = 'idle';
      if (revertHandle) {
        clearTimeout(revertHandle);
        revertHandle = null;
      }
      void refreshActivity();
    }
  });

  // Mirror the `pendingRefetch` cleanup pattern below: a dedicated $effect
  // tracks revertHandle so the timer is killed on component unmount —
  // otherwise navigating away mid-flight leaks a phantom 2s timer that
  // holds a closure reference to the destroyed reactive scope.
  $effect(() => {
    const h = revertHandle;
    return () => {
      if (h) clearTimeout(h);
    };
  });

  async function refreshActivity() {
    try {
      const next = await admin.getActivity(project.name);
      stats = next;
      statsError = null;
    } catch (err) {
      statsError = String(err);
    }
  }

  // Refresh activity whenever the WS surfaces a new event for THIS project.
  // The eventStream's `lastAt` advances on every IssueCreated/IssueUpdated
  // for whatever project the WS is currently connected to, so we gate on
  // `projects.current === project.name` to avoid noise from a stale socket.
  // Debounced to once per second so an event burst doesn't hammer the
  // activity endpoint — the sparkline only ticks at 1h granularity anyway.
  //
  // `lastRefetchAt` and `pendingRefetch` are deliberately plain `let` (not
  // `$state`): if they were reactive, the effect's read+write of
  // `pendingRefetch` inside the same flush would feed back into its own
  // dep set and trip `effect_update_depth_exceeded` once `admin.bumpUsed`
  // started churning the `project` prop reference on every WS tick.
  let lastRefetchAt = 0;
  let pendingRefetch: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    // Touch the dependencies so Svelte tracks them.
    const at = eventStream.lastAt;
    if (!at) return;
    if (projects.current !== project.name) return;
    const wait = Math.max(0, 1000 - (Date.now() - lastRefetchAt));
    if (pendingRefetch) clearTimeout(pendingRefetch);
    pendingRefetch = setTimeout(() => {
      lastRefetchAt = Date.now();
      pendingRefetch = null;
      void refreshActivity();
    }, wait);
    return () => {
      if (pendingRefetch) clearTimeout(pendingRefetch);
    };
  });

  const status = $derived(projectActivityStatus(project.last_used_at));
  const wh = $derived(
    formatWebhookHealth(project.last_webhook_status, project.last_webhook_at)
  );
  const sparkValues = $derived(stats?.hourly_buckets ?? new Array(24).fill(0));

  // ----- rename ----------------------------------------------------------
  let renaming = $state(false);
  let renameDraft = $state('');
  let renameBusy = $state(false);
  let renameError = $state<string | null>(null);

  function startRename() {
    renameDraft = project.name;
    renameError = null;
    renaming = true;
  }
  function cancelRename() {
    renaming = false;
    renameError = null;
  }
  async function commitRename() {
    const v = validateNewProjectName(renameDraft, project.name);
    if (!v.ok) {
      renameError = v.reason;
      return;
    }
    renameBusy = true;
    try {
      const renamed = await admin.renameProject(project.name, renameDraft.trim());
      renaming = false;
      toast.success(`Renamed to "${renamed.name}"`);
      void goto(`/projects/${encodeURIComponent(renamed.name)}`, { replaceState: true });
    } catch (err) {
      renameError = String(err);
    } finally {
      renameBusy = false;
    }
  }

  // ----- webhook ---------------------------------------------------------
  async function saveWebhook() {
    const url = webhookDraft.trim();
    savingWebhook = true;
    try {
      await admin.setWebhook(project.name, url || null);
      toast.success(url ? 'Webhook saved' : 'Webhook removed');
    } catch (err) {
      toast.error('Failed to save webhook', { description: String(err) });
    } finally {
      savingWebhook = false;
    }
  }

  async function testWebhook() {
    const url = webhookDraft.trim() || project.webhook_url || '';
    if (!url) {
      toast.error('Save a webhook URL first.');
      return;
    }
    testingWebhook = true;
    try {
      const body = {
        text: `errex test event for project "${project.name}". If you can read this, your webhook is wired up.`,
        attachments: [
          {
            color: 'good',
            fields: [
              { title: 'Project', value: project.name, short: true },
              { title: 'Source', value: 'errex (test)', short: true }
            ]
          }
        ]
      };
      const res = await fetch(url, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(body)
      });
      if (!res.ok) {
        toast.error(`Webhook returned ${res.status}`, {
          description: (await res.text()).slice(0, 140) || res.statusText
        });
        return;
      }
      toast.success('Test event delivered');
    } catch (err) {
      toast.error('Network error', { description: String(err) });
    } finally {
      testingWebhook = false;
    }
  }

  async function sendTest() {
    // Capture the project name at click time. If the user switches projects
    // while the fetch is in flight, the in-flight result must NOT mutate
    // testStatus on the now-active project — that would flash "Sent" on
    // the wrong project's button.
    const expectedProject = project.name;
    testStatus = 'sending';
    if (revertHandle) {
      clearTimeout(revertHandle);
      revertHandle = null;
    }
    const result = await sendTestEvent(project.dsn);
    if (project.name !== expectedProject) return;
    if (result.kind === 'ok') {
      testStatus = 'sent';
      revertHandle = setTimeout(() => {
        testStatus = 'idle';
        revertHandle = null;
      }, SENT_REVERT_MS);
      return;
    }
    testStatus = 'idle';
    if (result.kind === 'http') {
      toast.error(`Ingest returned ${result.status}`, {
        description: result.body || `HTTP ${result.status}`,
      });
    } else if (result.kind === 'blocked') {
      toast.error('Request blocked by browser', {
        description:
          'An ad blocker (uBlock Origin, etc.) blocked the test event. Disable it for this site, or use “or copy as curl” instead.',
      });
    } else {
      toast.error('Network error', { description: String(result.error) });
    }
  }

  // ----- rotate ----------------------------------------------------------
  let rotateOpen = $state(false);
  let rotatedDsn = $state<string | null>(null);
  let rotateBusy = $state(false);
  let rotatedCopied = $state(false);

  function openRotate() {
    rotateOpen = true;
    rotatedDsn = null;
    rotatedCopied = false;
  }
  function closeRotate() {
    rotateOpen = false;
    rotatedDsn = null;
    rotatedCopied = false;
  }
  async function performRotate() {
    rotateBusy = true;
    try {
      const next = await admin.rotateToken(project.name);
      rotatedDsn = next.dsn;
    } catch (err) {
      toast.error('Failed to rotate', { description: String(err) });
      closeRotate();
    } finally {
      rotateBusy = false;
    }
  }
  async function copyRotatedDsn() {
    if (!rotatedDsn) return;
    try {
      await navigator.clipboard.writeText(rotatedDsn);
      rotatedCopied = true;
      toast.success('New DSN copied');
    } catch (err) {
      toast.error('Could not copy', { description: String(err) });
    }
  }

  // ----- delete ----------------------------------------------------------
  let deleteOpen = $state(false);

  async function copyCurl() {
    const cmd = buildTestEventCurl(project.dsn);
    try {
      await navigator.clipboard.writeText(cmd);
      toast.success('Test command copied');
    } catch (err) {
      toast.error('Could not copy', { description: String(err) });
    }
  }
</script>

<div class="flex h-full flex-col overflow-y-auto">
  <div class="mx-auto flex w-full max-w-3xl flex-col gap-7 px-7 py-7">
    <!-- ----- Header strip ----- -->
    <header class="flex flex-col gap-1.5">
      <div class="flex items-center gap-2">
        {#if renaming}
          <form
            class="flex flex-1 items-center gap-2"
            onsubmit={(e) => {
              e.preventDefault();
              void commitRename();
            }}
          >
            <Input
              bind:value={renameDraft}
              autocomplete="off"
              autofocus
              aria-label="New project name"
              class="h-9 max-w-[280px] font-mono text-[14px]"
            />
            <Button type="submit" size="sm" disabled={renameBusy} class="h-9">
              {#if renameBusy}
                <Loader2 class="h-3.5 w-3.5 animate-spin" />
              {:else}
                <Check class="h-3.5 w-3.5" />
              {/if}
              Save
            </Button>
            <Button type="button" variant="ghost" size="sm" onclick={cancelRename} class="h-9">
              <X class="h-3.5 w-3.5" />
            </Button>
          </form>
        {:else}
          <h1 class="font-mono text-[18px] font-semibold tracking-tight">{project.name}</h1>
          <span
            class={cn('h-2 w-2 rounded-full', status.tone)}
            title={status.label}
            aria-label={`status: ${status.label}`}
          ></span>
          <Tooltip.Root>
            <Tooltip.Trigger>
              {#snippet child({ props })}
                <Button
                  {...props}
                  variant="ghost"
                  size="icon"
                  class="text-muted-foreground hover:text-foreground ml-1 h-7 w-7"
                  onclick={startRename}
                  aria-label="Rename project"
                >
                  <Pencil class="h-3.5 w-3.5" />
                </Button>
              {/snippet}
            </Tooltip.Trigger>
            <Tooltip.Content side="bottom">Rename project</Tooltip.Content>
          </Tooltip.Root>
        {/if}
      </div>
      {#if renameError}
        <p class="text-destructive text-[12px]">{renameError}</p>
      {/if}
      <p class="text-muted-foreground text-[12px]">
        created {relativeTime(project.created_at)}
        {#if project.last_used_at}
          · last event {relativeTime(project.last_used_at)}
        {:else}
          · no events yet
        {/if}
      </p>
    </header>

    <!-- ----- Activity ----- -->
    <section class="flex flex-col gap-3" aria-labelledby="lbl-activity">
      <Label id="lbl-activity" class="text-muted-foreground text-[10px] uppercase tracking-wider">
        Activity · last 24h
      </Label>
      <div class="grid grid-cols-3 gap-3">
        <div class="border-border bg-card rounded-md border px-3 py-2.5">
          <div class="text-[18px] font-semibold tabular-nums">
            {stats ? stats.events_24h.toLocaleString() : '—'}
          </div>
          <div class="text-muted-foreground text-[11px]">events 24h</div>
        </div>
        <div class="border-border bg-card rounded-md border px-3 py-2.5">
          <div class="text-[18px] font-semibold tabular-nums">
            {#if stats?.last_event_at}{relativeTime(stats.last_event_at)}{:else}—{/if}
          </div>
          <div class="text-muted-foreground text-[11px]">last seen</div>
        </div>
        <div class="border-border bg-card rounded-md border px-3 py-2.5">
          <div class="text-[18px] font-semibold tabular-nums">
            {stats ? stats.unique_issues_24h.toLocaleString() : '—'}
          </div>
          <div class="text-muted-foreground text-[11px]">unique issues</div>
        </div>
      </div>
      <div class="border-border bg-card rounded-md border p-3">
        <Sparkline values={sparkValues} width={520} height={48} accent class="w-full" />
      </div>
      {#if statsError}
        <p class="text-destructive text-[11px]">{statsError}</p>
      {/if}
    </section>

    <Separator />

    <!-- ----- Connection ----- -->
    <section class="flex flex-col gap-2" aria-labelledby="lbl-connection">
      <Label id="lbl-connection" class="text-muted-foreground text-[10px] uppercase tracking-wider">
        Connection · DSN
      </Label>
      <DsnSnippet dsn={project.dsn} label={`DSN for ${project.name}`} />
      <div class="flex flex-row items-center gap-3">
        <Button
          variant="outline"
          size="sm"
          onclick={sendTest}
          disabled={testStatus !== 'idle'}
          aria-label="Send a test event to this project's ingest endpoint"
          class={cn(testStatus === 'sent' && 'text-emerald-500 hover:text-emerald-500')}
        >
          {#if testStatus === 'sending'}
            <Loader2 class="h-3.5 w-3.5 animate-spin" />
            Sending…
          {:else if testStatus === 'sent'}
            <Check class="h-3.5 w-3.5" />
            Sent
          {:else}
            <Send class="h-3.5 w-3.5" />
            Send test event
          {/if}
        </Button>
        <Button
          variant="link"
          size="sm"
          onclick={copyCurl}
          class="text-muted-foreground hover:text-foreground h-auto gap-1.5 px-0 text-[11px] hover:no-underline"
        >
          <ClipboardCopy class="h-3 w-3" />
          or copy as curl
        </Button>
      </div>
    </section>

    <Separator />

    <!-- ----- Webhook ----- -->
    <section class="flex flex-col gap-2" aria-labelledby="lbl-webhook">
      <Label id="lbl-webhook" class="text-muted-foreground text-[10px] uppercase tracking-wider">
        Webhook · Slack / Discord / Teams
      </Label>
      <div class="flex items-center gap-2">
        <Input
          bind:value={webhookDraft}
          placeholder="https://hooks.slack.com/services/…"
          autocomplete="off"
          aria-label="Webhook URL"
          class="h-10 flex-1 text-[13px]"
        />
        <Button variant="outline" onclick={saveWebhook} disabled={savingWebhook} class="h-10">
          {#if savingWebhook}<Loader2 class="h-4 w-4 animate-spin" />{:else}Save{/if}
        </Button>
        <Tooltip.Root>
          <Tooltip.Trigger>
            {#snippet child({ props })}
              <Button
                {...props}
                variant="ghost"
                size="icon"
                class="h-10 w-10"
                aria-label="Send a test event to this webhook"
                onclick={testWebhook}
                disabled={testingWebhook}
              >
                {#if testingWebhook}
                  <Loader2 class="h-4 w-4 animate-spin" />
                {:else}
                  <Send class="h-4 w-4" />
                {/if}
              </Button>
            {/snippet}
          </Tooltip.Trigger>
          <Tooltip.Content side="left">Send a synthetic event to this URL</Tooltip.Content>
        </Tooltip.Root>
      </div>
      <p
        class={cn(
          'text-[11px]',
          wh.tone === 'ok' && 'text-emerald-500',
          wh.tone === 'fail' && 'text-destructive',
          wh.tone === 'never' && 'text-muted-foreground'
        )}
      >
        last delivery: {wh.label}
      </p>
    </section>

    <Separator />

    <!-- ----- Danger zone ----- -->
    <section class="flex flex-col gap-3" aria-labelledby="lbl-danger">
      <Label id="lbl-danger" class="text-destructive text-[10px] uppercase tracking-wider">
        Danger zone
      </Label>

      <div class="border-destructive/30 bg-destructive/5 flex items-center justify-between gap-3 rounded-md border p-3">
        <div class="flex flex-col gap-0.5">
          <p class="text-[13px] font-medium">Rotate ingest token</p>
          <p class="text-muted-foreground text-[11px]">
            Invalidates the current DSN immediately. SDKs using the old DSN stop sending events
            until you update them.
          </p>
        </div>
        <Button
          variant="outline"
          onclick={openRotate}
          class="border-destructive/40 text-destructive hover:bg-destructive/10 hover:text-destructive shrink-0"
        >
          <RotateCcw class="h-3.5 w-3.5" />
          Rotate
        </Button>
      </div>

      <div class="border-destructive/30 bg-destructive/5 flex items-center justify-between gap-3 rounded-md border p-3">
        <div class="flex flex-col gap-0.5">
          <p class="text-[13px] font-medium">Delete project</p>
          <p class="text-muted-foreground text-[11px]">
            Permanently destroys this project, every issue it owns, and every event payload.
            Cannot be undone.
          </p>
        </div>
        <Button
          variant="outline"
          onclick={() => (deleteOpen = true)}
          class="border-destructive/40 text-destructive hover:bg-destructive/10 hover:text-destructive shrink-0"
        >
          <Trash2 class="h-3.5 w-3.5" />
          Delete
        </Button>
      </div>
    </section>
  </div>
</div>

<!-- Rotate dialog: 2 phases. Phase 1 explains; phase 2 reveals the new DSN
     with a copy gate before "Done" closes the dialog. -->
<Dialog open={rotateOpen} onClose={closeRotate} class="w-[min(560px,calc(100vw-2rem))]">
  <div class="flex flex-col gap-5 p-6">
    <div class="flex flex-col gap-1.5">
      <h2 class="text-[15px] font-semibold tracking-tight">
        Rotate ingest token for <span class="font-mono">{project.name}</span>
      </h2>
      <p class="text-muted-foreground text-[12px]">
        The current DSN will stop accepting events the moment you confirm.
      </p>
    </div>

    <div class="flex flex-col gap-2">
      <Label class="text-muted-foreground text-[10px] uppercase tracking-wider">
        {rotatedDsn ? 'Old DSN (now invalid)' : 'Current DSN'}
      </Label>
      <code
        class="border-border bg-muted/40 text-muted-foreground truncate rounded-md border px-3 py-2 font-mono text-[11px]"
      >
        {project.dsn}
      </code>
    </div>

    {#if rotatedDsn}
      <div class="flex flex-col gap-2">
        <Label class="text-emerald-500 text-[10px] uppercase tracking-wider">
          New DSN — copy this before closing
        </Label>
        <DsnSnippet dsn={rotatedDsn} label="new DSN" />
        <Button
          variant="ghost"
          size="sm"
          onclick={copyRotatedDsn}
          class={cn('self-start', rotatedCopied && 'text-emerald-500 hover:text-emerald-500')}
        >
          {#if rotatedCopied}
            <Check class="h-3.5 w-3.5" /> Copied
          {:else}
            <ClipboardCopy class="h-3.5 w-3.5" /> Copy new DSN
          {/if}
        </Button>
      </div>
    {/if}

    <div class="flex justify-end gap-2 pt-2">
      {#if !rotatedDsn}
        <Button variant="ghost" onclick={closeRotate} disabled={rotateBusy}>Cancel</Button>
        <Button
          onclick={performRotate}
          disabled={rotateBusy}
          class="bg-destructive text-destructive-foreground hover:bg-destructive/90"
        >
          {#if rotateBusy}
            <Loader2 class="h-4 w-4 animate-spin" />
          {:else}
            <RotateCcw class="h-4 w-4" />
          {/if}
          Rotate now
        </Button>
      {:else}
        <Button onclick={closeRotate} disabled={!rotatedCopied}>
          {rotatedCopied ? 'Done' : 'Copy the DSN to enable Done'}
        </Button>
      {/if}
    </div>
  </div>
</Dialog>

<DeleteProjectModal
  open={deleteOpen}
  projectName={project.name}
  onClose={() => (deleteOpen = false)}
  onDeleted={() => {
    deleteOpen = false;
    void goto('/projects', { replaceState: true });
  }}
/>
