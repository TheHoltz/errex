<script lang="ts">
  import { Badge } from '$lib/components/ui/badge';

  type Props = { tags: Record<string, string> };
  let { tags }: Props = $props();

  const entries = $derived(Object.entries(tags));
</script>

<section class="px-3 py-2">
  <h2 class="text-[12px] font-semibold uppercase tracking-wider text-muted-foreground">Tags</h2>
  {#if entries.length === 0}
    <p class="text-muted-foreground mt-1 text-[12px]">Sem tags neste evento.</p>
  {:else}
    <ul class="mt-2 flex flex-wrap gap-1.5">
      {#each entries as [k, v] (k)}
        <li>
          <Badge variant="outline" class="font-mono">
            <span class="text-muted-foreground">{k}=</span>{v}
          </Badge>
        </li>
      {/each}
    </ul>
  {/if}
</section>
