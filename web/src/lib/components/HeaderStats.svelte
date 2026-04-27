<script lang="ts">
  import { eventStream } from '$lib/eventStream.svelte';
  import { issues, projects } from '$lib/stores.svelte';
  import Sparkline from './Sparkline.svelte';

  // The header reads `eventStream.tick` so the counters refresh on the same
  // 5 s heartbeat used by Freshness. A separate setInterval per counter
  // would also work but multiplies DOM updates needlessly.
  const ofProject = $derived(issues.list.filter((i) => i.project === projects.current));

  const newLastHour = $derived.by(() => {
    void eventStream.tick;
    const cutoff = Date.now() - 60 * 60 * 1000;
    return ofProject.filter(
      (i) => +new Date(i.first_seen) >= cutoff && i.status === 'unresolved'
    ).length;
  });

  const spiking = $derived.by(() => {
    void eventStream.tick;
    return ofProject.filter((i) => eventStream.isSpiking(i.id)).length;
  });

  const ratePerMin = $derived.by(() => {
    void eventStream.tick;
    return Math.round(eventStream.ratePerMin());
  });

  // 60-min sparkline of the global event stream, bucketed in 60 slots.
  const buckets = $derived.by(() => {
    void eventStream.tick;
    const slots = 60;
    const window = 60 * 60 * 1000;
    const start = Date.now() - window;
    const bucketMs = window / slots;
    const out = new Array<number>(slots).fill(0);
    for (const t of eventStream.global) {
      if (t < start) continue;
      const idx = Math.min(slots - 1, Math.floor((t - start) / bucketMs));
      out[idx] = (out[idx] ?? 0) + 1;
    }
    return out;
  });
</script>

<div class="flex items-center gap-5">
  <div class="flex items-baseline gap-2">
    <span class="text-foreground text-[13px] font-semibold tabular-nums">{newLastHour}</span>
    <span class="text-muted-foreground text-[10px] uppercase tracking-wider">novos·1h</span>
  </div>
  <div class="bg-border h-4 w-px"></div>
  <div class="flex items-baseline gap-2">
    <span
      class="text-[13px] font-semibold tabular-nums {spiking > 0 ? 'text-amber-400' : 'text-foreground'}"
    >
      {spiking}
    </span>
    <span class="text-muted-foreground text-[10px] uppercase tracking-wider">spike</span>
  </div>
  <div class="bg-border h-4 w-px"></div>
  <div class="flex items-center gap-3">
    <div class="flex items-baseline gap-2">
      <span class="text-foreground text-[13px] font-semibold tabular-nums">{ratePerMin}</span>
      <span class="text-muted-foreground text-[10px] uppercase tracking-wider">e/min</span>
    </div>
    <Sparkline values={buckets} width={72} height={14} accent={ratePerMin > 0} />
  </div>
</div>
