// Tests for the admin store. Auth is via the global session cookie now;
// the store no longer accepts or holds a token. We verify the wrapper
// behaviour (cache refresh after mutation, error classification) by
// spying on `api.admin.*`.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { admin } from './admin.svelte';
import { api, HttpError, type AdminProject } from './api';

const sample: AdminProject = {
  name: 'demo',
  token: 'sample-token',
  webhook_url: null,
  // Sentry-standard DSN: <scheme>://<token>@<host>/<project>.
  dsn: 'http://sample-token@test.local:9090/demo',
  ingest_url: 'http://test.local:9090/api/demo/envelope/?sentry_key=sample-token',
  created_at: '2026-01-01T00:00:00Z',
  last_used_at: null,
  last_webhook_status: null,
  last_webhook_at: null
};

beforeEach(() => {
  admin.projects = [];
  admin.error = null;
  admin.loading = false;
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('admin.loadProjects', () => {
  it('populates projects on success', async () => {
    vi.spyOn(api.admin, 'listProjects').mockResolvedValue([sample]);
    await admin.loadProjects();
    expect(admin.projects).toEqual([sample]);
    expect(admin.error).toBeNull();
  });

  it('classifies 401 as unauthorized', async () => {
    vi.spyOn(api.admin, 'listProjects').mockRejectedValue(new HttpError(401, 'unauthorized'));
    await admin.loadProjects();
    expect(admin.error).toBe('unauthorized');
  });

  it('classifies 403 as forbidden (viewer trying to load admin list)', async () => {
    vi.spyOn(api.admin, 'listProjects').mockRejectedValue(new HttpError(403, 'forbidden'));
    await admin.loadProjects();
    expect(admin.error).toBe('forbidden');
  });

  it('classifies other failures as network', async () => {
    vi.spyOn(api.admin, 'listProjects').mockRejectedValue(new Error('boom'));
    await admin.loadProjects();
    expect(admin.error).toBe('network');
  });
});

describe('admin.createProject', () => {
  it('returns the created project AND refreshes the cache', async () => {
    const create = vi.spyOn(api.admin, 'createProject').mockResolvedValue(sample);
    const list = vi.spyOn(api.admin, 'listProjects').mockResolvedValue([sample]);
    const result = await admin.createProject('demo');
    expect(result).toEqual(sample);
    expect(create).toHaveBeenCalledWith('demo');
    expect(list).toHaveBeenCalledOnce();
    expect(admin.projects).toEqual([sample]);
  });
});

describe('admin.setWebhook', () => {
  it('passes null through to clear', async () => {
    const spy = vi.spyOn(api.admin, 'setWebhook').mockResolvedValue(sample);
    vi.spyOn(api.admin, 'listProjects').mockResolvedValue([]);
    await admin.setWebhook('demo', null);
    expect(spy).toHaveBeenCalledWith('demo', null);
  });
});

describe('admin.rotateToken', () => {
  it('returns the new project shape', async () => {
    const rotated = { ...sample, token: 'new-token' };
    vi.spyOn(api.admin, 'rotateToken').mockResolvedValue(rotated);
    vi.spyOn(api.admin, 'listProjects').mockResolvedValue([rotated]);
    const result = await admin.rotateToken('demo');
    expect(result.token).toBe('new-token');
  });
});

describe('admin.renameProject', () => {
  it('passes old + new names through and refreshes the cache', async () => {
    const renamed = { ...sample, name: 'new-name' };
    const spy = vi.spyOn(api.admin, 'renameProject').mockResolvedValue(renamed);
    const list = vi.spyOn(api.admin, 'listProjects').mockResolvedValue([renamed]);
    const result = await admin.renameProject('demo', 'new-name');
    expect(spy).toHaveBeenCalledWith('demo', 'new-name');
    expect(result.name).toBe('new-name');
    expect(list).toHaveBeenCalledOnce();
    expect(admin.projects).toEqual([renamed]);
  });
});

describe('admin.deleteProject', () => {
  it('returns the destruction summary and refreshes the cache', async () => {
    const summary = { events_deleted: 12, issues_deleted: 3 };
    const spy = vi.spyOn(api.admin, 'deleteProject').mockResolvedValue(summary);
    vi.spyOn(api.admin, 'listProjects').mockResolvedValue([]);
    const result = await admin.deleteProject('doomed');
    expect(spy).toHaveBeenCalledWith('doomed');
    expect(result).toEqual(summary);
    expect(admin.projects).toEqual([]);
  });
});

describe('admin.destroyPreview', () => {
  it('returns counts without touching the cached list', async () => {
    admin.projects = [sample]; // Pre-populated cache that must NOT be cleared.
    const summary = { events_deleted: 5, issues_deleted: 2 };
    const spy = vi.spyOn(api.admin, 'destroyPreview').mockResolvedValue(summary);
    const list = vi.spyOn(api.admin, 'listProjects');
    const result = await admin.destroyPreview('demo');
    expect(spy).toHaveBeenCalledWith('demo');
    expect(result).toEqual(summary);
    expect(admin.projects).toEqual([sample]);
    expect(list).not.toHaveBeenCalled();
  });
});

describe('admin.getActivity', () => {
  it('returns 24h stats without mutating the cache', async () => {
    const stats = {
      events_24h: 42,
      unique_issues_24h: 3,
      last_event_at: '2026-04-26T12:00:00Z',
      hourly_buckets: Array(24).fill(0)
    };
    vi.spyOn(api.admin, 'getActivity').mockResolvedValue(stats);
    const result = await admin.getActivity('demo');
    expect(result.events_24h).toBe(42);
    expect(result.hourly_buckets).toHaveLength(24);
  });
});
