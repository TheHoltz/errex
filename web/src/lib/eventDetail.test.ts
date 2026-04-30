import { describe, expect, it } from 'vitest';
import {
  breadcrumbRelativeTime,
  dedupTags,
  partitionFrames,
  throwSiteIndex
} from './eventDetail';
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

  it('accepts Unix-seconds floats from JS SDKs (the in-the-wild format)', () => {
    // Sentry JS SDK ships breadcrumb.timestamp as a numeric float in
    // seconds, NOT an ISO string — every screenshot of "—" in the
    // breadcrumb table came from Date.parse silently returning NaN on
    // these inputs.
    const crashEpoch = Date.parse(crash) / 1000;
    expect(breadcrumbRelativeTime(crashEpoch, crashEpoch - 42)).toBe('-42s');
    expect(breadcrumbRelativeTime(crashEpoch, crashEpoch + 2)).toBe('+2s');
    expect(breadcrumbRelativeTime(crashEpoch, crashEpoch)).toBe('T-0');
  });

  it('mixes formats: ISO crash + numeric breadcrumb (or vice versa)', () => {
    const crashEpoch = Date.parse(crash) / 1000;
    expect(breadcrumbRelativeTime(crash, crashEpoch - 42)).toBe('-42s');
    expect(breadcrumbRelativeTime(crashEpoch, '2026-04-27T04:18:49.000Z')).toBe('-42s');
  });

  it('accepts numeric strings (some custom forwarders stringify the float)', () => {
    const crashEpoch = String(Date.parse(crash) / 1000);
    const bcEpoch = String(Date.parse(crash) / 1000 - 42);
    expect(breadcrumbRelativeTime(crashEpoch, bcEpoch)).toBe('-42s');
    expect(breadcrumbRelativeTime(`${crashEpoch}.5`, `${bcEpoch}.5`)).toBe('-42s');
  });

  it('accepts millisecond-scale numbers (Date.now() shape)', () => {
    const crashMs = Date.parse(crash);
    expect(breadcrumbRelativeTime(crashMs, crashMs - 42_000)).toBe('-42s');
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

describe('throwSiteIndex', () => {
  function frame(in_app: boolean | null | undefined, name = 'f'): Frame {
    return { function: name, filename: 'a.ts', in_app };
  }

  it('returns the last in_app frame index', () => {
    // Sentry orders frames oldest-first, so the throw site is the
    // last in-app entry — that's the row the user wants pre-expanded.
    const frames = [frame(false, 'lib1'), frame(true, 'app1'), frame(true, 'app2'), frame(false, 'lib2')];
    expect(throwSiteIndex(frames)).toBe(2);
  });

  it('falls back to the last frame when no in_app frames exist', () => {
    expect(throwSiteIndex([frame(false), frame(false)])).toBe(1);
  });

  it('returns -1 for empty input', () => {
    expect(throwSiteIndex([])).toBe(-1);
  });

  it('treats null/undefined in_app as library frames for ranking', () => {
    expect(throwSiteIndex([frame(null), frame(undefined), frame(true)])).toBe(2);
  });
});
