// ─────────────────────────────────────────────────────────────────────
//  Bridge between the unified-input string and the discrete filter
//  store fields (lib/stores.svelte.ts → FilterStore).
//
//  The store keeps `statuses`, `levels`, `sinceMs`, `spikingOnly`,
//  `sort`, `newOnly`, `staleOnly`, `limit` as the canonical state —
//  URL serialization, header chips and visibleIssues all read from
//  there. The unified input is a *view* on those fields: typing parses
//  and writes; external mutations (e.g. the header 1H chip) regenerate
//  the input text via `filterToQueryString`.
// ─────────────────────────────────────────────────────────────────────

import { parseQuery, type Query } from './queryParser';
import type { FilterStore } from './stores.svelte';
import type { IssueLevel, IssueStatus, SortKey } from './types';

const STATUS_DEFAULT: ReadonlySet<IssueStatus> = new Set(['unresolved']);
const STATUS_ORDER: IssueStatus[] = ['unresolved', 'resolved', 'muted', 'ignored'];
const LEVEL_ORDER: IssueLevel[] = ['fatal', 'error', 'warning', 'info', 'debug'];

function setEqualsDefaultStatuses(s: ReadonlySet<IssueStatus>): boolean {
  return s.size === STATUS_DEFAULT.size && [...STATUS_DEFAULT].every((v) => s.has(v));
}

// Apply a parsed Query to the filter store. Fields not mentioned in
// the query are reset to their defaults so the input text and the
// store fields agree about what's "set".
export function applyQueryToFilter(query: Query, filter: FilterStore): void {
  // Statuses default to {unresolved}; if the query says nothing about
  // status, restore that default. If it specifies something, use it.
  filter.statuses = query.statuses.size > 0 ? new Set(query.statuses) : new Set(STATUS_DEFAULT);

  // Negative-status would carve out from the {all} space. We don't yet
  // model "all - resolved"; treat negative-only as "not-default" by
  // expanding to the complement. Keeps the contract symmetric.
  if (query.negStatuses.size > 0 && query.statuses.size === 0) {
    const complement = new Set<IssueStatus>(STATUS_ORDER);
    for (const v of query.negStatuses) complement.delete(v);
    filter.statuses = complement;
  }

  filter.levels = new Set(query.levels);
  // Negative levels: same logic — if no positive levels were specified,
  // treat negation as a complement filter. Otherwise honour the
  // positive set as-is (negation against an explicit positive set
  // would be a contradiction the parser shouldn't try to interpret).
  if (query.negLevels.size > 0 && query.levels.size === 0) {
    const complement = new Set<IssueLevel>(LEVEL_ORDER);
    for (const v of query.negLevels) complement.delete(v);
    filter.levels = complement;
  }

  filter.sinceMs = query.sinceMs;
  filter.spikingOnly = query.spiking;
  filter.newOnly = query.newOnly;
  filter.staleOnly = query.staleOnly;
  filter.sort = query.sort ?? 'recent';
  filter.limit = query.limit;

  // Free-text + words → folded into filter.query so the existing
  // substring-matcher in visibleIssues finds them. Multi-term composes
  // as space-joined which the existing matcher reads as a single
  // substring; this is a deliberate simplification — multi-term
  // ANDed text search would need a richer downstream matcher.
  const textParts: string[] = [];
  for (const t of query.text) {
    if (t.isPattern) textParts.push(`/${t.value.replace(/\//g, '\\/')}/`); // approximate
    else textParts.push(t.value);
  }
  for (const w of query.words) textParts.push(w);
  filter.query = textParts.join(' ');
}

// Inverse: build a canonical query string from the filter store. Used
// to seed the unified input on mount and after external mutations
// (header chip, URL navigation) so the input always shows the truth.
export function filterToQueryString(filter: FilterStore): string {
  const parts: string[] = [];

  // Levels: csv when more than one, bare when single.
  if (filter.levels.size > 0) {
    const ordered = LEVEL_ORDER.filter((l) => filter.levels.has(l));
    parts.push(ordered.join(','));
  }

  // Statuses: only emit when non-default.
  if (!setEqualsDefaultStatuses(filter.statuses)) {
    const ordered = STATUS_ORDER.filter((s) => filter.statuses.has(s));
    parts.push(`status:${ordered.join(',')}`);
  }

  // Time window — emit a compact token (5m / 1h / 7d) when the value
  // matches a known preset; otherwise fall back to a numeric form.
  if (filter.sinceMs != null) {
    parts.push(formatSince(filter.sinceMs));
  }

  if (filter.spikingOnly) parts.push('spiking');
  if (filter.newOnly) parts.push('new');
  if (filter.staleOnly) parts.push('stale');
  if (filter.sort && filter.sort !== 'recent') parts.push(`sort:${filter.sort}`);
  if (filter.limit != null) parts.push(`limit:${filter.limit}`);

  // Free text last, so facets read first.
  if (filter.query.trim().length > 0) parts.push(filter.query.trim());

  return parts.join(' ');
}

function formatSince(ms: number): string {
  // Order matters — try day-grain first so "7d" beats "168h".
  const presets: Array<[number, string]> = [
    [7 * 24 * 60 * 60 * 1000, '7d'],
    [24 * 60 * 60 * 1000, '24h'],
    [60 * 60 * 1000, '1h'],
    [15 * 60 * 1000, '15m'],
    [5 * 60 * 1000, '5m']
  ];
  for (const [v, token] of presets) {
    if (ms === v) return token;
  }
  // Round to the nearest natural unit for a non-preset value.
  if (ms % (24 * 60 * 60 * 1000) === 0) return `${ms / (24 * 60 * 60 * 1000)}d`;
  if (ms % (60 * 60 * 1000) === 0) return `${ms / (60 * 60 * 1000)}h`;
  return `${Math.round(ms / 60000)}m`;
}

// Convenience for callers that just want to apply a string in one shot.
export function applyInputToFilter(input: string, filter: FilterStore): void {
  const { query } = parseQuery(input);
  applyQueryToFilter(query, filter);
}
