// Serialize/parse the issue list filter to/from URLSearchParams so the
// active filter is shareable, reload-safe, and bookmarkable. Defaults
// are omitted from the query string to keep clean URLs the common case.

import type { IssueLevel, IssueStatus } from './types';

export interface FilterState {
  query: string;
  statuses: Set<IssueStatus>;
  levels: Set<IssueLevel>;
}

const ALL_STATUSES: IssueStatus[] = ['unresolved', 'resolved', 'muted', 'ignored'];
const ALL_LEVELS: IssueLevel[] = ['debug', 'info', 'warning', 'error', 'fatal'];

function setEqualsDefaultStatuses(s: Set<IssueStatus>): boolean {
  return s.size === 1 && s.has('unresolved');
}

function csvSorted<T extends string>(s: Set<T>): string {
  return [...s].sort().join(',');
}

export function serializeFilterParams(f: FilterState): URLSearchParams {
  const p = new URLSearchParams();
  if (f.query.trim().length > 0) p.set('q', f.query);
  if (!setEqualsDefaultStatuses(f.statuses)) p.set('s', csvSorted(f.statuses));
  if (f.levels.size > 0) p.set('l', csvSorted(f.levels));
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

  return { query, statuses, levels };
}
