// Adapters from the raw Sentry-style envelope payload into the view-model
// shapes the components render. Sentry has accreted multiple "shapes" for
// breadcrumbs and tags over the years, so we normalize them here once
// rather than scattering the conditionals across templates.

import type {
  Breadcrumb,
  EventPayload,
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
