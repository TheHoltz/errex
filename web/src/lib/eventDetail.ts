// Adapters from the raw Sentry-style envelope payload into the view-model
// shapes the components render. Sentry has accreted multiple "shapes" for
// breadcrumbs and tags over the years, so we normalize them here once
// rather than scattering the conditionals across templates.

import type {
  Breadcrumb,
  EventPayload,
  Frame,
  IssueLevel,
  NormalizedEvent,
  Stack,
  StoredEvent
} from './types';

export function normalize(stored: StoredEvent): NormalizedEvent {
  const p = stored.payload ?? {};
  return {
    event_id: stored.event_id,
    received_at: stored.received_at,
    level: (p.level as IssueLevel | null | undefined) ?? null,
    release: p.release ?? null,
    environment: p.environment ?? null,
    exception: extractStack(p),
    breadcrumbs: extractBreadcrumbs(p),
    tags: extractTags(p),
    raw: stored
  };
}

function extractStack(p: EventPayload): Stack | null {
  const ex = p.exception?.values?.[0];
  if (!ex) return null;
  return {
    type: ex.type ?? null,
    value: ex.value ?? null,
    frames: ex.stacktrace?.frames ?? []
  };
}

function extractBreadcrumbs(p: EventPayload): Breadcrumb[] {
  const bc = p.breadcrumbs;
  if (!bc) return [];
  // Two shapes seen in the wild: `{ values: [...] }` (newer SDKs) and a
  // bare array (older Python SDK, some custom integrations).
  if (Array.isArray(bc)) return bc;
  return bc.values ?? [];
}

function extractTags(p: EventPayload): Record<string, string> {
  const t = p.tags;
  if (!t) return {};
  if (Array.isArray(t)) {
    const out: Record<string, string> = {};
    for (const pair of t) {
      if (Array.isArray(pair) && pair.length >= 2) out[String(pair[0])] = String(pair[1]);
    }
    return out;
  }
  // Object form: stringify values defensively — some SDKs emit numbers.
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(t)) out[k] = String(v);
  return out;
}

/**
 * Drop redundant `x.name` keys when `x` is already present. Sentry SDKs
 * commonly emit both — `os` carrying "macOS 15.3" and `os.name` carrying
 * "macOS" — which doubles the badge wall for no information gain. Same
 * goes for `device.family` when `device` already names the model.
 */
export function dedupTags(tags: Record<string, string>): Record<string, string> {
  const keys = new Set(Object.keys(tags));
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(tags)) {
    const dot = k.indexOf('.');
    if (dot > 0) {
      const prefix = k.slice(0, dot);
      const suffix = k.slice(dot + 1);
      if ((suffix === 'name' || suffix === 'family') && keys.has(prefix)) continue;
    }
    out[k] = v;
  }
  return out;
}

/**
 * Format a breadcrumb timestamp as a delta against the crash event so the
 * user reads "-42s" instead of doing wall-clock math against an absolute
 * `01:18:49.203`. T-0 is reserved for the breadcrumb that lands on the
 * crash itself; positive deltas (rare — usually clock skew) get a `+`.
 */
export function breadcrumbRelativeTime(
  crashTs: string,
  bcTs: string | null | undefined
): string {
  if (!bcTs) return '—';
  const crash = Date.parse(crashTs);
  const bc = Date.parse(bcTs);
  if (!Number.isFinite(crash) || !Number.isFinite(bc)) return '—';
  const deltaMs = bc - crash;
  if (deltaMs === 0) return 'T-0';
  const sign = deltaMs > 0 ? '+' : '-';
  const absSec = Math.round(Math.abs(deltaMs) / 1000);
  if (absSec < 90) return `${sign}${absSec}s`;
  const m = Math.floor(absSec / 60);
  const s = absSec % 60;
  return s === 0 ? `${sign}${m}m` : `${sign}${m}m${s}s`;
}

/**
 * Split frames into "your code" (in_app=true) vs "dependencies"
 * (in_app=false / null / undefined). The header `N in your code` is the
 * fastest way to answer "is this mine to fix?" before reading the trace.
 */
export function partitionFrames(frames: Frame[]): { inApp: number; lib: number } {
  let inApp = 0;
  let lib = 0;
  for (const f of frames) {
    if (f.in_app === true) inApp += 1;
    else lib += 1;
  }
  return { inApp, lib };
}

/**
 * Index of the "throw site" — the frame the eye should land on first.
 * Sentry orders frames oldest-first, so the innermost call (= where the
 * exception was raised) is the last in_app frame. If none are flagged
 * in_app we fall back to the last frame so we still pre-expand something
 * useful instead of leaving the trace fully collapsed.
 */
export function throwSiteIndex(frames: Frame[]): number {
  if (frames.length === 0) return -1;
  for (let i = frames.length - 1; i >= 0; i -= 1) {
    if (frames[i]?.in_app === true) return i;
  }
  return frames.length - 1;
}
