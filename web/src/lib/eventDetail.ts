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
 * Coerce a Sentry timestamp to milliseconds-since-epoch.
 *
 * Sentry SDKs are inconsistent: the spec asks for ISO-8601 strings, but
 * most JS SDKs ship Unix epoch *seconds* (often as a float with sub-second
 * precision). Older Python integrations emit ISO strings; some custom
 * forwarders emit milliseconds. We accept all three forms — falling back
 * to NaN for anything else so callers can render the em-dash placeholder.
 */
export function parseSentryTimestamp(ts: string | number | null | undefined): number {
  if (ts === null || ts === undefined) return Number.NaN;
  if (typeof ts === 'number') {
    if (!Number.isFinite(ts)) return Number.NaN;
    // Heuristic: anything below ~10^12 is seconds-since-epoch (year 33658
    // in milliseconds is the cutoff; year 2001 in seconds is 10^9). Above
    // that, treat as already-milliseconds.
    return ts < 1e12 ? ts * 1000 : ts;
  }
  // Numeric strings ("1729012345.678") slip in from a few SDKs that
  // stringify the Unix-seconds float. Date.parse returns NaN on those, so
  // try parseFloat first when the input has no separator that ISO needs.
  if (!ts.includes('-') && !ts.includes('T') && !ts.includes(':')) {
    const n = Number.parseFloat(ts);
    if (Number.isFinite(n)) return n < 1e12 ? n * 1000 : n;
  }
  const ms = Date.parse(ts);
  return Number.isFinite(ms) ? ms : Number.NaN;
}

/**
 * Format a breadcrumb timestamp as a delta against the crash event so the
 * user reads "-42s" instead of doing wall-clock math against an absolute
 * `01:18:49.203`. T-0 is reserved for the breadcrumb that lands on the
 * crash itself; positive deltas (rare — usually clock skew) get a `+`.
 */
export function breadcrumbRelativeTime(
  crashTs: string | number | null | undefined,
  bcTs: string | number | null | undefined
): string {
  const crash = parseSentryTimestamp(crashTs);
  const bc = parseSentryTimestamp(bcTs);
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
