// Pure utility tests. The simplest possible vitest target — confirms the
// runner, jsdom env, and module resolution are wired before we trust the
// rest of the suite.

import { describe, expect, it } from 'vitest';
import { cn, relativeTime, shortFingerprint } from './utils';

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
