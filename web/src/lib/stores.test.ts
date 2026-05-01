// Tests for the reactive global stores. Each test resets the singletons
// so order doesn't matter.

import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import {
  filter,
  issues,
  load,
  parseQuery,
  projects,
  selection,
  visibleIssues
} from './stores.svelte';
import type { Issue, IssueLevel, IssueStatus } from './types';

function issue(over: Partial<Issue>): Issue {
  return {
    id: 0,
    project: 'p',
    fingerprint: 'fp',
    title: 'T',
    culprit: null,
    level: null,
    status: 'unresolved',
    event_count: 1,
    first_seen: '2026-01-01T00:00:00Z',
    last_seen: '2026-01-01T00:00:00Z',
    ...over
  };
}

beforeEach(() => {
  issues.reset([]);
  filter.query = '';
  filter.statuses = new Set<IssueStatus>(['unresolved']);
  filter.levels = new Set<IssueLevel>();
  filter.sinceMs = null;
  filter.spikingOnly = false;
  filter.sort = 'recent';
  projects.current = 'p';
  selection.issueId = null;
  selection.event = null;
  load.initialLoad = false;
});

afterEach(() => {
  issues.reset([]);
});

describe('IssuesStore', () => {
  it('reset replaces the map', () => {
    issues.reset([issue({ id: 1, title: 'a' }), issue({ id: 2, title: 'b' })]);
    expect(issues.list.map((i) => i.id).sort()).toEqual([1, 2]);
    issues.reset([issue({ id: 3, title: 'c' })]);
    expect(issues.list.map((i) => i.id)).toEqual([3]);
  });

  it('upsert adds and updates by id', () => {
    issues.upsert(issue({ id: 1, title: 'first' }));
    issues.upsert(issue({ id: 1, title: 'second' }));
    expect(issues.list.length).toBe(1);
    expect(issues.list[0]?.title).toBe('second');
  });

  it('list returns every issue (order is no longer guaranteed)', () => {
    issues.reset([
      issue({ id: 1, last_seen: '2026-01-01T00:00:00Z' }),
      issue({ id: 2, last_seen: '2026-01-03T00:00:00Z' }),
      issue({ id: 3, last_seen: '2026-01-02T00:00:00Z' })
    ]);
    expect(issues.list.map((i) => i.id).sort()).toEqual([1, 2, 3]);
  });

  it('visibleIssues default sort = recent (last_seen DESC, preserves prior behavior)', () => {
    issues.reset([
      issue({ id: 1, last_seen: '2026-01-01T00:00:00Z' }),
      issue({ id: 2, last_seen: '2026-01-03T00:00:00Z' }),
      issue({ id: 3, last_seen: '2026-01-02T00:00:00Z' })
    ]);
    expect(visibleIssues().map((i) => i.id)).toEqual([2, 3, 1]);
  });
});

describe('visibleIssues', () => {
  it('filters by current project', () => {
    issues.reset([
      issue({ id: 1, project: 'p' }),
      issue({ id: 2, project: 'other' })
    ]);
    projects.current = 'p';
    expect(visibleIssues().map((i) => i.id)).toEqual([1]);
  });

  it('uses server-side issue.status (NOT client localStorage)', () => {
    issues.reset([
      issue({ id: 1, status: 'unresolved' }),
      issue({ id: 2, status: 'resolved' }),
      issue({ id: 3, status: 'muted' })
    ]);
    filter.statuses = new Set<IssueStatus>(['unresolved']);
    expect(visibleIssues().map((i) => i.id)).toEqual([1]);

    filter.statuses = new Set<IssueStatus>(['resolved', 'muted']);
    const ids = visibleIssues().map((i) => i.id).sort();
    expect(ids).toEqual([2, 3]);
  });

  it('filters by query against title, culprit, fingerprint (case-insensitive)', () => {
    issues.reset([
      issue({ id: 1, title: 'TypeError: x', fingerprint: 'abc' }),
      issue({ id: 2, title: 'NetworkError', culprit: 'fetch in api.ts', fingerprint: 'def' }),
      issue({ id: 3, title: 'unrelated', fingerprint: 'ghi' })
    ]);
    filter.query = 'TYPE';
    expect(visibleIssues().map((i) => i.id)).toEqual([1]);
    filter.query = 'fetch';
    expect(visibleIssues().map((i) => i.id)).toEqual([2]);
    filter.query = 'ghi';
    expect(visibleIssues().map((i) => i.id)).toEqual([3]);
  });
});

describe('FilterStore.toggleStatus', () => {
  it('adds and removes statuses idempotently', () => {
    filter.statuses = new Set();
    filter.toggleStatus('resolved');
    expect(filter.statuses.has('resolved')).toBe(true);
    filter.toggleStatus('resolved');
    expect(filter.statuses.has('resolved')).toBe(false);
  });
});

