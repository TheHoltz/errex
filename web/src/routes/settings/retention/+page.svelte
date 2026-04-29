<script lang="ts">
  import { CornerDownLeft, HelpCircle, Loader2, Save } from 'lucide-svelte';
  import { onMount } from 'svelte';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import { Separator } from '$lib/components/ui/separator';
  import { Switch } from '$lib/components/ui/switch';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { retention } from '$lib/retention.svelte';
  import { toast } from '$lib/toast.svelte';

  // Picked to match the preset balance the preview converged on. Used when
  // a user toggles a limit OFF (back from "unlimited") so the input lands
  // on a sensible value instead of an empty 0.
  const DEFAULTS = {
    events_per_issue_max: 100,
    issues_per_project_max: 500,
    event_retention_days: 30
  } as const;

  type FieldKey = keyof typeof DEFAULTS;

  // Static field config — keeps the markup loop short and the help text in
  // one place so a copy edit touches one line, not three.
  const FIELDS: ReadonlyArray<{
    key: FieldKey;
    label: string;
    overrideLabel: string;
    overrideHelp: string;
    help: string;
    suffix?: string;
  }> = [
    {
      key: 'events_per_issue_max',
      label: 'Events per issue',
      overrideLabel: 'unlimited',
      overrideHelp: 'No bound on events per issue.',
      help: 'Keep at most this many recent payloads per issue. The issue row (counts, first/last seen) is preserved when payloads roll off.'
    },
    {
      key: 'issues_per_project_max',
      label: 'Issues per project',
      overrideLabel: 'unlimited',
      overrideHelp: 'No bound on issues per project.',
      help: "Drop the oldest issues per project beyond this count. Cascades to the issue's events."
    },
    {
      key: 'event_retention_days',
      label: 'Event payload age',
      overrideLabel: 'use boot config',
      overrideHelp: 'Falls back to the ERREXD_RETENTION_DAYS env var.',
      help: 'Delete event payloads older than this many days.',
      suffix: 'days'
    }
  ] as const;

  onMount(() => {
    void retention.load();
    void retention.loadStats();
  });

  function setOverride(key: FieldKey, on: boolean) {
    if (on) {
      retention.draft[key] = 0;
    } else {
      retention.draft[key] = DEFAULTS[key];
    }
  }

  function formatBytes(b: number | undefined): { value: string; unit: string } {
    if (b == null) return { value: '—', unit: '' };
    if (b < 1024) return { value: String(b), unit: 'B' };
    if (b < 1024 * 1024) return { value: (b / 1024).toFixed(1), unit: 'KB' };
    if (b < 1024 * 1024 * 1024) return { value: (b / (1024 * 1024)).toFixed(b > 10 * 1024 * 1024 ? 0 : 1), unit: 'MB' };
    return { value: (b / (1024 * 1024 * 1024)).toFixed(2), unit: 'GB' };
  }

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

  function parseIntSafe(v: string): number {
    const trimmed = v.trim();
    if (trimmed === '') return 0;
    const n = Number.parseInt(trimmed, 10);
    return Number.isFinite(n) ? n : 0;
  }

  const bytes = $derived(formatBytes(retention.stats?.bytes));
</script>

<svelte:head>
  <title>Retention · Settings · errex</title>
</svelte:head>

