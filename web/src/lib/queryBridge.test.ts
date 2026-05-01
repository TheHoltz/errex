import { describe, expect, it } from 'vitest';
import { parseQuery } from './queryParser';
import { applyInputToFilter, applyQueryToFilter, filterToQueryString } from './queryBridge';
import type { FilterStore } from './stores.svelte';
import type { IssueLevel, IssueStatus, SortKey } from './types';

// Lightweight test double — we don't import the real $state-backed
// store because it lives inside Svelte's reactive runtime. The shape
// matches FilterStore's public fields, which is all the bridge cares
// about.
function freshFilter(): FilterStore {
  return {
    query: '',
    statuses: new Set<IssueStatus>(['unresolved']),
    levels: new Set<IssueLevel>(),
    sinceMs: null,
    spikingOnly: false,
    sort: 'recent' as SortKey,
    limit: null,
    newOnly: false,
    staleOnly: false,
    toggleStatus() {},
    toggleLevel() {}
  } as unknown as FilterStore;
}

describe('applyQueryToFilter', () => {
  it('writes a single level', () => {
    const f = freshFilter();
    applyInputToFilter('fatal', f);
    expect([...f.levels]).toEqual(['fatal']);
  });

  it('writes a comma-OR level list', () => {
    const f = freshFilter();
    applyInputToFilter('fatal,error', f);
    expect([...f.levels].sort()).toEqual(['error', 'fatal']);
  });

  it('writes a time window', () => {
    const f = freshFilter();
    applyInputToFilter('5m', f);
    expect(f.sinceMs).toBe(5 * 60 * 1000);
  });

  it('top N sets sort + limit', () => {
    const f = freshFilter();
    applyInputToFilter('top 10', f);
    expect(f.sort).toBe('count');
    expect(f.limit).toBe(10);
  });

  it('crashes alias adds fatal', () => {
    const f = freshFilter();
    applyInputToFilter('crashes', f);
    expect([...f.levels]).toEqual(['fatal']);
  });

  it('keywords flip newOnly / staleOnly / spikingOnly', () => {
    const f = freshFilter();
    applyInputToFilter('new spiking', f);
    expect(f.newOnly).toBe(true);
    expect(f.spikingOnly).toBe(true);
  });

  it('text + words fold into filter.query', () => {
    const f = freshFilter();
    applyInputToFilter('"timeout" OOM', f);
    expect(f.query).toBe('timeout OOM');
  });

  it('explicit status: replaces the default unresolved', () => {
    const f = freshFilter();
    applyInputToFilter('status:resolved', f);
    expect([...f.statuses]).toEqual(['resolved']);
  });

  it('empty input restores defaults', () => {
    const f = freshFilter();
    f.levels = new Set(['fatal']);
    f.sinceMs = 5 * 60 * 1000;
    applyInputToFilter('', f);
    expect(f.levels.size).toBe(0);
    expect(f.sinceMs).toBe(null);
  });
});

describe('filterToQueryString', () => {
  it('empty filter (defaults) → empty string', () => {
    expect(filterToQueryString(freshFilter())).toBe('');
  });

  it('a single level', () => {
    const f = freshFilter();
    f.levels = new Set(['fatal']);
    expect(filterToQueryString(f)).toBe('fatal');
  });

  it('multiple levels emit a comma list in canonical order', () => {
    const f = freshFilter();
    f.levels = new Set(['error', 'fatal']);
    expect(filterToQueryString(f)).toBe('fatal,error');
  });

  it('non-default status emits status:csv', () => {
    const f = freshFilter();
    f.statuses = new Set<IssueStatus>(['resolved', 'muted']);
    expect(filterToQueryString(f)).toBe('status:resolved,muted');
  });

  it('compact preset for sinceMs', () => {
    const f = freshFilter();
    f.sinceMs = 5 * 60 * 1000;
    expect(filterToQueryString(f)).toBe('5m');
    f.sinceMs = 60 * 60 * 1000;
    expect(filterToQueryString(f)).toBe('1h');
    f.sinceMs = 7 * 24 * 60 * 60 * 1000;
    expect(filterToQueryString(f)).toBe('7d');
  });

  it('non-preset minutes round to "Nm"', () => {
    const f = freshFilter();
    f.sinceMs = 45 * 60 * 1000;
    expect(filterToQueryString(f)).toBe('45m');
  });

  it('emits keywords + sort + limit in canonical order', () => {
    const f = freshFilter();
    f.spikingOnly = true;
    f.newOnly = true;
    f.sort = 'count';
    f.limit = 10;
    expect(filterToQueryString(f)).toBe('spiking new sort:count limit:10');
  });
});

describe('round-trip', () => {
  // Each pair: (input typed by the user) → (canonical form rendered
  // back from the store). They needn't be byte-identical because the
  // canonicaliser may reorder facets and pick preset tokens, but
  // re-parsing the canonical form must produce the same store state.
  const cases = [
    'fatal',
    'fatal,error',
    'fatal 5m',
    'top 10 errors today',
    'most recent 100 issues',
    'spiking unresolved',
    'crashes overnight'
  ];

  for (const input of cases) {
    it(`round-trips: "${input}"`, () => {
      const f1 = freshFilter();
      applyInputToFilter(input, f1);
      const canonical = filterToQueryString(f1);
      const f2 = freshFilter();
      applyInputToFilter(canonical, f2);
      // Re-applying the canonical form must produce the same store
      // shape. We compare key fields rather than the whole object so
      // an unrelated default doesn't make the assertion brittle.
      expect([...f2.levels].sort()).toEqual([...f1.levels].sort());
      expect([...f2.statuses].sort()).toEqual([...f1.statuses].sort());
      expect(f2.sinceMs).toBe(f1.sinceMs);
      expect(f2.spikingOnly).toBe(f1.spikingOnly);
      expect(f2.newOnly).toBe(f1.newOnly);
      expect(f2.staleOnly).toBe(f1.staleOnly);
      expect(f2.sort).toBe(f1.sort);
      expect(f2.limit).toBe(f1.limit);
    });
  }
});

describe('Query (low-level) → filter mapping', () => {
  // A direct test of applyQueryToFilter on a hand-built Query, useful
  // when we want to exercise edge cases the tokenizer wouldn't reach.
  it('-debug expands to the level complement when no positive levels are set', () => {
    const f = freshFilter();
    const { query } = parseQuery('-debug');
    applyQueryToFilter(query, f);
    expect(f.levels.has('debug')).toBe(false);
    expect(f.levels.has('fatal')).toBe(true);
    expect(f.levels.size).toBe(4); // every level except debug
  });
});
