<script lang="ts">
  import { Loader2, Save } from 'lucide-svelte';
  import { onMount } from 'svelte';
  import { Button } from '$lib/components/ui/button';
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '$lib/components/ui/card';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import { Separator } from '$lib/components/ui/separator';
  import { retention } from '$lib/retention.svelte';
  import { toast } from '$lib/toast.svelte';

  onMount(() => {
    void retention.load();
  });

  async function save() {
    const ok = await retention.save();
    if (ok) {
      toast.success('Retention settings saved');
    } else if (retention.error === 'invalid') {
      toast.error('Values must be non-negative integers');
    } else if (retention.error === 'forbidden') {
      toast.error('Admin role required to save retention settings');
    } else if (retention.error === 'unauthorized') {
      toast.error('Session expired — please sign in again');
    } else {
      toast.error('Could not save retention settings');
    }
  }
</script>

<svelte:head>
  <title>Retention · Settings · errex</title>
</svelte:head>

<div class="mx-auto max-w-2xl space-y-6 p-6">
  <header class="space-y-1">
    <h1 class="text-foreground text-base font-medium tracking-tight">Retention</h1>
    <p class="text-muted-foreground text-[12px] leading-5">
      Bound how much history errexd keeps. Settings are read once an hour by the retention task; a
      change here takes effect on the next tick. <span class="text-foreground font-medium">0</span>
      means unlimited for any field.
    </p>
  </header>

  <Separator />

  {#if retention.loading}
    <div class="text-muted-foreground flex items-center gap-2 text-[12px]">
      <Loader2 class="h-4 w-4 animate-spin" aria-hidden="true" />
      Loading…
    </div>
  {:else}
    <Card>
      <CardHeader>
        <CardTitle class="text-[13px]">Limits</CardTitle>
        <CardDescription class="text-[12px]">
          Each limit is applied independently. The retention task runs the strictest match — if both
          a per-issue and a per-project bound apply to the same row, the row is dropped on the first
          one it violates.
        </CardDescription>
      </CardHeader>
      <CardContent class="space-y-5">
        <div class="space-y-1.5">
          <Label for="events-per-issue" class="text-[12px]">Events per issue</Label>
          <Input
            id="events-per-issue"
            type="number"
            inputmode="numeric"
            min="0"
            step="1"
            bind:value={
              () => String(retention.draft.events_per_issue_max),
              (v: string) => (retention.draft.events_per_issue_max = parseIntSafe(v))
            }
            class="font-mono"
            aria-describedby="events-per-issue-help"
          />
          <p id="events-per-issue-help" class="text-muted-foreground text-[11px]">
            Keep at most this many recent event payloads per issue. Older payloads are deleted; the
            issue row (counts, first/last seen) is preserved.
          </p>
        </div>

        <div class="space-y-1.5">
          <Label for="issues-per-project" class="text-[12px]">Issues per project</Label>
          <Input
            id="issues-per-project"
            type="number"
            inputmode="numeric"
            min="0"
            step="1"
            bind:value={
              () => String(retention.draft.issues_per_project_max),
              (v: string) => (retention.draft.issues_per_project_max = parseIntSafe(v))
            }
            class="font-mono"
            aria-describedby="issues-per-project-help"
          />
          <p id="issues-per-project-help" class="text-muted-foreground text-[11px]">
            Drop the oldest issues per project beyond this count. Cascades to the issue's events.
          </p>
        </div>

        <div class="space-y-1.5">
          <Label for="event-retention-days" class="text-[12px]">Event payload age (days)</Label>
          <Input
            id="event-retention-days"
            type="number"
            inputmode="numeric"
            min="0"
            step="1"
            bind:value={
              () => String(retention.draft.event_retention_days),
              (v: string) => (retention.draft.event_retention_days = parseIntSafe(v))
            }
            class="font-mono"
            aria-describedby="event-retention-days-help"
          />
          <p id="event-retention-days-help" class="text-muted-foreground text-[11px]">
            Delete event payloads older than this many days. <span class="text-foreground"
              >0 falls back to the boot config</span
            > (<code class="rounded px-1 font-mono text-[11px]">ERREXD_RETENTION_DAYS</code>).
          </p>
        </div>
      </CardContent>
    </Card>

    <div class="flex items-center justify-end gap-2">
      <Button
        variant="ghost"
        disabled={!retention.dirty || retention.saving}
        onclick={() => retention.reset()}
      >
        Discard
      </Button>
      <Button disabled={!retention.dirty || retention.saving || !retention.isDraftValid()} onclick={save}>
        {#if retention.saving}
          <Loader2 class="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
          Saving…
        {:else}
          <Save class="h-3.5 w-3.5" aria-hidden="true" />
          Save
        {/if}
      </Button>
    </div>
  {/if}
</div>

<script context="module" lang="ts">
  // `<input type="number">` may emit `''` for an empty field. Coerce that
  // to 0 so the form treats "blank" as "unlimited" instead of NaN.
  function parseIntSafe(v: string): number {
    const trimmed = v.trim();
    if (trimmed === '') return 0;
    const n = Number.parseInt(trimmed, 10);
    return Number.isFinite(n) ? n : 0;
  }
</script>
