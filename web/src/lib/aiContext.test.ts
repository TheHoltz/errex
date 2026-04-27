import { describe, expect, it } from 'vitest';
import { formatIssueContext } from './aiContext';
import type { Issue, NormalizedEvent, StoredEvent } from './types';

const baseIssue: Issue = {
  id: 42,
  project: 'web-frontend',
  fingerprint: '76a0087db05f2ac8',
  title: 'TypeError: oops',
  culprit: 'formatPlan in src/lib/billing.ts',
  level: 'error',
  status: 'unresolved',
  event_count: 7,
  first_seen: '2026-04-27T03:00:00Z',
  last_seen: '2026-04-27T04:30:00Z'
};

function makeEvent(payload: Record<string, unknown>): NormalizedEvent {
  const stored: StoredEvent = {
    event_id: 'eid-1',
    received_at: '2026-04-27T04:30:01Z',
    payload: payload as never
  };
  return {
    event_id: 'eid-1',
    received_at: '2026-04-27T04:30:01Z',
    level: 'error',
    release: 'web@1.2.3',
    environment: 'production',
    exception: {
      type: 'TypeError',
      value: 'oops',
      frames: [
        { function: 'a', filename: 'a.ts', lineno: 1, in_app: true },
        { function: 'b', filename: 'b.ts', lineno: 2, in_app: false }
      ]
    },
    breadcrumbs: [
      { timestamp: '2026-04-27T04:29:00Z', category: 'navigation', message: '/' },
      { timestamp: '2026-04-27T04:29:30Z', category: 'ui.click', message: 'btn.retry' }
    ],
    tags: { browser: 'Chrome 134', os: 'macOS 15.3' },
    raw: stored
  };
}

describe('formatIssueContext', () => {
  it('renders the core sections when event is present', () => {
    const ev = makeEvent({
      contexts: {
        os: { name: 'macOS', version: '15.3' },
        browser: { name: 'Chrome', version: '134' }
      },
      user: { id: 'u1', email: 'a@b.c' },
      request: { url: 'https://x/y', method: 'GET' }
    });
    const out = formatIssueContext(baseIssue, ev);

    expect(out).toContain('# TypeError: oops');
    expect(out).toContain('Project: web-frontend');
    expect(out).toContain('Fingerprint: 76a0087db05f2ac8');
    expect(out).toContain('Event count: 7');
    expect(out).toContain('## Exception');
    expect(out).toContain('TypeError');
    expect(out).toContain('## Stack trace');
    expect(out).toContain('a.ts:1');
    expect(out).toContain('## Breadcrumbs');
    expect(out).toContain('navigation');
    expect(out).toContain('btn.retry');
    expect(out).toContain('## Tags');
    expect(out).toContain('browser: Chrome 134');
    expect(out).toContain('## Context');
    expect(out).toContain('macOS');
    expect(out).toContain('## User');
    expect(out).toContain('a@b.c');
    expect(out).toContain('## Request');
    expect(out).toContain('https://x/y');
    expect(out).toContain('## Raw payload');
  });

  it('falls back to issue-only output when event is null', () => {
    const out = formatIssueContext(baseIssue, null);
    expect(out).toContain('# TypeError: oops');
    expect(out).toContain('No event payload available');
    expect(out).not.toContain('## Stack trace');
  });

  it('marks in-app frames distinctly', () => {
    const out = formatIssueContext(baseIssue, makeEvent({}));
    expect(out).toMatch(/a\.ts:1.*in_app/);
    expect(out).not.toMatch(/b\.ts:2.*in_app/);
  });

  it('omits empty optional sections cleanly', () => {
    const ev = makeEvent({});
    // Strip out exception and breadcrumbs to check empty handling.
    const stripped: NormalizedEvent = {
      ...ev,
      exception: null,
      breadcrumbs: [],
      tags: {}
    };
    const out = formatIssueContext(baseIssue, stripped);
    expect(out).not.toContain('## Stack trace');
    expect(out).not.toContain('## Breadcrumbs');
    expect(out).not.toContain('## Tags');
    // Raw payload always present.
    expect(out).toContain('## Raw payload');
  });
});