describe('FilterStore.toggleLevel', () => {
  it('adds and removes levels idempotently', () => {
    filter.levels = new Set();
    filter.toggleLevel('error');
    expect(filter.levels.has('error')).toBe(true);
    filter.toggleLevel('error');
    expect(filter.levels.has('error')).toBe(false);
  });
});

describe('visibleIssues + sinceMs filter', () => {
  const NOW = Date.parse('2026-04-27T12:00:00Z');

  it('keeps every issue when sinceMs is null', () => {
    issues.reset([
      issue({ id: 1, first_seen: '2026-04-27T11:30:00Z', status: 'unresolved' }),
      issue({ id: 2, first_seen: '2026-04-26T00:00:00Z', status: 'unresolved' })
    ]);
    filter.sinceMs = null;
    expect(visibleIssues({ now: NOW }).map((i) => i.id).sort()).toEqual([1, 2]);
  });

  it('keeps only issues first seen within sinceMs of now', () => {
    issues.reset([
      issue({ id: 1, first_seen: '2026-04-27T11:30:00Z', status: 'unresolved' }), // 30 min old
      issue({ id: 2, first_seen: '2026-04-27T10:30:00Z', status: 'unresolved' }), // 90 min old
      issue({ id: 3, first_seen: '2026-04-27T11:59:30Z', status: 'unresolved' })  //  30s old
    ]);
    filter.sinceMs = 60 * 60 * 1000; // 1h
    expect(visibleIssues({ now: NOW }).map((i) => i.id).sort()).toEqual([1, 3]);
  });

  it('drops issues with malformed first_seen when a since filter is active', () => {
    issues.reset([
      issue({ id: 1, first_seen: 'not-a-date', status: 'unresolved' }),
      issue({ id: 2, first_seen: '2026-04-27T11:30:00Z', status: 'unresolved' })
    ]);
    filter.sinceMs = 60 * 60 * 1000;
    expect(visibleIssues({ now: NOW }).map((i) => i.id)).toEqual([2]);
  });
});

describe('visibleIssues + spikingOnly filter', () => {
  it('passes through when spikingOnly is false', () => {
    issues.reset([
      issue({ id: 1, status: 'unresolved' }),
      issue({ id: 2, status: 'unresolved' })
    ]);
    filter.spikingOnly = false;
    expect(visibleIssues({ isSpiking: () => false }).map((i) => i.id).sort()).toEqual([1, 2]);
  });

  it('keeps only issues for which the predicate returns true', () => {
    issues.reset([
      issue({ id: 1, status: 'unresolved' }),
      issue({ id: 2, status: 'unresolved' }),
      issue({ id: 3, status: 'unresolved' })
    ]);
    filter.spikingOnly = true;
    const isSpiking = (id: number) => id === 2;
    expect(visibleIssues({ isSpiking }).map((i) => i.id)).toEqual([2]);
  });

  it('treats a missing predicate as nothing-spikes when spikingOnly is true', () => {
    issues.reset([issue({ id: 1, status: 'unresolved' })]);
    filter.spikingOnly = true;
    expect(visibleIssues().map((i) => i.id)).toEqual([]);
  });
});

describe('visibleIssues + level filter', () => {
  it('keeps all levels when the level filter is empty', () => {
    issues.reset([
      issue({ id: 1, level: 'error', status: 'unresolved' }),
      issue({ id: 2, level: 'warning', status: 'unresolved' }),
      issue({ id: 3, level: null, status: 'unresolved' })
    ]);
    filter.levels = new Set();
    expect(visibleIssues().map((i) => i.id).sort()).toEqual([1, 2, 3]);
  });

  it('narrows to selected levels (case-insensitive against issue.level)', () => {
    issues.reset([
      issue({ id: 1, level: 'error', status: 'unresolved' }),
      issue({ id: 2, level: 'WARNING', status: 'unresolved' }),
      issue({ id: 3, level: 'fatal', status: 'unresolved' })
    ]);
    filter.levels = new Set<IssueLevel>(['error', 'fatal']);
    expect(visibleIssues().map((i) => i.id).sort()).toEqual([1, 3]);
  });

  it('drops issues whose level is null when a level filter is active', () => {
    issues.reset([
      issue({ id: 1, level: null, status: 'unresolved' }),
      issue({ id: 2, level: 'error', status: 'unresolved' })
    ]);
    filter.levels = new Set<IssueLevel>(['error']);
    expect(visibleIssues().map((i) => i.id)).toEqual([2]);
  });
});

