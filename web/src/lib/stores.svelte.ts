import type {
  ConnectionStatus,
  Issue,
  IssueLevel,
  IssueStatus,
  NormalizedEvent,
  ProjectSummary,
  SortKey
} from './types';

// Reactive global state via Svelte 5 runes. Modules are evaluated once, so the
// rune values live for the lifetime of the app — components import the proxy
// objects below and read fields directly to subscribe.

class IssuesStore {
  byId = $state<Map<number, Issue>>(new Map());

  list = $derived.by(() => Array.from(this.byId.values()));

  reset(issues: Issue[]) {
    const m = new Map<number, Issue>();
    for (const i of issues) m.set(i.id, i);
    this.byId = m;
  }

  upsert(issue: Issue) {
    const m = new Map(this.byId);
    m.set(issue.id, issue);
    this.byId = m;
  }

  get(id: number): Issue | undefined {
    return this.byId.get(id);
  }
}

export class FilterStore {
  query = $state('');
  // Default view shows what's actionable; resolved/muted/ignored are hidden
  // until the user opts in. Power users can toggle via the list header.
  statuses = $state<Set<IssueStatus>>(new Set(['unresolved']));
  // Empty set = no level filter (all levels visible). Non-empty = include
  // only those levels. Distinct from statuses where the default is a
  // single-element set, since "no level filter" is the much more common
  // resting state.
  levels = $state<Set<IssueLevel>>(new Set());
  // Window in ms applied to `first_seen`; null = no since filter. Driven
  // from header chips ("New (1h)") so the live counters double as filter
  // entry points.
  sinceMs = $state<number | null>(null);
  // When true, visibleIssues drops every issue that is not currently
  // spiking. The actual spike predicate lives in `eventStream` and is
  // passed in at call-time to keep this module pure.
  spikingOnly = $state(false);
  // Server-side ordering of the visible list. Default = newest activity
  // (preserves prior behavior). Mirrors the URL token defined in
  // lib/filterUrl.ts.
  sort = $state<SortKey>('recent');
  // Cap the visible-issues result to N rows. Driven by `top N`/`limit:N`
  // tokens in the unified input. null = no cap (full list).
  limit = $state<number | null>(null);
  // "Just-appeared" filter — keep only issues whose first_seen is inside
  // the active time window (or the last hour when no window is set).
  // Driven by `new` / `fresh` keywords in the unified input.
  newOnly = $state(false);
  // Inverse: issues that have gone quiet — last_seen older than 24h and
  // status still unresolved. Driven by `stale` / `old` keywords.
  staleOnly = $state(false);

  toggleStatus(s: IssueStatus) {
    const next = new Set(this.statuses);
    if (next.has(s)) next.delete(s);
    else next.add(s);
    this.statuses = next;
  }

  toggleLevel(l: IssueLevel) {
    const next = new Set(this.levels);
    if (next.has(l)) next.delete(l);
    else next.add(l);
    this.levels = next;
  }
}

class ConnectionStore {
  status = $state<ConnectionStatus>('connecting');
  serverVersion = $state<string | null>(null);
}

class ProjectsStore {
  available = $state<ProjectSummary[]>([]);
  current = $state<string>('default');
}

class SelectionStore {
  issueId = $state<number | null>(null);
  // Hydrated from `/api/issues/:id/event` whenever issueId changes; null
  // while in flight or when the issue has no events stored yet.
  event = $state<NormalizedEvent | null>(null);
  eventLoading = $state(false);
}

class LoadStore {
  // True until the first WS Snapshot arrives (or the initial REST fallback
  // resolves). Used to render skeletons rather than "empty" states on boot.
  initialLoad = $state(true);
}

export const issues = new IssuesStore();
export const filter = new FilterStore();
export const connection = new ConnectionStore();
export const projects = new ProjectsStore();
export const selection = new SelectionStore();
export const load = new LoadStore();

// Computed for the issue list pane. Svelte 5 disallows exporting `$derived`
// values directly from a module, so we expose a getter; consumers read it
// from inside a component, where Svelte still tracks the dependencies.
//
// Status comes from the server-side `Issue.status` field (broadcast via
// IssueUpdated WS messages and persisted in SQLite). Earlier iterations
// kept this in localStorage; that gave each browser its own truth and was
// wrong for any team larger than one.
export interface VisibleIssuesOpts {
  /** Wall-clock used by the `sinceMs` filter. Defaults to Date.now(). */
  now?: number;
  /**
   * Spike predicate, supplied by the caller (typically backed by
   * `eventStream.isSpiking`). Kept as a parameter so this module stays
   * pure and unit-testable without standing up the WS event stream.
   */
  isSpiking?: (id: number) => boolean;
}

