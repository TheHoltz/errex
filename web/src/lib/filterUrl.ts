// Serialize/parse the issue list filter to/from URLSearchParams so the
// active filter is shareable, reload-safe, and bookmarkable. Defaults
// are omitted from the query string to keep clean URLs the common case.

import type { IssueLevel, IssueStatus, SortKey } from './types';

export interface FilterState {
  query: string;
  statuses: Set<IssueStatus>;
  levels: Set<IssueLevel>;
  sinceMs: number | null;
  spikingOnly: boolean;
  sort: SortKey;
}

const ALL_STATUSES: IssueStatus[] = ['unresolved', 'resolved', 'muted', 'ignored'];
const ALL_LEVELS: IssueLevel[] = ['debug', 'info', 'warning', 'error', 'fatal'];
export const ALL_SORTS: SortKey[] = ['recent', 'stale', 'count', 'created', 'oldest'];

// Whitelisted "since" presets. Keeping the URL token symbolic (1h/24h/7d)
// rather than raw milliseconds means a stale share link can't smuggle in
// an arbitrary window, and the encoded URL stays human-readable.
const SINCE_PRESETS: Record<string, number> = {
  '1h': 60 * 60 * 1000,
  '24h': 24 * 60 * 60 * 1000,
  '7d': 7 * 24 * 60 * 60 * 1000
};

function setEqualsDefaultStatuses(s: Set<IssueStatus>): boolean {
  return s.size === 1 && s.has('unresolved');
}

function csvSorted<T extends string>(s: Set<T>): string {
  return [...s].sort().join(',');
}

function sinceMsToToken(ms: number): string | null {
  for (const [token, value] of Object.entries(SINCE_PRESETS)) {
    if (value === ms) return token;
  }
  return null;
}

export function serializeFilterParams(f: FilterState): URLSearchParams {
  const p = new URLSearchParams();
  if (f.query.trim().length > 0) p.set('q', f.query);
  if (!setEqualsDefaultStatuses(f.statuses)) p.set('s', csvSorted(f.statuses));
  if (f.levels.size > 0) p.set('l', csvSorted(f.levels));
  if (f.sinceMs != null) {
    const token = sinceMsToToken(f.sinceMs);
    if (token) p.set('since', token);
  }
  if (f.spikingOnly) p.set('spike', '1');
  if (f.sort !== 'recent') p.set('sort', f.sort);
  return p;
}

export function parseFilterParams(p: URLSearchParams): FilterState {
  const query = p.get('q') ?? '';

  const sRaw = p.get('s');
  let statuses: Set<IssueStatus>;
  if (sRaw == null) {
    statuses = new Set<IssueStatus>(['unresolved']);
  } else {
    statuses = new Set<IssueStatus>(
      sRaw
        .split(',')
        .map((t) => t.trim())
        .filter((t): t is IssueStatus => (ALL_STATUSES as string[]).includes(t))
    );
    // Empty after filtering invalid tokens — fall back rather than show
    // nothing on a malformed share link.
    if (statuses.size === 0) statuses = new Set<IssueStatus>(['unresolved']);
  }

  const lRaw = p.get('l');
  const levels =
    lRaw == null
      ? new Set<IssueLevel>()
      : new Set<IssueLevel>(
          lRaw
            .split(',')
            .map((t) => t.trim())
            .filter((t): t is IssueLevel => (ALL_LEVELS as string[]).includes(t))
        );

  const sinceRaw = p.get('since');
  const sinceMs = sinceRaw != null && sinceRaw in SINCE_PRESETS ? SINCE_PRESETS[sinceRaw]! : null;

  const spikingOnly = p.get('spike') === '1';

  const sortRaw = p.get('sort');
  const sort: SortKey =
    sortRaw != null && (ALL_SORTS as string[]).includes(sortRaw)
      ? (sortRaw as SortKey)
      : 'recent';

  return { query, statuses, levels, sinceMs, spikingOnly, sort };
}
