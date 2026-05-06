// Pure utility tests. The simplest possible vitest target — confirms the
// runner, jsdom env, and module resolution are wired before we trust the
// rest of the suite.

import { describe, expect, it } from 'vitest';
import { cn, countTier, midTruncatePath, relativeTime, shortFingerprint } from './utils';

describe('cn', () => {
  it('joins truthy class names', () => {
    expect(cn('a', 'b')).toBe('a b');
  });

  it('drops falsy values', () => {
    expect(cn('a', false, null, undefined, 'b')).toBe('a b');
  });

  it('twMerge resolves conflicting tailwind utilities (last wins)', () => {
    expect(cn('px-2', 'px-4')).toBe('px-4');
  });
});

describe('shortFingerprint', () => {
  it('truncates long fingerprints with an ellipsis', () => {
    expect(shortFingerprint('abcdef0123456789')).toBe('abcdef0123…');
  });

  it('leaves short fingerprints alone', () => {
    expect(shortFingerprint('abc123')).toBe('abc123');
  });
});

describe('midTruncatePath', () => {
  it('returns the input unchanged when shorter than max', () => {
    expect(midTruncatePath('/short/path.js', 32)).toBe('/short/path.js');
  });

  it('preserves the trailing segment when truncating', () => {
    const out = midTruncatePath('/a/b/c/d/e/long/long/long/bundle-abc123.js', 32);
    expect(out.endsWith('/bundle-abc123.js')).toBe(true);
    expect(out).toContain('…');
    expect(out.length).toBeLessThanOrEqual(32);
  });

  it('handles paths with no separator by truncating from the left', () => {
    expect(midTruncatePath('aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.js', 16)).toMatch(/^…/);
  });

  it('handles backslash separators (Windows)', () => {
    const out = midTruncatePath('C\\very\\long\\folder\\bundle.js', 20);
    expect(out.endsWith('\\bundle.js')).toBe(true);
  });
});

describe('countTier', () => {
  it('returns "low" below 10', () => {
    expect(countTier(0)).toBe('low');
    expect(countTier(1)).toBe('low');
    expect(countTier(9)).toBe('low');
  });

  it('returns "mid" from 10 through 99', () => {
    expect(countTier(10)).toBe('mid');
    expect(countTier(50)).toBe('mid');
    expect(countTier(99)).toBe('mid');
  });

  it('returns "high" at 100 and above', () => {
    expect(countTier(100)).toBe('high');
    expect(countTier(1000)).toBe('high');
  });
});

describe('relativeTime', () => {
  // Pin "now" so the tests are deterministic across timezones.
  const now = Date.parse('2026-04-26T12:00:00Z');

  it('formats sub-minute deltas in seconds', () => {
    const t = new Date(now - 30_000).toISOString();
    expect(relativeTime(t, now)).toMatch(/30 seconds ago|30s ago/);
  });

  it('formats sub-hour deltas in minutes', () => {
    const t = new Date(now - 5 * 60_000).toISOString();
    expect(relativeTime(t, now)).toMatch(/5 minutes ago|5m ago/);
  });

  it('formats sub-day deltas in hours', () => {
    const t = new Date(now - 3 * 60 * 60_000).toISOString();
    expect(relativeTime(t, now)).toMatch(/3 hours ago|3h ago/);
  });

  it('formats day-or-more deltas in days', () => {
    const t = new Date(now - 2 * 24 * 60 * 60_000).toISOString();
    expect(relativeTime(t, now)).toMatch(/2 days ago|2d ago/);
  });
});