describe('visibleIssues + sort', () => {
  function fixture() {
    // Insertion order intentionally shuffled so no single test's expected
    // permutation matches insertion order (a no-op comparator would fail
    // every test, not pass by accident on `oldest`).
    issues.reset([
      issue({ id: 3, event_count: 1,  first_seen: '2026-01-03T00:00:00Z', last_seen: '2026-01-06T00:00:00Z' }),
      issue({ id: 1, event_count: 3,  first_seen: '2026-01-01T00:00:00Z', last_seen: '2026-01-05T00:00:00Z' }),
      issue({ id: 4, event_count: 7,  first_seen: '2026-01-04T00:00:00Z', last_seen: '2026-01-03T00:00:00Z' }),
      issue({ id: 2, event_count: 11, first_seen: '2026-01-02T00:00:00Z', last_seen: '2026-01-04T00:00:00Z' })
    ]);
  }

  it('sort=stale orders by last_seen ASC', () => {
    fixture();
    filter.sort = 'stale';
    expect(visibleIssues().map((i) => i.id)).toEqual([4, 2, 1, 3]);
  });

  it('sort=count orders by event_count DESC', () => {
    fixture();
    filter.sort = 'count';
    expect(visibleIssues().map((i) => i.id)).toEqual([2, 4, 1, 3]);
  });

  it('sort=created orders by first_seen DESC', () => {
    fixture();
    filter.sort = 'created';
    expect(visibleIssues().map((i) => i.id)).toEqual([4, 3, 2, 1]);
  });

  it('sort=oldest orders by first_seen ASC', () => {
    fixture();
    filter.sort = 'oldest';
    expect(visibleIssues().map((i) => i.id)).toEqual([1, 2, 3, 4]);
  });

  it('breaks ties on id ASC for determinism', () => {
    issues.reset([
      issue({ id: 9, event_count: 5, last_seen: '2026-01-01T00:00:00Z' }),
      issue({ id: 5, event_count: 5, last_seen: '2026-01-01T00:00:00Z' }),
      issue({ id: 7, event_count: 5, last_seen: '2026-01-01T00:00:00Z' })
    ]);
    filter.sort = 'count';
    expect(visibleIssues().map((i) => i.id)).toEqual([5, 7, 9]);
  });
});

describe('parseQuery', () => {
  it('returns empty for blank input', () => {
    expect(parseQuery('').mode).toBe('empty');
    expect(parseQuery('   ').mode).toBe('empty');
  });

  it('returns substring for plain text', () => {
    const p = parseQuery('Error 403');
    expect(p.mode).toBe('substring');
    if (p.mode === 'substring') expect(p.q).toBe('Error 403');
  });

  it('returns substring when query is just /', () => {
    expect(parseQuery('/').mode).toBe('substring');
  });

  it('returns regex for /pattern with optional trailing slash', () => {
    const a = parseQuery('/Error.*403');
    const b = parseQuery('/Error.*403/');
    expect(a.mode).toBe('regex');
    expect(b.mode).toBe('regex');
    if (a.mode === 'regex' && b.mode === 'regex') {
      expect(a.re.test('Error: HTTP status code: 403')).toBe(true);
      expect(b.re.test('Error: HTTP status code: 403')).toBe(true);
    }
  });

  it('regex is case-insensitive', () => {
    const p = parseQuery('/error/');
    expect(p.mode).toBe('regex');
    if (p.mode === 'regex') expect(p.re.test('ERROR')).toBe(true);
  });

  it('returns badRegex for invalid syntax', () => {
    const p = parseQuery('/Error[/');
    expect(p.mode).toBe('badRegex');
  });

  it('returns badRegex when source is empty (//)', () => {
    expect(parseQuery('//').mode).toBe('badRegex');
  });
});

describe('visibleIssues + regex query', () => {
  it('regex query matches title and culprit, skips fingerprint', () => {
    issues.reset([
      issue({ id: 1, title: 'Error: HTTP status code: 403', culprit: 'ci in core-A.js', fingerprint: '403' }),
      issue({ id: 2, title: 'Error: HTTP status code: 500', culprit: 'ci in core-B.js', fingerprint: '500' }),
      issue({ id: 3, title: 'web boot abc', culprit: null, fingerprint: '403' }),
      issue({ id: 4, title: 'GSAP target not found', culprit: null, fingerprint: 'xyz' })
    ]);
    filter.query = '/Error.*403/';
    expect(visibleIssues().map((i) => i.id)).toEqual([1]);
  });

  it('regex query is case-insensitive', () => {
    issues.reset([
      issue({ id: 1, title: 'NetworkError', culprit: null }),
      issue({ id: 2, title: 'TypeError', culprit: null })
    ]);
    filter.query = '/network/';
    expect(visibleIssues().map((i) => i.id)).toEqual([1]);
  });

  it('invalid regex falls back to literal substring of the full string (including leading /)', () => {
    issues.reset([
      issue({ id: 1, title: 'message includes /Error[ literal', culprit: null }),
      issue({ id: 2, title: 'unrelated', culprit: null })
    ]);
    filter.query = '/Error[';
    expect(visibleIssues().map((i) => i.id)).toEqual([1]);
  });

  it('substring path still works for non-regex queries (covers fingerprint)', () => {
    issues.reset([
      issue({ id: 1, title: 'a', fingerprint: 'cafebabe' }),
      issue({ id: 2, title: 'b', fingerprint: 'deadbeef' })
    ]);
    filter.query = 'cafe';
    expect(visibleIssues().map((i) => i.id)).toEqual([1]);
  });
});
