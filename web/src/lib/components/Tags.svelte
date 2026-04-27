<script lang="ts">
  import { Tag } from 'lucide-svelte';
  import { Badge } from '$lib/components/ui/badge';
  import { dedupTags } from '$lib/eventDetail';

  type Props = { tags: Record<string, string> };
  let { tags }: Props = $props();

  const entries = $derived(Object.entries(dedupTags(tags)));
</script>

<section class="px-3 py-2">
  <h2
    class="flex items-center gap-1.5 text-[12px] font-semibold uppercase tracking-wider text-muted-foreground"
  >
    <Tag class="h-3.5 w-3.5" />
    Tags
  </h2>
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
