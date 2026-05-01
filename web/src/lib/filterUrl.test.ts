import { describe, expect, it } from 'vitest';
import { ALL_SORTS, parseFilterParams, serializeFilterParams } from './filterUrl';
import type { IssueLevel, IssueStatus, SortKey } from './types';

const allStatuses: IssueStatus[] = ['unresolved', 'resolved', 'muted', 'ignored'];
const allLevels: IssueLevel[] = ['debug', 'info', 'warning', 'error', 'fatal'];

describe('serializeFilterParams', () => {
  it('omits q when blank', () => {
    const p = serializeFilterParams({
      query: '',
      statuses: new Set<IssueStatus>(['unresolved']),
      levels: new Set<IssueLevel>(),
      sinceMs: null,
      spikingOnly: false,
      sort: 'recent'
    });
    expect(p.has('q')).toBe(false);
  });

  it('omits s when statuses is the default {unresolved}', () => {
    const p = serializeFilterParams({
      query: '',
      statuses: new Set<IssueStatus>(['unresolved']),
      levels: new Set<IssueLevel>(),
      sinceMs: null,
      spikingOnly: false,
      sort: 'recent'
    });
    expect(p.has('s')).toBe(false);
  });

  it('emits s as a sorted CSV when statuses diverge from the default', () => {
    const p = serializeFilterParams({
      query: '',
      statuses: new Set<IssueStatus>(['resolved', 'muted']),
      levels: new Set<IssueLevel>(),
      sinceMs: null,
      spikingOnly: false,
      sort: 'recent'
    });
    expect(p.get('s')).toBe('muted,resolved');
  });

  it('omits l when levels is empty', () => {
    const p = serializeFilterParams({
      query: '',
      statuses: new Set<IssueStatus>(['unresolved']),
      levels: new Set<IssueLevel>(),
      sinceMs: null,
      spikingOnly: false,
      sort: 'recent'
    });
    expect(p.has('l')).toBe(false);
  });

  it('emits l as a sorted CSV when levels are set', () => {
    const p = serializeFilterParams({
      query: '',
      statuses: new Set<IssueStatus>(['unresolved']),
      levels: new Set<IssueLevel>(['fatal', 'error']),
      sinceMs: null,
      spikingOnly: false,
      sort: 'recent'
    });
    expect(p.get('l')).toBe('error,fatal');
  });

  it('keeps query intact (URLSearchParams handles encoding)', () => {
    const p = serializeFilterParams({
      query: 'auth fail',
      statuses: new Set<IssueStatus>(['unresolved']),
      levels: new Set<IssueLevel>(),
      sinceMs: null,
      spikingOnly: false,
      sort: 'recent'
    });
    expect(p.get('q')).toBe('auth fail');
  });
});

