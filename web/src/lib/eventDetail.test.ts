import { describe, expect, it } from 'vitest';
import { breadcrumbRelativeTime, dedupTags, partitionFrames } from './eventDetail';
import type { Frame } from './types';

describe('dedupTags', () => {
  it('drops `x.name` when `x` is also present', () => {
    const out = dedupTags({
      browser: 'Chrome 134',
      'browser.name': 'Chrome',
      os: 'macOS 15.3',
      'os.name': 'macOS'
    });
    expect(out).toEqual({ browser: 'Chrome 134', os: 'macOS 15.3' });
  });

  it('keeps unrelated `.name` keys', () => {
    const out = dedupTags({ 'feature.name': 'billing' });
    expect(out).toEqual({ 'feature.name': 'billing' });
  });

  it('drops `device.family` when `device` is present', () => {
    const out = dedupTags({ device: 'MacBookPro18,3', 'device.family': 'Mac' });
    expect(out).toEqual({ device: 'MacBookPro18,3' });
  });

  it('handles empty input', () => {
    expect(dedupTags({})).toEqual({});
  });
});

describe('breadcrumbRelativeTime', () => {
  const crash = '2026-04-27T04:19:31.000Z';

  it('returns T-0 for the crash timestamp itself', () => {
    expect(breadcrumbRelativeTime(crash, crash)).toBe('T-0');
  });

  it('returns negative seconds for breadcrumbs before the crash', () => {
    expect(breadcrumbRelativeTime(crash, '2026-04-27T04:18:49.000Z')).toBe('-42s');
    expect(breadcrumbRelativeTime(crash, '2026-04-27T04:19:30.000Z')).toBe('-1s');
  });

  it('falls back to em-dash when breadcrumb timestamp is missing', () => {
    expect(breadcrumbRelativeTime(crash, undefined)).toBe('—');
    expect(breadcrumbRelativeTime(crash, null)).toBe('—');
  });

  it('switches to minutes past 90 seconds', () => {
    expect(breadcrumbRelativeTime(crash, '2026-04-27T04:17:00.000Z')).toBe('-2m31s');
    expect(breadcrumbRelativeTime(crash, '2026-04-27T04:00:00.000Z')).toBe('-19m31s');
  });

  it('handles breadcrumbs after the crash with positive prefix', () => {
    expect(breadcrumbRelativeTime(crash, '2026-04-27T04:19:33.000Z')).toBe('+2s');
  });
});

describe('partitionFrames', () => {
  function frame(in_app: boolean | null | undefined): Frame {
    return { function: 'f', filename: 'a.ts', in_app };
  }

  it('counts in_app vs library frames', () => {
    const out = partitionFrames([frame(true), frame(true), frame(false), frame(null)]);
    expect(out).toEqual({ inApp: 2, lib: 2 });
  });

  it('treats undefined/null in_app as library frames', () => {
    expect(partitionFrames([frame(undefined)])).toEqual({ inApp: 0, lib: 1 });
  });

  it('handles empty input', () => {
    expect(partitionFrames([])).toEqual({ inApp: 0, lib: 0 });
  });
});
