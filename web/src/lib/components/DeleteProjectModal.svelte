<script lang="ts">
  import { AlertTriangle, Loader2, Trash2 } from 'lucide-svelte';
  import { admin } from '$lib/admin.svelte';
  import { Button } from '$lib/components/ui/button';
  import { Dialog } from '$lib/components/ui/dialog';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import { isDeleteConfirmed } from '$lib/projectsConsole';
  import { toast } from '$lib/toast.svelte';
  import type { DeleteSummary } from '$lib/api';

  type Props = {
    open: boolean;
    projectName: string;
    onClose: () => void;
    onDeleted: () => void;
  };

  let { open, projectName, onClose, onDeleted }: Props = $props();

  let typed = $state('');
  let busy = $state(false);
  let preview = $state<DeleteSummary | null>(null);
  let previewError = $state<string | null>(null);

  // Fetch the destruction count when the modal opens. The server query is
  // cheap (two COUNT queries) and we want the operator to see the magnitude
  // BEFORE typing the name, not after.
  $effect(() => {
    if (open) {
      typed = '';
      preview = null;
      previewError = null;
      void admin
        .destroyPreview(projectName)
        .then((p) => (preview = p))
        .catch((err) => (previewError = String(err)));
    }
  });

  const confirmed = $derived(isDeleteConfirmed(typed, projectName));

  async function performDelete() {
    if (!confirmed) return;
    busy = true;
    try {
      const summary = await admin.deleteProject(projectName);
      toast.success(
        `Deleted "${projectName}" (${summary.events_deleted} events, ${summary.issues_deleted} issues)`
      );
      onDeleted();
    } catch (err) {
      toast.error('Failed to delete project', { description: String(err) });
    } finally {
      busy = false;
    }
  }
</script>

<Dialog {open} {onClose} class="w-[min(560px,calc(100vw-2rem))]">
  <div class="flex flex-col gap-5 p-6">
    <div class="flex items-start gap-3">
      <AlertTriangle class="text-destructive mt-0.5 h-5 w-5 shrink-0" aria-hidden="true" />
      <div class="flex flex-col gap-1.5">
        <h2 class="text-[15px] font-semibold tracking-tight">
          Delete project <span class="font-mono">{projectName}</span>
        </h2>
        <p class="text-muted-foreground text-[12px]">
          This permanently destroys the project and all of its data. This cannot be undone.
        </p>
      </div>
    </div>

    <div class="border-destructive/30 bg-destructive/5 rounded-md border p-3 text-[12px]">
      {#if previewError}
        <span class="text-destructive">Could not load destruction summary: {previewError}</span>
      {:else if preview === null}
        <span class="text-muted-foreground inline-flex items-center gap-2">
          <Loader2 class="h-3 w-3 animate-spin" />
          Counting what would be deleted…
        </span>
      {:else if preview.events_deleted === 0 && preview.issues_deleted === 0}
        <span>This project has no issues or events. Safe to delete.</span>
      {:else}
        Permanently deletes <strong>{preview.events_deleted.toLocaleString()}</strong>
        events and <strong>{preview.issues_deleted.toLocaleString()}</strong> issues.
      {/if}
    </div>

    <div class="flex flex-col gap-2">
      <Label for="confirm-name" class="text-[12px]">
        Type
        <span class="font-mono">{projectName}</span>
        to confirm
      </Label>
      <Input
        id="confirm-name"
        bind:value={typed}
        autocomplete="off"
        placeholder={projectName}
        class="h-10 font-mono text-[13px]"
        aria-label={`Type ${projectName} to confirm deletion`}
      />
    </div>

    <div class="flex justify-end gap-2 pt-2">
      <Button variant="ghost" onclick={onClose} disabled={busy}>Cancel</Button>
      <Button
        onclick={performDelete}
        disabled={!confirmed || busy}
        class="bg-destructive text-destructive-foreground hover:bg-destructive/90"
      >
        {#if busy}
          <Loader2 class="h-4 w-4 animate-spin" />
        {:else}
          <Trash2 class="h-4 w-4" />
        {/if}
        Delete forever
      </Button>
    </div>
  </div>
</Dialog>
