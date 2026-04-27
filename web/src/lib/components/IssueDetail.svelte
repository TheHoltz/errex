<script lang="ts">
  import {
    Ban,
    Bell,
    BellOff,
    Bot,
    Check,
    Link as LinkIcon,
    SquareMousePointer,
    RotateCcw,
    UserMinus,
    UserPlus
  } from 'lucide-svelte';
  import { actions } from '$lib/actions.svelte';
  import { formatIssueContext } from '$lib/aiContext';
  import * as Avatar from '$lib/components/ui/avatar';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import { Separator } from '$lib/components/ui/separator';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { toggleMute, toggleResolve } from '$lib/issueOps';
  import { selection } from '$lib/stores.svelte';
  import { toast } from '$lib/toast.svelte';
  import type { Issue } from '$lib/types';
  import { cn, relativeTime, shortFingerprint } from '$lib/utils';
  import Breadcrumbs from './Breadcrumbs.svelte';
  import StackTrace from './StackTrace.svelte';
  import Tags from './Tags.svelte';

  type Props = { issue: Issue | null };
  let { issue }: Props = $props();

  const event = $derived(selection.event);
  const eventLoading = $derived(selection.eventLoading);
  const assignee = $derived(issue ? actions.assigneeFor(issue) : null);

  function levelDot(level: string | null | undefined): string {
    switch (level) {
      case 'fatal':
        return 'bg-red-500';
      case 'error':
        return 'bg-destructive';
      case 'warning':
        return 'bg-amber-500';
      case 'info':
        return 'bg-blue-400';
      case 'debug':
        return 'bg-muted-foreground';
      default:
        return 'bg-muted-foreground/60';
    }
  }

  function onResolve() {
    if (!issue) return;
    void toggleResolve(issue);
  }

  function onMute() {
    if (!issue) return;
    void toggleMute(issue);
  }

  function onAssign() {
    if (!issue) return;
    // Assignee remains client-only until the proto adds an assignee field;
    // the toast Undo restores whatever was there.
    if (assignee === actions.me) {
      const prev = actions.unassign(issue);
      toast.success('Assignment removed', {
        undo: () => issue && actions.setAssignee(issue, prev)
      });
    } else {
      const prev = actions.assignToMe(issue);
      toast.success(`Assigned to ${actions.me}`, {
        undo: () => issue && actions.setAssignee(issue, prev)
      });
    }
  }

  function onCopyLink() {
    if (!issue) return;
    const url = `${location.origin}/issues/${issue.id}`;
    navigator.clipboard?.writeText(url).then(
      () => toast.success('Link copied'),
      () => toast.error('Could not copy')
    );
  }

  function onCopyAIContext() {
    if (!issue) return;
    const text = formatIssueContext(issue, selection.event);
    navigator.clipboard?.writeText(text).then(
      () =>
        toast.success('AI context copied', {
          description: `${text.length.toLocaleString()} chars — paste into Claude/ChatGPT`
        }),
      () => toast.error('Could not copy')
    );
  }

  const assigneeInitial = $derived(assignee ? assignee[0]!.toUpperCase() : '');
</script>

