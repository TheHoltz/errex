// Per-arrival timestamps used for live counters and sparklines. Both buffers
// are bounded — global to the last hour, per-issue to the last 30 min — so
// memory stays flat even under sustained ingest.
//
// The WS client calls `record(issueId)` on every IssueCreated/IssueUpdated.
// Reactivity: arrays are reassigned (not mutated) so $derived consumers
// re-run, and a `tick` rune advances on a 5s interval so callers that depend
// on "now" (rate, spiking, freshness) refresh without explicit invalidation.

const HOUR_MS = 60 * 60 * 1000;
const HALF_HOUR_MS = 30 * 60 * 1000;
const SPIKE_WINDOW_MS = 5 * 60 * 1000;

class EventStreamStore {
  // Wall-clock arrival times, oldest → newest. Pruned on every record().
  global = $state<number[]>([]);
  perIssue = $state<Map<number, number[]>>(new Map());
  lastAt = $state<number | null>(null);

  // Advances every 5 s so $derived counters/freshness re-evaluate without
  // each consumer wiring its own setInterval.
  tick = $state(0);

  record(issueId: number, when: number = Date.now()) {
    this.lastAt = when;
    this.global = prune(this.global.concat(when), when - HOUR_MS);

    const next = new Map(this.perIssue);
    const prev = next.get(issueId) ?? [];
    next.set(issueId, prune(prev.concat(when), when - HALF_HOUR_MS));
    this.perIssue = next;
  }

  // Hard reset (e.g. project switch). Keeps the consumer-visible shape stable.
  clear() {
    this.global = [];
    this.perIssue = new Map();
    this.lastAt = null;
  }

  ratePerMin(window: number = SPIKE_WINDOW_MS, now = Date.now()): number {
    const cutoff = now - window;
    const recent = countAfter(this.global, cutoff);
    return (recent / window) * 60_000;
  }

  // True when the rate over the last `window` is at least `factor`× the rate
  // over the prior `window` (and there is enough signal to mean anything).
  isSpiking(issueId: number, factor = 3, window = SPIKE_WINDOW_MS, now = Date.now()): boolean {
    const arr = this.perIssue.get(issueId);
    if (!arr || arr.length < 6) return false;
    const recent = countAfter(arr, now - window);
    const prior = countAfter(arr, now - 2 * window) - recent;
    return recent >= 5 && recent >= factor * Math.max(prior, 1);
  }

  // Bucket an issue's recent events into N equal-width slots over a window
  // ending at `now`. Returns counts oldest → newest. Used by Sparkline.
  buckets(issueId: number, slots = 30, window = HALF_HOUR_MS, now = Date.now()): number[] {
    const arr = this.perIssue.get(issueId);
    const out = new Array<number>(slots).fill(0);
    if (!arr || arr.length === 0) return out;
    const start = now - window;
    const bucketMs = window / slots;
    for (const t of arr) {
      if (t < start) continue;
      const idx = Math.min(slots - 1, Math.floor((t - start) / bucketMs));
      out[idx] = (out[idx] ?? 0) + 1;
    }
    return out;
  }
}

function prune(arr: number[], cutoff: number): number[] {
  // Cheap left-trim: in steady state most entries are within window.
  let i = 0;
  while (i < arr.length && (arr[i] ?? 0) < cutoff) i++;
  return i === 0 ? arr : arr.slice(i);
}

function countAfter(arr: number[], cutoff: number): number {
  // Linear scan from the right since `arr` is monotonic non-decreasing.
  let n = 0;
  for (let i = arr.length - 1; i >= 0; i--) {
    if ((arr[i] ?? 0) >= cutoff) n++;
    else break;
  }
  return n;
}

export const eventStream = new EventStreamStore();

if (typeof window !== 'undefined') {
  // Single shared timer for "now-relative" derivations; cheap and avoids
  // per-component intervals. Cleared on HMR via the SvelteKit reset hook.
  setInterval(() => {
    eventStream.tick++;
  }, 5_000);
}
