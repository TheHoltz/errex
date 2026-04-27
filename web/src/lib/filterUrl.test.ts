import { describe, expect, it } from 'vitest';
import { parseFilterParams, serializeFilterParams } from './filterUrl';
import type { IssueLevel, IssueStatus } from './types';

const allStatuses: IssueStatus[] = ['unresolved', 'resolved', 'muted', 'ignored'];
const allLevels: IssueLevel[] = ['debug', 'info', 'warning', 'error', 'fatal'];

describe('serializeFilterParams', () => {
  it('omits q when blank', () => {
    const p = serializeFilterParams({
      query: '',
      statuses: new Set<IssueStatus>(['unresolved']),
      levels: new Set<IssueLevel>(),
      sinceMs: null,
      spikingOnly: false
    });
    expect(p.has('q')).toBe(false);
  });

  it('omits s when statuses is the default {unresolved}', () => {
    const p = serializeFilterParams({
      query: '',
      statuses: new Set<IssueStatus>(['unresolved']),
      levels: new Set<IssueLevel>(),
      sinceMs: null,
      spikingOnly: false
    });
    expect(p.has('s')).toBe(false);
  });

  it('emits s as a sorted CSV when statuses diverge from the default', () => {
    const p = serializeFilterParams({
      query: '',
      statuses: new Set<IssueStatus>(['resolved', 'muted']),
      levels: new Set<IssueLevel>(),
      sinceMs: null,
      spikingOnly: false
    });
    expect(p.get('s')).toBe('muted,resolved');
  });

  it('omits l when levels is empty', () => {
    const p = serializeFilterParams({
      query: '',
      statuses: new Set<IssueStatus>(['unresolved']),
      levels: new Set<IssueLevel>(),
      sinceMs: null,
      spikingOnly: false
    });
    expect(p.has('l')).toBe(false);
  });

  it('emits l as a sorted CSV when levels are set', () => {
    const p = serializeFilterParams({
      query: '',
      statuses: new Set<IssueStatus>(['unresolved']),
      levels: new Set<IssueLevel>(['fatal', 'error']),
      sinceMs: null,
      spikingOnly: false
    });
    expect(p.get('l')).toBe('error,fatal');
  });

  it('keeps query intact (URLSearchParams handles encoding)', () => {
    const p = serializeFilterParams({
      query: 'auth fail',
      statuses: new Set<IssueStatus>(['unresolved']),
      levels: new Set<IssueLevel>(),
      sinceMs: null,
      spikingOnly: false
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
      spikingOnly: false
    };
    const round = parseFilterParams(serializeFilterParams(initial));
    expect(round.query).toBe('auth');
    expect([...round.statuses].sort()).toEqual(['muted', 'resolved']);
    expect([...round.levels].sort()).toEqual(['error', 'fatal']);
  });

  it('round-trips since=1h and spike=1', () => {
    const round = parseFilterParams(
      serializeFilterParams({
        query: '',
        statuses: new Set<IssueStatus>(['unresolved']),
        levels: new Set<IssueLevel>(),
        sinceMs: 60 * 60 * 1000,
        spikingOnly: true
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
      spikingOnly: false
    });
    expect(p.has('since')).toBe(false);
    expect(p.has('spike')).toBe(false);
  });

  it('parses recognised since presets only (1h, 24h, 7d)', () => {
    expect(parseFilterParams(new URLSearchParams('since=1h')).sinceMs).toBe(60 * 60 * 1000);
    expect(parseFilterParams(new URLSearchParams('since=24h')).sinceMs).toBe(24 * 60 * 60 * 1000);
    expect(parseFilterParams(new URLSearchParams('since=7d')).sinceMs).toBe(7 * 24 * 60 * 60 * 1000);
    expect(parseFilterParams(new URLSearchParams('since=bogus')).sinceMs).toBeNull();
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
          spikingOnly: false
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
          spikingOnly: false
        })
      );
      expect(round.levels.has(l)).toBe(true);
    }
  });
});
