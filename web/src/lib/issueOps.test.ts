// Tests for the issueOps helper: each mutator hits the API exactly once,
// passes the right payload, and queues a toast with an Undo that reverses
// the change.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { api } from './api';
import { toggleIgnore, toggleMute, toggleResolve } from './issueOps';
import { toast } from './toast.svelte';
import type { Issue, IssueStatus } from './types';

function issue(over: Partial<Issue> = {}): Issue {
  return {
    id: 7,
    project: 'p',
    fingerprint: 'fp',
    title: 'T',
    culprit: null,
    level: null,
    status: 'unresolved',
    event_count: 1,
    first_seen: 'now',
    last_seen: 'now',
    ...over
  };
}

beforeEach(() => {
  toast.list = [];
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('toggleResolve', () => {
  it('moves unresolved → resolved and posts to the API', async () => {
    const spy = vi.spyOn(api, 'setStatus').mockResolvedValue(issue({ status: 'resolved' }));
    await toggleResolve(issue({ status: 'unresolved' }));
    expect(spy).toHaveBeenCalledWith(7, 'resolved');
    expect(toast.list[0]?.message).toMatch(/resolvida/i);
  });

  it('moves resolved → unresolved (reopen)', async () => {
    const spy = vi.spyOn(api, 'setStatus').mockResolvedValue(issue({ status: 'unresolved' }));
    await toggleResolve(issue({ status: 'resolved' }));
    expect(spy).toHaveBeenCalledWith(7, 'unresolved');
    expect(toast.list[0]?.message).toMatch(/reaberta/i);
  });

  it('toast Undo calls setStatus with the previous value', async () => {
    const spy = vi.spyOn(api, 'setStatus').mockResolvedValue(issue());
    await toggleResolve(issue({ status: 'unresolved' }));
    spy.mockClear();
    toast.list[0]?.undo?.();
    expect(spy).toHaveBeenCalledWith(7, 'unresolved');
  });

  it('shows an error toast if the API throws', async () => {
    vi.spyOn(api, 'setStatus').mockRejectedValueOnce(new Error('boom'));
    await expect(toggleResolve(issue())).rejects.toThrow();
    expect(toast.list[0]?.variant).toBe('error');
  });
});

describe('toggleMute', () => {
  it.each<[IssueStatus, IssueStatus]>([
    ['unresolved', 'muted'],
    ['resolved', 'muted'],
    ['muted', 'unresolved']
  ])('from %s → %s', async (from, to) => {
    const spy = vi.spyOn(api, 'setStatus').mockResolvedValue(issue({ status: to }));
    await toggleMute(issue({ status: from }));
    expect(spy).toHaveBeenCalledWith(7, to);
  });
});

describe('toggleIgnore', () => {
  it('unresolved → ignored', async () => {
    const spy = vi.spyOn(api, 'setStatus').mockResolvedValue(issue({ status: 'ignored' }));
    await toggleIgnore(issue({ status: 'unresolved' }));
    expect(spy).toHaveBeenCalledWith(7, 'ignored');
  });

  it('ignored → unresolved', async () => {
    const spy = vi.spyOn(api, 'setStatus').mockResolvedValue(issue({ status: 'unresolved' }));
    await toggleIgnore(issue({ status: 'ignored' }));
    expect(spy).toHaveBeenCalledWith(7, 'unresolved');
  });
});
