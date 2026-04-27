<script lang="ts">
  import { X } from 'lucide-svelte';
  import { Button } from '$lib/components/ui/button';
  import { toast } from '$lib/toast.svelte';
  import { cn } from '$lib/utils';

  function variantClass(variant: string) {
    switch (variant) {
      case 'success':
        return 'border-emerald-500/30 bg-emerald-500/10 text-emerald-200';
      case 'warning':
        return 'border-amber-500/30 bg-amber-500/10 text-amber-200';
      case 'error':
        return 'border-destructive/40 bg-destructive/15 text-destructive-foreground';
      default:
        return 'border-border bg-popover text-popover-foreground';
    }
  }
</script>

<div
  class="pointer-events-none fixed bottom-4 right-4 z-50 flex w-80 max-w-[calc(100vw-2rem)] flex-col-reverse gap-2"
  role="region"
  aria-label="Notifications"
>
  {#each toast.list as t (t.id)}
    <div
      class={cn(
        'pointer-events-auto flex items-start gap-2 rounded-md border px-3 py-2 shadow-lg',
        'text-[12px] backdrop-blur',
        variantClass(t.variant)
      )}
      role="status"
    >
      <div class="min-w-0 flex-1">
        <p class="font-medium leading-tight">{t.message}</p>
        {#if t.description}
          <p class="mt-0.5 text-muted-foreground leading-snug">{t.description}</p>
        {/if}
      </div>
      {#if t.undo}
        <Button
          variant="link"
          size="sm"
          onclick={() => {
            t.undo?.();
            toast.dismiss(t.id);
          }}
          class="h-auto shrink-0 p-0 font-medium"
        >
          Undo
        </Button>
      {/if}
      <Button
        variant="ghost"
        size="icon"
        aria-label="Close"
        onclick={() => toast.dismiss(t.id)}
        class="text-muted-foreground hover:text-foreground h-5 w-5 shrink-0"
      >
        <X class="h-3 w-3" />
      </Button>
    </div>
  {/each}
</div>
