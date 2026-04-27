// Tests for the assignee-only ActionsStore. (Status is server-driven and
// tested elsewhere via api.test.ts + stores.test.ts.)

import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { actions } from './actions.svelte';
import type { Issue } from './types';

const sample: Issue = {
  id: 1,
  project: 'p',
  fingerprint: 'fp-1',
  title: 'T',
  culprit: null,
  level: null,
  status: 'unresolved',
  event_count: 1,
  first_seen: 'now',
  last_seen: 'now'
};

beforeEach(() => {
  localStorage.clear();
  actions.byFingerprint = new Map();
  actions.setMe('me');
});

afterEach(() => {
  localStorage.clear();
});

describe('actions.assigneeFor', () => {
  it('returns null when not set', () => {
    expect(actions.assigneeFor(sample)).toBeNull();
  });

  it('returns the set assignee', () => {
    actions.setAssignee(sample, 'daisy');
    expect(actions.assigneeFor(sample)).toBe('daisy');
  });
});

describe('actions.setAssignee', () => {
  it('returns the previous value (for Undo)', () => {
    actions.setAssignee(sample, 'first');
    const prev = actions.setAssignee(sample, 'second');
    expect(prev).toBe('first');
  });

  it('persists across hydrate cycles via localStorage', () => {
    actions.setAssignee(sample, 'daisy');
    actions.byFingerprint = new Map(); // simulate fresh tab
    actions.hydrate();
    expect(actions.assigneeFor(sample)).toBe('daisy');
  });

  it('null clears the assignee', () => {
    actions.setAssignee(sample, 'daisy');
    actions.setAssignee(sample, null);
    expect(actions.assigneeFor(sample)).toBeNull();
  });
});

describe('actions.assignToMe / unassign', () => {
  it('assignToMe applies actions.me', () => {
    actions.setMe('daisy');
    actions.assignToMe(sample);
    expect(actions.assigneeFor(sample)).toBe('daisy');
  });

  it('unassign removes the entry', () => {
    actions.assignToMe(sample);
    actions.unassign(sample);
    expect(actions.assigneeFor(sample)).toBeNull();
  });
});

describe('actions.setMe', () => {
  it('persists to localStorage', () => {
    actions.setMe('alice');
    expect(localStorage.getItem('errex.me.v1')).toBe('alice');
  });

  it('falls back to "me" on empty', () => {
    actions.setMe('   ');
    expect(actions.me).toBe('me');
  });
});