<div class="mx-auto max-w-2xl space-y-6 p-6">
  {#if retention.loading && !retention.stats}
    <div class="text-muted-foreground flex items-center gap-2 text-[12px]">
      <Loader2 class="h-4 w-4 animate-spin" aria-hidden="true" />
      Loading…
    </div>
  {:else}
    <!-- HERO ----------------------------------------------------------- -->
    <section class="space-y-6 pb-2">
      <div class="flex items-baseline justify-between">
        <h1 class="text-foreground text-base font-medium tracking-tight">Retention</h1>
        <span
          class="text-muted-foreground inline-flex items-center gap-1.5 text-[11px] font-light tabular-nums"
        >
          <span class="bg-primary inline-block h-1 w-1 animate-pulse rounded-full"></span>
          read once an hour
        </span>
      </div>

      <div class="space-y-3">
        <div class="flex items-baseline gap-2">
          <span
            class="text-primary text-[44px] font-semibold leading-none tracking-tight tabular-nums"
          >
            {bytes.value}
          </span>
          <span class="text-muted-foreground text-[14px] font-light">
            {bytes.unit}{bytes.unit ? ' on disk' : ''}
          </span>
        </div>
        <div class="text-muted-foreground text-[12px] font-light tabular-nums">
          <span class="text-foreground font-medium"
            >{retention.stats?.issues?.toLocaleString() ?? '—'}</span
          >
          issues
          <span class="mx-2 opacity-40">·</span>
          <span class="text-foreground font-medium"
            >{retention.stats?.events?.toLocaleString() ?? '—'}</span
          >
          events
          <span class="mx-2 opacity-40">·</span>
          oldest
          <span class="text-foreground font-medium">
            {#if retention.stats?.oldest_event_age_days == null}
              —
            {:else}
              {retention.stats.oldest_event_age_days}d
            {/if}
          </span>
        </div>
      </div>
    </section>

    <Separator />

    <!-- LIMITS --------------------------------------------------------- -->
    <section class="pt-5">
      <div class="mb-6 flex items-baseline justify-between">
        <h2 class="text-foreground text-[14px] font-medium tracking-tight">Limits</h2>
        <span class="text-muted-foreground text-[11px] font-light tabular-nums">
          <span class="text-foreground">{retention.activeLimitCount}</span><span class="opacity-60"
            >/{FIELDS.length} active</span
          >
        </span>
      </div>

      <div class="space-y-7">
        {#each FIELDS as f (f.key)}
          <div>
            <div
              class="grid grid-cols-[1fr_minmax(140px,180px)] items-baseline gap-x-5"
              class:opacity-55={retention.draft[f.key] === 0}
            >
              <div class="flex items-baseline gap-1.5">
                <Label for={f.key} class="text-[12.5px] font-medium tracking-tight">
                  {f.label}
                </Label>
                <Tooltip.Root>
                  <Tooltip.Trigger>
                    {#snippet child({ props })}
                      <button
                        {...props}
                        type="button"
                        class="text-muted-foreground/40 hover:text-foreground inline-flex align-[-1px]"
                        aria-label={`About ${f.label}`}
                      >
                        <HelpCircle class="h-3 w-3" />
                      </button>
                    {/snippet}
                  </Tooltip.Trigger>
                  <Tooltip.Content
                    side="top"
                    sideOffset={6}
                    class="max-w-65 text-[11px] font-light leading-relaxed"
                  >
                    {f.help}
                  </Tooltip.Content>
                </Tooltip.Root>
              </div>

              <div class="flex items-baseline gap-2">
                <Input
                  id={f.key}
                  type="number"
                  inputmode="numeric"
                  min="0"
                  step="1"
                  disabled={retention.draft[f.key] === 0}
                  class="text-right font-mono text-[13px] font-medium"
                  value={String(retention.draft[f.key])}
                  oninput={(e) =>
                    (retention.draft[f.key] = parseIntSafe(
                      (e.currentTarget as HTMLInputElement).value
                    ))}
                />
                {#if f.suffix}
                  <span class="text-muted-foreground w-8 text-[11px] font-light">{f.suffix}</span>
                {/if}
              </div>
            </div>

            <div class="mt-2 flex items-center gap-3">
              <div class="inline-flex items-center gap-2">
                <Switch
                  id={f.key + '-override'}
                  checked={retention.draft[f.key] === 0}
                  onCheckedChange={(on) => setOverride(f.key, on)}
                />
                <Label
                  for={f.key + '-override'}
                  class={retention.draft[f.key] === 0
                    ? 'text-foreground text-[11px] font-light whitespace-nowrap'
                    : 'text-muted-foreground text-[11px] font-light whitespace-nowrap'}
                >
                  {f.overrideLabel}
                </Label>
              </div>
              {#if retention.draft[f.key] === 0}
                <span class="text-muted-foreground/70 text-[11px] font-light italic">
                  · {f.overrideHelp}
                </span>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    </section>

    <!-- FOOTER --------------------------------------------------------- -->
    <footer
      class="bg-background/95 sticky bottom-0 -mx-1 mt-8 flex items-center justify-between gap-3 border-t px-1 py-4 backdrop-blur"
    >
      <p class="text-muted-foreground text-[12px] font-light tabular-nums">
        {#if retention.activeLimitCount === 0}
          no limits set — errex will keep all history
        {:else}
          <span class="text-foreground font-semibold">{retention.activeLimitCount}</span> of
          {FIELDS.length} limits active · changes apply on next sweep
        {/if}
      </p>
      <div class="flex items-center gap-1">
        <Button
          variant="ghost"
          disabled={!retention.dirty || retention.saving}
          onclick={() => retention.reset()}
        >
          Discard
        </Button>
        <Button
          disabled={!retention.dirty || retention.saving || !retention.isDraftValid()}
          onclick={save}
          class="gap-2"
        >
          {#if retention.saving}
            <Loader2 class="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
            Saving…
          {:else}
            <Save class="h-3.5 w-3.5" aria-hidden="true" />
            Save
            <kbd
              class="bg-primary-foreground/15 inline-flex items-center rounded px-1 py-px text-[9.5px] font-light"
            >
              <CornerDownLeft class="h-2.5 w-2.5" />
            </kbd>
          {/if}
        </Button>
      </div>
    </footer>
  {/if}
</div>