describe('parseFilterParams', () => {
  it('returns the defaults when nothing is set', () => {
    const f = parseFilterParams(new URLSearchParams(''));
    expect(f.query).toBe('');
    expect([...f.statuses]).toEqual(['unresolved']);
    expect(f.levels.size).toBe(0);
    expect(f.sort).toBe('recent');
  });

  it('parses q', () => {
    const f = parseFilterParams(new URLSearchParams('q=auth'));
    expect(f.query).toBe('auth');
  });

  it('parses s as a CSV of valid IssueStatus tokens; drops unknowns', () => {
    const f = parseFilterParams(new URLSearchParams('s=resolved,muted,bogus'));
    expect([...f.statuses].sort()).toEqual(['muted', 'resolved']);
  });

  it('falls back to {unresolved} when s is present but yields nothing valid', () => {
    const f = parseFilterParams(new URLSearchParams('s=bogus,xxx'));
    expect([...f.statuses]).toEqual(['unresolved']);
  });

  it('parses l as a CSV of valid IssueLevel tokens; drops unknowns', () => {
    const f = parseFilterParams(new URLSearchParams('l=error,fatal,xxx'));
    expect([...f.levels].sort()).toEqual(['error', 'fatal']);
  });

  it('round-trips a non-default state', () => {
    const initial = {
      query: 'auth',
      statuses: new Set<IssueStatus>(['resolved', 'muted']),
      levels: new Set<IssueLevel>(['error', 'fatal']),
      sinceMs: null,
      spikingOnly: false,
      sort: 'count' as SortKey
    };
    const round = parseFilterParams(serializeFilterParams(initial));
    expect(round.query).toBe('auth');
    expect([...round.statuses].sort()).toEqual(['muted', 'resolved']);
    expect([...round.levels].sort()).toEqual(['error', 'fatal']);
    expect(round.sort).toBe('count');
  });

  it('round-trips since=1h and spike=1', () => {
    const round = parseFilterParams(
      serializeFilterParams({
        query: '',
        statuses: new Set<IssueStatus>(['unresolved']),
        levels: new Set<IssueLevel>(),
        sinceMs: 60 * 60 * 1000,
        spikingOnly: true,
        sort: 'recent'
      })
    );
    expect(round.sinceMs).toBe(60 * 60 * 1000);
    expect(round.spikingOnly).toBe(true);
  });

  it('omits since/spike from the URL when defaults', () => {
    const p = serializeFilterParams({
      query: '',
      statuses: new Set<IssueStatus>(['unresolved']),
      levels: new Set<IssueLevel>(),
      sinceMs: null,
      spikingOnly: false,
      sort: 'recent'
    });
    expect(p.has('since')).toBe(false);
    expect(p.has('spike')).toBe(false);
  });

  it('parses recognised since presets only (5m, 15m, 1h, 24h, 7d)', () => {
    expect(parseFilterParams(new URLSearchParams('since=5m')).sinceMs).toBe(5 * 60 * 1000);
    expect(parseFilterParams(new URLSearchParams('since=15m')).sinceMs).toBe(15 * 60 * 1000);
    expect(parseFilterParams(new URLSearchParams('since=1h')).sinceMs).toBe(60 * 60 * 1000);
    expect(parseFilterParams(new URLSearchParams('since=24h')).sinceMs).toBe(24 * 60 * 60 * 1000);
    expect(parseFilterParams(new URLSearchParams('since=7d')).sinceMs).toBe(7 * 24 * 60 * 60 * 1000);
    expect(parseFilterParams(new URLSearchParams('since=bogus')).sinceMs).toBeNull();
    expect(parseFilterParams(new URLSearchParams('since=10m')).sinceMs).toBeNull();
  });

  it('round-trips since=5m and since=15m', () => {
    for (const [token, ms] of [['5m', 5 * 60 * 1000], ['15m', 15 * 60 * 1000]] as const) {
      const params = serializeFilterParams({
        query: '',
        statuses: new Set(['unresolved']),
        levels: new Set(),
        sinceMs: ms,
        spikingOnly: false,
        sort: 'recent'
      });
      expect(params.get('since')).toBe(token);
      expect(parseFilterParams(params).sinceMs).toBe(ms);
    }
  });

  it('parses spike=1 as true; anything else as false', () => {
    expect(parseFilterParams(new URLSearchParams('spike=1')).spikingOnly).toBe(true);
    expect(parseFilterParams(new URLSearchParams('spike=0')).spikingOnly).toBe(false);
    expect(parseFilterParams(new URLSearchParams('')).spikingOnly).toBe(false);
  });

  it('treats every recognised status/level as round-trippable', () => {
    for (const s of allStatuses) {
      const round = parseFilterParams(
        serializeFilterParams({
          query: '',
          statuses: new Set<IssueStatus>([s]),
          levels: new Set<IssueLevel>(),
          sinceMs: null,
          spikingOnly: false,
          sort: 'recent'
        })
      );
      expect(round.statuses.has(s)).toBe(true);
    }
    for (const l of allLevels) {
      const round = parseFilterParams(
        serializeFilterParams({
          query: '',
          statuses: new Set<IssueStatus>(['unresolved']),
          levels: new Set<IssueLevel>([l]),
          sinceMs: null,
          spikingOnly: false,
          sort: 'recent'
        })
      );
      expect(round.levels.has(l)).toBe(true);
    }
  });
});

describe('serializeFilterParams + sort', () => {
  it('omits sort when default (recent)', () => {
    const p = serializeFilterParams({
      query: '',
      statuses: new Set<IssueStatus>(['unresolved']),
      levels: new Set<IssueLevel>(),
      sinceMs: null,
      spikingOnly: false,
      sort: 'recent'
    });
    expect(p.has('sort')).toBe(false);
  });

  it('emits sort token for non-default keys', () => {
    for (const s of ALL_SORTS.filter((k) => k !== 'recent')) {
      const p = serializeFilterParams({
        query: '',
        statuses: new Set<IssueStatus>(['unresolved']),
        levels: new Set<IssueLevel>(),
        sinceMs: null,
        spikingOnly: false,
        sort: s
      });
      expect(p.get('sort')).toBe(s);
    }
  });
});

describe('parseFilterParams + sort', () => {
  it('defaults to recent when sort is absent', () => {
    expect(parseFilterParams(new URLSearchParams('')).sort).toBe('recent');
  });

  it('parses every recognised sort token', () => {
    for (const s of ALL_SORTS) {
      expect(parseFilterParams(new URLSearchParams(`sort=${s}`)).sort).toBe(s);
    }
  });

  it('falls back to recent on an unknown token', () => {
    expect(parseFilterParams(new URLSearchParams('sort=bogus')).sort).toBe('recent');
  });

  it('round-trips every sort key', () => {
    for (const s of ALL_SORTS) {
      const round = parseFilterParams(
        serializeFilterParams({
          query: '',
          statuses: new Set<IssueStatus>(['unresolved']),
          levels: new Set<IssueLevel>(),
          sinceMs: null,
          spikingOnly: false,
          sort: s
        })
      );
      expect(round.sort).toBe(s);
    }
  });

  it('preserves a regex query verbatim through the URL (no special handling)', () => {
    const p = serializeFilterParams({
      query: '/Error.*403/',
      statuses: new Set<IssueStatus>(['unresolved']),
      levels: new Set<IssueLevel>(),
      sinceMs: null,
      spikingOnly: false,
      sort: 'recent'
    });
    expect(p.get('q')).toBe('/Error.*403/');
    expect(parseFilterParams(p).query).toBe('/Error.*403/');
  });
});