/**
 * Parsed shape of `filter.query`. Drives both the row predicate and the
 * UI mode indicator on the input — single source of truth so the tag the
 * user sees never disagrees with what the list shows.
 */
export type ParsedQuery =
  | { mode: 'empty' }
  | { mode: 'substring'; q: string }
  | { mode: 'regex'; re: RegExp }
  | { mode: 'badRegex'; q: string };

export function parseQuery(raw: string): ParsedQuery {
  const q = raw.trim();
  if (!q) return { mode: 'empty' };
  // Single `/` is just a one-char substring; need at least `/x` to mean regex.
  if (!q.startsWith('/') || q.length < 2) return { mode: 'substring', q };
  // Leading `/` is the delimiter; trailing `/` is optional and stripped.
  let src = q.slice(1);
  if (src.endsWith('/')) src = src.slice(0, -1);
  if (!src) return { mode: 'badRegex', q };
  try {
    return { mode: 'regex', re: new RegExp(src, 'i') };
  } catch {
    return { mode: 'badRegex', q };
  }
}

function compareIssues(a: Issue, b: Issue, key: SortKey): number {
  // Tie-break on id ASC for deterministic order across renders.
  const tie = a.id - b.id;
  switch (key) {
    case 'recent':
      return (
        Date.parse(b.last_seen) - Date.parse(a.last_seen) || tie
      );
    case 'stale':
      return (
        Date.parse(a.last_seen) - Date.parse(b.last_seen) || tie
      );
    case 'count':
      return b.event_count - a.event_count || tie;
  }
}

const HOUR_MS = 60 * 60 * 1000;
const DAY_MS = 24 * HOUR_MS;

export function visibleIssues(opts: VisibleIssuesOpts = {}): Issue[] {
  const parsed = parseQuery(filter.query);
  const levelFilter = filter.levels;
  const now = opts.now ?? Date.now();
  const sinceCutoff = filter.sinceMs == null ? null : now - filter.sinceMs;
  // "new" reuses the active window when set; defaults to the last hour
  // so a bare `new` token still filters something useful.
  const newCutoff = now - (filter.sinceMs ?? HOUR_MS);
  // "stale" means first_seen older than 24h AND status still open.
  const staleCutoff = now - DAY_MS;
  const filtered = issues.list.filter((i) => {
    if (i.project !== projects.current) return false;
    if (!filter.statuses.has(i.status)) return false;
    if (levelFilter.size > 0) {
      const lvl = i.level?.toLowerCase() ?? '';
      // Issues whose level is null can't satisfy a positive level filter;
      // dropping them is correct — "show me only fatals" should not
      // accidentally include unlabelled rows.
      if (!(lvl && levelFilter.has(lvl as IssueLevel))) return false;
    }
    if (sinceCutoff != null) {
      const seen = Date.parse(i.first_seen);
      // Drop rows whose timestamp won't parse rather than risk including
      // them via a NaN comparison short-circuit.
      if (!Number.isFinite(seen) || seen < sinceCutoff) return false;
    }
    if (filter.newOnly) {
      const seen = Date.parse(i.first_seen);
      if (!Number.isFinite(seen) || seen < newCutoff) return false;
    }
    if (filter.staleOnly) {
      const seen = Date.parse(i.first_seen);
      if (!Number.isFinite(seen) || seen > staleCutoff) return false;
      if (i.status !== 'unresolved') return false;
    }
    if (filter.spikingOnly) {
      if (!opts.isSpiking || !opts.isSpiking(i.id)) return false;
    }
    if (!matches(i, parsed)) return false;
    return true;
  });
  // Sort in place — `filtered` is a fresh array from `.filter()`, not aliased
  // to anything external, so mutating it here is safe.
  filtered.sort((a, b) => compareIssues(a, b, filter.sort));
  // Limit applies AFTER the sort so "top 10" picks the 10 highest by the
  // current sort axis, not 10 random issues that happen to come first.
  return filter.limit != null && filter.limit > 0
    ? filtered.slice(0, filter.limit)
    : filtered;
}

function matches(i: Issue, parsed: ParsedQuery): boolean {
  switch (parsed.mode) {
    case 'empty':
      return true;
    case 'regex':
      return parsed.re.test(i.title) || parsed.re.test(i.culprit ?? '');
    case 'substring':
    case 'badRegex': {
      // Bad regex falls back to a literal-substring search of the full
      // string (including the leading slash) so the user is never staring
      // at a blank list because of a typo.
      const ql = parsed.q.toLowerCase();
      return (
        i.title.toLowerCase().includes(ql) ||
        (i.culprit?.toLowerCase().includes(ql) ?? false) ||
        i.fingerprint.toLowerCase().includes(ql)
      );
    }
  }
}