{#if !issue}
  <div class="text-muted-foreground flex h-full flex-col items-center justify-center gap-3 p-8 text-center">
    <SquareMousePointer class="h-8 w-8 opacity-60" />
    <p class="text-[13px]">Select an issue to inspect.</p>
    <p class="text-[12px]">
      <kbd class="border-border mx-0.5 rounded border px-1 font-mono">j</kbd>/<kbd
        class="border-border mx-0.5 rounded border px-1 font-mono">k</kbd
      > to navigate.
    </p>
  </div>
{:else}
  <div class="flex h-full flex-col">
    <header class="flex flex-col gap-3 border-b border-border px-6 py-4">
      <h1 class="text-[14px] font-semibold tracking-tight">{issue.title}</h1>

      <div class="flex flex-wrap items-center gap-1.5">
        {#if issue.level}
          <Badge variant="outline" class="gap-1.5 px-2 py-0.5 text-[11px]">
            <span class={cn('h-2 w-2 rounded-full', levelDot(issue.level))}></span>
            {issue.level}
          </Badge>
        {/if}
        {#if event?.environment}
          <Badge variant="outline" class="px-2 py-0.5 text-[11px] uppercase tracking-wide">
            {event.environment}
          </Badge>
        {/if}
        {#if event?.release}
          <Badge variant="outline" class="px-2 py-0.5 font-mono text-[11px]">
            {event.release}
          </Badge>
        {/if}
        {#if issue.status === 'resolved'}
          <Badge variant="outline" class="gap-1.5 px-2 py-0.5 text-[11px]">
            <Check class="text-emerald-500 h-3.5 w-3.5" /> resolved
          </Badge>
        {:else if issue.status === 'muted'}
          <Badge variant="outline" class="gap-1.5 px-2 py-0.5 text-[11px]">
            <BellOff class="h-3.5 w-3.5" /> muted
          </Badge>
        {:else if issue.status === 'ignored'}
          <Badge variant="outline" class="gap-1.5 px-2 py-0.5 text-[11px]">
            <Ban class="h-3.5 w-3.5" /> ignored
          </Badge>
        {/if}
        {#if assignee}
          <Badge variant="outline" class="gap-1.5 px-2 py-0.5 text-[11px]">
            <Avatar.Root class="h-4 w-4 text-[9px]">
              <Avatar.Fallback class="bg-accent text-foreground">{assigneeInitial}</Avatar.Fallback>
            </Avatar.Root>
            {assignee}
          </Badge>
        {/if}
      </div>

      {#if issue.culprit}
        <p class="font-mono text-[12px] text-muted-foreground">{issue.culprit}</p>
      {/if}

      <p class="text-muted-foreground text-[11px]">
        <span class="font-mono">#{shortFingerprint(issue.fingerprint)}</span>
        {#if issue.event_count <= 1}
          · seen {relativeTime(issue.last_seen)}
        {:else}
          · {issue.event_count} events
          · 1st {relativeTime(issue.first_seen)}
          · last {relativeTime(issue.last_seen)}
        {/if}
      </p>

      <div class="mt-2 flex items-center gap-1.5">
        <Tooltip.Root>
          <Tooltip.Trigger>
            {#snippet child({ props })}
              <Button
                {...props}
                variant="default"
                size="icon"
                class="h-9 w-9"
                aria-label="Copy AI context"
                onclick={onCopyAIContext}
                disabled={eventLoading}
              >
                <Bot class="h-4 w-4" />
              </Button>
            {/snippet}
          </Tooltip.Trigger>
          <Tooltip.Content>
            Copy AI context
            <kbd class="text-muted-foreground ml-1 font-mono text-[10px]">⌘⇧C</kbd>
          </Tooltip.Content>
        </Tooltip.Root>

        <Tooltip.Root>
          <Tooltip.Trigger>
            {#snippet child({ props })}
              <Button
                {...props}
                variant="ghost"
                size="icon"
                class="h-9 w-9"
                aria-label={issue.status === 'resolved' ? 'Reopen' : 'Resolve'}
                onclick={onResolve}
              >
                {#if issue.status === 'resolved'}
                  <RotateCcw class="h-4 w-4" />
                {:else}
                  <Check class="h-4 w-4" />
                {/if}
              </Button>
            {/snippet}
          </Tooltip.Trigger>
          <Tooltip.Content>
            {issue.status === 'resolved' ? 'Reopen' : 'Resolve'}
            <kbd class="text-muted-foreground ml-1 font-mono text-[10px]">E</kbd>
          </Tooltip.Content>
        </Tooltip.Root>

        <Tooltip.Root>
          <Tooltip.Trigger>
            {#snippet child({ props })}
              <Button
                {...props}
                variant="ghost"
                size="icon"
                class="h-9 w-9"
                aria-label={assignee === actions.me ? 'Unassign' : 'Assign to me'}
                onclick={onAssign}
              >
                {#if assignee === actions.me}
                  <UserMinus class="h-4 w-4" />
                {:else}
                  <UserPlus class="h-4 w-4" />
                {/if}
              </Button>
            {/snippet}
          </Tooltip.Trigger>
          <Tooltip.Content>
            {assignee === actions.me ? 'Unassign' : 'Assign to me'}
            <kbd class="text-muted-foreground ml-1 font-mono text-[10px]">A</kbd>
          </Tooltip.Content>
        </Tooltip.Root>

        <Separator orientation="vertical" class="mx-1 h-6" />

        <Tooltip.Root>
          <Tooltip.Trigger>
            {#snippet child({ props })}
              <Button
                {...props}
                variant="ghost"
                size="icon"
                class="h-9 w-9"
                aria-label={issue.status === 'muted' ? 'Reactivate' : 'Mute'}
                onclick={onMute}
              >
                {#if issue.status === 'muted'}
                  <Bell class="h-4 w-4" />
                {:else}
                  <BellOff class="h-4 w-4" />
                {/if}
              </Button>
            {/snippet}
          </Tooltip.Trigger>
          <Tooltip.Content>
            {issue.status === 'muted' ? 'Reactivate' : 'Mute'}
            <kbd class="text-muted-foreground ml-1 font-mono text-[10px]">M</kbd>
          </Tooltip.Content>
        </Tooltip.Root>

        <Tooltip.Root>
          <Tooltip.Trigger>
            {#snippet child({ props })}
              <Button
                {...props}
                variant="ghost"
                size="icon"
                class="h-9 w-9"
                aria-label="Copy link"
                onclick={onCopyLink}
              >
                <LinkIcon class="h-4 w-4" />
              </Button>
            {/snippet}
          </Tooltip.Trigger>
          <Tooltip.Content>Copy link</Tooltip.Content>
        </Tooltip.Root>
      </div>
    </header>

    <div class="flex-1 overflow-y-auto">
      <StackTrace exception={event?.exception ?? null} loading={eventLoading} />
      <Separator />
      <Breadcrumbs
        breadcrumbs={event?.breadcrumbs ?? []}
        crashTimestamp={event?.raw.payload.timestamp ?? event?.received_at ?? null}
      />
      <Separator />
      <Tags tags={event?.tags ?? {}} />
    </div>
  </div>
{/if}
