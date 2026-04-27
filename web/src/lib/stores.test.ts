// Tests for the reactive global stores. Each test resets the singletons
// so order doesn't matter.

import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import {
  filter,
  issues,
  load,
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

  it('list orders by last_seen desc', () => {
    issues.reset([
      issue({ id: 1, last_seen: '2026-01-01T00:00:00Z' }),
      issue({ id: 2, last_seen: '2026-01-03T00:00:00Z' }),
      issue({ id: 3, last_seen: '2026-01-02T00:00:00Z' })
    ]);
    expect(issues.list.map((i) => i.id)).toEqual([2, 3, 1]);
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
